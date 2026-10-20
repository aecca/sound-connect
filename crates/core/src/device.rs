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
    /// Choices for what its noise control button switches between; empty
    /// when it can't be changed.
    pub noise_button: Vec<NoiseButton>,
    /// Speak-to-Chat's sensitivity and when it ends can be changed.
    pub speak_to_chat_options: bool,
    pub equalizer: Option<EqLayout>,
}

impl Device {
    /// The listening mode after `current` in [`modes`](Self::modes), and the
    /// first after the last: what a "next mode" shortcut switches to.
    pub fn mode_after(&self, current: Option<ListeningMode>) -> Option<ListeningMode> {
        let position = current.and_then(|current| self.modes.iter().position(|mode| mode.mode == current));
        let next = position.map_or(0, |position| (position + 1) % self.modes.len());
        self.modes.get(next).map(|mode| mode.mode)
    }

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
            noise_button: Vec::new(),
            speak_to_chat_options: false,
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

impl ListeningMode {
    pub const ALL: [Self; 5] =
        [Self::NoiseCancelling, Self::NoiseCancellingLow, Self::WindReduction, Self::Ambient, Self::Off];

    /// A name that stays the same, for commands and scripts.
    pub fn id(self) -> &'static str {
        match self {
            Self::NoiseCancelling => "noise-cancelling",
            Self::NoiseCancellingLow => "noise-cancelling-low",
            Self::WindReduction => "wind-reduction",
            Self::Ambient => "ambient",
            Self::Off => "off",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.id() == id)
    }
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
    /// Pause playback while the headset is off your head or out of your ears.
    PauseWhenTakenOff,
    /// A button that can either change noise cancelling or call the phone's
    /// voice assistant (Bose's Action button) changes noise cancelling.
    ActionButtonNoiseCancelling,
}

impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Self::SpeakToChat => "Speak-to-Chat",
            Self::Dsee => "DSEE Extreme",
            Self::VoicePrompts => "Voice Prompts",
            Self::PauseWhenTakenOff => "Pause When Taken Off",
            Self::ActionButtonNoiseCancelling => "Action Button for Noise Cancelling",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::SpeakToChat => "Switches to ambient sound when you start talking",
            Self::Dsee => "Restores detail lost in compressed music",
            Self::VoicePrompts => "Spoken announcements, like the battery level when turning on",
            Self::PauseWhenTakenOff => "Pauses music when you take them off, and plays it when you put them back on",
            Self::ActionButtonNoiseCancelling => {
                "The Action button switches noise cancelling, instead of your phone's voice assistant"
            }
        }
    }
}

/// When the headset turns itself off while unused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AutoOff {
    Never,
    Minutes(u16),
    /// A while after being taken off, for headsets that can tell.
    WhenTakenOff,
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
            Self::WhenTakenOff => "When taken off".into(),
        }
    }
}

/// The listening modes the headset's noise control button switches between,
/// in turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoiseButton {
    NoiseCancellingAmbientOff,
    NoiseCancellingAmbient,
    NoiseCancellingOff,
    AmbientOff,
}

impl NoiseButton {
    pub const ALL: [Self; 4] =
        [Self::NoiseCancellingAmbientOff, Self::NoiseCancellingAmbient, Self::NoiseCancellingOff, Self::AmbientOff];

    pub fn name(self) -> &'static str {
        match self {
            Self::NoiseCancellingAmbientOff => "Noise Cancelling, Ambient Sound and Off",
            Self::NoiseCancellingAmbient => "Noise Cancelling and Ambient Sound",
            Self::NoiseCancellingOff => "Noise Cancelling and Off",
            Self::AmbientOff => "Ambient Sound and Off",
        }
    }
}

/// How readily Speak-to-Chat notices you talking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VoiceSensitivity {
    Auto,
    High,
    Low,
}

impl VoiceSensitivity {
    pub const ALL: [Self; 3] = [Self::Auto, Self::High, Self::Low];

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::High => "High",
            Self::Low => "Low",
        }
    }
}

/// When Speak-to-Chat goes back to the previous mode after you stop talking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpeakToChatEnd {
    /// After about 15 seconds.
    Short,
    /// After about 30 seconds.
    Standard,
    /// After about a minute.
    Long,
    /// Only when you change the mode.
    Never,
}

impl SpeakToChatEnd {
    pub const ALL: [Self; 4] = [Self::Short, Self::Standard, Self::Long, Self::Never];

    pub fn name(self) -> &'static str {
        match self {
            Self::Short => "After 15 seconds",
            Self::Standard => "After 30 seconds",
            Self::Long => "After 1 minute",
            Self::Never => "Never",
        }
    }
}

/// A device the headset is connected to; with multipoint, two at once, one
/// of them usually this computer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConnectedDevice {
    /// How the headset tells it apart, for [`Setting::PlayFrom`].
    pub id: String,
    pub name: String,
    /// The headset plays this device's audio.
    pub playing: bool,
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
    NoiseButton(NoiseButton),
    VoiceSensitivity(VoiceSensitivity),
    SpeakToChatEnd(SpeakToChatEnd),
    Equalizer(Equalizer),
    /// The devices it's connected to, when it says.
    ConnectedDevices(Vec<ConnectedDevice>),
}

/// Something the user changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Mode(ListeningMode),
    AmbientLevel(u8),
    FocusOnVoice(bool),
    Feature(Feature, bool),
    AutoOff(AutoOff),
    NoiseButton(NoiseButton),
    VoiceSensitivity(VoiceSensitivity),
    SpeakToChatEnd(SpeakToChatEnd),
    EqualizerPreset(EqPreset),
    EqualizerBands(EqBands),
    /// Play the audio of this connected device, by its
    /// [`id`](ConnectedDevice::id).
    PlayFrom(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_the_modes_and_wraps_around() {
        let mut device = Device::unidentified(Brand::Sony, "WH-1000XM5");
        assert_eq!(device.mode_after(None), None);
        device.modes = [ListeningMode::Off, ListeningMode::Ambient, ListeningMode::NoiseCancelling]
            .map(|mode| Mode { mode, name: "" })
            .to_vec();
        assert_eq!(device.mode_after(Some(ListeningMode::Off)), Some(ListeningMode::Ambient));
        assert_eq!(device.mode_after(Some(ListeningMode::NoiseCancelling)), Some(ListeningMode::Off));
        // Not known yet, or not one of this headset's: start from the first.
        assert_eq!(device.mode_after(None), Some(ListeningMode::Off));
        assert_eq!(device.mode_after(Some(ListeningMode::WindReduction)), Some(ListeningMode::Off));
    }

    #[test]
    fn names_modes_for_scripts() {
        for mode in ListeningMode::ALL {
            assert_eq!(ListeningMode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(ListeningMode::from_id("Ambient"), None);
    }

    #[test]
    fn names_auto_off_times() {
        assert_eq!(AutoOff::Never.name(), "Never");
        assert_eq!(AutoOff::Minutes(20).name(), "After 20 minutes");
        assert_eq!(AutoOff::Minutes(60).name(), "After 1 hour");
        assert_eq!(AutoOff::Minutes(180).name(), "After 3 hours");
        assert_eq!(AutoOff::WhenTakenOff.name(), "When taken off");
    }
}
