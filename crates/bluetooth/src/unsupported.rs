use std::sync::mpsc;

use sc_core::Setting;

use crate::Event;

pub fn run(_name_filter: Option<String>, _requests: mpsc::Receiver<Setting>, events: async_channel::Sender<Event>) {
    let _ = events.send_blocking(Event::Unsupported("This system isn't supported yet.".into()));
}
