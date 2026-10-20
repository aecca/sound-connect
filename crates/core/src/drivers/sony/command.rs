//! Sony's requests and the replies parsed from what the headset sends.
//!
//! Byte layouts follow the v2 protocol (WH-1000XM5 and newer) as documented
//! by Gadgetbridge, Sony Device Center (`libs/sony-protocol`) and
//! SonyHeadphonesClient (`libmdr`). The basics (noise control, battery,
//! codec, equalizer, DSEE, Speak-to-Chat) are checked against a WH-1000XM5;
//! the rest come from those projects and messages they captured from other
//! models, as the tests say, and haven't been tried on hardware here yet.

use super::frame::kind;
use crate::device::{AutoOff, Battery, ConnectedDevice, ListeningMode, NoiseButton, SpeakToChatEnd, VoiceSensitivity};
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

/// How a headset lists the devices it's paired with: with each one's
/// Bluetooth class or without, depending on the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceList {
    Classic,
    WithClass,
}

/// Speak-to-Chat's options travel together, like the noise-control state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChatOptions {
    pub sensitivity: VoiceSensitivity,
    pub end: SpeakToChatEnd,
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
    SpeakToChatOptions,
    SetSpeakToChatOptions(ChatOptions),
    Equalizer(EqLayout),
    SetEqualizerPreset(EqLayout, EqPreset),
    /// Switches to the Manual preset with these values.
    SetEqualizerBands(EqBands),
    /// DSEE Extreme, Sony's upscaling for compressed music.
    Dsee,
    SetDsee(bool),
    PauseWhenTakenOff,
    SetPauseWhenTakenOff(bool),
    AutoOff,
    /// One of [`auto_off_choices`].
    SetAutoOff(AutoOff),
    NoiseButton,
    SetNoiseButton(NoiseButton),
    /// Which functions of the second command table the headset has.
    Table2Functions,
    /// The devices it's paired with, and which it's connected to.
    ConnectedDevices(DeviceList),
    /// Play the audio of the connected device with this address, as the
    /// headset writes it: "AA:BB:CC:DD:EE:FF".
    PlayFrom([u8; 17]),
}

/// What the headset sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Reply {
    /// The handshake's answer. `v2` is false for older headsets, which this
    /// app doesn't support yet; `table2` says whether it has the second
    /// command table.
    Ready {
        v2: bool,
        table2: bool,
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
    SpeakToChatOptions(ChatOptions),
    Equalizer(Equalizer),
    Dsee(bool),
    PauseWhenTakenOff(bool),
    AutoOff(AutoOff),
    NoiseButton(NoiseButton),
    Table2Functions(Vec<u8>),
    ConnectedDevices(Vec<ConnectedDevice>),
    /// The headset answered a [`Request::PlayFrom`], whether or not it could
    /// switch (it can't during a call, for one).
    PlayedFrom,
}

/// Noise-control "inquired type" used by v2 headsets. 0x15 gets no reply on
/// a WH-1000XM5, 0x17 does.
const NOISE_TYPE: u8 = 0x17;
/// Speak-to-Chat lives among the "system" settings (0xf6..0xf9), as type 0x0c.
/// Its value is inverted: 0x00 means on.
const SPEAK_TO_CHAT: u8 = 0x0c;
/// Pausing when taken off is the system setting of type 0x01, inverted too.
const PAUSE_WHEN_TAKEN_OFF: u8 = 0x01;
/// Automatic power off of headsets that know when they're worn. Others use
/// type 0x04, which isn't supported here yet.
const AUTO_OFF_WEARING: u8 = 0x05;
/// Its choices, as Sony's app offers them, and the two bytes each is sent as.
const AUTO_OFF_CODES: [(AutoOff, [u8; 2]); 2] = [(AutoOff::WhenTakenOff, [0x10, 0x00]), (AutoOff::Never, [0x11, 0x00])];
/// Timers the same setting can also hold, set by other apps: minutes, by
/// the first byte.
const AUTO_OFF_TIMERS: [(u8, u16); 5] = [(0x00, 5), (0x01, 30), (0x02, 60), (0x03, 180), (0x04, 15)];
/// What the NC/AMB button switches between, among the noise-control settings.
const NOISE_BUTTON: u8 = 0x30;
/// Five-band equalizer values are sent offset by 10, so -10..=10 becomes 0..=20.
const EQ_OFFSET: i8 = 10;
/// Battery "inquired types".
const BATTERY_SINGLE: u8 = 0x00;
const BATTERY_EARBUDS: u8 = 0x09;
/// Older earbuds (e.g. WF-C700N) report left/right under this type instead.
const BATTERY_EARBUDS_ALT: u8 = 0x01;
const BATTERY_CASE: u8 = 0x0a;

/// Second-table functions for listing the devices a headset is paired with.
const DEVICE_LIST_CLASSIC: u8 = 0x30;
const DEVICE_LIST_WITH_CLASS: [u8; 2] = [0x32, 0x33];
/// Switching which connected device plays, among the second table's
/// peripheral settings.
const PLAYBACK_DEVICE: u8 = 0x01;

impl Request {
    /// The command table it's in, as a frame's kind.
    pub fn kind(&self) -> u8 {
        match self {
            Self::Table2Functions | Self::ConnectedDevices(_) | Self::PlayFrom(_) => kind::COMMAND_2,
            _ => kind::COMMAND,
        }
    }

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
            Self::SpeakToChatOptions => vec![0xfa, SPEAK_TO_CHAT],
            Self::SetSpeakToChatOptions(options) => {
                vec![0xfc, SPEAK_TO_CHAT, sensitivity_code(options.sensitivity), chat_end_code(options.end)]
            }
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
            Self::PauseWhenTakenOff => vec![0xf6, PAUSE_WHEN_TAKEN_OFF],
            Self::SetPauseWhenTakenOff(on) => vec![0xf8, PAUSE_WHEN_TAKEN_OFF, u8::from(!on)],
            Self::AutoOff => vec![0x26, AUTO_OFF_WEARING],
            Self::SetAutoOff(choice) => {
                let (_, code) = AUTO_OFF_CODES
                    .iter()
                    .find(|(offered, _)| offered == choice)
                    .expect("only offered choices are sent");
                vec![0x28, AUTO_OFF_WEARING, code[0], code[1]]
            }
            Self::NoiseButton => vec![0x66, NOISE_BUTTON],
            Self::SetNoiseButton(choice) => vec![0x68, NOISE_BUTTON, noise_button_code(*choice)],
            Self::Table2Functions => vec![0x06, 0x00],
            Self::ConnectedDevices(list) => vec![0x36, device_list_code(*list)],
            Self::PlayFrom(address) => [&[0x3c, PLAYBACK_DEVICE][..], address].concat(),
        }
    }
}

/// How a headset lists its devices, from the second-table functions it has.
pub(super) fn device_list(functions: &[u8]) -> Option<DeviceList> {
    if functions.iter().any(|function| DEVICE_LIST_WITH_CLASS.contains(function)) {
        Some(DeviceList::WithClass)
    } else if functions.contains(&DEVICE_LIST_CLASSIC) {
        Some(DeviceList::Classic)
    } else {
        None
    }
}

fn device_list_code(list: DeviceList) -> u8 {
    match list {
        DeviceList::Classic => 0x00,
        DeviceList::WithClass => 0x02,
    }
}

/// The automatic power off choices of headsets that know when they're worn.
pub(super) fn auto_off_choices() -> Vec<AutoOff> {
    AUTO_OFF_CODES.iter().map(|(choice, _)| *choice).collect()
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

fn sensitivity_code(sensitivity: VoiceSensitivity) -> u8 {
    match sensitivity {
        VoiceSensitivity::Auto => 0x00,
        VoiceSensitivity::High => 0x01,
        VoiceSensitivity::Low => 0x02,
    }
}

fn chat_end_code(end: SpeakToChatEnd) -> u8 {
    match end {
        SpeakToChatEnd::Short => 0x00,
        SpeakToChatEnd::Standard => 0x01,
        SpeakToChatEnd::Long => 0x02,
        SpeakToChatEnd::Never => 0x03,
    }
}

/// 0x00 would make the button do nothing, which isn't offered.
fn noise_button_code(choice: NoiseButton) -> u8 {
    match choice {
        NoiseButton::NoiseCancellingAmbientOff => 0x01,
        NoiseButton::NoiseCancellingAmbient => 0x02,
        NoiseButton::NoiseCancellingOff => 0x03,
        NoiseButton::AmbientOff => 0x04,
    }
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
        // The last byte is 0x00 when the second table is there.
        0x01 => Some(Reply::Ready { v2: p.len() == 8, table2: p.len() == 8 && p[7] == 0x00 }),
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
        0xf7 | 0xf9 if p.len() == 3 && p[1] == PAUSE_WHEN_TAKEN_OFF => Some(Reply::PauseWhenTakenOff(p[2] == 0x00)),
        0xfb | 0xfd if p.len() == 4 && p[1] == SPEAK_TO_CHAT => {
            let sensitivity = VoiceSensitivity::ALL.into_iter().find(|&s| sensitivity_code(s) == p[2])?;
            let end = SpeakToChatEnd::ALL.into_iter().find(|&end| chat_end_code(end) == p[3])?;
            Some(Reply::SpeakToChatOptions(ChatOptions { sensitivity, end }))
        }
        0x67 | 0x69 if p.len() == 3 && p[1] == NOISE_BUTTON => {
            NoiseButton::ALL.into_iter().find(|&choice| noise_button_code(choice) == p[2]).map(Reply::NoiseButton)
        }
        0x27 | 0x29 if p.len() >= 3 && p[1] == AUTO_OFF_WEARING => {
            let choice = AUTO_OFF_CODES.iter().find(|(_, code)| code[0] == p[2]).map(|(choice, _)| *choice);
            let timer = || {
                AUTO_OFF_TIMERS.iter().find(|(code, _)| *code == p[2]).map(|&(_, minutes)| AutoOff::Minutes(minutes))
            };
            choice.or_else(timer).map(Reply::AutoOff)
        }
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

/// Parses a reply or notification from the second command table, per
/// SonyHeadphonesClient, checked against messages captured from a WF-1000XM5.
pub(super) fn parse_table_2(p: &[u8]) -> Option<Reply> {
    match *p.first()? {
        // The functions and a priority for each.
        0x07 if p.len() >= 3 && p[1] == 0x00 => {
            let count = usize::from(p[2]);
            let (pairs, _) = p[3..].as_chunks::<2>();
            Some(Reply::Table2Functions(pairs.iter().take(count).map(|&[function, _priority]| function).collect()))
        }
        0x37 | 0x39 if p.len() >= 3 && [0x00, 0x02].contains(&p[1]) => {
            connected_devices(&p[2..], p[1] == 0x02).map(Reply::ConnectedDevices)
        }
        0x3d if p.len() >= 2 && p[1] == PLAYBACK_DEVICE => Some(Reply::PlayedFrom),
        _ => None,
    }
}

/// `<count>`, then for each paired device its address as 17 characters, the
/// slot it's connected in (0 when it isn't), its class (3 bytes, in the
/// layout that has it), and its name after its length; then the slot whose
/// audio plays. Only the connected devices come back.
fn connected_devices(p: &[u8], with_class: bool) -> Option<Vec<ConnectedDevice>> {
    let (&count, mut rest) = p.split_first()?;
    let mut paired = Vec::new();
    for _ in 0..count {
        let (address, after) = rest.split_at_checked(17)?;
        let (&slot, after) = after.split_first()?;
        let after = if with_class { after.get(3..)? } else { after };
        let (&length, after) = after.split_first()?;
        let (name, after) = after.split_at_checked(length.into())?;
        paired.push((address, slot, name));
        rest = after;
    }
    let &playing = rest.first()?;
    let connected = paired.into_iter().filter(|&(_, slot, _)| slot != 0);
    Some(
        connected
            .map(|(address, slot, name)| ConnectedDevice {
                id: String::from_utf8_lossy(address).into_owned(),
                name: String::from_utf8_lossy(name).into_owned(),
                playing: slot == playing,
            })
            .collect(),
    )
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
        assert_eq!(
            parse(&[0x01, 0x00, 0x03, 0x00, 0x20, 0x16, 0x00, 0x00]),
            Some(Reply::Ready { v2: true, table2: true })
        );
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

    // Layout per Gadgetbridge and SonyHeadphonesClient; the first reply was
    // captured from a WF-1000XM5 (firmware 6.1.0), in SonyHeadphonesClient's
    // test data.
    #[test]
    fn encodes_and_parses_speak_to_chat_options() {
        let options = |sensitivity, end| Some(Reply::SpeakToChatOptions(ChatOptions { sensitivity, end }));
        assert_eq!(parse(&[0xfb, 0x0c, 0x00, 0x00]), options(VoiceSensitivity::Auto, SpeakToChatEnd::Short));
        assert_eq!(parse(&[0xfd, 0x0c, 0x02, 0x03]), options(VoiceSensitivity::Low, SpeakToChatEnd::Never));
        // The same opcodes carry other settings.
        assert_eq!(parse(&[0xfb, 0x03, 0x00, 0x00]), None);
        assert_eq!(Request::SpeakToChatOptions.payload(), [0xfa, 0x0c]);
        let high_long = ChatOptions { sensitivity: VoiceSensitivity::High, end: SpeakToChatEnd::Long };
        assert_eq!(Request::SetSpeakToChatOptions(high_long).payload(), [0xfc, 0x0c, 0x01, 0x02]);
    }

    // Replies captured from a WF-1000XM5 (firmware 6.1.0), from
    // SonyHeadphonesClient's test data.
    #[test]
    fn pause_when_taken_off_is_inverted_on_the_wire() {
        assert_eq!(parse(&[0xf7, 0x01, 0x00]), Some(Reply::PauseWhenTakenOff(true)));
        assert_eq!(parse(&[0xf9, 0x01, 0x01]), Some(Reply::PauseWhenTakenOff(false)));
        assert_eq!(Request::PauseWhenTakenOff.payload(), [0xf6, 0x01]);
        assert_eq!(Request::SetPauseWhenTakenOff(true).payload(), [0xf8, 0x01, 0x00]);
        assert_eq!(Request::SetPauseWhenTakenOff(false).payload(), [0xf8, 0x01, 0x01]);
    }

    #[test]
    fn encodes_and_parses_automatic_power_off() {
        assert_eq!(parse(&[0x27, 0x05, 0x10, 0x00]), Some(Reply::AutoOff(AutoOff::WhenTakenOff)));
        assert_eq!(parse(&[0x29, 0x05, 0x11, 0x00]), Some(Reply::AutoOff(AutoOff::Never)));
        // Timers aren't offered, but another app may have set one.
        assert_eq!(parse(&[0x27, 0x05, 0x01, 0x01]), Some(Reply::AutoOff(AutoOff::Minutes(30))));
        assert_eq!(parse(&[0x27, 0x05, 0x99, 0x00]), None);
        assert_eq!(Request::AutoOff.payload(), [0x26, 0x05]);
        assert_eq!(Request::SetAutoOff(AutoOff::WhenTakenOff).payload(), [0x28, 0x05, 0x10, 0x00]);
        assert_eq!(Request::SetAutoOff(AutoOff::Never).payload(), [0x28, 0x05, 0x11, 0x00]);
        assert_eq!(auto_off_choices(), [AutoOff::WhenTakenOff, AutoOff::Never]);
    }

    // Layout per SonyHeadphonesClient, whose users run it on WH-1000XM5 and XM6;
    // Gadgetbridge has the same choice codes.
    #[test]
    fn encodes_and_parses_the_noise_control_button() {
        assert_eq!(Request::NoiseButton.payload(), [0x66, 0x30]);
        assert_eq!(Request::SetNoiseButton(NoiseButton::NoiseCancellingAmbient).payload(), [0x68, 0x30, 0x02]);
        assert_eq!(parse(&[0x67, 0x30, 0x01]), Some(Reply::NoiseButton(NoiseButton::NoiseCancellingAmbientOff)));
        assert_eq!(parse(&[0x69, 0x30, 0x04]), Some(Reply::NoiseButton(NoiseButton::AmbientOff)));
        // A button that does nothing isn't one of the choices.
        assert_eq!(parse(&[0x67, 0x30, 0x00]), None);
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

    /// A device list in the layout a WF-1000XM5 sends (firmware 6.1.0, per
    /// SonyHeadphonesClient's test data), with made-up devices.
    fn listed(entries: &[(&str, u8, &str)], playing: u8) -> Vec<u8> {
        let mut p = vec![0x37, 0x02, u8::try_from(entries.len()).unwrap()];
        for (address, slot, name) in entries {
            p.extend(address.as_bytes());
            p.push(*slot);
            p.extend([0x5a, 0x02, 0x0c]);
            p.push(u8::try_from(name.len()).unwrap());
            p.extend(name.as_bytes());
        }
        p.push(playing);
        p
    }

    #[test]
    fn parses_the_connected_devices() {
        let list = listed(
            &[
                ("11:22:33:44:55:66", 1, "Laptop"),
                ("AA:BB:CC:DD:EE:FF", 2, "Phone"),
                ("00:11:22:33:44:55", 0, "Old PC"),
            ],
            2,
        );
        let device = |id: &str, name: &str, playing| ConnectedDevice { id: id.into(), name: name.into(), playing };
        assert_eq!(
            parse_table_2(&list),
            Some(Reply::ConnectedDevices(vec![
                device("11:22:33:44:55:66", "Laptop", false),
                device("AA:BB:CC:DD:EE:FF", "Phone", true),
            ]))
        );
        // The layout without each device's class.
        let classic = [&[0x39, 0x00, 0x01][..], b"11:22:33:44:55:66", &[0x01, 0x06], b"Laptop", &[0x01]].concat();
        assert_eq!(
            parse_table_2(&classic),
            Some(Reply::ConnectedDevices(vec![device("11:22:33:44:55:66", "Laptop", true)]))
        );
        // Cut short.
        assert_eq!(parse_table_2(&list[..list.len() - 1]), None);
    }

    #[test]
    fn reads_which_second_table_functions_there_are() {
        // Captured from a WF-1000XM5 (firmware 6.1.0).
        let functions = parse_table_2(&[
            0x07, 0x00, 0x0b, 0x42, 0x3d, 0x53, 0xff, 0x54, 0xff, 0xf1, 0xff, 0x60, 0xff, 0x61, 0xff, 0xf2, 0xff, 0x63,
            0xff, 0xf8, 0x30, 0x32, 0x04, 0x31, 0xff,
        ]);
        let Some(Reply::Table2Functions(functions)) = functions else { panic!("not parsed: {functions:?}") };
        assert_eq!(functions, [0x42, 0x53, 0x54, 0xf1, 0x60, 0x61, 0xf2, 0x63, 0xf8, 0x32, 0x31]);
        assert_eq!(device_list(&functions), Some(DeviceList::WithClass));
        assert_eq!(device_list(&[0x30, 0x31]), Some(DeviceList::Classic));
        assert_eq!(device_list(&[0x42]), None);
    }

    #[test]
    fn switches_which_device_plays() {
        let request = Request::PlayFrom(*b"AA:BB:CC:DD:EE:FF");
        assert_eq!(request.kind(), kind::COMMAND_2);
        assert_eq!(request.payload(), [&[0x3c, 0x01][..], b"AA:BB:CC:DD:EE:FF"].concat());
        assert_eq!(Request::ConnectedDevices(DeviceList::WithClass).payload(), [0x36, 0x02]);
        assert_eq!(Request::Battery.kind(), kind::COMMAND);
        assert_eq!(parse_table_2(&[0x3d, 0x01, 0x02]), Some(Reply::PlayedFrom));
    }

    #[test]
    fn ignores_unknown_messages() {
        assert_eq!(parse(&[0xaa, 0x00]), None);
        assert_eq!(parse(&[]), None);
    }
}
