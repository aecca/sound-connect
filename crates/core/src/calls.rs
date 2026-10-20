//! Ambient sound during calls: while an app records from a microphone, as in
//! a video call, the headset switches to ambient sound so you hear your own
//! voice, and back to its mode once the call is over.
//!
//! Apps tell [`CallAmbience`] whether anything is recording; it says which
//! mode to switch to. Whatever brand the headset is, the app only needs to
//! pass `None` for headsets without ambient sound.

use std::time::{Duration, Instant};

use crate::device::ListeningMode;

#[derive(Debug, Default)]
pub struct CallAmbience {
    in_call: bool,
    /// When the microphone went unused during a call.
    idle_since: Option<Instant>,
    /// The mode to go back to, when the call switched it to ambient sound.
    previous: Option<ListeningMode>,
}

impl CallAmbience {
    /// How long the microphone must stay unused before the call counts as
    /// over: apps let go of it for a moment, e.g. while joining a meeting.
    pub const SETTLE: Duration = Duration::from_secs(3);

    /// Whether a call is going on, as far as it can tell: from when recording
    /// starts until it has stopped for [`SETTLE`](Self::SETTLE).
    pub fn in_call(&self) -> bool {
        self.in_call
    }

    /// Call when recording starts or stops, and again while it has stopped
    /// [`in_call`](Self::in_call), so it notices the call is over. `mode` is
    /// the headset's current mode, or `None` while there's none to change: no
    /// headset, or one without ambient sound. Returns the mode to switch the
    /// headset to, if any.
    pub fn update(&mut self, recording: bool, mode: Option<ListeningMode>, now: Instant) -> Option<ListeningMode> {
        if recording {
            self.idle_since = None;
            if self.in_call {
                return None;
            }
            self.in_call = true;
            self.previous = mode.filter(|&mode| mode != ListeningMode::Ambient);
            return self.previous.map(|_| ListeningMode::Ambient);
        }
        if !self.in_call {
            return None;
        }
        let idle_since = *self.idle_since.get_or_insert(now);
        if now.duration_since(idle_since) < Self::SETTLE {
            return None;
        }
        self.in_call = false;
        self.idle_since = None;
        // Unless the user picked another mode during the call.
        let previous = self.previous.take()?;
        (mode == Some(ListeningMode::Ambient)).then_some(previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::ListeningMode::{Ambient, NoiseCancelling, Off};

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn switches_to_ambient_for_a_call_and_back_after_it() {
        let start = Instant::now();
        let mut calls = CallAmbience::default();
        assert_eq!(calls.update(true, Some(NoiseCancelling), start), Some(Ambient));
        // The headset reports the change; nothing more to do while the call goes on.
        assert_eq!(calls.update(true, Some(Ambient), start + SECOND), None);

        let ended = start + 60 * SECOND;
        assert_eq!(calls.update(false, Some(Ambient), ended), None);
        assert!(calls.in_call());
        assert_eq!(calls.update(false, Some(Ambient), ended + CallAmbience::SETTLE), Some(NoiseCancelling));
        assert!(!calls.in_call());
        assert_eq!(calls.update(false, Some(NoiseCancelling), ended + 2 * CallAmbience::SETTLE), None);
    }

    #[test]
    fn rides_out_a_moment_without_the_microphone() {
        let start = Instant::now();
        let mut calls = CallAmbience::default();
        calls.update(true, Some(Off), start);
        assert_eq!(calls.update(false, Some(Ambient), start + SECOND), None);
        assert_eq!(calls.update(true, Some(Ambient), start + 2 * SECOND), None);
        // The pause before doesn't count towards this one.
        assert_eq!(calls.update(false, Some(Ambient), start + 3 * SECOND), None);
        assert_eq!(calls.update(false, Some(Ambient), start + 4 * SECOND), None);
        assert_eq!(calls.update(false, Some(Ambient), start + 6 * SECOND), Some(Off));
    }

    #[test]
    fn keeps_a_mode_the_user_picked_during_the_call() {
        let start = Instant::now();
        let mut calls = CallAmbience::default();
        calls.update(true, Some(NoiseCancelling), start);
        calls.update(false, Some(Off), start + SECOND);
        assert_eq!(calls.update(false, Some(Off), start + SECOND + CallAmbience::SETTLE), None);
    }

    #[test]
    fn leaves_ambient_sound_alone_when_it_was_already_on() {
        let start = Instant::now();
        let mut calls = CallAmbience::default();
        assert_eq!(calls.update(true, Some(Ambient), start), None);
        calls.update(false, Some(Ambient), start + SECOND);
        assert_eq!(calls.update(false, Some(Ambient), start + SECOND + CallAmbience::SETTLE), None);
    }

    #[test]
    fn does_nothing_without_a_headset_to_switch() {
        let start = Instant::now();
        let mut calls = CallAmbience::default();
        assert_eq!(calls.update(true, None, start), None);
        // A headset connecting during the call is left as it is.
        assert_eq!(calls.update(true, Some(NoiseCancelling), start + SECOND), None);
        calls.update(false, Some(NoiseCancelling), start + 2 * SECOND);
        assert_eq!(calls.update(false, Some(NoiseCancelling), start + 2 * SECOND + CallAmbience::SETTLE), None);
    }
}
