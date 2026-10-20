//! Sony headphones, over the control protocol of their phone app (v2:
//! WH-1000XM5 and newer).

mod command;
mod frame;
mod profile;
mod session;

use std::time::Instant;

use command::{ChatOptions, DeviceList, NoiseControl, Reply, Request, device_list};
use profile::{Profile, profile_for};
use session::Session;

use crate::device::{Device, Feature, Setting, Update};
use crate::driver::{Driver, Frame, Output};

/// The control service of v2 headsets, the ones this driver speaks to.
pub const SERVICE: &str = "956C7B26-D49A-4BA8-B03F-B17D393CB6E2";
/// The control service of older (v1) headsets. They're recognized so the app
/// can say they aren't supported yet.
pub const SERVICE_V1: &str = "96CC203E-5068-46AD-B32D-E316F5E069BA";

pub struct SonyDriver {
    device: Device,
    profile: Profile,
    session: Session,
    /// Sony takes the whole noise-control state in each change, so the last
    /// one the headset reported is kept to build the next.
    noise: Option<NoiseControl>,
    /// Likewise for Speak-to-Chat's options.
    chat: Option<ChatOptions>,
    /// How the headset lists the devices it's connected to, once it's said it can.
    devices: Option<DeviceList>,
}

impl SonyDriver {
    pub fn new(name: &str) -> Self {
        let profile = profile_for(name);
        Self {
            device: profile.device(name),
            profile,
            session: Session::default(),
            noise: None,
            chat: None,
            devices: None,
        }
    }

    fn request_all(&mut self, requests: Vec<Request>, now: Instant) -> Vec<Frame> {
        requests.into_iter().flat_map(|request| self.session.request(request, now)).collect()
    }

    fn change_noise(&mut self, now: Instant, change: impl FnOnce(&mut NoiseControl)) -> Vec<Frame> {
        // Nothing to build on until the headset has reported its state.
        let Some(mut noise) = self.noise else { return Vec::new() };
        change(&mut noise);
        if self.noise == Some(noise) {
            return Vec::new();
        }
        self.noise = Some(noise);
        self.session.request(Request::SetNoiseControl(noise), now)
    }

    fn change_chat(&mut self, now: Instant, change: impl FnOnce(&mut ChatOptions)) -> Vec<Frame> {
        let Some(mut chat) = self.chat.filter(|_| self.device.speak_to_chat_options) else { return Vec::new() };
        change(&mut chat);
        if self.chat == Some(chat) {
            return Vec::new();
        }
        self.chat = Some(chat);
        self.session.request(Request::SetSpeakToChatOptions(chat), now)
    }

    /// What to show for a reply, and anything to ask the headset because of it.
    fn handle(&mut self, reply: Reply, now: Instant) -> (Vec<Update>, Vec<Frame>) {
        let ask = |driver: &mut Self, request| driver.session.request(request, now);
        match reply {
            Reply::Ready { v2: true, table2: true } => (Vec::new(), ask(self, Request::Table2Functions)),
            Reply::Table2Functions(functions) => {
                self.devices = device_list(&functions);
                let writes = self.devices.map(|list| ask(self, Request::ConnectedDevices(list))).unwrap_or_default();
                (Vec::new(), writes)
            }
            // Read the list again: switching may not have worked, e.g. during a call.
            Reply::PlayedFrom => {
                let writes = self.devices.map(|list| ask(self, Request::ConnectedDevices(list))).unwrap_or_default();
                (Vec::new(), writes)
            }
            reply => (self.updates(reply), Vec::new()),
        }
    }

    fn updates(&mut self, reply: Reply) -> Vec<Update> {
        match reply {
            Reply::Ready { v2: true, .. } | Reply::Table2Functions(_) | Reply::PlayedFrom => Vec::new(),
            Reply::Ready { v2: false, .. } => {
                vec![Update::Unsupported("This headset uses an older protocol that isn't supported yet.".into())]
            }
            Reply::Firmware(version) => vec![Update::Firmware(version)],
            Reply::Battery(battery) => vec![Update::Battery(battery)],
            Reply::EarbudBattery { left, right } => vec![Update::Earbuds { left, right }],
            Reply::CaseBattery(battery) => vec![Update::CaseBattery(battery)],
            Reply::Codec(codec) => vec![Update::Codec(codec)],
            Reply::NoiseControl(noise) => {
                self.noise = Some(noise);
                vec![
                    Update::Mode(noise.mode),
                    Update::AmbientLevel(noise.ambient_level),
                    Update::FocusOnVoice(noise.focus_on_voice),
                ]
            }
            Reply::SpeakToChat(on) => vec![Update::Feature(Feature::SpeakToChat, on)],
            Reply::SpeakToChatOptions(chat) => {
                self.chat = Some(chat);
                vec![Update::VoiceSensitivity(chat.sensitivity), Update::SpeakToChatEnd(chat.end)]
            }
            Reply::Dsee(on) => vec![Update::Feature(Feature::Dsee, on)],
            Reply::Equalizer(equalizer) => vec![Update::Equalizer(equalizer)],
            Reply::PauseWhenTakenOff(on) => vec![Update::Feature(Feature::PauseWhenTakenOff, on)],
            Reply::AutoOff(auto_off) => vec![Update::AutoOff(auto_off)],
            Reply::NoiseButton(choice) => vec![Update::NoiseButton(choice)],
            Reply::ConnectedDevices(devices) => vec![Update::ConnectedDevices(devices)],
        }
    }
}

impl Driver for SonyDriver {
    fn device(&self) -> &Device {
        &self.device
    }

    fn start(&mut self, now: Instant) -> Output {
        let writes = self.request_all(self.profile.initial_requests(), now);
        Output { writes, updates: vec![Update::Device(self.device.clone())] }
    }

    fn receive(&mut self, bytes: &[u8], now: Instant) -> Output {
        let received = self.session.receive(bytes, now);
        let mut output = Output { writes: received.writes, updates: Vec::new() };
        for reply in received.replies {
            let (updates, writes) = self.handle(reply, now);
            output.updates.extend(updates);
            output.writes.extend(writes);
        }
        output
    }

    fn set(&mut self, setting: Setting, now: Instant) -> Vec<Frame> {
        match setting {
            Setting::Mode(mode) if self.device.modes.iter().any(|offered| offered.mode == mode) => {
                self.change_noise(now, |noise| noise.mode = mode)
            }
            Setting::AmbientLevel(level) => self.change_noise(now, |noise| noise.ambient_level = level),
            Setting::FocusOnVoice(on) => self.change_noise(now, |noise| noise.focus_on_voice = on),
            Setting::Feature(Feature::SpeakToChat, on) if self.profile.speak_to_chat => {
                self.session.request(Request::SetSpeakToChat(on), now)
            }
            Setting::Feature(Feature::Dsee, on) if self.profile.dsee => self.session.request(Request::SetDsee(on), now),
            Setting::Feature(Feature::PauseWhenTakenOff, on) if self.profile.wear_sensor => {
                self.session.request(Request::SetPauseWhenTakenOff(on), now)
            }
            Setting::AutoOff(auto_off) if self.device.auto_off.contains(&auto_off) => {
                self.session.request(Request::SetAutoOff(auto_off), now)
            }
            Setting::NoiseButton(choice) if self.device.noise_button.contains(&choice) => {
                self.session.request(Request::SetNoiseButton(choice), now)
            }
            Setting::VoiceSensitivity(sensitivity) => self.change_chat(now, |chat| chat.sensitivity = sensitivity),
            Setting::SpeakToChatEnd(end) => self.change_chat(now, |chat| chat.end = end),
            Setting::PlayFrom(id) if self.devices.is_some() => match id.as_bytes().try_into() {
                Ok(address) => self.session.request(Request::PlayFrom(address), now),
                // Not an address this headset gave.
                Err(_) => Vec::new(),
            },
            Setting::EqualizerPreset(preset) => match self.profile.equalizer {
                Some(layout) => self.session.request(Request::SetEqualizerPreset(layout, preset), now),
                None => Vec::new(),
            },
            Setting::EqualizerBands(bands) if self.profile.equalizer.is_some() => {
                self.session.request(Request::SetEqualizerBands(bands), now)
            }
            _ => Vec::new(),
        }
    }

    fn refresh(&mut self, now: Instant) -> Vec<Frame> {
        self.request_all(self.profile.battery_requests(), now)
    }

    fn tick(&mut self, now: Instant) -> Vec<Frame> {
        self.session.tick(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{AutoOff, Battery, ListeningMode, NoiseButton, SpeakToChatEnd, VoiceSensitivity};
    use frame::{Message, kind};

    /// Feeds the driver what a WH-1000XM5 says: an ACK, then a message.
    fn headset_says(driver: &mut SonyDriver, payload: &[u8], now: Instant) -> Output {
        let mut bytes = Message { kind: kind::ACK, seq: 1, payload: Vec::new() }.encode();
        bytes.extend(Message::command(0, payload.to_vec()).encode());
        driver.receive(&bytes, now)
    }

    #[test]
    fn starts_by_describing_the_headset_and_asking_for_its_state() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-1000XM5");
        let output = driver.start(now);
        assert_eq!(output.writes, [Message::command(0, vec![0x00, 0x00]).encode()]);
        assert!(matches!(&output.updates[..], [Update::Device(device)] if device.model == "WH-1000XM5"));
    }

    #[test]
    fn turns_replies_into_updates() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-1000XM5");
        driver.start(now);
        let output = headset_says(&mut driver, &[0x67, 0x17, 0x01, 0x01, 0x01, 0x00, 0x14], now);
        assert_eq!(
            output.updates,
            [Update::Mode(ListeningMode::Ambient), Update::AmbientLevel(20), Update::FocusOnVoice(false)]
        );
        let output = headset_says(&mut driver, &[0x23, 0x00, 0x21, 0x00], now);
        assert_eq!(output.updates, [Update::Battery(Battery { level: 33, charging: false })]);
    }

    #[test]
    fn builds_noise_changes_on_the_reported_state() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-1000XM5");
        // Before the headset reports its state there's nothing to build on.
        assert!(driver.set(Setting::Mode(ListeningMode::NoiseCancelling), now).is_empty());

        headset_says(&mut driver, &[0x67, 0x17, 0x01, 0x01, 0x01, 0x01, 0x0e], now);
        let frames = driver.set(Setting::Mode(ListeningMode::NoiseCancelling), now);
        assert_eq!(frames, [Message::command(1, vec![0x68, 0x17, 0x01, 1, 0, 1, 14]).encode()]);
    }

    #[test]
    fn ignores_settings_the_headset_does_not_have() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-CH720N");
        assert!(driver.set(Setting::Feature(Feature::SpeakToChat, true), now).is_empty());
        assert!(driver.set(Setting::Feature(Feature::VoicePrompts, true), now).is_empty());
        assert!(driver.set(Setting::Feature(Feature::PauseWhenTakenOff, true), now).is_empty());
        assert!(driver.set(Setting::AutoOff(AutoOff::Never), now).is_empty());
        assert!(driver.set(Setting::NoiseButton(NoiseButton::AmbientOff), now).is_empty());
    }

    /// Feeds the driver a second-table message: an ACK, then the message.
    fn headset_says_in_table_2(driver: &mut SonyDriver, payload: &[u8], now: Instant) -> Output {
        let mut bytes = Message { kind: kind::ACK, seq: 1, payload: Vec::new() }.encode();
        bytes.extend(Message::command_2(0, payload.to_vec()).encode());
        driver.receive(&bytes, now)
    }

    #[test]
    fn lists_connected_devices_and_switches_which_plays() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WF-1000XM5");
        // Before the headset says how it lists them, there's nothing to switch.
        assert!(driver.set(Setting::PlayFrom("AA:BB:CC:DD:EE:FF".into()), now).is_empty());

        let output = headset_says(&mut driver, &[0x01, 0x00, 0x03, 0x00, 0x20, 0x16, 0x00, 0x00], now);
        assert!(output.writes.ends_with(&[Message::command_2(1, vec![0x06, 0x00]).encode()]));
        let output = headset_says_in_table_2(&mut driver, &[0x07, 0x00, 0x02, 0x32, 0x04, 0x31, 0xff], now);
        assert!(output.writes.ends_with(&[Message::command_2(1, vec![0x36, 0x02]).encode()]));

        let mut list = vec![0x37, 0x02, 0x02];
        for (address, slot, name) in [(b"11:22:33:44:55:66", 1, &b"Laptop"[..]), (b"AA:BB:CC:DD:EE:FF", 2, b"Phone")] {
            list.extend(address);
            list.extend([slot, 0x5a, 0x02, 0x0c, u8::try_from(name.len()).unwrap()]);
            list.extend(name);
        }
        list.push(1);
        let output = headset_says_in_table_2(&mut driver, &list, now);
        let [Update::ConnectedDevices(devices)] = &output.updates[..] else { panic!("{:?}", output.updates) };
        assert_eq!(
            devices.iter().map(|device| (device.name.as_str(), device.playing)).collect::<Vec<_>>(),
            [("Laptop", true), ("Phone", false)]
        );

        let frames = driver.set(Setting::PlayFrom("AA:BB:CC:DD:EE:FF".into()), now);
        assert_eq!(frames, [Message::command_2(1, [&[0x3c, 0x01][..], b"AA:BB:CC:DD:EE:FF"].concat()).encode()]);
        assert!(driver.set(Setting::PlayFrom("not an address".into()), now).is_empty());
        // Once it has answered, the list is read again to show what happened.
        let output = headset_says_in_table_2(&mut driver, &[0x3d, 0x01, 0x00], now);
        assert!(output.writes.ends_with(&[Message::command_2(1, vec![0x36, 0x02]).encode()]));
    }

    #[test]
    fn changes_speak_to_chat_options_together() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-1000XM5");
        assert!(driver.set(Setting::SpeakToChatEnd(SpeakToChatEnd::Never), now).is_empty());

        let output = headset_says(&mut driver, &[0xfb, 0x0c, 0x01, 0x00], now);
        assert_eq!(
            output.updates,
            [Update::VoiceSensitivity(VoiceSensitivity::High), Update::SpeakToChatEnd(SpeakToChatEnd::Short)]
        );
        let frames = driver.set(Setting::SpeakToChatEnd(SpeakToChatEnd::Never), now);
        assert_eq!(frames, [Message::command(1, vec![0xfc, 0x0c, 0x01, 0x03]).encode()]);
    }

    #[test]
    fn turns_off_and_pauses_when_taken_off() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-1000XM5");
        let output = headset_says(&mut driver, &[0x27, 0x05, 0x10, 0x00], now);
        assert_eq!(output.updates, [Update::AutoOff(AutoOff::WhenTakenOff)]);
        let output = headset_says(&mut driver, &[0xf7, 0x01, 0x00], now);
        assert_eq!(output.updates, [Update::Feature(Feature::PauseWhenTakenOff, true)]);

        let frames = driver.set(Setting::AutoOff(AutoOff::Never), now);
        assert_eq!(frames, [Message::command(1, vec![0x28, 0x05, 0x11, 0x00]).encode()]);
        // Only the choices Sony's app offers on this model go out.
        assert!(driver.set(Setting::AutoOff(AutoOff::Minutes(30)), now).is_empty());
    }

    #[test]
    fn reports_older_headsets_as_unsupported() {
        let now = Instant::now();
        let mut driver = SonyDriver::new("WH-1000XM4");
        let output = headset_says(&mut driver, &[0x01, 0x00, 0x40, 0x10], now);
        assert!(matches!(&output.updates[..], [Update::Unsupported(_)]));
    }
}
