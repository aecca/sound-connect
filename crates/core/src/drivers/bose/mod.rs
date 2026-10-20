//! Bose headphones, over BMAP, the protocol of the Bose Connect app.
//!
//! Supported: QuietComfort 35 and QuietComfort 35 II. Other Bose headsets
//! speak BMAP too, but differently enough (other channels, other functions,
//! authentication) that they're reported as unsupported for now.

mod packet;

use std::time::Instant;

use packet::{Function, Operator, Packet, PacketReader};

use crate::device::{AutoOff, Battery, BatteryLayout, Brand, Device, Feature, ListeningMode, Mode, Setting, Update};
use crate::driver::{Driver, Frame, Output};
use crate::queue::StopAndWait;

/// Apple's Wireless iAP service, which Bose headsets advertise and are recognized by; BMAP isn't on its channel.
pub const SERVICE: &str = "00000000-DECA-FADE-DECA-DEAFDECACAFF";
/// Serial Port, whose channel carries BMAP on the QuietComfort 35 and 35 II.
pub const CHANNEL_SERVICE: &str = "00001101-0000-1000-8000-00805F9B34FB";

/// Models this driver knows, by the product ID the headset reports.
const MODELS: [(u16, &str); 2] = [(0x400c, "QuietComfort 35"), (0x4020, "QuietComfort 35 II")];

/// Noise cancelling levels, as the QuietComfort 35 encodes them.
const NOISE_LEVELS: [(ListeningMode, u8); 4] = [
    (ListeningMode::Off, 0),
    (ListeningMode::NoiseCancelling, 1),
    (ListeningMode::WindReduction, 2),
    (ListeningMode::NoiseCancellingLow, 3),
];

const AUTO_OFF: [AutoOff; 6] = [
    AutoOff::Never,
    AutoOff::Minutes(5),
    AutoOff::Minutes(20),
    AutoOff::Minutes(40),
    AutoOff::Minutes(60),
    AutoOff::Minutes(180),
];

const VOICE_PROMPTS_ON: u8 = 1 << 5;
const LANGUAGE_MASK: u8 = 0x1f;

/// The QuietComfort 35 II's Action button, on a single press, and the two
/// things it can do, per bosectl.
const ACTION_BUTTON: [u8; 2] = [0x10, 0x04];
const ACTION_VOICE_ASSISTANT: u8 = 0x01;
const ACTION_NOISE_CANCELLING: u8 = 0x02;
/// Only the QuietComfort 35 II has the Action button.
const HAS_ACTION_BUTTON: u16 = 0x4020;

pub struct BoseDriver {
    device: Device,
    reader: PacketReader,
    requests: StopAndWait<Packet>,
    /// Voice prompts share a byte with their language, which must be sent
    /// back unchanged when turning them on or off.
    language: Option<u8>,
}

impl BoseDriver {
    pub fn new(name: &str) -> Self {
        Self {
            device: Device::unidentified(Brand::Bose, name),
            reader: PacketReader::default(),
            requests: StopAndWait::default(),
            language: None,
        }
    }

    fn request(&mut self, packet: Packet, now: Instant) -> Vec<Frame> {
        let function = packet.function;
        let is_change = packet.operator == Operator::SetGet;
        self.requests.push(packet, |queued| is_change && queued.function == function);
        self.send_next(now)
    }

    fn send_next(&mut self, now: Instant) -> Vec<Frame> {
        self.requests.next(now).map(|packet| packet.encode()).into_iter().collect()
    }

    /// The device as apps see it, once the headset has said what it is.
    fn identify(&mut self, product_id: u16) -> Update {
        let Some((_, model)) = MODELS.iter().find(|(id, _)| *id == product_id) else {
            return Update::Unsupported(format!("This Bose model (product 0x{product_id:04x}) isn't supported yet."));
        };
        self.device = Device {
            brand: Brand::Bose,
            model: (*model).to_owned(),
            tested: false,
            battery: BatteryLayout::Single,
            modes: vec![
                Mode { mode: ListeningMode::Off, name: "Off" },
                Mode { mode: ListeningMode::NoiseCancellingLow, name: "Low" },
                Mode { mode: ListeningMode::NoiseCancelling, name: "High" },
            ],
            ambient: None,
            features: if product_id == HAS_ACTION_BUTTON {
                vec![Feature::VoicePrompts, Feature::ActionButtonNoiseCancelling]
            } else {
                vec![Feature::VoicePrompts]
            },
            auto_off: AUTO_OFF.to_vec(),
            noise_button: Vec::new(),
            speak_to_chat_options: false,
            equalizer: None,
        };
        if self.device.features.contains(&Feature::ActionButtonNoiseCancelling) {
            // Goes out after the other start-up reads.
            self.requests.push(Packet::get(Function::BUTTONS), |_| false);
        }
        Update::Device(self.device.clone())
    }

    fn update(&mut self, packet: &Packet) -> Option<Update> {
        if packet.operator != Operator::Status {
            return None;
        }
        let payload = &packet.payload;
        match packet.function {
            Function::PRODUCT => match payload[..] {
                [high, low, ..] => Some(self.identify(u16::from_be_bytes([high, low]))),
                _ => None,
            },
            Function::FIRMWARE => Some(Update::Firmware(String::from_utf8_lossy(payload).into_owned())),
            Function::BATTERY => payload.first().map(|&level| Update::Battery(Battery { level, charging: false })),
            Function::NOISE_CANCELLING => {
                let level = *payload.first()?;
                NOISE_LEVELS.iter().find(|(_, code)| *code == level).map(|(mode, _)| Update::Mode(*mode))
            }
            Function::VOICE_PROMPTS => {
                let byte = *payload.first()?;
                self.language = Some(byte & LANGUAGE_MASK);
                Some(Update::Feature(Feature::VoicePrompts, byte & VOICE_PROMPTS_ON != 0))
            }
            Function::AUTO_OFF => {
                let minutes = *payload.first()?;
                Some(Update::AutoOff(if minutes == 0 { AutoOff::Never } else { AutoOff::Minutes(minutes.into()) }))
            }
            Function::BUTTONS => match payload[..] {
                [button, press, action, ..] if [button, press] == ACTION_BUTTON => match action {
                    ACTION_NOISE_CANCELLING => Some(Update::Feature(Feature::ActionButtonNoiseCancelling, true)),
                    ACTION_VOICE_ASSISTANT => Some(Update::Feature(Feature::ActionButtonNoiseCancelling, false)),
                    // Set to something else, e.g. turned off, by another app.
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
}

impl Driver for BoseDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn start(&mut self, now: Instant) -> Output {
        let functions = [
            Function::INIT,
            Function::PRODUCT,
            Function::FIRMWARE,
            Function::BATTERY,
            Function::NOISE_CANCELLING,
            Function::VOICE_PROMPTS,
            Function::AUTO_OFF,
        ];
        let writes = functions.into_iter().flat_map(|function| self.request(Packet::get(function), now)).collect();
        Output::from_writes(writes)
    }

    fn receive(&mut self, bytes: &[u8], now: Instant) -> Output {
        let mut updates = Vec::new();
        for packet in self.reader.push(bytes) {
            let answers_request = self.requests.in_flight().is_some_and(|request| request.function == packet.function);
            if answers_request && packet.operator.is_answer() {
                self.requests.complete();
            }
            updates.extend(self.update(&packet));
        }
        Output { writes: self.send_next(now), updates }
    }

    fn set(&mut self, setting: Setting, now: Instant) -> Vec<Frame> {
        match setting {
            Setting::Mode(mode) if self.device.modes.iter().any(|offered| offered.mode == mode) => {
                match NOISE_LEVELS.iter().find(|(level_mode, _)| *level_mode == mode) {
                    Some((_, code)) => self.request(Packet::set_get(Function::NOISE_CANCELLING, vec![*code]), now),
                    None => Vec::new(),
                }
            }
            Setting::Feature(Feature::VoicePrompts, on) if self.device.features.contains(&Feature::VoicePrompts) => {
                // Without the language, the change would also reset it.
                let Some(language) = self.language else { return Vec::new() };
                let byte = language | if on { VOICE_PROMPTS_ON } else { 0 };
                self.request(Packet::set_get(Function::VOICE_PROMPTS, vec![byte]), now)
            }
            Setting::Feature(Feature::ActionButtonNoiseCancelling, on)
                if self.device.features.contains(&Feature::ActionButtonNoiseCancelling) =>
            {
                let action = if on { ACTION_NOISE_CANCELLING } else { ACTION_VOICE_ASSISTANT };
                self.request(
                    Packet::set_get(Function::BUTTONS, [ACTION_BUTTON[0], ACTION_BUTTON[1], action].to_vec()),
                    now,
                )
            }
            Setting::AutoOff(auto_off) if self.device.auto_off.contains(&auto_off) => {
                let minutes = match auto_off {
                    AutoOff::Never => 0,
                    AutoOff::Minutes(minutes) => u8::try_from(minutes).expect("the offered choices fit in a byte"),
                    // Not one of the choices this headset offers.
                    AutoOff::WhenTakenOff => return Vec::new(),
                };
                self.request(Packet::set_get(Function::AUTO_OFF, vec![minutes]), now)
            }
            _ => Vec::new(),
        }
    }

    fn refresh(&mut self, now: Instant) -> Vec<Frame> {
        self.request(Packet::get(Function::BATTERY), now)
    }

    fn tick(&mut self, now: Instant) -> Vec<Frame> {
        self.requests.tick(now).map(|packet| packet.encode()).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A QuietComfort 35 II's answer to a get, as bosectl and based-connect
    /// record it.
    fn status(function: Function, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![function.block, function.id, 0x03, u8::try_from(payload.len()).unwrap()];
        bytes.extend_from_slice(payload);
        bytes
    }

    fn identified(now: Instant) -> BoseDriver {
        let mut driver = BoseDriver::new("Bose QC35 II");
        driver.start(now);
        driver.receive(&status(Function::INIT, &[1, 0, 0, 0, 0]), now);
        driver.receive(&status(Function::PRODUCT, &[0x40, 0x20, 0x02]), now);
        driver
    }

    #[test]
    fn starts_with_the_init_read_alone() {
        let mut driver = BoseDriver::new("Bose QC35 II");
        let output = driver.start(Instant::now());
        assert_eq!(output.writes, [vec![0x00, 0x01, 0x01, 0x00]]);
    }

    #[test]
    fn asks_for_the_next_value_once_one_is_answered() {
        let now = Instant::now();
        let mut driver = BoseDriver::new("Bose QC35 II");
        driver.start(now);
        let output = driver.receive(&status(Function::INIT, &[1, 0, 0, 0, 0]), now);
        assert_eq!(output.writes, [vec![0x00, 0x03, 0x01, 0x00]]);
    }

    #[test]
    fn identifies_the_model_by_its_product_id() {
        let now = Instant::now();
        let mut driver = BoseDriver::new("Living room headphones");
        driver.start(now);
        driver.receive(&status(Function::INIT, &[1, 0, 0, 0, 0]), now);
        let output = driver.receive(&status(Function::PRODUCT, &[0x40, 0x20, 0x02]), now);
        let [Update::Device(device)] = &output.updates[..] else { panic!("{:?}", output.updates) };
        assert_eq!(device.model, "QuietComfort 35 II");
        assert_eq!(device.features, [Feature::VoicePrompts, Feature::ActionButtonNoiseCancelling]);
    }

    #[test]
    fn leaves_the_action_button_out_on_the_first_quietcomfort_35() {
        let now = Instant::now();
        let mut driver = BoseDriver::new("Bose QC35");
        driver.start(now);
        driver.receive(&status(Function::INIT, &[1, 0, 0, 0, 0]), now);
        let output = driver.receive(&status(Function::PRODUCT, &[0x40, 0x0c, 0x00]), now);
        let [Update::Device(device)] = &output.updates[..] else { panic!("{:?}", output.updates) };
        assert_eq!(device.features, [Feature::VoicePrompts]);
        assert!(driver.set(Setting::Feature(Feature::ActionButtonNoiseCancelling, true), now).is_empty());
    }

    #[test]
    fn switches_the_action_button_between_assistant_and_noise_cancelling() {
        let now = Instant::now();
        let mut driver = identified(now);
        for (function, payload) in [
            (Function::FIRMWARE, &b"4.8.1"[..]),
            (Function::BATTERY, &[80]),
            (Function::NOISE_CANCELLING, &[0, 0x0b]),
            (Function::VOICE_PROMPTS, &[0x21]),
        ] {
            driver.receive(&status(function, payload), now);
        }
        // The Action button is read last, once the model is known.
        let output = driver.receive(&status(Function::AUTO_OFF, &[20]), now);
        assert_eq!(output.writes, [vec![0x01, 0x09, 0x01, 0x00]]);
        // A QuietComfort 35's answer, as bosectl recorded it: the voice assistant.
        let output = driver.receive(&status(Function::BUTTONS, &[0x10, 0x04, 0x01, 0x07]), now);
        assert_eq!(output.updates, [Update::Feature(Feature::ActionButtonNoiseCancelling, false)]);

        assert_eq!(
            driver.set(Setting::Feature(Feature::ActionButtonNoiseCancelling, true), now),
            [vec![0x01, 0x09, 0x02, 0x03, 0x10, 0x04, 0x02]]
        );
    }

    #[test]
    fn reports_other_bose_models_as_unsupported() {
        let now = Instant::now();
        let mut driver = BoseDriver::new("Bose QC Ultra");
        driver.start(now);
        let output = driver.receive(&status(Function::PRODUCT, &[0x40, 0x75, 0x00]), now);
        assert!(matches!(&output.updates[..], [Update::Unsupported(_)]));
    }

    #[test]
    fn reads_the_state() {
        let now = Instant::now();
        let mut driver = identified(now);
        let mut bytes = status(Function::FIRMWARE, b"4.8.1");
        bytes.extend(status(Function::BATTERY, &[80]));
        bytes.extend(status(Function::NOISE_CANCELLING, &[3, 0x0b]));
        bytes.extend(status(Function::VOICE_PROMPTS, &[0x21]));
        bytes.extend(status(Function::AUTO_OFF, &[20]));
        assert_eq!(
            driver.receive(&bytes, now).updates,
            [
                Update::Firmware("4.8.1".into()),
                Update::Battery(Battery { level: 80, charging: false }),
                Update::Mode(ListeningMode::NoiseCancellingLow),
                Update::Feature(Feature::VoicePrompts, true),
                Update::AutoOff(AutoOff::Minutes(20)),
            ]
        );
    }

    #[test]
    fn changes_settings_with_set_get() {
        let now = Instant::now();
        let mut driver = identified(now);
        // Answer the remaining start-up reads, in the order they go out.
        driver.receive(&status(Function::FIRMWARE, b"4.8.1"), now);
        driver.receive(&status(Function::BATTERY, &[80]), now);
        driver.receive(&status(Function::NOISE_CANCELLING, &[0, 0x0b]), now);
        driver.receive(&status(Function::VOICE_PROMPTS, &[0x21]), now);
        driver.receive(&status(Function::AUTO_OFF, &[20]), now);
        driver.receive(&status(Function::BUTTONS, &[0x10, 0x04, 0x01, 0x07]), now);

        assert_eq!(
            driver.set(Setting::Mode(ListeningMode::NoiseCancelling), now),
            [vec![0x01, 0x06, 0x02, 0x01, 0x01]]
        );
        driver.receive(&status(Function::NOISE_CANCELLING, &[1, 0x0b]), now);
        assert_eq!(
            driver.set(Setting::Feature(Feature::VoicePrompts, false), now),
            [vec![0x01, 0x03, 0x02, 0x01, 0x01]]
        );
        driver.receive(&status(Function::VOICE_PROMPTS, &[0x01]), now);
        assert_eq!(driver.set(Setting::AutoOff(AutoOff::Never), now), [vec![0x01, 0x04, 0x02, 0x01, 0x00]]);
    }

    #[test]
    fn ignores_what_the_model_does_not_have() {
        let now = Instant::now();
        let mut driver = identified(now);
        assert!(driver.set(Setting::Mode(ListeningMode::Ambient), now).is_empty());
        assert!(driver.set(Setting::AutoOff(AutoOff::Minutes(7)), now).is_empty());
        assert!(driver.set(Setting::Feature(Feature::SpeakToChat, true), now).is_empty());
    }
}
