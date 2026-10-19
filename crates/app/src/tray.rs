//! The menu bar icon, and the panel it opens and closes.

use std::time::{Duration, Instant};

use gpui_kit::*;
use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::headset::Headset;
use crate::panel::{self, Panel};

struct PanelWindow {
    handle: Option<AnyWindowHandle>,
    /// When the panel last closed because the user clicked elsewhere.
    dismissed_at: Option<Instant>,
    tray: TrayIcon,
}

impl Global for PanelWindow {}

pub fn install(headset: Entity<Headset>, cx: &mut App) {
    let tray = TrayIconBuilder::new()
        .with_icon_templated(crate::icon::headphones())
        .with_tooltip("Sound Connect")
        .build()
        .expect("failed to add the menu bar icon");
    cx.set_global(PanelWindow { handle: None, dismissed_at: None, tray });

    let (clicks, clicked) = async_channel::unbounded();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Down, rect, .. } =
            event
        {
            let _ = clicks.send_blocking(rect);
        }
    }));
    cx.spawn(async move |cx| {
        while let Ok(rect) = clicked.recv().await {
            cx.update(|cx| toggle(rect, &headset, cx));
        }
    })
    .detach();
}

/// Opens the panel without a click and keeps it open. Used to check the
/// design during development.
pub fn open_now(headset: &Entity<Headset>, cx: &mut App) {
    let state = cx.global::<PanelWindow>();
    if state.handle.is_none()
        && let Some(icon) = state.tray.rect()
    {
        open(icon, headset.clone(), true, cx);
    }
}

/// Closes the panel. Called when it loses focus.
pub fn dismiss(window: &mut Window, cx: &mut App) {
    let state = cx.global_mut::<PanelWindow>();
    state.handle = None;
    state.dismissed_at = Some(Instant::now());
    window.remove_window();
}

fn toggle(icon: Rect, headset: &Entity<Headset>, cx: &mut App) {
    let state = cx.global::<PanelWindow>();
    if let Some(handle) = state.handle {
        let _ = handle.update(cx, |_, window, cx| dismiss(window, cx));
        return;
    }
    // Clicking the icon while the panel is open first closes it by taking
    // focus away; that same click shouldn't open it again.
    if state.dismissed_at.is_some_and(|at| at.elapsed() < Duration::from_millis(300)) {
        return;
    }
    open(icon, headset.clone(), false, cx);
}

fn open(icon: Rect, headset: Entity<Headset>, pinned: bool, cx: &mut App) {
    // The icon's position comes in physical pixels; windows use points.
    let scale = screen_scale_factor();
    let icon_center = (icon.position.x + icon.size.width as f64 / 2.) / scale;
    let icon_bottom = (icon.position.y + icon.size.height as f64) / scale;
    let origin = point(px((icon_center as f32 - panel::WIDTH / 2.).max(8.)), px(icon_bottom as f32 + 6.));

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size(px(panel::WIDTH), px(panel::HEIGHT))))),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    };
    cx.activate(true);
    match gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Panel::new(headset, pinned, window, cx))) {
        Ok((handle, _)) => cx.global_mut::<PanelWindow>().handle = Some(handle),
        Err(error) => eprintln!("failed to open the panel: {error:#}"),
    }
}

#[cfg(target_os = "macos")]
fn screen_scale_factor() -> f64 {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    MainThreadMarker::new()
        .and_then(|mtm| NSScreen::screens(mtm).firstObject())
        .map_or(2., |screen| screen.backingScaleFactor())
}

#[cfg(not(target_os = "macos"))]
fn screen_scale_factor() -> f64 {
    1.
}
