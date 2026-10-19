//! Sony's conversation with one headset.
//!
//! One request may be in flight; the headset ACKs it with the sequence
//! number to use next, and every message the headset sends must be ACKed
//! back.

use std::mem::discriminant;
use std::time::Instant;

use super::command::{Reply, Request, parse};
use super::frame::{FrameReader, Message};
use crate::driver::Frame;
use crate::queue::StopAndWait;

#[derive(Default)]
pub(super) struct Session {
    seq: u8,
    reader: FrameReader,
    requests: StopAndWait<Request>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Received {
    pub writes: Vec<Frame>,
    pub replies: Vec<Reply>,
}

impl Session {
    /// Queues a request and returns any frame to write now. A noise-control
    /// or equalizer change replaces one of its kind that hasn't gone out yet.
    pub fn request(&mut self, request: Request, now: Instant) -> Vec<Frame> {
        let coalesces = matches!(request, Request::SetNoiseControl(_) | Request::SetEqualizerBands(_));
        let kind = discriminant(&request);
        self.requests.push(request, |queued| coalesces && discriminant(queued) == kind);
        self.send_next(now)
    }

    pub fn receive(&mut self, bytes: &[u8], now: Instant) -> Received {
        let mut received = Received::default();
        for message in self.reader.push(bytes).into_iter().flatten() {
            if message.is_ack() {
                self.seq = message.seq;
                self.requests.complete();
            } else {
                received.writes.push(Message::ack_for(message.seq).encode());
                received.replies.extend(parse(&message.payload));
            }
        }
        received.writes.extend(self.send_next(now));
        received
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Frame> {
        self.requests.tick(now).map(|request| self.encode(&request)).into_iter().collect()
    }

    fn send_next(&mut self, now: Instant) -> Vec<Frame> {
        self.requests.next(now).map(|request| self.encode(&request)).into_iter().collect()
    }

    fn encode(&self, request: &Request) -> Frame {
        Message::command(self.seq, request.payload()).encode()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::device::{Battery, ListeningMode};
    use crate::drivers::sony::command::{NoiseControl, NoiseLayout};
    use crate::drivers::sony::frame::kind;

    fn ack(seq: u8) -> Frame {
        Message { kind: kind::ACK, seq, payload: Vec::new() }.encode()
    }

    #[test]
    fn sends_one_request_at_a_time() {
        let now = Instant::now();
        let mut s = Session::default();
        assert_eq!(s.request(Request::Init, now), [Message::command(0, vec![0x00, 0x00]).encode()]);
        assert!(s.request(Request::Battery, now).is_empty());

        // The ACK carries the sequence number for the next request.
        let received = s.receive(&ack(1), now);
        assert_eq!(received.writes, [Message::command(1, vec![0x22, 0x00]).encode()]);
    }

    #[test]
    fn acks_and_parses_what_the_headset_sends() {
        let now = Instant::now();
        let mut s = Session::default();
        let reply = Message::command(0, vec![0x23, 0x00, 0x21, 0x00]).encode();
        let received = s.receive(&reply, now);
        assert_eq!(received.writes, [Message::ack_for(0).encode()]);
        assert_eq!(received.replies, [Reply::Battery(Battery { level: 33, charging: false })]);
    }

    #[test]
    fn keeps_only_the_latest_pending_noise_change() {
        let now = Instant::now();
        let mut s = Session::default();
        s.request(Request::Init, now);
        for level in [5, 10, 15] {
            let nc = NoiseControl {
                mode: ListeningMode::Ambient,
                focus_on_voice: false,
                ambient_level: level,
                layout: NoiseLayout::Standard,
            };
            s.request(Request::SetNoiseControl(nc), now);
        }
        let received = s.receive(&ack(1), now);
        assert_eq!(received.writes, [Message::command(1, vec![0x68, 0x17, 0x01, 1, 1, 0, 15]).encode()]);
    }

    #[test]
    fn resends_an_unanswered_request_with_the_same_sequence_number() {
        let start = Instant::now();
        let mut s = Session::default();
        let init = s.request(Request::Init, start);
        assert_eq!(s.tick(start + Duration::from_secs(1)), init);
    }
}
