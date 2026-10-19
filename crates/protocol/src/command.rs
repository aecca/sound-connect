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
}

/// Noise-control "inquired type" used by v2 headsets. 0x15 gets no reply on
/// a WH-1000XM5, 0x17 does.
const NOISE_TYPE: u8 = 0x17;
/// Speak-to-Chat lives among the "system" settings (0xf6..0xf9), as type 0x0c.
/// Its value is inverted: 0x00 means on.
const SPEAK_TO_CHAT: u8 = 0x0c;

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
    fn ignores_unknown_messages() {
        assert_eq!(parse(&[0xaa, 0x00]), None);
        assert_eq!(parse(&[]), None);
    }
}
