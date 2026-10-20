//! The tray icon as a StatusNotifierItem over D-Bus, through `ksni`, so no
//! GTK main loop has to run next to GPUI's. Hosts report where the user
//! clicked rather than where the icon is, and sometimes not even that.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use gpui_kit::layer_shell::{self, KeyboardInteractivity, Layer, LayerShellNotSupportedError, LayerShellOptions};
use gpui_kit::*;
use ksni::blocking::TrayMethods;

use crate::{icon, panel};

/// Where the user clicked, in screen coordinates, when the host says.
pub type Anchor = Option<Point<Pixels>>;

/// Space between the panel and the click or the screen's edges.
const MARGIN: f32 = 8.;

pub struct Tray {
    /// Changes for the tooltip, for the thread that owns the icon.
    statuses: Option<mpsc::Sender<String>>,
}

impl Tray {
    pub fn new(clicks: async_channel::Sender<Anchor>) -> Self {
        // Keep going without a tray host: the icon appears once one starts.
        let handle = match Item::new(clicks).assume_sni_available(true).spawn() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("failed to add the tray icon: {error}");
                return Self { statuses: None };
            }
        };
        // Updating the icon waits on D-Bus, which can take a while when the
        // tray host restarts, so it happens away from the UI.
        let (statuses, changes) = mpsc::channel::<String>();
        let spawned = thread::Builder::new().name("tray".into()).spawn(move || {
            while let Ok(mut status) = changes.recv() {
                // Only the latest one matters.
                status = changes.try_iter().last().unwrap_or(status);
                handle.update(|item| item.status = status);
            }
        });
        Self { statuses: spawned.is_ok().then_some(statuses) }
    }

    /// What the icon's tooltip says under the app's name, like the mode and
    /// battery level.
    pub fn set_status(&self, status: String) {
        if let Some(statuses) = &self.statuses {
            let _ = statuses.send(status);
        }
    }

    /// Where to open the panel without a click: the top-right corner.
    pub fn anchor(&self) -> Option<Anchor> {
        Some(None)
    }
}

struct Item {
    clicks: async_channel::Sender<Anchor>,
    pixmap: ksni::Icon,
    status: String,
}

impl Item {
    fn new(clicks: async_channel::Sender<Anchor>) -> Self {
        // White, for the dark panels most desktops use. Only shown when the
        // icon theme has no headphones.
        let data = icon::headphones().into_iter().flat_map(|alpha| [alpha, 255, 255, 255]).collect();
        let pixmap = ksni::Icon { width: icon::SIZE as i32, height: icon::SIZE as i32, data };
        Self { clicks, pixmap, status: String::new() }
    }
}

impl ksni::Tray for Item {
    fn id(&self) -> String {
        "sound-connect".into()
    }

    fn title(&self) -> String {
        "Sound Connect".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn icon_name(&self) -> String {
        "audio-headphones-symbolic".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.pixmap.clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Sound Connect".into(), description: self.status.clone(), ..Default::default() }
    }

    fn activate(&mut self, x: i32, y: i32) {
        // Hosts that don't know where the click was send 0, 0.
        let click = (x, y) != (0, 0);
        let _ = self.clicks.try_send(click.then(|| point(px(x as f32), px(y as f32))));
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        eprintln!("no system tray to show the icon in ({reason:?}); on GNOME this needs the AppIndicator extension");
        true
    }
}

pub fn panel_origin(click: Anchor, cx: &App) -> Point<Pixels> {
    place(click, screen_bounds(click, cx))
}

/// Puts the panel next to the click, below it when the click is near the
/// top of the screen and above it otherwise, or in the top-right corner
/// without one. Either way it stays on screen.
fn place(click: Anchor, screen: Bounds<Pixels>) -> Point<Pixels> {
    let (width, height) = (panel::WIDTH, panel::HEIGHT);
    let (left, top, right, bottom) =
        (f32::from(screen.left()), f32::from(screen.top()), f32::from(screen.right()), f32::from(screen.bottom()));

    let (x, y) = match click {
        Some(click) => {
            let (x, y) = (f32::from(click.x), f32::from(click.y));
            let below = y < (top + bottom) / 2.;
            (x - width / 2., if below { y + MARGIN } else { y - height - MARGIN })
        }
        None => (right - width - MARGIN, top + MARGIN),
    };
    point(px(x.min(right - width - MARGIN).max(left + MARGIN)), px(y.min(bottom - height - MARGIN).max(top + MARGIN)))
}

/// The display showing `point`, else the primary one.
fn screen_bounds(point: Option<Point<Pixels>>, cx: &App) -> Bounds<Pixels> {
    point
        .and_then(|point| cx.displays().into_iter().find(|display| display.bounds().contains(&point)))
        .or_else(|| cx.primary_display())
        .map(|display| display.bounds())
        .unwrap_or_default()
}

/// Wayland ignores where windows ask to go. Where the compositor supports
/// layer-shell (most do, GNOME doesn't), the panel becomes a layer pinned to
/// its spot; elsewhere it's a pop-up the compositor places. X11 places
/// pop-ups where they ask.
pub fn open_window(
    bounds: Bounds<Pixels>,
    cx: &mut App,
    build: impl Fn(&mut Window, &mut App) -> Entity<panel::Panel>,
) -> Result<AnyWindowHandle> {
    static LAYER_SHELL: AtomicBool = AtomicBool::new(true);

    if cx.compositor_name() == "Wayland" && LAYER_SHELL.load(Ordering::Relaxed) {
        match gpui_kit::open_window(layer_options(bounds, cx), cx, &build) {
            Err(error) if error.is::<LayerShellNotSupportedError>() => LAYER_SHELL.store(false, Ordering::Relaxed),
            result => return result.map(|(handle, _)| handle),
        }
    }
    gpui_kit::open_window(super::window_options(bounds), cx, build).map(|(handle, _)| handle)
}

fn layer_options(bounds: Bounds<Pixels>, cx: &App) -> WindowOptions {
    let display = cx.displays().into_iter().find(|display| display.bounds().contains(&bounds.origin));
    let screen = display.as_ref().map(|display| display.bounds()).unwrap_or_default();
    WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "sound-connect".into(),
            layer: Layer::Top,
            anchor: layer_shell::Anchor::TOP | layer_shell::Anchor::LEFT,
            // Measure from the screen's edges, not from the panels on it.
            exclusive_zone: Some(px(-1.)),
            margin: Some((bounds.top() - screen.top(), px(0.), px(0.), bounds.left() - screen.left())),
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }),
        display_id: display.map(|display| display.id()),
        ..super::window_options(bounds)
    }
}

#[cfg(test)]
mod tests {
    // Not a glob import: GPUI's own `test` attribute would shadow the built-in one.
    use gpui_kit::{Bounds, Pixels, point, px, size};

    use super::{MARGIN, place};
    use crate::panel::HEIGHT;

    fn screen(x: f32, y: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(1920.), px(1080.)))
    }

    #[test]
    fn opens_in_the_top_right_corner_without_a_click() {
        assert_eq!(place(None, screen(0., 0.)), point(px(1612.), px(8.)));
    }

    #[test]
    fn drops_down_from_a_top_bar_and_stays_on_screen() {
        // An icon at the far right: the panel can't center on it.
        assert_eq!(place(Some(point(px(1900.), px(12.))), screen(0., 0.)), point(px(1612.), px(20.)));
        assert_eq!(place(Some(point(px(1000.), px(12.))), screen(0., 0.)), point(px(850.), px(20.)));
    }

    /// Where the panel's top goes so its bottom sits just above a click at `y`.
    fn above(y: f32) -> f32 {
        y - HEIGHT - MARGIN
    }

    #[test]
    fn rises_above_a_bottom_panel() {
        assert_eq!(place(Some(point(px(40.), px(1060.))), screen(0., 0.)), point(px(8.), px(above(1060.))));
    }

    #[test]
    fn stays_on_the_clicked_screen() {
        assert_eq!(place(Some(point(px(1960.), px(1060.))), screen(1920., 0.)), point(px(1928.), px(above(1060.))));
    }
}
