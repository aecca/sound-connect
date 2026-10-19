//! Keeps a link to the connected Sony headset on a background thread.
//!
//! The UI sends [`Request`]s and receives [`Event`]s. The thread finds the
//! headset, opens the vendor channel, runs the protocol [`Session`] and
//! reconnects on its own when the headset goes away.

use std::sync::mpsc;

pub use sc_protocol::{Battery, Codec, NoiseControl, NoiseMode, Request, Update};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// No supported headset is connected; still looking.
    Searching,
    Connected { name: String },
    Update(Update),
    Disconnected,
    /// This platform or headset can't be controlled yet.
    Unsupported(String),
}

pub struct Link {
    pub requests: mpsc::Sender<Request>,
    pub events: async_channel::Receiver<Event>,
}

/// Starts the background link. `name_filter` limits it to headsets whose
/// name contains the given text.
pub fn start(name_filter: Option<String>) -> Link {
    let (requests, request_rx) = mpsc::channel();
    let (event_tx, events) = async_channel::unbounded();
    std::thread::Builder::new()
        .name("bluetooth".into())
        .spawn(move || {
            #[cfg(target_os = "macos")]
            macos::run(name_filter, request_rx, event_tx);
            #[cfg(not(target_os = "macos"))]
            unsupported::run(name_filter, request_rx, event_tx);
        })
        .expect("failed to start the Bluetooth thread");
    Link { requests, events }
}

/// Requests sent right after connecting, to fill in the UI.
pub(crate) const INITIAL_REQUESTS: [Request; 6] = [
    Request::Init,
    Request::FirmwareVersion,
    Request::Battery,
    Request::Codec,
    Request::NoiseControl,
    Request::SpeakToChat,
];
