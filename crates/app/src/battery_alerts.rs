//! "Low Battery Alerts": a notification when the headset's battery runs low,
//! through org.freedesktop.Notifications, which GNOME, KDE, mako and the
//! rest provide. [`LowBattery`] decides when.

use std::thread;

use gpui_kit::*;
use sc_core::LowBattery;

use crate::headset::Headset;
use crate::settings::Settings;

pub fn start(headset: Entity<Headset>, cx: &mut App) {
    let mut low = LowBattery::default();
    cx.observe(&headset, move |headset, cx| {
        let headset = headset.read(cx);
        let (Some(battery), Some(device)) = (headset.battery, &headset.device) else { return };
        if !low.update(battery) || !cx.global::<Settings>().low_battery_alerts {
            return;
        }
        let body = format!("{} {} has {}% left.", device.brand.name(), device.model, battery.level);
        // Talking to the notification service can take a moment.
        let _ = thread::Builder::new().name("notification".into()).spawn(move || {
            if let Err(error) = notify("Your headphones' battery is low", &body) {
                eprintln!("couldn't show a notification: {error}");
            }
        });
    })
    .detach();
}

#[cfg(target_os = "linux")]
fn notify(summary: &str, body: &str) -> Result<(), dbus::Error> {
    use std::time::Duration;

    use dbus::arg::PropMap;
    use dbus::blocking::Connection;

    let session = Connection::new_session()?;
    let notifications =
        session.with_proxy("org.freedesktop.Notifications", "/org/freedesktop/Notifications", Duration::from_secs(5));
    // App name, ID to replace (none), icon, summary, body, actions, hints,
    // and how long to show it (the server's choice).
    let (_id,): (u32,) = notifications.method_call(
        "org.freedesktop.Notifications",
        "Notify",
        ("Sound Connect", 0u32, "sound-connect", summary, body, Vec::<&str>::new(), PropMap::new(), -1i32),
    )?;
    Ok(())
}

/// The Mac has its own app; this build of the panel is for trying it out.
#[cfg(not(target_os = "linux"))]
fn notify(summary: &str, body: &str) -> Result<(), std::convert::Infallible> {
    eprintln!("{summary} {body}");
    Ok(())
}
