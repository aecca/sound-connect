//! What apps know about a headset, whatever its brand: which controls it
//! has ([`Device`]), what it reports ([`Update`]) and what the user can
//! change ([`Setting`]).

use std::ops::RangeInclusive;

use crate::equalizer::{EqBands, EqLayout, EqPreset, Equalizer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Brand {
    Sony,
    Bose,
}

impl Brand {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sony => "Sony",
            Self::Bose => "Bose",
        }
    }
}

/// A headset and the controls it offers. Apps build their UI from this, so
/// they never need to know which brand they're talking to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub brand: Brand,
    pub model: String,
    /// Confirmed working on real hardware.
    pub tested: bool,
    pub battery: BatteryLayout,
    /// Listening modes, in the order the brand's own app lists them.
    pub modes: Vec<Mode>,
    /// Settings for [`ListeningMode::Ambient`], when the headset has them.
    pub ambient: Option<Ambient>,
    pub features: Vec<Feature>,
    /// Choices for turning off by itself; empty when it can't be changed.
    pub auto_off: Vec<AutoOff>,
    pub equalizer: Option<EqLayout>,
}

impl Device {
    /// A headset that's been recognized but whose controls aren't known yet.
    pub fn unidentified(brand: Brand, model: impl Into<String>) -> Self {
        Self {
            brand,
            model: model.into(),
            tested: false,
            battery: BatteryLayout::Single,
            modes: Vec::new(),
            ambient: None,
            features: Vec::new(),
            auto_off: Vec::new(),
            equalizer: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryLayout {
    /// One battery (over-ear headphones).
    Single,
    /// Left and right earbuds plus the charging case.
    Earbuds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    /// Percent.
    pub level: u8,
    pub charging: bool,
}

/// How much outside sound the headset lets in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListeningMode {
    NoiseCancelling,
    /// Lighter noise cancelling (Bose's "Low").
    NoiseCancellingLow,
    /// Noise cancelling tuned for wind.
    WindReduction,
    /// Lets outside sound in (Sony's "Ambient Sound").
    Ambient,
    Off,
}

/// A listening mode under the name the brand gives it, e.g. Bose's "High".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub mode: ListeningMode,
    pub name: &'static str,
}

/// What can be adjusted in [`ListeningMode::Ambient`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ambient {
    /// How much outside sound to let in.
    pub levels: RangeInclusive<u8>,
    pub focus_on_voice: bool,
}

/// A feature that's simply on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feature {
    SpeakToChat,
    Dsee,
    VoicePrompts,
}

impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Self::SpeakToChat => "Speak-to-Chat",
            Self::Dsee => "DSEE Extreme",
            Self::VoicePrompts => "Voice Prompts",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::SpeakToChat => "Switches to ambient sound when you start talking",
            Self::Dsee => "Restores detail lost in compressed music",
            Self::VoicePrompts => "Spoken announcements, like the battery level when turning on",
        }
    }
}

/// When the headset turns itself off while unused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AutoOff {
    Never,
    Minutes(u16),
}

impl AutoOff {
    pub fn name(self) -> String {
        match self {
            Self::Never => "Never".into(),
            Self::Minutes(minutes) if minutes % 60 == 0 => match minutes / 60 {
                1 => "After 1 hour".into(),
                hours => format!("After {hours} hours"),
            },
            Self::Minutes(minutes) => format!("After {minutes} minutes"),
        }
    }
}

/// Something the headset reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// What the headset is and which controls it has. Sent once it's known,
    /// and again if it changes.
    Device(Device),
    /// The headset can't be controlled; the reason is shown to the user.
    Unsupported(String),
    Firmware(String),
    Codec(String),
    Battery(Battery),
    /// `None` for an earbud that isn't reporting, e.g. while in its case.
    Earbuds {
        left: Option<Battery>,
        right: Option<Battery>,
    },
    CaseBattery(Battery),
    Mode(ListeningMode),
    AmbientLevel(u8),
    FocusOnVoice(bool),
    Feature(Feature, bool),
    AutoOff(AutoOff),
    Equalizer(Equalizer),
}

/// Something the user changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Mode(ListeningMode),
    AmbientLevel(u8),
    FocusOnVoice(bool),
    Feature(Feature, bool),
    AutoOff(AutoOff),
    EqualizerPreset(EqPreset),
    EqualizerBands(EqBands),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_auto_off_times() {
        assert_eq!(AutoOff::Never.name(), "Never");
        assert_eq!(AutoOff::Minutes(20).name(), "After 20 minutes");
        assert_eq!(AutoOff::Minutes(60).name(), "After 1 hour");
        assert_eq!(AutoOff::Minutes(180).name(), "After 3 hours");
    }
}
