//! The tray icon, and the panel it opens and closes. The icon is platform
//! code: the menu bar through `tray-icon` on macOS, a StatusNotifierItem
//! through `ksni` on Linux.

use std::time::{Duration, Instant};

use gpui_kit::*;

use crate::headset::Headset;
use crate::panel::{self, Panel};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(target_os = "linux"))]
mod macos;
#[cfg(not(target_os = "linux"))]
use macos as platform;

struct PanelWindow {
    handle: Option<AnyWindowHandle>,
    /// When the panel last closed because the user clicked elsewhere.
    dismissed_at: Option<Instant>,
    tray: platform::Tray,
}

impl Global for PanelWindow {}

pub fn install(headset: Entity<Headset>, cx: &mut App) {
    let (clicks, clicked) = async_channel::unbounded();
    let tray = platform::Tray::new(clicks);
    cx.set_global(PanelWindow { handle: None, dismissed_at: None, tray });
    cx.spawn(async move |cx| {
        while let Ok(anchor) = clicked.recv().await {
            cx.update(|cx| toggle(anchor, &headset, cx));
        }
    })
    .detach();
}

/// Opens the panel without a click and keeps it open. Used to check the
/// design during development.
pub fn open_now(headset: &Entity<Headset>, cx: &mut App) {
    let state = cx.global::<PanelWindow>();
    if state.handle.is_none()
        && let Some(anchor) = state.tray.anchor()
    {
        open(anchor, headset.clone(), true, cx);
    }
}

/// Closes the panel. Called when it loses focus.
pub fn dismiss(window: &mut Window, cx: &mut App) {
    let state = cx.global_mut::<PanelWindow>();
    state.handle = None;
    state.dismissed_at = Some(Instant::now());
    window.remove_window();
}

fn toggle(anchor: platform::Anchor, headset: &Entity<Headset>, cx: &mut App) {
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
    open(anchor, headset.clone(), false, cx);
}

fn open(anchor: platform::Anchor, headset: Entity<Headset>, pinned: bool, cx: &mut App) {
    let origin = platform::panel_origin(anchor, cx);
    let bounds = Bounds::new(origin, size(px(panel::WIDTH), px(panel::HEIGHT)));
    cx.activate(true);
    let build = |window: &mut Window, cx: &mut App| cx.new(|cx| Panel::new(headset.clone(), pinned, window, cx));
    match platform::open_window(bounds, cx, build) {
        Ok(handle) => cx.global_mut::<PanelWindow>().handle = Some(handle),
        Err(error) => eprintln!("failed to open the panel: {error:#}"),
    }
}

/// A borderless pop-up showing the panel at `bounds`.
fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: true,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    }
}
