//! The menu bar icon, through `tray-icon`. Clicks report the icon's
//! rectangle, so the panel drops down right under it.

use gpui_kit::*;
use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::{icon, panel};

/// The icon's rectangle on screen, in physical pixels.
pub type Anchor = Rect;

pub struct Tray(TrayIcon);

impl Tray {
    pub fn new(clicks: async_channel::Sender<Anchor>) -> Self {
        // A template image: only the alpha channel matters, macOS picks the color.
        let rgba = icon::headphones().into_iter().flat_map(|alpha| [0, 0, 0, alpha]).collect();
        let image = tray_icon::Icon::from_rgba(rgba, icon::SIZE, icon::SIZE).expect("icon has the right size");
        let tray = TrayIconBuilder::new()
            .with_icon_templated(image)
            .with_tooltip("Sound Connect")
            .build()
            .expect("failed to add the menu bar icon");

        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left, button_state: MouseButtonState::Down, rect, ..
            } = event
            {
                let _ = clicks.send_blocking(rect);
            }
        }));
        Self(tray)
    }

    /// Where to open the panel without a click.
    pub fn anchor(&self) -> Option<Anchor> {
        self.0.rect()
    }
}

/// Centers the panel under the icon.
pub fn panel_origin(icon: Anchor, _cx: &App) -> Point<Pixels> {
    // The icon's position comes in physical pixels; windows use points.
    let scale = screen_scale_factor();
    let icon_center = (icon.position.x + icon.size.width as f64 / 2.) / scale;
    let icon_bottom = (icon.position.y + icon.size.height as f64) / scale;
    point(px((icon_center as f32 - panel::WIDTH / 2.).max(8.)), px(icon_bottom as f32 + 6.))
}

pub fn open_window(
    bounds: Bounds<Pixels>,
    cx: &mut App,
    build: impl Fn(&mut Window, &mut App) -> Entity<panel::Panel>,
) -> Result<AnyWindowHandle> {
    gpui_kit::open_window(super::window_options(bounds), cx, build).map(|(handle, _)| handle)
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
