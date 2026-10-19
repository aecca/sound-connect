//! Typed requests and the state updates parsed from replies and notifications.
//!
//! Byte layouts follow the v2 protocol (WH-1000XM5 and newer) as documented
//! by Gadgetbridge and Sony Device Center (`libs/sony-protocol`), and are
//! checked against a WH-1000XM5. Layouts only other models use are marked as
//! such and haven't been tried on hardware by this project.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseMode {
    NoiseCancelling,
    Ambient,
    Off,
    /// Noise cancelling tuned for wind, on headsets with
    /// [`NoiseLayout::WithWind`].
    WindReduction,
}

/// How a headset lays out its noise-control message. Read from its replies
/// and kept in [`NoiseControl`], so changes go back in the same layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoiseLayout {
    /// `.. on, ambient, focus, level` (WH-1000XM5).
    #[default]
    Standard,
    /// `.. on, ambient, wind, focus, level` (WF-1000XM4 and WF-1000XM5, per
    /// Gadgetbridge).
    WithWind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseControl {
    pub mode: NoiseMode,
    pub focus_on_voice: bool,
    /// 1..=20
    pub ambient_level: u8,
    pub layout: NoiseLayout,
}

impl Default for NoiseControl {
    fn default() -> Self {
        Self {
            mode: NoiseMode::NoiseCancelling,
            focus_on_voice: false,
            ambient_level: 20,
            layout: NoiseLayout::Standard,
        }
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

/// Which equalizer a headset has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqLayout {
    /// Five bands plus Clear Bass (WH-1000XM5 and most models).
    FiveBands,
    /// Ten bands, no Clear Bass (WH-1000XM6, per Sony Device Center).
    TenBands,
}

impl EqLayout {
    fn code(self) -> u8 {
        match self {
            Self::FiveBands => 0x00,
            Self::TenBands => 0x04,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Init,
    FirmwareVersion,
    /// Battery of a single-battery headset.
    Battery,
    /// Left and right earbud batteries.
    EarbudBattery,
    /// Earbuds' charging case battery.
    CaseBattery,
    Codec,
    NoiseControl,
    SetNoiseControl(NoiseControl),
    SpeakToChat,
    SetSpeakToChat(bool),
    Equalizer(EqLayout),
    SetEqualizerPreset(EqLayout, EqPreset),
    /// Switches to the Manual preset with these values.
    SetEqualizerBands(EqBands),
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
/// Five-band equalizer values are sent offset by 10, so -10..=10 becomes 0..=20.
const EQ_OFFSET: i8 = 10;
/// Battery "inquired types".
const BATTERY_SINGLE: u8 = 0x00;
const BATTERY_EARBUDS: u8 = 0x09;
/// Older earbuds (e.g. WF-C700N) report left/right under this type instead.
const BATTERY_EARBUDS_ALT: u8 = 0x01;
const BATTERY_CASE: u8 = 0x0a;

impl Request {
    pub fn payload(&self) -> Vec<u8> {
        match self {
            Self::Init => vec![0x00, 0x00],
            Self::FirmwareVersion => vec![0x04, 0x02],
            Self::Battery => vec![0x22, BATTERY_SINGLE],
            Self::EarbudBattery => vec![0x22, BATTERY_EARBUDS],
            Self::CaseBattery => vec![0x22, BATTERY_CASE],
            Self::Codec => vec![0x12, 0x02],
            Self::NoiseControl => vec![0x66, NOISE_TYPE],
            Self::SetNoiseControl(nc) => noise_payload(0x68, nc),
            Self::SpeakToChat => vec![0xf6, SPEAK_TO_CHAT],
            Self::SetSpeakToChat(on) => vec![0xf8, SPEAK_TO_CHAT, (!on) as u8, 0x01],
            Self::Equalizer(layout) => vec![0x56, layout.code()],
            Self::SetEqualizerPreset(layout, preset) => vec![0x58, layout.code(), preset.code(), 0x00],
            Self::SetEqualizerBands(EqBands::Five { clear_bass, bands }) => {
                let mut payload = vec![0x58, EqLayout::FiveBands.code(), EqPreset::Manual.code(), 6];
                let values = std::iter::once(*clear_bass).chain(bands.iter().copied());
                payload.extend(values.map(|v| (v.clamp(-10, 10) + EQ_OFFSET) as u8));
                payload
            }
            Self::SetEqualizerBands(EqBands::Ten(bands)) => {
                let mut payload = vec![0x58, EqLayout::TenBands.code(), EqPreset::Manual.code(), 10];
                payload.extend(bands.iter().map(|v| (*v).min(12)));
                payload
            }
            Self::Dsee => vec![0xe6, 0x01],
            Self::SetDsee(on) => vec![0xe8, 0x01, *on as u8],
        }
    }
}

fn noise_payload(opcode: u8, nc: &NoiseControl) -> Vec<u8> {
    let on = (nc.mode != NoiseMode::Off) as u8;
    let ambient = (nc.mode == NoiseMode::Ambient) as u8;
    let mut payload = vec![opcode, NOISE_TYPE, 0x01, on, ambient];
    if nc.layout == NoiseLayout::WithWind {
        payload.push(if nc.mode == NoiseMode::WindReduction { 0x03 } else { 0x02 });
    }
    payload.extend([nc.focus_on_voice as u8, nc.ambient_level.clamp(1, 20)]);
    payload
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// The headset answered the handshake. `v2` is false for older headsets,
    /// which this app does not support yet.
    Ready { v2: bool },
    Firmware(String),
    Battery(Battery),
    /// `None` for an earbud that isn't reporting, e.g. while in its case.
    EarbudBattery { left: Option<Battery>, right: Option<Battery> },
    CaseBattery(Battery),
    Codec(Codec),
    NoiseControl(NoiseControl),
    SpeakToChat(bool),
    Equalizer(Equalizer),
    Dsee(bool),
}

/// Parses a reply or notification payload. Returns `None` for messages this
/// app does not use yet.
pub fn parse(p: &[u8]) -> Option<Update> {
    let battery = |i: usize| Battery { level: p[i], charging: p[i + 1] == 1 };
    match *p.first()? {
        0x01 => Some(Update::Ready { v2: p.len() == 8 }),
        0x05 if p.len() > 3 && p[1] == 0x02 => {
            Some(Update::Firmware(String::from_utf8_lossy(&p[3..]).into_owned()))
        }
        0x23 | 0x25 if p.len() >= 4 => match p[1] {
            BATTERY_SINGLE => Some(Update::Battery(battery(2))),
            BATTERY_CASE => Some(Update::CaseBattery(battery(2))),
            BATTERY_EARBUDS | BATTERY_EARBUDS_ALT if p.len() >= 6 => {
                let earbud = |i: usize| (p[i] != 0).then(|| battery(i));
                Some(Update::EarbudBattery { left: earbud(2), right: earbud(4) })
            }
            _ => None,
        },
        0x13 | 0x15 if p.len() >= 3 && p[1] == 0x02 => Some(Update::Codec(Codec::from_byte(p[2]))),
        0x67 | 0x69 if p.len() == 7 && p[1] == NOISE_TYPE => Some(Update::NoiseControl(NoiseControl {
            mode: noise_mode(p[3], p[4], None),
            focus_on_voice: p[5] == 1,
            ambient_level: p[6],
            layout: NoiseLayout::Standard,
        })),
        0x67 | 0x69 if p.len() == 8 && p[1] == NOISE_TYPE => Some(Update::NoiseControl(NoiseControl {
            mode: noise_mode(p[3], p[4], Some(p[5])),
            focus_on_voice: p[6] == 1,
            ambient_level: p[7],
            layout: NoiseLayout::WithWind,
        })),
        0xf7 | 0xf9 if p.len() >= 3 && p[1] == SPEAK_TO_CHAT => Some(Update::SpeakToChat(p[2] == 0x00)),
        0x57 | 0x59 if p.len() == 10 && p[1] == EqLayout::FiveBands.code() && p[3] == 6 => {
            let value = |i: usize| p[i] as i8 - EQ_OFFSET;
            Some(Update::Equalizer(Equalizer {
                preset: EqPreset::from_code(p[2]),
                bands: EqBands::Five {
                    clear_bass: value(4),
                    bands: [value(5), value(6), value(7), value(8), value(9)],
                },
            }))
        }
        0x57 | 0x59 if p.len() == 14 && p[1] == EqLayout::TenBands.code() && p[3] == 10 => {
            Some(Update::Equalizer(Equalizer {
                preset: EqPreset::from_code(p[2]),
                bands: EqBands::Ten(p[4..14].try_into().unwrap()),
            }))
        }
        0xe7 | 0xe9 if p.len() >= 3 && p[1] == 0x01 => Some(Update::Dsee(p[2] == 0x01)),
        _ => None,
    }
}

fn noise_mode(on: u8, ambient: u8, wind: Option<u8>) -> NoiseMode {
    match (on, ambient, wind) {
        (0, _, _) => NoiseMode::Off,
        (_, _, Some(0x03 | 0x05)) => NoiseMode::WindReduction,
        (_, 0, _) => NoiseMode::NoiseCancelling,
        _ => NoiseMode::Ambient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(mode: NoiseMode, focus_on_voice: bool, ambient_level: u8) -> NoiseControl {
        NoiseControl { mode, focus_on_voice, ambient_level, layout: NoiseLayout::Standard }
    }

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
            Some(Update::NoiseControl(noise(NoiseMode::Ambient, false, 20)))
        );
        assert_eq!(
            parse(&[0x57, 0x00, 0x00, 0x06, 10, 10, 10, 10, 10, 10]),
            Some(Update::Equalizer(Equalizer {
                preset: EqPreset::Off,
                bands: EqBands::Five { clear_bass: 0, bands: [0; 5] },
            }))
        );
        assert_eq!(parse(&[0xe7, 0x01, 0x00]), Some(Update::Dsee(false)));
    }

    #[test]
    fn encodes_noise_control_changes() {
        let nc = noise(NoiseMode::NoiseCancelling, false, 12);
        assert_eq!(Request::SetNoiseControl(nc).payload(), [0x68, 0x17, 0x01, 1, 0, 0, 12]);
        let ambient = noise(NoiseMode::Ambient, true, 20);
        assert_eq!(Request::SetNoiseControl(ambient).payload(), [0x68, 0x17, 0x01, 1, 1, 1, 20]);
        let off = noise(NoiseMode::Off, false, 0);
        assert_eq!(Request::SetNoiseControl(off).payload(), [0x68, 0x17, 0x01, 0, 0, 0, 1]);
    }

    #[test]
    fn noise_control_round_trips_through_its_notification() {
        let nc = noise(NoiseMode::Ambient, true, 7);
        let mut notify = Request::SetNoiseControl(nc).payload();
        notify[0] = 0x69;
        assert_eq!(parse(&notify), Some(Update::NoiseControl(nc)));
    }

    // WF-1000XM4/XM5 layout with the wind byte, per Gadgetbridge.
    #[test]
    fn handles_the_wind_layout() {
        let wind = parse(&[0x67, 0x17, 0x01, 0x01, 0x00, 0x03, 0x00, 0x14]);
        let Some(Update::NoiseControl(nc)) = wind else { panic!("not parsed: {wind:?}") };
        assert_eq!((nc.mode, nc.layout), (NoiseMode::WindReduction, NoiseLayout::WithWind));

        let to_ambient = NoiseControl { mode: NoiseMode::Ambient, focus_on_voice: true, ..nc };
        assert_eq!(Request::SetNoiseControl(to_ambient).payload(), [0x68, 0x17, 0x01, 1, 1, 0x02, 1, 20]);
        assert_eq!(Request::SetNoiseControl(nc).payload(), [0x68, 0x17, 0x01, 1, 0, 0x03, 0, 20]);
    }

    // Earbud layouts, per Gadgetbridge and Sony Device Center.
    #[test]
    fn parses_earbud_batteries() {
        let full = |level| Some(Battery { level, charging: false });
        assert_eq!(
            parse(&[0x23, 0x09, 80, 0, 75, 1]),
            Some(Update::EarbudBattery { left: full(80), right: Some(Battery { level: 75, charging: true }) })
        );
        assert_eq!(parse(&[0x25, 0x09, 0, 0, 60, 0]), Some(Update::EarbudBattery { left: None, right: full(60) }));
        assert_eq!(parse(&[0x23, 0x0a, 40, 1]), Some(Update::CaseBattery(Battery { level: 40, charging: true })));
        assert_eq!(Request::EarbudBattery.payload(), [0x22, 0x09]);
        assert_eq!(Request::CaseBattery.payload(), [0x22, 0x0a]);
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
    fn encodes_and_parses_the_five_band_equalizer() {
        assert_eq!(
            Request::SetEqualizerPreset(EqLayout::FiveBands, EqPreset::BassBoost).payload(),
            [0x58, 0x00, 0x16, 0x00]
        );
        let bands = EqBands::Five { clear_bass: 3, bands: [-10, -1, 0, 1, 12] };
        assert_eq!(Request::SetEqualizerBands(bands).payload(), [0x58, 0x00, 0xa0, 0x06, 13, 0, 9, 10, 11, 20]);
        assert_eq!(
            parse(&[0x59, 0x00, 0xa0, 0x06, 13, 0, 9, 10, 11, 20]),
            Some(Update::Equalizer(Equalizer {
                preset: EqPreset::Manual,
                bands: EqBands::Five { clear_bass: 3, bands: [-10, -1, 0, 1, 10] },
            }))
        );
        let unknown = parse(&[0x57, 0x00, 0x99, 0x06, 10, 10, 10, 10, 10, 10]);
        assert!(matches!(unknown, Some(Update::Equalizer(Equalizer { preset: EqPreset::Unknown(0x99), .. }))));
    }

    // WH-1000XM6 layout, per Sony Device Center (issue #10).
    #[test]
    fn encodes_and_parses_the_ten_band_equalizer() {
        assert_eq!(Request::Equalizer(EqLayout::TenBands).payload(), [0x56, 0x04]);
        let bands = EqBands::Ten([0, 1, 2, 3, 4, 5, 6, 7, 8, 20]);
        assert_eq!(
            Request::SetEqualizerBands(bands).payload(),
            [0x58, 0x04, 0xa0, 0x0a, 0, 1, 2, 3, 4, 5, 6, 7, 8, 12]
        );
        assert_eq!(
            parse(&[0x57, 0x04, 0x10, 0x0a, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6]),
            Some(Update::Equalizer(Equalizer { preset: EqPreset::Bright, bands: EqBands::Ten([6; 10]) }))
        );
    }

    #[test]
    fn encodes_and_parses_dsee() {
        assert_eq!(Request::SetDsee(true).payload(), [0xe8, 0x01, 0x01]);
        assert_eq!(parse(&[0xe9, 0x01, 0x01]), Some(Update::Dsee(true)));
    }

    #[test]
    fn ignores_unknown_messages() {
        assert_eq!(parse(&[0xaa, 0x00]), None);
        assert_eq!(parse(&[]), None);
    }
}
