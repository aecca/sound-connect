//! What the app knows about the connected headset. The panel renders it and
//! changes it through [`Headset::change`], which updates the UI right away
//! and tells the headset.

use std::collections::HashMap;
use std::sync::mpsc;

use gpui_kit::*;
use sc_bluetooth::Event;
use sc_core::{AutoOff, Battery, Device, Feature, ListeningMode, Setting, Update};

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
    pub codec: Option<String>,
    pub mode: Option<ListeningMode>,
    pub ambient_level: Option<u8>,
    pub focus_on_voice: Option<bool>,
    pub features: HashMap<Feature, bool>,
    pub auto_off: Option<AutoOff>,
    requests: mpsc::Sender<Setting>,
}

impl Headset {
    pub fn new(requests: mpsc::Sender<Setting>) -> Self {
        Self {
            connection: Connection::Searching,
            name: None,
            device: None,
            battery: None,
            codec: None,
            mode: None,
            ambient_level: None,
            focus_on_voice: None,
            features: HashMap::new(),
            auto_off: None,
            requests,
        }
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Searching | Event::Disconnected => *self = Self::new(self.requests.clone()),
            Event::Connected { name } => {
                self.connection = Connection::Connected;
                self.name = Some(name);
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
            Update::Mode(mode) => self.mode = Some(mode),
            Update::AmbientLevel(level) => self.ambient_level = Some(level),
            Update::FocusOnVoice(on) => self.focus_on_voice = Some(on),
            Update::Feature(feature, on) => {
                self.features.insert(feature, on);
            }
            Update::AutoOff(auto_off) => self.auto_off = Some(auto_off),
            // Not shown in this panel yet.
            Update::Firmware(_) | Update::Earbuds { .. } | Update::CaseBattery(_) | Update::Equalizer(_) => {}
        }
    }

    /// Applies a change the user made and sends it to the headset, unless
    /// nothing actually changed (a slider reports every position).
    pub fn change(&mut self, setting: Setting, cx: &mut Context<Self>) {
        let changed = match setting {
            Setting::Mode(mode) => self.mode.replace(mode) != Some(mode),
            Setting::AmbientLevel(level) => self.ambient_level.replace(level) != Some(level),
            Setting::FocusOnVoice(on) => self.focus_on_voice.replace(on) != Some(on),
            Setting::Feature(feature, on) => self.features.insert(feature, on) != Some(on),
            Setting::AutoOff(auto_off) => self.auto_off.replace(auto_off) != Some(auto_off),
            // This panel has no equalizer; the change still goes through.
            Setting::EqualizerPreset(_) | Setting::EqualizerBands(_) => true,
        };
        if changed {
            let _ = self.requests.send(setting);
            cx.notify();
        }
    }
}
