//! Keeps a link to a connected headset on a background thread.
//!
//! The UI sends [`Request`]s and receives [`Event`]s. The thread finds a
//! headset offering a supported control service, opens that channel, lets
//! the brand's [driver](sc_core::Driver) speak to it and reconnects on its
//! own when the headset goes away.

use std::sync::mpsc;

use sc_core::{Setting, Update};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod unsupported;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// No supported headset is connected; still looking.
    Searching,
    /// Supported headsets paired with this computer but not connected, by
    /// name, which [`Request::Connect`] can connect.
    Paired(Vec<String>),
    /// Connecting a paired headset didn't work, e.g. because it's off.
    ConnectFailed,
    Connected {
        name: String,
    },
    Update(Update),
    Disconnected,
    /// This platform can't control headsets yet.
    Unsupported(String),
}

/// What the UI asks of the link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// A change for the connected headset.
    Change(Setting),
    /// Connect this paired headset, by name, while none is connected.
    Connect(String),
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
            #[cfg(target_os = "linux")]
            linux::run(name_filter, request_rx, event_tx);
            #[cfg(target_os = "macos")]
            macos::run(name_filter, request_rx, event_tx);
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            unsupported::run(name_filter, request_rx, event_tx);
        })
        .expect("failed to start the Bluetooth thread");
    Link { requests, events }
}
