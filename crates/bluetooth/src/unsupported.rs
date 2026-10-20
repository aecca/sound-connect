use std::sync::mpsc;

use crate::{Event, Request};

pub fn run(_name_filter: Option<String>, _requests: mpsc::Receiver<Request>, events: async_channel::Sender<Event>) {
    let _ = events.send_blocking(Event::Unsupported("This system isn't supported yet.".into()));
}
