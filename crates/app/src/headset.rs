//! What the app knows about the connected headset. The panel renders it and
//! changes it through [`Headset::change`], which updates the UI right away
//! and tells the headset.

use std::collections::HashMap;
use std::sync::mpsc;

use gpui_kit::*;
use sc_bluetooth::{Event, PairedHeadset, Request};
use sc_core::{
    AutoOff, Battery, ConnectedDevice, Device, EqPreset, Equalizer, Feature, ListeningMode, NoiseButton, Setting,
    SpeakToChatEnd, Update, VoiceSensitivity,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    Searching,
    Connected,
    Unsupported(String),
}

pub struct Headset {
    pub connection: Connection,
    pub name: Option<String>,
    /// Which controls the headset has, once its driver knows.
    pub device: Option<Device>,
    pub battery: Option<Battery>,
    /// Earbuds report each side and the case instead of `battery`; `None`
    /// for one that isn't reporting, e.g. a bud in its case.
    pub left_battery: Option<Battery>,
    pub right_battery: Option<Battery>,
    pub case_battery: Option<Battery>,
    pub codec: Option<String>,
    pub mode: Option<ListeningMode>,
    pub ambient_level: Option<u8>,
    pub focus_on_voice: Option<bool>,
    pub features: HashMap<Feature, bool>,
    pub auto_off: Option<AutoOff>,
    pub noise_button: Option<NoiseButton>,
    pub voice_sensitivity: Option<VoiceSensitivity>,
    pub speak_to_chat_end: Option<SpeakToChatEnd>,
    pub equalizer: Option<Equalizer>,
    /// The devices the headset is connected to, when it says.
    pub connected_devices: Vec<ConnectedDevice>,
    /// Supported headsets paired with this computer but not connected.
    pub paired: Vec<PairedHeadset>,
    /// The paired headset the user asked to connect, while that's under way.
    pub connecting: Option<String>,
    /// A problem to show while not connected, e.g. a failed connection.
    pub notice: Option<&'static str>,
    requests: mpsc::Sender<Request>,
}

impl Headset {
    pub fn new(requests: mpsc::Sender<Request>) -> Self {
        Self {
            connection: Connection::Searching,
            name: None,
            device: None,
            battery: None,
            left_battery: None,
            right_battery: None,
            case_battery: None,
            codec: None,
            mode: None,
            ambient_level: None,
            focus_on_voice: None,
            features: HashMap::new(),
            auto_off: None,
            noise_button: None,
            voice_sensitivity: None,
            speak_to_chat_end: None,
            equalizer: None,
            connected_devices: Vec::new(),
            paired: Vec::new(),
            connecting: None,
            notice: None,
            requests,
        }
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            // Forget the headset, but not what's paired or being connected.
            Event::Searching | Event::Disconnected => {
                let (paired, connecting, notice) =
                    (std::mem::take(&mut self.paired), self.connecting.take(), self.notice);
                *self = Self { paired, connecting, notice, ..Self::new(self.requests.clone()) };
            }
            Event::Paired(headsets) => self.paired = headsets,
            Event::ConnectFailed => {
                self.connecting = None;
                self.notice = Some("Couldn't connect. Are they on?");
            }
            Event::ControlsUnreachable => {
                self.connecting = None;
                self.notice = Some("They're connected, but their settings can't be reached yet.");
            }
            Event::Connected { name } => {
                self.connection = Connection::Connected;
                self.name = Some(name);
                self.connecting = None;
                self.notice = None;
            }
            Event::Unsupported(reason) => self.connection = Connection::Unsupported(reason),
            Event::Update(update) => self.update(update),
        }
    }

    fn update(&mut self, update: Update) {
        match update {
            Update::Device(device) => self.device = Some(device),
            Update::Unsupported(reason) => self.connection = Connection::Unsupported(reason),
            Update::Codec(codec) => self.codec = Some(codec),
            Update::Battery(battery) => self.battery = Some(battery),
            Update::Earbuds { left, right } => (self.left_battery, self.right_battery) = (left, right),
            Update::CaseBattery(battery) => self.case_battery = Some(battery),
            Update::Mode(mode) => self.mode = Some(mode),
            Update::AmbientLevel(level) => self.ambient_level = Some(level),
            Update::FocusOnVoice(on) => self.focus_on_voice = Some(on),
            Update::Feature(feature, on) => {
                self.features.insert(feature, on);
            }
            Update::AutoOff(auto_off) => self.auto_off = Some(auto_off),
            Update::NoiseButton(choice) => self.noise_button = Some(choice),
            Update::VoiceSensitivity(sensitivity) => self.voice_sensitivity = Some(sensitivity),
            Update::SpeakToChatEnd(end) => self.speak_to_chat_end = Some(end),
            Update::Equalizer(equalizer) => self.equalizer = Some(equalizer),
            Update::ConnectedDevices(devices) => self.connected_devices = devices,
            // Not shown on Linux.
            Update::Firmware(_) => {}
        }
    }

    /// The headset's battery, or the emptier earbud's.
    pub fn lowest_battery(&self) -> Option<Battery> {
        self.battery.or_else(|| [self.left_battery, self.right_battery].into_iter().flatten().min_by_key(|b| b.level))
    }

    /// Whether the headset has this listening mode.
    pub fn offers(&self, mode: ListeningMode) -> bool {
        self.device.as_ref().is_some_and(|device| device.modes.iter().any(|offered| offered.mode == mode))
    }

    /// Applies a change the user made and sends it to the headset, unless
    /// nothing actually changed (a slider reports every position).
    pub fn change(&mut self, setting: Setting, cx: &mut Context<Self>) {
        let changed = match &setting {
            &Setting::Mode(mode) => self.mode.replace(mode) != Some(mode),
            &Setting::AmbientLevel(level) => self.ambient_level.replace(level) != Some(level),
            &Setting::FocusOnVoice(on) => self.focus_on_voice.replace(on) != Some(on),
            &Setting::Feature(feature, on) => self.features.insert(feature, on) != Some(on),
            &Setting::AutoOff(auto_off) => self.auto_off.replace(auto_off) != Some(auto_off),
            &Setting::NoiseButton(choice) => self.noise_button.replace(choice) != Some(choice),
            &Setting::VoiceSensitivity(sensitivity) => self.voice_sensitivity.replace(sensitivity) != Some(sensitivity),
            &Setting::SpeakToChatEnd(end) => self.speak_to_chat_end.replace(end) != Some(end),
            &Setting::EqualizerPreset(preset) => match &mut self.equalizer {
                Some(equalizer) if equalizer.preset != preset => {
                    equalizer.preset = preset;
                    true
                }
                _ => false,
            },
            // Moving a band switches to the manual preset.
            &Setting::EqualizerBands(bands) => {
                let manual = Equalizer { preset: EqPreset::Manual, bands };
                self.equalizer.is_some() && self.equalizer.replace(manual) != Some(manual)
            }
            Setting::PlayFrom(id) => {
                let changed = self.connected_devices.iter().any(|device| device.id == *id && !device.playing);
                for device in &mut self.connected_devices {
                    device.playing = device.id == *id;
                }
                changed
            }
        };
        if changed {
            let _ = self.requests.send(Request::Change(setting));
            cx.notify();
        }
    }

    /// Connects a paired headset, like the Connect button for AirPods.
    pub fn connect(&mut self, headset: &PairedHeadset, cx: &mut Context<Self>) {
        self.notice = None;
        self.connecting = Some(headset.name.clone());
        let _ = self.requests.send(Request::Connect(headset.address.clone()));
        cx.notify();
    }
}
