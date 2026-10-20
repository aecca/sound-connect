//! What each Sony model supports, so the app only offers controls that work
//! and asks for the right things after connecting.
//!
//! Capabilities come from Sony Device Center's `DeviceProfileRegistry.cpp`
//! (MIT) and Gadgetbridge's per-model coordinators. Only models marked
//! `tested` have been tried on real hardware by this project.

use super::command::{Request, auto_off_choices};
use crate::device::{Ambient, BatteryLayout, Brand, Device, Feature, ListeningMode, Mode, NoiseButton};
use crate::equalizer::EqLayout;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Profile {
    /// The model's name, or "" when it isn't recognized.
    pub model: &'static str,
    pub tested: bool,
    pub battery: BatteryLayout,
    pub wind_reduction: bool,
    pub speak_to_chat: bool,
    pub equalizer: Option<EqLayout>,
    pub dsee: bool,
    /// Knows when it's worn: it can pause when taken off, and turn off a while
    /// after.
    pub wear_sensor: bool,
    /// What its NC/AMB button switches between can be changed.
    pub noise_button: bool,
}

/// Headsets that aren't in the list get the WH-1000XM5's feature set, the
/// common v2 one. Anything they don't answer simply stays hidden.
const UNKNOWN: Profile = Profile {
    model: "",
    tested: false,
    battery: BatteryLayout::Single,
    wind_reduction: false,
    speak_to_chat: true,
    equalizer: Some(EqLayout::FiveBands),
    dsee: true,
    wear_sensor: true,
    noise_button: false,
};

const fn over_ear(model: &'static str, speak_to_chat: bool, equalizer: EqLayout) -> Profile {
    Profile { model, speak_to_chat, equalizer: Some(equalizer), ..UNKNOWN }
}

const fn earbuds(model: &'static str, wind_reduction: bool) -> Profile {
    Profile { model, battery: BatteryLayout::Earbuds, wind_reduction, ..UNKNOWN }
}

/// Name fragments (upper case) and the profile they identify.
const PROFILES: [(&[&str], Profile); 8] = [
    (
        &["WH-1000XM5"],
        Profile { tested: true, noise_button: true, ..over_ear("WH-1000XM5", true, EqLayout::FiveBands) },
    ),
    (&["WH-1000XM6"], Profile { noise_button: true, ..over_ear("WH-1000XM6", true, EqLayout::TenBands) }),
    (&["WH-CH720N"], Profile { wear_sensor: false, ..over_ear("WH-CH720N", false, EqLayout::FiveBands) }),
    (&["ULT WEAR", "WH-ULT900N"], Profile { wear_sensor: false, ..over_ear("ULT WEAR", false, EqLayout::FiveBands) }),
    (&["WF-1000XM4"], earbuds("WF-1000XM4", true)),
    (&["WF-1000XM5"], earbuds("WF-1000XM5", true)),
    (&["WF-1000XM6"], earbuds("WF-1000XM6", false)),
    (&["LINKBUDS S", "WF-LS900N"], earbuds("LinkBuds S", false)),
];

/// The profile for a headset, from its Bluetooth name.
pub(super) fn profile_for(name: &str) -> Profile {
    let name = name.to_uppercase();
    PROFILES
        .iter()
        .find(|(fragments, _)| fragments.iter().any(|fragment| name.contains(fragment)))
        .map_or(UNKNOWN, |(_, profile)| *profile)
}

impl Profile {
    /// The headset as apps see it. `name` stands in for an unknown model.
    pub fn device(&self, name: &str) -> Device {
        let mut modes = vec![
            Mode { mode: ListeningMode::Off, name: "Off" },
            Mode { mode: ListeningMode::Ambient, name: "Ambient Sound" },
            Mode { mode: ListeningMode::NoiseCancelling, name: "Noise Cancelling" },
        ];
        if self.wind_reduction {
            modes.push(Mode { mode: ListeningMode::WindReduction, name: "Wind Noise Reduction" });
        }
        let features = [
            (self.speak_to_chat, Feature::SpeakToChat),
            (self.dsee, Feature::Dsee),
            (self.wear_sensor, Feature::PauseWhenTakenOff),
        ]
        .into_iter()
        .filter_map(|(has, feature)| has.then_some(feature))
        .collect();
        Device {
            brand: Brand::Sony,
            model: if self.model.is_empty() { name.to_owned() } else { self.model.to_owned() },
            tested: self.tested,
            battery: self.battery,
            modes,
            ambient: Some(Ambient { levels: 1..=20, focus_on_voice: true }),
            features,
            auto_off: if self.wear_sensor { auto_off_choices() } else { Vec::new() },
            noise_button: if self.noise_button { NoiseButton::ALL.to_vec() } else { Vec::new() },
            equalizer: self.equalizer,
        }
    }

    /// What to ask the headset right after connecting, to fill in the UI.
    pub fn initial_requests(&self) -> Vec<Request> {
        let mut requests = vec![Request::Init, Request::FirmwareVersion];
        requests.extend(self.battery_requests());
        requests.extend([Request::Codec, Request::NoiseControl]);
        if self.speak_to_chat {
            requests.push(Request::SpeakToChat);
        }
        if let Some(layout) = self.equalizer {
            requests.push(Request::Equalizer(layout));
        }
        if self.dsee {
            requests.push(Request::Dsee);
        }
        if self.wear_sensor {
            requests.extend([Request::PauseWhenTakenOff, Request::AutoOff]);
        }
        if self.noise_button {
            requests.push(Request::NoiseButton);
        }
        requests
    }

    /// What to ask to refresh the battery levels.
    pub fn battery_requests(&self) -> Vec<Request> {
        match self.battery {
            BatteryLayout::Single => vec![Request::Battery],
            BatteryLayout::Earbuds => vec![Request::EarbudBattery, Request::CaseBattery],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::AutoOff;

    #[test]
    fn recognizes_models_by_name() {
        assert_eq!(profile_for("WH-1000XM5").model, "WH-1000XM5");
        assert!(profile_for("WH-1000XM5").tested);
        assert_eq!(profile_for("Andy's WF-1000XM5").battery, BatteryLayout::Earbuds);
        assert_eq!(profile_for("LinkBuds S").model, "LinkBuds S");
        assert_eq!(profile_for("wh-1000xm6").equalizer, Some(EqLayout::TenBands));
        assert_eq!(profile_for("Some Speaker"), UNKNOWN);
    }

    #[test]
    fn only_the_xm5_is_marked_tested() {
        let tested: Vec<_> = PROFILES.iter().filter(|(_, p)| p.tested).map(|(_, p)| p.model).collect();
        assert_eq!(tested, ["WH-1000XM5"]);
    }

    #[test]
    fn asks_each_model_for_what_it_has() {
        // What the WH-1000XM5 was tested with, then what other apps change on
        // it: pausing and turning off when taken off, and its NC/AMB button.
        assert_eq!(
            profile_for("WH-1000XM5").initial_requests(),
            [
                Request::Init,
                Request::FirmwareVersion,
                Request::Battery,
                Request::Codec,
                Request::NoiseControl,
                Request::SpeakToChat,
                Request::Equalizer(EqLayout::FiveBands),
                Request::Dsee,
                Request::PauseWhenTakenOff,
                Request::AutoOff,
                Request::NoiseButton,
            ]
        );
        assert!(!profile_for("WF-1000XM5").initial_requests().contains(&Request::NoiseButton));
        let earbuds = profile_for("WF-1000XM5").initial_requests();
        assert!(earbuds.contains(&Request::EarbudBattery) && earbuds.contains(&Request::CaseBattery));
        assert!(!earbuds.contains(&Request::Battery));
        assert!(!profile_for("WH-CH720N").initial_requests().contains(&Request::SpeakToChat));
        assert!(!profile_for("ULT WEAR").initial_requests().contains(&Request::AutoOff));
        assert!(profile_for("WH-1000XM6").initial_requests().contains(&Request::Equalizer(EqLayout::TenBands)));
    }

    #[test]
    fn describes_the_device_for_apps() {
        let xm5 = profile_for("WH-1000XM5").device("WH-1000XM5");
        assert_eq!(xm5.features, [Feature::SpeakToChat, Feature::Dsee, Feature::PauseWhenTakenOff]);
        assert_eq!(xm5.auto_off, [AutoOff::WhenTakenOff, AutoOff::Never]);
        assert_eq!(xm5.modes.len(), 3);
        assert!(profile_for("WH-CH720N").device("WH-CH720N").auto_off.is_empty());

        let wf = profile_for("WF-1000XM5").device("WF-1000XM5");
        assert!(wf.modes.iter().any(|mode| mode.mode == ListeningMode::WindReduction));

        let unknown = profile_for("Sony Speaker").device("Sony Speaker");
        assert_eq!((unknown.model.as_str(), unknown.tested), ("Sony Speaker", false));
    }
}
