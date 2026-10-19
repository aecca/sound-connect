//! Equalizer settings as apps show them. How each brand encodes them lives
//! in its driver.

/// Sound presets, as named in Sony's app. `Manual` uses the custom bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EqPreset {
    Off,
    Bright,
    Excited,
    Mellow,
    Relaxed,
    Vocal,
    TrebleBoost,
    BassBoost,
    Speech,
    Manual,
    Custom1,
    Custom2,
    /// A preset this app doesn't know, by the headset's own code.
    Unknown(u8),
}

impl EqPreset {
    /// Every known preset, in the order Sony's app lists them.
    pub const ALL: [EqPreset; 12] = [
        Self::Off,
        Self::Bright,
        Self::Excited,
        Self::Mellow,
        Self::Relaxed,
        Self::Vocal,
        Self::TrebleBoost,
        Self::BassBoost,
        Self::Speech,
        Self::Manual,
        Self::Custom1,
        Self::Custom2,
    ];

    pub fn name(self) -> String {
        match self {
            Self::Off => "Off".into(),
            Self::Bright => "Bright".into(),
            Self::Excited => "Excited".into(),
            Self::Mellow => "Mellow".into(),
            Self::Relaxed => "Relaxed".into(),
            Self::Vocal => "Vocal".into(),
            Self::TrebleBoost => "Treble Boost".into(),
            Self::BassBoost => "Bass Boost".into(),
            Self::Speech => "Speech".into(),
            Self::Manual => "Manual".into(),
            Self::Custom1 => "Custom 1".into(),
            Self::Custom2 => "Custom 2".into(),
            Self::Unknown(code) => format!("Preset 0x{code:02x}"),
        }
    }
}

/// Which equalizer a headset has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqLayout {
    /// Five bands plus Clear Bass (WH-1000XM5 and most Sony models).
    FiveBands,
    /// Ten bands, no Clear Bass (WH-1000XM6).
    TenBands,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqBands {
    /// Steps from -10 to +10. Bands are 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz and
    /// 16 kHz.
    Five { clear_bass: i8, bands: [i8; 5] },
    /// Raw device values from 0 to 12. How they map to dB isn't known yet.
    Ten([u8; 10]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Equalizer {
    pub preset: EqPreset,
    pub bands: EqBands,
}
