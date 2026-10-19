//! The contract every brand's driver fulfills.

use std::time::Instant;

use crate::device::{Device, Setting, Update};

/// Bytes to write to the headset's control channel, as one write.
pub type Frame = Vec<u8>;

/// What a driver wants done after handling something: frames to write, in
/// order, and changes to show.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub writes: Vec<Frame>,
    pub updates: Vec<Update>,
}

impl Output {
    pub fn from_writes(writes: Vec<Frame>) -> Self {
        Self { writes, updates: Vec::new() }
    }
}

/// Speaks one brand's protocol over an open control channel, without doing
/// any I/O: transports pass in what they read and write out what it returns.
/// `now` lets a driver time out requests the headset never answered.
pub trait Driver: Send {
    /// What the headset is and which controls it has, as far as known.
    fn device(&self) -> &Device;

    /// Called once the channel is open: the handshake and first reads.
    fn start(&mut self, now: Instant) -> Output;

    /// Handles bytes read from the channel, in whatever chunks they came.
    fn receive(&mut self, bytes: &[u8], now: Instant) -> Output;

    /// Applies a change the user made. Settings the headset doesn't have are
    /// ignored.
    fn set(&mut self, setting: Setting, now: Instant) -> Vec<Frame>;

    /// Asks for values that change on their own, like the battery level.
    fn refresh(&mut self, now: Instant) -> Vec<Frame>;

    /// Call a few times a second: resends unanswered requests.
    fn tick(&mut self, now: Instant) -> Vec<Frame>;
}
