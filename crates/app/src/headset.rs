//! What the app knows about the connected headset. The panel renders it and
//! calls the setters, which update the UI right away and tell the headset.

use std::sync::mpsc;

use gpui_kit::*;
use sc_bluetooth::{Battery, Codec, Event, NoiseControl, Request, Update};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    Searching,
    Connected,
    Unsupported(String),
}

pub struct Headset {
    pub connection: Connection,
    pub name: Option<String>,
    pub battery: Option<Battery>,
    pub codec: Option<Codec>,
    pub firmware: Option<String>,
    pub noise: Option<NoiseControl>,
    pub speak_to_chat: Option<bool>,
    requests: mpsc::Sender<Request>,
}

impl Headset {
    pub fn new(requests: mpsc::Sender<Request>) -> Self {
        Self {
            connection: Connection::Searching,
            name: None,
            battery: None,
            codec: None,
            firmware: None,
            noise: None,
            speak_to_chat: None,
            requests,
        }
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Searching | Event::Disconnected => {
                *self = Self::new(self.requests.clone());
            }
            Event::Connected { name } => {
                self.connection = Connection::Connected;
                self.name = Some(name);
            }
            Event::Unsupported(reason) => self.connection = Connection::Unsupported(reason),
            Event::Update(update) => match update {
                Update::Ready { .. } => {}
                Update::Firmware(version) => self.firmware = Some(version),
                Update::Battery(battery) => self.battery = Some(battery),
                Update::Codec(codec) => self.codec = Some(codec),
                Update::NoiseControl(noise) => self.noise = Some(noise),
                Update::SpeakToChat(on) => self.speak_to_chat = Some(on),
                // Not shown in this panel yet.
                Update::Equalizer(_) | Update::Dsee(_) => {}
            },
        }
    }

    pub fn set_noise(&mut self, noise: NoiseControl, cx: &mut Context<Self>) {
        if self.noise == Some(noise) {
            return;
        }
        self.noise = Some(noise);
        let _ = self.requests.send(Request::SetNoiseControl(noise));
        cx.notify();
    }

    pub fn set_speak_to_chat(&mut self, on: bool, cx: &mut Context<Self>) {
        self.speak_to_chat = Some(on);
        let _ = self.requests.send(Request::SetSpeakToChat(on));
        cx.notify();
    }

    /// Asks the headset for fresh values, e.g. when the panel opens.
    pub fn refresh(&self) {
        if self.connection == Connection::Connected {
            let _ = self.requests.send(Request::Battery);
            let _ = self.requests.send(Request::NoiseControl);
        }
    }
}
