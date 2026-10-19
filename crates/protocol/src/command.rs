//! Typed requests and the state updates parsed from replies and notifications.
//!
//! Byte layouts follow the v2 protocol (WH-1000XM5 and newer) as documented
//! by Gadgetbridge and Sony Device Center, and checked against a WH-1000XM5.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseMode {
    NoiseCancelling,
    Ambient,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseControl {
    pub mode: NoiseMode,
    pub focus_on_voice: bool,
    /// 1..=20
    pub ambient_level: u8,
}

impl Default for NoiseControl {
    fn default() -> Self {
        Self { mode: NoiseMode::NoiseCancelling, focus_on_voice: false, ambient_level: 20 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Sbc,
    Aac,
    Ldac,
    AptX,
    AptXHd,
    Unknown(u8),
}

impl Codec {
    fn from_byte(b: u8) -> Self {
        match b {
            0x01 => Self::Sbc,
            0x02 => Self::Aac,
            0x10 => Self::Ldac,
            0x20 => Self::AptX,
            0x21 => Self::AptXHd,
            other => Self::Unknown(other),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Sbc => "SBC".into(),
            Self::Aac => "AAC".into(),
            Self::Ldac => "LDAC".into(),
            Self::AptX => "aptX".into(),
            Self::AptXHd => "aptX HD".into(),
            Self::Unknown(b) => format!("0x{b:02x}"),
        }
    }
}

/// Sound presets, as named in Sony's app. `Manual` uses the custom bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    Unknown(u8),
}

impl EqPreset {
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

    fn code(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::Bright => 0x10,
            Self::Excited => 0x11,
            Self::Mellow => 0x12,
            Self::Relaxed => 0x13,
            Self::Vocal => 0x14,
            Self::TrebleBoost => 0x15,
            Self::BassBoost => 0x16,
            Self::Speech => 0x17,
            Self::Manual => 0xa0,
            Self::Custom1 => 0xa1,
            Self::Custom2 => 0xa2,
            Self::Unknown(code) => code,
        }
    }

    fn from_code(code: u8) -> Self {
        Self::ALL.into_iter().find(|p| p.code() == code).unwrap_or(Self::Unknown(code))
    }

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

/// Five-band equalizer with Clear Bass (WH-1000XM5 and similar). Every
/// value is a step from -10 to +10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Equalizer {
    pub preset: EqPreset,
    pub clear_bass: i8,
    /// 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz, 16 kHz.
    pub bands: [i8; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Init,
    FirmwareVersion,
    Battery,
    Codec,
    NoiseControl,
    SetNoiseControl(NoiseControl),
    SpeakToChat,
    SetSpeakToChat(bool),
    Equalizer,
    SetEqualizerPreset(EqPreset),
    /// Switches to the Manual preset with these values.
    SetEqualizerBands { clear_bass: i8, bands: [i8; 5] },
    /// DSEE Extreme, Sony's upscaling for compressed music.
    Dsee,
    SetDsee(bool),
}

/// Noise-control "inquired type" used by v2 headsets. 0x15 gets no reply on
/// a WH-1000XM5, 0x17 does.
const NOISE_TYPE: u8 = 0x17;
/// Speak-to-Chat lives among the "system" settings (0xf6..0xf9), as type 0x0c.
/// Its value is inverted: 0x00 means on.
const SPEAK_TO_CHAT: u8 = 0x0c;
/// Equalizer layout with five bands plus Clear Bass. (The WH-1000XM6 uses a
/// ten-band layout, 0x04, which isn't supported yet.)
const EQ_FIVE_BANDS: u8 = 0x00;
/// Equalizer values are sent offset by 10, so -10..=10 becomes 0..=20.
const EQ_OFFSET: i8 = 10;

impl Request {
    pub fn payload(&self) -> Vec<u8> {
        match self {
            Self::Init => vec![0x00, 0x00],
            Self::FirmwareVersion => vec![0x04, 0x02],
            Self::Battery => vec![0x22, 0x00],
            Self::Codec => vec![0x12, 0x02],
            Self::NoiseControl => vec![0x66, NOISE_TYPE],
            Self::SetNoiseControl(nc) => vec![
                0x68,
                NOISE_TYPE,
                0x01,
                (nc.mode != NoiseMode::Off) as u8,
                (nc.mode == NoiseMode::Ambient) as u8,
                nc.focus_on_voice as u8,
                nc.ambient_level.clamp(1, 20),
            ],
            Self::SpeakToChat => vec![0xf6, SPEAK_TO_CHAT],
            Self::SetSpeakToChat(on) => vec![0xf8, SPEAK_TO_CHAT, (!on) as u8, 0x01],
            Self::Equalizer => vec![0x56, EQ_FIVE_BANDS],
            Self::SetEqualizerPreset(preset) => vec![0x58, EQ_FIVE_BANDS, preset.code(), 0x00],
            Self::SetEqualizerBands { clear_bass, bands } => {
                let mut payload = vec![0x58, EQ_FIVE_BANDS, EqPreset::Manual.code(), 0x06];
                let values = std::iter::once(*clear_bass).chain(bands.iter().copied());
                payload.extend(values.map(|v| (v.clamp(-10, 10) + EQ_OFFSET) as u8));
                payload
            }
            Self::Dsee => vec![0xe6, 0x01],
            Self::SetDsee(on) => vec![0xe8, 0x01, *on as u8],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// The headset answered the handshake. `v2` is false for older headsets,
    /// which this app does not support yet.
    Ready { v2: bool },
    Firmware(String),
    Battery(Battery),
    Codec(Codec),
    NoiseControl(NoiseControl),
    SpeakToChat(bool),
    Equalizer(Equalizer),
    Dsee(bool),
}

/// Parses a reply or notification payload. Returns `None` for messages this
/// app does not use yet.
pub fn parse(p: &[u8]) -> Option<Update> {
    match *p.first()? {
        0x01 => Some(Update::Ready { v2: p.len() == 8 }),
        0x05 if p.len() > 3 && p[1] == 0x02 => {
            Some(Update::Firmware(String::from_utf8_lossy(&p[3..]).into_owned()))
        }
        0x23 | 0x25 if p.len() >= 4 && p[1] == 0x00 => {
            Some(Update::Battery(Battery { level: p[2], charging: p[3] == 1 }))
        }
        0x13 | 0x15 if p.len() >= 3 && p[1] == 0x02 => Some(Update::Codec(Codec::from_byte(p[2]))),
        0x67 | 0x69 if p.len() == 7 && p[1] == NOISE_TYPE => {
            let mode = match (p[3], p[4]) {
                (0, _) => NoiseMode::Off,
                (_, 0) => NoiseMode::NoiseCancelling,
                _ => NoiseMode::Ambient,
            };
            Some(Update::NoiseControl(NoiseControl {
                mode,
                focus_on_voice: p[5] == 1,
                ambient_level: p[6],
            }))
        }
        0xf7 | 0xf9 if p.len() >= 3 && p[1] == SPEAK_TO_CHAT => Some(Update::SpeakToChat(p[2] == 0x00)),
        0x57 | 0x59 if p.len() == 10 && p[1] == EQ_FIVE_BANDS && p[3] == 0x06 => {
            let value = |i: usize| p[i] as i8 - EQ_OFFSET;
            Some(Update::Equalizer(Equalizer {
                preset: EqPreset::from_code(p[2]),
                clear_bass: value(4),
                bands: [value(5), value(6), value(7), value(8), value(9)],
            }))
        }
        0xe7 | 0xe9 if p.len() >= 3 && p[1] == 0x01 => Some(Update::Dsee(p[2] == 0x01)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Replies captured from a WH-1000XM5 (firmware 2.4.1).
    #[test]
    fn parses_real_replies() {
        assert_eq!(parse(&[0x01, 0x00, 0x03, 0x00, 0x20, 0x16, 0x00, 0x00]), Some(Update::Ready { v2: true }));
        assert_eq!(
            parse(&[0x05, 0x02, 0x05, b'2', b'.', b'4', b'.', b'1']),
            Some(Update::Firmware("2.4.1".into()))
        );
        assert_eq!(
            parse(&[0x23, 0x00, 0x21, 0x00]),
            Some(Update::Battery(Battery { level: 33, charging: false }))
        );
        assert_eq!(parse(&[0x13, 0x02, 0x02]), Some(Update::Codec(Codec::Aac)));
        assert_eq!(
            parse(&[0x67, 0x17, 0x01, 0x01, 0x01, 0x00, 0x14]),
            Some(Update::NoiseControl(NoiseControl {
                mode: NoiseMode::Ambient,
                focus_on_voice: false,
                ambient_level: 20,
            }))
        );
    }

    #[test]
    fn encodes_noise_control_changes() {
        let nc = NoiseControl { mode: NoiseMode::NoiseCancelling, focus_on_voice: false, ambient_level: 12 };
        assert_eq!(Request::SetNoiseControl(nc).payload(), [0x68, 0x17, 0x01, 1, 0, 0, 12]);

        let ambient = NoiseControl { mode: NoiseMode::Ambient, focus_on_voice: true, ambient_level: 20 };
        assert_eq!(Request::SetNoiseControl(ambient).payload(), [0x68, 0x17, 0x01, 1, 1, 1, 20]);

        let off = NoiseControl { mode: NoiseMode::Off, focus_on_voice: false, ambient_level: 0 };
        assert_eq!(Request::SetNoiseControl(off).payload(), [0x68, 0x17, 0x01, 0, 0, 0, 1]);
    }

    #[test]
    fn noise_control_round_trips_through_its_notification() {
        let nc = NoiseControl { mode: NoiseMode::Ambient, focus_on_voice: true, ambient_level: 7 };
        let mut notify = Request::SetNoiseControl(nc).payload();
        notify[0] = 0x69;
        assert_eq!(parse(&notify), Some(Update::NoiseControl(nc)));
    }

    #[test]
    fn speak_to_chat_is_inverted_on_the_wire() {
        assert_eq!(Request::SetSpeakToChat(true).payload(), [0xf8, 0x0c, 0x00, 0x01]);
        assert_eq!(Request::SetSpeakToChat(false).payload(), [0xf8, 0x0c, 0x01, 0x01]);
        assert_eq!(parse(&[0xf7, 0x0c, 0x00, 0x01]), Some(Update::SpeakToChat(true)));
        assert_eq!(parse(&[0xf9, 0x0c, 0x01, 0x01]), Some(Update::SpeakToChat(false)));
        // Other system settings share these opcodes.
        assert_eq!(parse(&[0xf7, 0x01, 0x00, 0x01]), None);
    }

    #[test]
    fn encodes_and_parses_the_equalizer() {
        assert_eq!(Request::SetEqualizerPreset(EqPreset::BassBoost).payload(), [0x58, 0x00, 0x16, 0x00]);
        assert_eq!(
            Request::SetEqualizerBands { clear_bass: 3, bands: [-10, -1, 0, 1, 12] }.payload(),
            [0x58, 0x00, 0xa0, 0x06, 13, 0, 9, 10, 11, 20]
        );
        assert_eq!(
            parse(&[0x59, 0x00, 0xa0, 0x06, 13, 0, 9, 10, 11, 20]),
            Some(Update::Equalizer(Equalizer {
                preset: EqPreset::Manual,
                clear_bass: 3,
                bands: [-10, -1, 0, 1, 10],
            }))
        );
        assert_eq!(parse(&[0x57, 0x00, 0x99, 0x06, 10, 10, 10, 10, 10, 10]).map(|u| match u {
            Update::Equalizer(eq) => eq.preset,
            _ => unreachable!(),
        }), Some(EqPreset::Unknown(0x99)));
    }

    #[test]
    fn encodes_and_parses_dsee() {
        assert_eq!(Request::SetDsee(true).payload(), [0xe8, 0x01, 0x01]);
        assert_eq!(parse(&[0xe7, 0x01, 0x00]), Some(Update::Dsee(false)));
    }

    #[test]
    fn ignores_unknown_messages() {
        assert_eq!(parse(&[0xaa, 0x00]), None);
        assert_eq!(parse(&[]), None);
    }
}
