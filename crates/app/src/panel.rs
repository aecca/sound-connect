//! The small window that opens from the tray icon. Styled after the AirPods
//! section of the macOS menu bar: system font, translucent background,
//! circular icons and the system accent color. Which controls it shows comes
//! from the headset's [`Device`], so it works for any brand.

use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Selectable, Sizable, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use sc_core::{AutoOff, Battery, Device, Feature, ListeningMode, Setting, SpeakToChatEnd, VoiceSensitivity};

use crate::calls::Calls;
use crate::equalizer;
use crate::headset::{Connection, Headset};
use crate::settings::{self, Settings};
use crate::update::{self, Status, Updates};

pub const WIDTH: f32 = 300.;
pub const HEIGHT: f32 = 460.;

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
        let level = f32::from(headset.read(cx).ambient_level.unwrap_or(20));
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
            cx.observe_global::<Updates>(|_, cx| cx.notify()),
            cx.observe_global::<Settings>(|_, cx| cx.notify()),
            cx.observe_global::<Calls>(|_, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                cx.notify();
            }),
            // Follow changes made on the headset itself, e.g. its NC button.
            cx.observe_in(&headset, window, |this, headset, window, cx| {
                if let Some(level) = headset.read(cx).ambient_level.map(f32::from)
                    && this.ambient_level.read(cx).value().start() != level
                {
                    this.ambient_level.update(cx, |slider, cx| slider.set_value(level, window, cx));
                }
                cx.notify();
            }),
            cx.subscribe_in(&ambient_level, window, |this, _, event: &SliderEvent, _, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                // The slider moves in whole steps from 1 to 20.
                let level = value.start().round().clamp(1., 20.) as u8;
                this.headset.update(cx, |headset, cx| headset.change(Setting::AmbientLevel(level), cx));
            }),
        ];

        Self { headset, ambient_level, was_active: false, pinned, _subscriptions: subscriptions }
    }

    fn header(&self, p: Palette, cx: &App) -> impl IntoElement {
        let headset = self.headset.read(cx);
        let title = match (&headset.device, &headset.name) {
            (Some(device), _) => format!("{} {}", device.brand.name(), device.model),
            (None, Some(name)) => name.clone(),
            (None, None) => "Headphones".into(),
        };
        let status = match &headset.connection {
            Connection::Searching => "Not connected".to_owned(),
            Connection::Unsupported(_) => "Not supported".to_owned(),
            Connection::Connected => match &headset.codec {
                Some(codec) => format!("Connected · {codec}"),
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
            .children(update_button(p, cx))
    }

    /// Each earbud and the case, like the AirPods menu.
    fn earbuds(&self, p: Palette, cx: &App) -> Option<impl IntoElement> {
        let headset = self.headset.read(cx);
        let sides = [("L", headset.left_battery), ("R", headset.right_battery), ("Case", headset.case_battery)];
        let reporting: Vec<_> = sides.into_iter().filter_map(|(side, battery)| Some((side, battery?))).collect();
        if reporting.is_empty() {
            return None;
        }
        let row =
            h_flex().px_2().pb_1().gap_4().justify_end().children(reporting.into_iter().map(|(side, battery)| {
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(div().text_size(px(11.)).text_color(p.secondary).child(side))
                    .child(battery_indicator(battery, p))
            }));
        Some(row)
    }

    fn modes(&self, device: &Device, p: Palette, cx: &App) -> impl IntoElement {
        let current = self.headset.read(cx).mode;
        v_flex().gap_0p5().child(section_title("Noise Control", p)).children(device.modes.iter().map(|mode| {
            let headset = self.headset.clone();
            let selected = current == Some(mode.mode);
            let setting = Setting::Mode(mode.mode);
            h_flex()
                .id(mode.name)
                .px_2()
                .py_1()
                .gap_2p5()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|style| style.bg(p.hover))
                .child(icon_circle(
                    mode_icon(mode.mode),
                    26.,
                    if selected { p.accent } else { p.fill },
                    if selected { white() } else { p.label },
                ))
                .child(mode.name)
                .on_click(move |_, _, cx| headset.update(cx, |headset, cx| headset.change(setting.clone(), cx)))
        }))
    }

    /// Ambient options stay in place, dimmed outside ambient mode, so the
    /// panel doesn't jump around.
    fn ambient(&self, focus_on_voice: bool, p: Palette, cx: &App) -> impl IntoElement {
        let headset = self.headset.read(cx);
        let active = headset.mode == Some(ListeningMode::Ambient);
        let level = headset.ambient_level.map(|level| level.to_string()).unwrap_or_default();
        let focus = headset.focus_on_voice.unwrap_or(false);
        let entity = self.headset.clone();

        v_flex()
            .px_2()
            .pt_1()
            .gap_2()
            .when(!active, |this| this.opacity(0.4))
            .child(
                v_flex()
                    .gap_1p5()
                    .child(
                        h_flex()
                            .justify_between()
                            .text_size(px(12.))
                            .child("Ambient Sound Level")
                            .child(div().text_color(p.secondary).child(level)),
                    )
                    .child(Slider::new(&self.ambient_level).disabled(!active)),
            )
            .when(focus_on_voice, |this| {
                this.child(toggle_row("focus-on-voice", "Focus on Voice", None, focus, !active, p, move |on, cx| {
                    entity.update(cx, |headset, cx| headset.change(Setting::FocusOnVoice(on), cx))
                }))
            })
    }

    /// "Ambient Sound During Calls", when the headset has ambient sound and
    /// this computer can tell when the microphone is in use.
    fn calls(&self, p: Palette, cx: &App) -> Option<impl IntoElement> {
        if !cx.global::<Calls>().supported || !self.headset.read(cx).offers(ListeningMode::Ambient) {
            return None;
        }
        let on = cx.global::<Settings>().ambient_during_calls;
        Some(toggle_row(
            "calls",
            "Ambient Sound During Calls",
            Some("Hear yourself while an app uses the microphone"),
            on,
            false,
            p,
            |on, cx| settings::change(cx, |settings| settings.ambient_during_calls = on),
        ))
    }

    fn low_battery_alerts(p: Palette, cx: &App) -> impl IntoElement {
        toggle_row(
            "low-battery-alerts",
            "Low Battery Alerts",
            Some("A notification at 20% and at 10%"),
            cx.global::<Settings>().low_battery_alerts,
            false,
            p,
            |on, cx| settings::change(cx, |settings| settings.low_battery_alerts = on),
        )
    }

    /// The on/off features, with Speak-to-Chat's options right under it.
    fn features(&self, device: &Device, p: Palette, cx: &App) -> impl IntoElement {
        let states = &self.headset.read(cx).features;
        v_flex().children(device.features.iter().filter_map(|&feature| {
            let on = *states.get(&feature)?;
            let headset = self.headset.clone();
            let row =
                toggle_row(feature.name(), feature.name(), Some(feature.description()), on, false, p, move |on, cx| {
                    headset.update(cx, |headset, cx| headset.change(Setting::Feature(feature, on), cx))
                });
            let options =
                (feature == Feature::SpeakToChat).then(|| self.speak_to_chat_options(device, p, cx)).flatten();
            Some(v_flex().child(row).children(options))
        }))
    }

    /// The current preset, opening the equalizer window when clicked.
    fn equalizer(&self, p: Palette, cx: &App) -> Option<impl IntoElement> {
        let preset = self.headset.read(cx).equalizer?.preset;
        let headset = self.headset.clone();
        let row = h_flex()
            .id("equalizer")
            .px_2()
            .py_1()
            .justify_between()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|style| style.bg(p.hover))
            .child("Equalizer")
            .child(div().text_color(p.secondary).child(format!("{} ›", preset.name())))
            .on_click(move |_, _, cx| equalizer::show(&headset, cx));
        Some(row)
    }

    /// Shown once the headset has reported it, like the features.
    fn auto_off(&self, device: &Device, p: Palette, cx: &App) -> Option<impl IntoElement> {
        let current = self.headset.read(cx).auto_off.filter(|_| !device.auto_off.is_empty())?;
        let headset = self.headset.clone();
        let row = v_flex()
            .px_2()
            .py_1()
            .gap_1p5()
            .child("Turn Off Automatically")
            .child(choice_buttons("auto-off", &device.auto_off, current, short_name, false, move |choice, cx| {
                headset.update(cx, |headset, cx| headset.change(Setting::AutoOff(choice), cx))
            }))
            .text_color(p.label);
        Some(row)
    }

    /// Speak-to-Chat's sensitivity and when it ends, once the headset has
    /// said. Dimmed, like the ambient options, while it's off.
    fn speak_to_chat_options(&self, device: &Device, p: Palette, cx: &App) -> Option<impl IntoElement> {
        let headset = self.headset.read(cx);
        let (Some(sensitivity), Some(end)) = (headset.voice_sensitivity, headset.speak_to_chat_end) else {
            return None;
        };
        if !device.speak_to_chat_options {
            return None;
        }
        let on = headset.features.get(&Feature::SpeakToChat) == Some(&true);
        let (for_sensitivity, for_end) = (self.headset.clone(), self.headset.clone());
        let set_sensitivity = move |sensitivity, cx: &mut App| {
            for_sensitivity.update(cx, |headset, cx| headset.change(Setting::VoiceSensitivity(sensitivity), cx))
        };
        let set_end =
            move |end, cx: &mut App| for_end.update(cx, |headset, cx| headset.change(Setting::SpeakToChatEnd(end), cx));
        let options = v_flex()
            .px_2()
            .py_1()
            .gap_1p5()
            .when(!on, |this| this.opacity(0.4))
            .child("Speak-to-Chat Sensitivity")
            .child(choice_buttons(
                "voice-sensitivity",
                &VoiceSensitivity::ALL,
                sensitivity,
                |s| s.name().into(),
                !on,
                set_sensitivity,
            ))
            .child("Back to the Previous Mode")
            .child(choice_buttons(
                "speak-to-chat-end",
                &SpeakToChatEnd::ALL,
                end,
                |end| short_end_name(end).into(),
                !on,
                set_end,
            ))
            .text_color(p.label);
        Some(options)
    }

    /// Which of the devices the headset is connected to it plays, when it's
    /// connected to more than this computer.
    fn play_from(&self, p: Palette, cx: &App) -> Option<impl IntoElement> {
        let devices = &self.headset.read(cx).connected_devices;
        if devices.len() < 2 {
            return None;
        }
        let ids: Vec<String> = devices.iter().map(|device| device.id.clone()).collect();
        let headset = self.headset.clone();
        let row = v_flex().px_2().py_1().gap_1p5().child("Play Audio From").child(
            RadioGroup::vertical("play-from")
                .children(devices.iter().enumerate().map(|(index, device)| {
                    Radio::new(index).label(device.name.clone()).small().w_full().text_size(px(12.))
                }))
                .selected_index(devices.iter().position(|device| device.playing))
                .on_change(move |&index, _, cx| {
                    if let Some(id) = ids.get(index) {
                        headset.update(cx, |headset, cx| headset.change(Setting::PlayFrom(id.clone()), cx));
                    }
                }),
        );
        Some(row.text_color(p.label))
    }

    /// What the headset's noise control button switches between, once the
    /// headset has said.
    fn noise_button(&self, device: &Device, p: Palette, cx: &App) -> Option<impl IntoElement> {
        let current = self.headset.read(cx).noise_button?;
        let choices = device.noise_button.clone();
        let selected = choices.iter().position(|&choice| choice == current);
        let headset = self.headset.clone();
        let row =
            v_flex().px_2().py_1().gap_1p5().child("Noise Control Button").child(
                RadioGroup::vertical("noise-button")
                    .children(choices.iter().enumerate().map(|(index, choice)| {
                        Radio::new(index).label(choice.name()).small().w_full().text_size(px(12.))
                    }))
                    .selected_index(selected)
                    .on_change(move |&index, _, cx| {
                        if let Some(&choice) = choices.get(index) {
                            headset.update(cx, |headset, cx| headset.change(Setting::NoiseButton(choice), cx));
                        }
                    }),
            );
        Some(row.text_color(p.label))
    }

    /// While no headset is connected: what's going on, and a button to
    /// connect each supported headset that's paired.
    fn searching(&self, p: Palette, cx: &App) -> AnyElement {
        let headset = self.headset.read(cx);
        if let Some(name) = &headset.connecting {
            return Self::message(&format!("Connecting {name}…"), "", p).into_any_element();
        }
        let body = headset.notice.unwrap_or("Make sure they're on and connected to this computer.");
        let buttons = headset.paired.iter().map(|paired| {
            let (entity, paired) = (self.headset.clone(), paired.clone());
            Button::new(SharedString::from(format!("connect {}", paired.address)))
                .label(format!("Connect {}", paired.name))
                .small()
                .w_full()
                .on_click(move |_, _, cx| entity.update(cx, |headset, cx| headset.connect(&paired, cx)))
        });
        v_flex()
            .flex_1()
            .child(Self::message("Looking for your headphones", body, p))
            .child(v_flex().px_6().pb_4().gap_2().children(buttons))
            .into_any_element()
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

    fn controls(&self, device: &Device, p: Palette, cx: &App) -> AnyElement {
        v_flex()
            .id("controls")
            .flex_1()
            .overflow_y_scroll()
            .when(!device.modes.is_empty(), |this| this.child(self.modes(device, p, cx)))
            .when_some(device.ambient.as_ref(), |this, ambient| this.child(self.ambient(ambient.focus_on_voice, p, cx)))
            .children(self.calls(p, cx))
            .when(!device.features.is_empty(), |this| this.child(separator(p)).child(self.features(device, p, cx)))
            .when_some(self.equalizer(p, cx), |this, equalizer| this.child(separator(p)).child(equalizer))
            .when_some(self.auto_off(device, p, cx), |this, auto_off| this.child(separator(p)).child(auto_off))
            .when_some(self.noise_button(device, p, cx), |this, button| this.child(separator(p)).child(button))
            .when_some(self.play_from(p, cx), |this, play_from| this.child(separator(p)).child(play_from))
            .child(separator(p))
            .child(Self::low_battery_alerts(p, cx))
            .into_any_element()
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_window(window);
        let headset = self.headset.read(cx);
        let body = match (&headset.connection, &headset.device) {
            (Connection::Searching, _) => self.searching(p, cx),
            (Connection::Unsupported(reason), _) => {
                Self::message("These headphones aren't supported yet", reason, p).into_any_element()
            }
            (Connection::Connected, None) => Self::message("Connecting…", "", p).into_any_element(),
            (Connection::Connected, Some(device)) => self.controls(&device.clone(), p, cx),
        };

        v_flex()
            .size_full()
            .p_1p5()
            .bg(p.background)
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .text_color(p.label)
            .child(self.header(p, cx))
            .children(self.earbuds(p, cx))
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

/// A blue Update button, like the system's, while a new version is out.
fn update_button(p: Palette, cx: &App) -> Option<AnyElement> {
    let label = match cx.global::<Updates>().status {
        Status::UpToDate => return None,
        Status::Installing(_) => {
            return Some(div().text_size(px(11.)).text_color(p.secondary).child("Updating…").into_any_element());
        }
        Status::Available(_) => "Update",
        Status::Failed(_) => "Try Again",
    };
    let button = div()
        .id("update")
        .px_2p5()
        .py_0p5()
        .rounded_full()
        .cursor_pointer()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(white())
        .bg(p.accent)
        .child(label)
        .on_click(|_, _, cx| update::install(cx));
    Some(button.into_any_element())
}

fn mode_icon(mode: ListeningMode) -> &'static str {
    match mode {
        ListeningMode::NoiseCancelling | ListeningMode::NoiseCancellingLow | ListeningMode::WindReduction => {
            "icons/ear-off.svg"
        }
        ListeningMode::Ambient => "icons/ear.svg",
        ListeningMode::Off => "icons/circle-off.svg",
    }
}

/// A row of buttons, one per choice, with `current` selected.
fn choice_buttons<T: Copy + PartialEq + 'static>(
    id: &'static str,
    choices: &[T],
    current: T,
    label: impl Fn(T) -> SharedString,
    disabled: bool,
    on_choose: impl Fn(T, &mut App) + 'static,
) -> impl IntoElement {
    let choices = choices.to_vec();
    ButtonGroup::new(id)
        .outline()
        .xsmall()
        .w_full()
        .disabled(disabled)
        .children(
            choices
                .iter()
                .enumerate()
                .map(|(index, &choice)| Button::new(index).label(label(choice)).flex_1().selected(choice == current)),
        )
        .on_click(move |clicked, _, cx| {
            if let Some(&choice) = clicked.first().and_then(|&index| choices.get(index)) {
                on_choose(choice, cx);
            }
        })
}

fn short_end_name(end: SpeakToChatEnd) -> &'static str {
    match end {
        SpeakToChatEnd::Short => "15 s",
        SpeakToChatEnd::Standard => "30 s",
        SpeakToChatEnd::Long => "1 min",
        SpeakToChatEnd::Never => "Never",
    }
}

/// Auto-off choices short enough to sit side by side.
fn short_name(auto_off: AutoOff) -> SharedString {
    match auto_off {
        AutoOff::Never => "Never".into(),
        AutoOff::Minutes(minutes) if minutes % 60 == 0 => format!("{}h", minutes / 60).into(),
        AutoOff::Minutes(minutes) => format!("{minutes}m").into(),
        AutoOff::WhenTakenOff => "When taken off".into(),
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
    let level = f32::from(battery.level.min(100)) / 100.;
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
            // Takes what's left and wraps, so a long caption can't push the switch out.
            v_flex()
                .flex_1()
                .min_w_0()
                .child(label)
                .children(caption.map(|caption| div().text_size(px(11.)).text_color(p.secondary).child(caption))),
        )
        .child(
            Switch::new(id)
                .flex_none()
                .checked(on)
                .small()
                .color(p.accent)
                .disabled(disabled)
                .on_change(move |&on, _, cx| on_change(on, cx)),
        )
}
