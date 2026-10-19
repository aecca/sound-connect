//! The small window that opens under the menu bar icon. Styled after the
//! AirPods section of the macOS menu bar: system font, translucent
//! background, circular icons and the system accent color.

use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use sc_bluetooth::{Battery, NoiseControl, NoiseMode};

use crate::headset::{Connection, Headset};

pub const WIDTH: f32 = 300.;
pub const HEIGHT: f32 = 392.;

const MODES: [(NoiseMode, &str, &str); 3] = [
    (NoiseMode::NoiseCancelling, "Noise Cancelling", "icons/ear-off.svg"),
    (NoiseMode::Ambient, "Ambient Sound", "icons/ear.svg"),
    (NoiseMode::Off, "Off", "icons/circle-off.svg"),
];

/// macOS system colors for the current appearance.
#[derive(Clone, Copy)]
struct Palette {
    background: Hsla,
    label: Hsla,
    secondary: Hsla,
    separator: Hsla,
    fill: Hsla,
    hover: Hsla,
    accent: Hsla,
    green: Hsla,
    red: Hsla,
}

impl Palette {
    fn for_window(window: &Window) -> Self {
        match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self {
                background: rgba(0x28282cb8).into(),
                label: rgba(0xffffffe6).into(),
                secondary: rgba(0xffffff8c).into(),
                separator: rgba(0xffffff1f).into(),
                fill: rgba(0xffffff24).into(),
                hover: rgba(0xffffff14).into(),
                accent: rgb(0x0a84ff).into(),
                green: rgb(0x30d158).into(),
                red: rgb(0xff453a).into(),
            },
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self {
                background: rgba(0xf6f6f6c8).into(),
                label: rgba(0x000000d9).into(),
                secondary: rgba(0x0000008c).into(),
                separator: rgba(0x0000001a).into(),
                fill: rgba(0x00000014).into(),
                hover: rgba(0x0000000d).into(),
                accent: rgb(0x007aff).into(),
                green: rgb(0x34c759).into(),
                red: rgb(0xff3b30).into(),
            },
        }
    }
}

pub struct Panel {
    headset: Entity<Headset>,
    ambient_level: Entity<SliderState>,
    was_active: bool,
    /// Stays open when it loses focus (development aid).
    pinned: bool,
    _subscriptions: Vec<Subscription>,
}

impl Panel {
    pub fn new(headset: Entity<Headset>, pinned: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let level = headset.read(cx).noise.map_or(20., |n| n.ambient_level as f32);
        let ambient_level = cx.new(|_| SliderState::new().min(1.).max(20.).step(1.).default_value(level));

        let subscriptions = vec![
            // Close like a menu when the user clicks anywhere else.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.was_active = true;
                } else if this.was_active && !this.pinned {
                    crate::tray::dismiss(window, cx);
                }
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                cx.notify();
            }),
            // Follow changes made on the headset itself, e.g. its NC button.
            cx.observe_in(&headset, window, |this, headset, window, cx| {
                if let Some(noise) = headset.read(cx).noise {
                    let level = noise.ambient_level as f32;
                    if this.ambient_level.read(cx).value().start() != level {
                        this.ambient_level.update(cx, |slider, cx| slider.set_value(level, window, cx));
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(&ambient_level, window, |this, _, event: &SliderEvent, _, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                let level = value.start().round() as u8;
                this.headset.update(cx, |headset, cx| {
                    if let Some(noise) = headset.noise {
                        headset.set_noise(NoiseControl { ambient_level: level, ..noise }, cx);
                    }
                });
            }),
        ];

        headset.read(cx).refresh();
        Self { headset, ambient_level, was_active: false, pinned, _subscriptions: subscriptions }
    }

    fn header(&self, p: Palette, cx: &App) -> impl IntoElement {
        let headset = self.headset.read(cx);
        let title = headset.name.clone().unwrap_or_else(|| "Headphones".into());
        let status = match &headset.connection {
            Connection::Searching => "Not connected".to_owned(),
            Connection::Unsupported(_) => "Not supported".to_owned(),
            Connection::Connected => match headset.codec {
                Some(codec) => format!("Connected · {}", codec.name()),
                None => "Connected".to_owned(),
            },
        };

        h_flex()
            .px_2()
            .py_1p5()
            .gap_2p5()
            .child(icon_circle("icons/headphones.svg", 32., p.fill, p.label))
            .child(
                v_flex()
                    .flex_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().text_size(px(11.)).text_color(p.secondary).child(status)),
            )
            .children(headset.battery.map(|battery| battery_indicator(battery, p)))
    }

    fn noise_controls(&self, noise: NoiseControl, p: Palette) -> impl IntoElement {
        let ambient = noise.mode == NoiseMode::Ambient;
        let headset = self.headset.clone();

        v_flex()
            .gap_0p5()
            .child(section_title("Noise Control", p))
            .children(MODES.iter().map(|&(mode, label, icon)| {
                let headset = self.headset.clone();
                let selected = noise.mode == mode;
                h_flex()
                    .id(label)
                    .px_2()
                    .py_1()
                    .gap_2p5()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|style| style.bg(p.hover))
                    .child(icon_circle(
                        icon,
                        26.,
                        if selected { p.accent } else { p.fill },
                        if selected { white() } else { p.label },
                    ))
                    .child(label)
                    .on_click(move |_, _, cx| {
                        headset.update(cx, |headset, cx| headset.set_noise(NoiseControl { mode, ..noise }, cx));
                    })
            }))
            // Ambient options stay in place, dimmed, so the panel doesn't jump around.
            .child(
                v_flex()
                    .px_2()
                    .pt_1()
                    .gap_2()
                    .when(!ambient, |this| this.opacity(0.4))
                    .child(
                        v_flex()
                            .gap_1p5()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .text_size(px(12.))
                                    .child("Ambient Sound Level")
                                    .child(div().text_color(p.secondary).child(noise.ambient_level.to_string())),
                            )
                            .child(Slider::new(&self.ambient_level).disabled(!ambient)),
                    )
                    .child(toggle_row(
                        "focus-on-voice",
                        "Focus on Voice",
                        None,
                        noise.focus_on_voice,
                        !ambient,
                        p,
                        move |on, cx| {
                            headset.update(cx, |headset, cx| {
                                headset.set_noise(NoiseControl { focus_on_voice: on, ..noise }, cx)
                            })
                        },
                    )),
            )
    }

    fn message(title: &str, body: &str, p: Palette) -> impl IntoElement {
        v_flex()
            .flex_1()
            .justify_center()
            .items_center()
            .gap_1()
            .px_6()
            .text_center()
            .child(div().font_weight(FontWeight::MEDIUM).child(title.to_owned()))
            .child(div().text_size(px(11.)).text_color(p.secondary).child(body.to_owned()))
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_window(window);
        let headset = self.headset.read(cx);
        let connection = headset.connection.clone();
        let noise = headset.noise;
        let speak_to_chat = headset.speak_to_chat;

        let body = match (connection, noise) {
            (Connection::Searching, _) => Self::message(
                "Looking for your headphones",
                "Make sure they're on and connected to this computer.",
                p,
            )
            .into_any_element(),
            (Connection::Unsupported(reason), _) => {
                Self::message("These headphones aren't supported yet", &reason, p).into_any_element()
            }
            (Connection::Connected, None) => Self::message("Connecting…", "", p).into_any_element(),
            (Connection::Connected, Some(noise)) => {
                let headset = self.headset.clone();
                v_flex()
                    .flex_1()
                    .child(self.noise_controls(noise, p))
                    .child(separator(p))
                    .children(speak_to_chat.map(|on| {
                        toggle_row(
                            "speak-to-chat",
                            "Speak-to-Chat",
                            Some("Switches to ambient sound when you talk"),
                            on,
                            false,
                            p,
                            move |on, cx| headset.update(cx, |headset, cx| headset.set_speak_to_chat(on, cx)),
                        )
                    }))
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .p_1p5()
            .bg(p.background)
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .text_color(p.label)
            .child(self.header(p, cx))
            .child(separator(p))
            .child(v_flex().flex_1().child(body))
            .child(separator(p))
            .child(
                h_flex()
                    .px_2()
                    .py_0p5()
                    .justify_between()
                    .items_center()
                    .child(div().text_size(px(11.)).text_color(p.secondary).child("Sound Connect Beta"))
                    .child(
                        div()
                            .id("quit")
                            .px_2()
                            .py_0p5()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|style| style.bg(p.hover))
                            .child("Quit")
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            )
    }
}

fn icon_circle(path: &'static str, diameter: f32, background: Hsla, color: Hsla) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(diameter))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .child(svg().path(path).size(px(diameter * 0.55)).text_color(color))
}

fn section_title(title: &'static str, p: Palette) -> impl IntoElement {
    div()
        .px_2()
        .pt_1()
        .pb_0p5()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.secondary)
        .child(title)
}

fn separator(p: Palette) -> impl IntoElement {
    div().mx_2().my_1p5().h(px(1.)).bg(p.separator)
}

/// A battery drawn like the one in the macOS menu bar.
fn battery_indicator(battery: Battery, p: Palette) -> impl IntoElement {
    let level = battery.level.min(100) as f32 / 100.;
    let fill = if battery.charging {
        p.green
    } else if battery.level <= 20 {
        p.red
    } else {
        p.label
    };
    h_flex()
        .gap_1p5()
        .items_center()
        .child(div().text_size(px(11.)).text_color(p.secondary).child(format!("{}%", battery.level)))
        .child(
            h_flex()
                .items_center()
                .child(
                    div()
                        .w(px(22.))
                        .h(px(11.))
                        .p(px(1.5))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(p.secondary)
                        .child(div().h_full().w(relative(level)).rounded(px(1.5)).bg(fill)),
                )
                .child(div().ml(px(1.)).w(px(1.5)).h(px(4.)).rounded_r(px(1.)).bg(p.secondary)),
        )
}

fn toggle_row(
    id: &'static str,
    label: &'static str,
    caption: Option<&'static str>,
    on: bool,
    disabled: bool,
    p: Palette,
    on_change: impl Fn(bool, &mut App) + 'static,
) -> impl IntoElement {
    h_flex()
        .px_2()
        .py_1()
        .gap_3()
        .justify_between()
        .child(
            v_flex()
                .child(label)
                .children(caption.map(|c| div().text_size(px(11.)).text_color(p.secondary).child(c))),
        )
        .child(
            Switch::new(id)
                .checked(on)
                .small()
                .color(p.accent)
                .disabled(disabled)
                .on_change(move |&on, _, cx| on_change(on, cx)),
        )
}
