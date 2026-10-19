//! Sony's requests and the replies parsed from what the headset sends.
//!
//! Byte layouts follow the v2 protocol (WH-1000XM5 and newer) as documented
//! by Gadgetbridge and Sony Device Center (`libs/sony-protocol`), and are
//! checked against a WH-1000XM5. Layouts only other models use are marked as
//! such and haven't been tried on hardware by this project.

use crate::device::{Battery, ListeningMode};
use crate::equalizer::{EqBands, EqLayout, EqPreset, Equalizer};

/// How a headset lays out its noise-control message. Read from its replies
/// and kept in [`NoiseControl`], so changes go back in the same layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum NoiseLayout {
    /// `.. on, ambient, focus, level` (WH-1000XM5).
    #[default]
    Standard,
    /// `.. on, ambient, wind, focus, level` (WF-1000XM4 and WF-1000XM5, per
    /// Gadgetbridge).
    WithWind,
}

/// Sony sends and takes the whole noise-control state in one message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NoiseControl {
    pub mode: ListeningMode,
    pub focus_on_voice: bool,
    /// 1..=20
    pub ambient_level: u8,
    pub layout: NoiseLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Request {
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

/// What the headset sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Reply {
    /// The handshake's answer. `v2` is false for older headsets, which this
    /// app doesn't support yet.
    Ready {
        v2: bool,
    },
    Firmware(String),
    Battery(Battery),
    /// `None` for an earbud that isn't reporting, e.g. while in its case.
    EarbudBattery {
        left: Option<Battery>,
        right: Option<Battery>,
    },
    CaseBattery(Battery),
    Codec(String),
    NoiseControl(NoiseControl),
    SpeakToChat(bool),
    Equalizer(Equalizer),
    Dsee(bool),
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
            Self::SetSpeakToChat(on) => vec![0xf8, SPEAK_TO_CHAT, u8::from(!on), 0x01],
            Self::Equalizer(layout) => vec![0x56, layout_code(*layout)],
            Self::SetEqualizerPreset(layout, preset) => vec![0x58, layout_code(*layout), preset_code(*preset), 0x00],
            Self::SetEqualizerBands(EqBands::Five { clear_bass, bands }) => {
                let mut payload = vec![0x58, layout_code(EqLayout::FiveBands), preset_code(EqPreset::Manual), 6];
                let values = std::iter::once(*clear_bass).chain(bands.iter().copied());
                payload.extend(values.map(eq_byte));
                payload
            }
            Self::SetEqualizerBands(EqBands::Ten(bands)) => {
                let mut payload = vec![0x58, layout_code(EqLayout::TenBands), preset_code(EqPreset::Manual), 10];
                payload.extend(bands.iter().map(|value| (*value).min(12)));
                payload
            }
            Self::Dsee => vec![0xe6, 0x01],
            Self::SetDsee(on) => vec![0xe8, 0x01, u8::from(*on)],
        }
    }
}

fn noise_payload(opcode: u8, nc: &NoiseControl) -> Vec<u8> {
    let on = u8::from(nc.mode != ListeningMode::Off);
    let ambient = u8::from(nc.mode == ListeningMode::Ambient);
    let mut payload = vec![opcode, NOISE_TYPE, 0x01, on, ambient];
    if nc.layout == NoiseLayout::WithWind {
        payload.push(if nc.mode == ListeningMode::WindReduction { 0x03 } else { 0x02 });
    }
    payload.extend([u8::from(nc.focus_on_voice), nc.ambient_level.clamp(1, 20)]);
    payload
}

/// A five-band step, -10..=10, as the 0..=20 the headset uses.
fn eq_byte(value: i8) -> u8 {
    // Never negative once clamped.
    (value.clamp(-10, 10) + EQ_OFFSET) as u8
}

/// The headset's 0..=20 as a five-band step, -10..=10. Out-of-range bytes
/// are capped rather than trusted.
fn eq_value(byte: u8) -> i8 {
    byte.min(20) as i8 - EQ_OFFSET
}

fn layout_code(layout: EqLayout) -> u8 {
    match layout {
        EqLayout::FiveBands => 0x00,
        EqLayout::TenBands => 0x04,
    }
}

fn preset_code(preset: EqPreset) -> u8 {
    match preset {
        EqPreset::Off => 0x00,
        EqPreset::Bright => 0x10,
        EqPreset::Excited => 0x11,
        EqPreset::Mellow => 0x12,
        EqPreset::Relaxed => 0x13,
        EqPreset::Vocal => 0x14,
        EqPreset::TrebleBoost => 0x15,
        EqPreset::BassBoost => 0x16,
        EqPreset::Speech => 0x17,
        EqPreset::Manual => 0xa0,
        EqPreset::Custom1 => 0xa1,
        EqPreset::Custom2 => 0xa2,
        EqPreset::Unknown(code) => code,
    }
}

fn preset_from_code(code: u8) -> EqPreset {
    EqPreset::ALL.into_iter().find(|preset| preset_code(*preset) == code).unwrap_or(EqPreset::Unknown(code))
}

fn codec_name(code: u8) -> String {
    match code {
        0x01 => "SBC".into(),
        0x02 => "AAC".into(),
        0x10 => "LDAC".into(),
        0x20 => "aptX".into(),
        0x21 => "aptX HD".into(),
        other => format!("0x{other:02x}"),
    }
}

/// Parses a reply or notification payload. Returns `None` for messages this
/// app doesn't use.
pub(super) fn parse(p: &[u8]) -> Option<Reply> {
    let battery = |i: usize| Battery { level: p[i], charging: p[i + 1] == 1 };
    match *p.first()? {
        0x01 => Some(Reply::Ready { v2: p.len() == 8 }),
        0x05 if p.len() > 3 && p[1] == 0x02 => Some(Reply::Firmware(String::from_utf8_lossy(&p[3..]).into_owned())),
        0x23 | 0x25 if p.len() >= 4 => match p[1] {
            BATTERY_SINGLE => Some(Reply::Battery(battery(2))),
            BATTERY_CASE => Some(Reply::CaseBattery(battery(2))),
            BATTERY_EARBUDS | BATTERY_EARBUDS_ALT if p.len() >= 6 => {
                let earbud = |i: usize| (p[i] != 0).then(|| battery(i));
                Some(Reply::EarbudBattery { left: earbud(2), right: earbud(4) })
            }
            _ => None,
        },
        0x13 | 0x15 if p.len() >= 3 && p[1] == 0x02 => Some(Reply::Codec(codec_name(p[2]))),
        0x67 | 0x69 if p.len() == 7 && p[1] == NOISE_TYPE => Some(Reply::NoiseControl(NoiseControl {
            mode: noise_mode(p[3], p[4], None),
            focus_on_voice: p[5] == 1,
            ambient_level: p[6],
            layout: NoiseLayout::Standard,
        })),
        0x67 | 0x69 if p.len() == 8 && p[1] == NOISE_TYPE => Some(Reply::NoiseControl(NoiseControl {
            mode: noise_mode(p[3], p[4], Some(p[5])),
            focus_on_voice: p[6] == 1,
            ambient_level: p[7],
            layout: NoiseLayout::WithWind,
        })),
        0xf7 | 0xf9 if p.len() >= 3 && p[1] == SPEAK_TO_CHAT => Some(Reply::SpeakToChat(p[2] == 0x00)),
        0x57 | 0x59 if p.len() == 10 && p[1] == layout_code(EqLayout::FiveBands) && p[3] == 6 => {
            Some(Reply::Equalizer(Equalizer {
                preset: preset_from_code(p[2]),
                bands: EqBands::Five {
                    clear_bass: eq_value(p[4]),
                    bands: [eq_value(p[5]), eq_value(p[6]), eq_value(p[7]), eq_value(p[8]), eq_value(p[9])],
                },
            }))
        }
        0x57 | 0x59 if p.len() == 14 && p[1] == layout_code(EqLayout::TenBands) && p[3] == 10 => {
            let mut bands = [0; 10];
            bands.copy_from_slice(&p[4..14]);
            Some(Reply::Equalizer(Equalizer { preset: preset_from_code(p[2]), bands: EqBands::Ten(bands) }))
        }
        0xe7 | 0xe9 if p.len() >= 3 && p[1] == 0x01 => Some(Reply::Dsee(p[2] == 0x01)),
        _ => None,
    }
}

fn noise_mode(on: u8, ambient: u8, wind: Option<u8>) -> ListeningMode {
    match (on, ambient, wind) {
        (0, _, _) => ListeningMode::Off,
        (_, _, Some(0x03 | 0x05)) => ListeningMode::WindReduction,
        (_, 0, _) => ListeningMode::NoiseCancelling,
        _ => ListeningMode::Ambient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(mode: ListeningMode, focus_on_voice: bool, ambient_level: u8) -> NoiseControl {
        NoiseControl { mode, focus_on_voice, ambient_level, layout: NoiseLayout::Standard }
    }

    // Replies captured from a WH-1000XM5 (firmware 2.4.1).
    #[test]
    fn parses_real_replies() {
        assert_eq!(parse(&[0x01, 0x00, 0x03, 0x00, 0x20, 0x16, 0x00, 0x00]), Some(Reply::Ready { v2: true }));
        assert_eq!(parse(&[0x05, 0x02, 0x05, b'2', b'.', b'4', b'.', b'1']), Some(Reply::Firmware("2.4.1".into())));
        assert_eq!(parse(&[0x23, 0x00, 0x21, 0x00]), Some(Reply::Battery(Battery { level: 33, charging: false })));
        assert_eq!(parse(&[0x13, 0x02, 0x02]), Some(Reply::Codec("AAC".into())));
        assert_eq!(
            parse(&[0x67, 0x17, 0x01, 0x01, 0x01, 0x00, 0x14]),
            Some(Reply::NoiseControl(noise(ListeningMode::Ambient, false, 20)))
        );
        assert_eq!(
            parse(&[0x57, 0x00, 0x00, 0x06, 10, 10, 10, 10, 10, 10]),
            Some(Reply::Equalizer(Equalizer {
                preset: EqPreset::Off,
                bands: EqBands::Five { clear_bass: 0, bands: [0; 5] },
            }))
        );
        assert_eq!(parse(&[0xe7, 0x01, 0x00]), Some(Reply::Dsee(false)));
    }

    #[test]
    fn encodes_noise_control_changes() {
        let nc = noise(ListeningMode::NoiseCancelling, false, 12);
        assert_eq!(Request::SetNoiseControl(nc).payload(), [0x68, 0x17, 0x01, 1, 0, 0, 12]);
        let ambient = noise(ListeningMode::Ambient, true, 20);
        assert_eq!(Request::SetNoiseControl(ambient).payload(), [0x68, 0x17, 0x01, 1, 1, 1, 20]);
        let off = noise(ListeningMode::Off, false, 0);
        assert_eq!(Request::SetNoiseControl(off).payload(), [0x68, 0x17, 0x01, 0, 0, 0, 1]);
    }

    #[test]
    fn noise_control_round_trips_through_its_notification() {
        let nc = noise(ListeningMode::Ambient, true, 7);
        let mut notify = Request::SetNoiseControl(nc).payload();
        notify[0] = 0x69;
        assert_eq!(parse(&notify), Some(Reply::NoiseControl(nc)));
    }

    // WF-1000XM4/XM5 layout with the wind byte, per Gadgetbridge.
    #[test]
    fn handles_the_wind_layout() {
        let wind = parse(&[0x67, 0x17, 0x01, 0x01, 0x00, 0x03, 0x00, 0x14]);
        let Some(Reply::NoiseControl(nc)) = wind else { panic!("not parsed: {wind:?}") };
        assert_eq!((nc.mode, nc.layout), (ListeningMode::WindReduction, NoiseLayout::WithWind));

        let to_ambient = NoiseControl { mode: ListeningMode::Ambient, focus_on_voice: true, ..nc };
        assert_eq!(Request::SetNoiseControl(to_ambient).payload(), [0x68, 0x17, 0x01, 1, 1, 0x02, 1, 20]);
        assert_eq!(Request::SetNoiseControl(nc).payload(), [0x68, 0x17, 0x01, 1, 0, 0x03, 0, 20]);
    }

    // Earbud layouts, per Gadgetbridge and Sony Device Center.
    #[test]
    fn parses_earbud_batteries() {
        let level = |level| Some(Battery { level, charging: false });
        assert_eq!(
            parse(&[0x23, 0x09, 80, 0, 75, 1]),
            Some(Reply::EarbudBattery { left: level(80), right: Some(Battery { level: 75, charging: true }) })
        );
        assert_eq!(parse(&[0x25, 0x09, 0, 0, 60, 0]), Some(Reply::EarbudBattery { left: None, right: level(60) }));
        assert_eq!(parse(&[0x23, 0x0a, 40, 1]), Some(Reply::CaseBattery(Battery { level: 40, charging: true })));
        assert_eq!(Request::EarbudBattery.payload(), [0x22, 0x09]);
        assert_eq!(Request::CaseBattery.payload(), [0x22, 0x0a]);
    }

    #[test]
    fn speak_to_chat_is_inverted_on_the_wire() {
        assert_eq!(Request::SetSpeakToChat(true).payload(), [0xf8, 0x0c, 0x00, 0x01]);
        assert_eq!(Request::SetSpeakToChat(false).payload(), [0xf8, 0x0c, 0x01, 0x01]);
        assert_eq!(parse(&[0xf7, 0x0c, 0x00, 0x01]), Some(Reply::SpeakToChat(true)));
        assert_eq!(parse(&[0xf9, 0x0c, 0x01, 0x01]), Some(Reply::SpeakToChat(false)));
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
            Some(Reply::Equalizer(Equalizer {
                preset: EqPreset::Manual,
                bands: EqBands::Five { clear_bass: 3, bands: [-10, -1, 0, 1, 10] },
            }))
        );
        let unknown = parse(&[0x57, 0x00, 0x99, 0x06, 10, 10, 10, 10, 10, 10]);
        assert!(matches!(unknown, Some(Reply::Equalizer(Equalizer { preset: EqPreset::Unknown(0x99), .. }))));
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
            Some(Reply::Equalizer(Equalizer { preset: EqPreset::Bright, bands: EqBands::Ten([6; 10]) }))
        );
    }

    #[test]
    fn caps_out_of_range_equalizer_values() {
        let reply = parse(&[0x57, 0x00, 0xa0, 0x06, 0xff, 21, 0, 10, 20, 0x80]);
        let Some(Reply::Equalizer(Equalizer { bands: EqBands::Five { clear_bass, bands }, .. })) = reply else {
            panic!("not parsed: {reply:?}");
        };
        assert_eq!((clear_bass, bands), (10, [10, -10, 0, 10, 10]));
    }

    #[test]
    fn encodes_and_parses_dsee() {
        assert_eq!(Request::SetDsee(true).payload(), [0xe8, 0x01, 0x01]);
        assert_eq!(parse(&[0xe9, 0x01, 0x01]), Some(Reply::Dsee(true)));
    }

    #[test]
    fn ignores_unknown_messages() {
        assert_eq!(parse(&[0xaa, 0x00]), None);
        assert_eq!(parse(&[]), None);
    }
}
