//! The equalizer window, like the Mac's: pick a preset or shape the manual
//! bands. Moving a band switches the preset to Manual.

use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use sc_core::{EqBands, EqLayout, EqPreset, Setting};

use crate::headset::Headset;

const FIVE_BANDS: [&str; 5] = ["400", "1k", "2.5k", "6.3k", "16k"];
const TEN_BANDS: [&str; 10] = ["31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];

/// The open window, so asking again brings it forward instead of opening another.
struct Open(AnyWindowHandle);

impl Global for Open {}

pub fn show(headset: &Entity<Headset>, cx: &mut App) {
    let open = cx.try_global::<Open>().map(|open| open.0);
    if let Some(handle) = open
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(600.), px(420.)), cx))),
        titlebar: Some(TitlebarOptions { title: Some("Equalizer".into()), ..Default::default() }),
        is_resizable: false,
        is_minimizable: false,
        ..Default::default()
    };
    let headset = headset.clone();
    match gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| EqualizerWindow::new(headset, window, cx))) {
        Ok((handle, _)) => cx.set_global(Open(handle)),
        Err(error) => eprintln!("failed to open the equalizer: {error:#}"),
    }
}

pub struct EqualizerWindow {
    headset: Entity<Headset>,
    /// The layout the sliders are for, and one slider per band (Clear Bass
    /// last, with five bands).
    layout: Option<EqLayout>,
    sliders: Vec<Entity<SliderState>>,
    _subscriptions: Vec<Subscription>,
}

impl EqualizerWindow {
    fn new(headset: Entity<Headset>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe_in(&headset, window, |this, _, window, cx| {
            this.follow_headset(window, cx);
            cx.notify();
        })];
        let mut this = Self { headset, layout: None, sliders: Vec::new(), _subscriptions: subscriptions };
        this.follow_headset(window, cx);
        this
    }

    /// Makes sliders for the headset's bands, and moves them to its values,
    /// e.g. after it changed presets.
    fn follow_headset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(equalizer) = self.headset.read(cx).equalizer else { return };
        let (layout, values) = match equalizer.bands {
            EqBands::Five { clear_bass, bands } => (
                EqLayout::FiveBands,
                bands.iter().chain([&clear_bass]).map(|&value| f32::from(value)).collect::<Vec<_>>(),
            ),
            EqBands::Ten(bands) => (EqLayout::TenBands, bands.iter().map(|&value| f32::from(value)).collect()),
        };
        if self.layout != Some(layout) {
            self.layout = Some(layout);
            self.make_sliders(layout, &values, window, cx);
            return;
        }
        for (slider, &value) in self.sliders.iter().zip(&values) {
            if slider.read(cx).value().start() != value {
                slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
            }
        }
    }

    fn make_sliders(&mut self, layout: EqLayout, values: &[f32], window: &mut Window, cx: &mut Context<Self>) {
        let (min, max) = match layout {
            EqLayout::FiveBands => (-10., 10.),
            // Raw device values; their dB scale isn't known yet.
            EqLayout::TenBands => (0., 12.),
        };
        self.sliders = values
            .iter()
            .map(|&value| cx.new(|_| SliderState::new().min(min).max(max).step(1.).default_value(value)))
            .collect();
        self._subscriptions.truncate(1);
        for (index, slider) in self.sliders.clone().into_iter().enumerate() {
            let subscription = cx.subscribe_in(&slider, window, move |this, _, event: &SliderEvent, _, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                this.move_band(index, value.start().round(), cx);
            });
            self._subscriptions.push(subscription);
        }
    }

    /// Sets one band, keeping the others as the headset has them.
    fn move_band(&mut self, index: usize, value: f32, cx: &mut Context<Self>) {
        let Some(equalizer) = self.headset.read(cx).equalizer else { return };
        let bands = match equalizer.bands {
            EqBands::Five { mut clear_bass, mut bands } => {
                // In -10..=10 once rounded, so it fits.
                let value = value as i8;
                match bands.get_mut(index) {
                    Some(band) => *band = value,
                    None => clear_bass = value,
                }
                EqBands::Five { clear_bass, bands }
            }
            EqBands::Ten(mut bands) => {
                let Some(band) = bands.get_mut(index) else { return };
                // In 0..=12 once rounded, so it fits.
                *band = value as u8;
                EqBands::Ten(bands)
            }
        };
        self.headset.update(cx, |headset, cx| headset.change(Setting::EqualizerBands(bands), cx));
    }

    fn presets(&self, current: EqPreset) -> impl IntoElement {
        let headset = self.headset.clone();
        v_flex().gap_2().w(px(160.)).child(div().font_weight(FontWeight::SEMIBOLD).child("Preset")).child(
            RadioGroup::vertical("presets")
                .children(
                    EqPreset::ALL
                        .iter()
                        .enumerate()
                        .map(|(index, preset)| Radio::new(index).label(preset.name()).small().text_size(px(13.))),
                )
                .selected_index(EqPreset::ALL.iter().position(|&preset| preset == current))
                .on_change(move |&index, _, cx| {
                    if let Some(&preset) = EqPreset::ALL.get(index) {
                        headset.update(cx, |headset, cx| headset.change(Setting::EqualizerPreset(preset), cx));
                    }
                }),
        )
    }

    fn bands(&self, cx: &App) -> impl IntoElement {
        let labels: &[&str] = match self.layout {
            Some(EqLayout::FiveBands) => &FIVE_BANDS,
            Some(EqLayout::TenBands) => &TEN_BANDS,
            None => &[],
        };
        let five = self.layout == Some(EqLayout::FiveBands);
        h_flex().gap_3().items_end().children(self.sliders.iter().enumerate().map(|(index, slider)| {
            let label = labels.get(index).copied().unwrap_or("Clear Bass");
            let value = slider.read(cx).value().start() as i32;
            v_flex()
                .items_center()
                .gap_1p5()
                .w(px(if five { 44. } else { 30. }))
                // Clear Bass stands apart from the bands.
                .when(five && index == FIVE_BANDS.len(), |this| this.ml_4())
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .when(five, |this| this.child(if value > 0 { format!("+{value}") } else { value.to_string() })),
                )
                .child(Slider::new(slider).vertical().h(px(170.)))
                .child(div().text_size(px(11.)).whitespace_nowrap().child(label))
        }))
    }
}

impl Render for EqualizerWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let body = match self.headset.read(cx).equalizer {
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Connect your headphones to change the equalizer.")
                .into_any_element(),
            Some(equalizer) => {
                let note = match self.layout {
                    Some(EqLayout::TenBands) => "Ten-band support hasn't been tested on real headphones yet.",
                    _ => "Moving a slider switches the preset to Manual.",
                };
                h_flex()
                    .flex_1()
                    .gap_8()
                    .items_start()
                    .child(self.presets(equalizer.preset))
                    .child(
                        v_flex()
                            .gap_4()
                            .child(self.bands(cx))
                            .child(div().text_size(px(12.)).text_color(theme.muted_foreground).child(note)),
                    )
                    .into_any_element()
            }
        };
        v_flex().size_full().p_5().bg(theme.background).text_color(theme.foreground).text_size(px(13.)).child(body)
    }
}
