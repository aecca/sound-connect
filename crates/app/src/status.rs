//! What `sound-connect status` says about the headset: a line for people, or
//! JSON for scripts and status bars. The JSON has the fields Waybar's custom
//! modules read (`text`, `alt`, `class`, `tooltip`, `percentage`) next to
//! plain ones (`connected`, `name`, `model`, `mode`, `battery`, `charging`).

use serde_json::{Map, Value, json};

use crate::headset::{Connection, Headset};

pub fn text(headset: &Headset) -> String {
    let Some(summary) = Summary::of(headset) else { return "No headphones are connected.".into() };
    let mut parts = vec![summary.mode_name.unwrap_or("Connected").to_owned()];
    if let Some((level, charging)) = summary.battery {
        parts.push(if charging { format!("battery {level}%, charging") } else { format!("battery {level}%") });
    }
    format!("{}: {}", summary.model, parts.join(", "))
}

pub fn json(headset: &Headset) -> String {
    let summary = Summary::of(headset);
    let mut status = Map::new();
    let class = summary.as_ref().map_or("disconnected", |summary| summary.mode.unwrap_or("connected"));
    let battery = summary.as_ref().and_then(|summary| summary.battery);
    status.insert("text".into(), battery.map(|(level, _)| format!("{level}%")).unwrap_or_default().into());
    status.insert("alt".into(), class.into());
    status.insert("class".into(), class.into());
    status.insert("tooltip".into(), text(headset).into());
    if let Some((level, _)) = battery {
        status.insert("percentage".into(), level.into());
    }
    status.insert("connected".into(), summary.is_some().into());
    status.insert("name".into(), json!(headset.name));
    status.insert("model".into(), json!(summary.as_ref().map(|summary| &summary.model)));
    status.insert("mode".into(), json!(summary.as_ref().and_then(|summary| summary.mode)));
    status.insert("battery".into(), json!(battery.map(|(level, _)| level)));
    status.insert("charging".into(), json!(battery.map(|(_, charging)| charging)));
    Value::Object(status).to_string()
}

/// A connected headset, as far as it's known.
struct Summary {
    model: String,
    mode: Option<&'static str>,
    mode_name: Option<&'static str>,
    battery: Option<(u8, bool)>,
}

impl Summary {
    fn of(headset: &Headset) -> Option<Self> {
        let device = headset.device.as_ref().filter(|_| headset.connection == Connection::Connected)?;
        let mode = headset.mode.and_then(|current| device.modes.iter().find(|mode| mode.mode == current));
        Some(Self {
            model: format!("{} {}", device.brand.name(), device.model),
            mode: mode.map(|mode| mode.mode.id()),
            mode_name: mode.map(|mode| mode.name),
            battery: headset.lowest_battery().map(|battery| (battery.level, battery.charging)),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use sc_bluetooth::Event;
    use sc_core::{Battery, Brand, Device, ListeningMode, Mode, Update};

    // Not a glob import: GPUI's own `test` attribute would shadow the built-in one.
    use super::{json, text};
    use crate::headset::Headset;

    fn xm5() -> Headset {
        let mut device = Device::unidentified(Brand::Sony, "WH-1000XM5");
        device.modes = vec![
            Mode { mode: ListeningMode::Off, name: "Off" },
            Mode { mode: ListeningMode::Ambient, name: "Ambient Sound" },
            Mode { mode: ListeningMode::NoiseCancelling, name: "Noise Cancelling" },
        ];
        let mut headset = Headset::new(mpsc::channel().0);
        headset.apply(Event::Connected { name: "Andy's WH-1000XM5".into() });
        headset.apply(Event::Update(Update::Device(device)));
        headset.apply(Event::Update(Update::Mode(ListeningMode::Ambient)));
        headset.apply(Event::Update(Update::Battery(Battery { level: 80, charging: false })));
        headset
    }

    #[test]
    fn describes_a_connected_headset() {
        assert_eq!(text(&xm5()), "Sony WH-1000XM5: Ambient Sound, battery 80%");
        let status: serde_json::Value = serde_json::from_str(&json(&xm5())).unwrap();
        assert_eq!(
            status,
            serde_json::json!({
                "text": "80%",
                "alt": "ambient",
                "class": "ambient",
                "tooltip": "Sony WH-1000XM5: Ambient Sound, battery 80%",
                "percentage": 80,
                "connected": true,
                "name": "Andy's WH-1000XM5",
                "model": "Sony WH-1000XM5",
                "mode": "ambient",
                "battery": 80,
                "charging": false,
            })
        );
    }

    #[test]
    fn goes_by_the_emptier_earbud() {
        let mut device = Device::unidentified(Brand::Sony, "WF-1000XM5");
        device.modes = vec![Mode { mode: ListeningMode::NoiseCancelling, name: "Noise Cancelling" }];
        let mut headset = Headset::new(mpsc::channel().0);
        headset.apply(Event::Connected { name: "WF-1000XM5".into() });
        headset.apply(Event::Update(Update::Device(device)));
        headset.apply(Event::Update(Update::Mode(ListeningMode::NoiseCancelling)));
        let (left, right) = (Battery { level: 70, charging: false }, Battery { level: 60, charging: false });
        headset.apply(Event::Update(Update::Earbuds { left: Some(left), right: Some(right) }));
        headset.apply(Event::Update(Update::CaseBattery(Battery { level: 20, charging: false })));
        assert_eq!(text(&headset), "Sony WF-1000XM5: Noise Cancelling, battery 60%");
    }

    #[test]
    fn says_when_nothing_is_connected() {
        let headset = Headset::new(mpsc::channel().0);
        assert_eq!(text(&headset), "No headphones are connected.");
        let status: serde_json::Value = serde_json::from_str(&json(&headset)).unwrap();
        assert_eq!(status["class"], "disconnected");
        assert_eq!(status["text"], "");
        assert_eq!(status["connected"], false);
        // Waybar reads it as a number, so it's left out rather than null.
        assert!(status.get("percentage").is_none());
    }
}
