//! Conversation state with one headset, independent of any Bluetooth API.
//!
//! The headset uses stop-and-wait: one request may be in flight, the headset
//! ACKs it with the sequence number to use next, and every message the
//! headset sends must be ACKed back. Transports feed bytes in and write out
//! whatever frames the session returns.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::command::{Request, Update, parse};
use crate::frame::{FrameReader, Message};

const RETRY_AFTER: Duration = Duration::from_millis(1000);
const MAX_RETRIES: u8 = 2;

#[derive(Default)]
pub struct Session {
    seq: u8,
    reader: FrameReader,
    queue: VecDeque<Request>,
    in_flight: Option<InFlight>,
}

struct InFlight {
    frame: Vec<u8>,
    sent_at: Instant,
    retries: u8,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    /// Frames to write to the headset, in order.
    pub writes: Vec<Vec<u8>>,
    pub updates: Vec<Update>,
}

impl Session {
    /// Queues a request and returns any frames to write now. A new
    /// noise-control change replaces one that has not been sent yet, so
    /// dragging a slider doesn't build a backlog.
    pub fn request(&mut self, request: Request, now: Instant) -> Vec<Vec<u8>> {
        if matches!(request, Request::SetNoiseControl(_)) {
            self.queue.retain(|r| !matches!(r, Request::SetNoiseControl(_)));
        }
        self.queue.push_back(request);
        self.pump(now)
    }

    pub fn receive(&mut self, bytes: &[u8], now: Instant) -> Output {
        let mut out = Output::default();
        for message in self.reader.push(bytes).into_iter().flatten() {
            if message.is_ack() {
                self.seq = message.seq;
                self.in_flight = None;
            } else {
                out.writes.push(Message::ack_for(message.seq).encode());
                out.updates.extend(parse(&message.payload));
            }
        }
        out.writes.extend(self.pump(now));
        out
    }

    /// Call periodically: resends a request the headset never ACKed, and
    /// gives up on it after a few tries so the queue keeps moving.
    pub fn tick(&mut self, now: Instant) -> Vec<Vec<u8>> {
        if let Some(in_flight) = &mut self.in_flight
            && now.duration_since(in_flight.sent_at) >= RETRY_AFTER
        {
            if in_flight.retries < MAX_RETRIES {
                in_flight.retries += 1;
                in_flight.sent_at = now;
                return vec![in_flight.frame.clone()];
            }
            self.in_flight = None;
        }
        self.pump(now)
    }

    fn pump(&mut self, now: Instant) -> Vec<Vec<u8>> {
        if self.in_flight.is_some() {
            return Vec::new();
        }
        let Some(request) = self.queue.pop_front() else {
            return Vec::new();
        };
        let frame = Message::command(self.seq, request.payload()).encode();
        self.in_flight = Some(InFlight { frame: frame.clone(), sent_at: now, retries: 0 });
        vec![frame]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Battery, NoiseControl, NoiseMode};

    fn ack(seq: u8) -> Vec<u8> {
        Message { kind: crate::frame::kind::ACK, seq, payload: vec![] }.encode()
    }

    #[test]
    fn sends_one_request_at_a_time() {
        let now = Instant::now();
        let mut s = Session::default();
        assert_eq!(s.request(Request::Init, now), vec![Message::command(0, vec![0x00, 0x00]).encode()]);
        assert!(s.request(Request::Battery, now).is_empty());

        // The ACK carries the sequence number for the next request.
        let out = s.receive(&ack(1), now);
        assert_eq!(out.writes, vec![Message::command(1, vec![0x22, 0x00]).encode()]);
    }

    #[test]
    fn acks_and_parses_what_the_headset_sends() {
        let now = Instant::now();
        let mut s = Session::default();
        let reply = Message::command(0, vec![0x23, 0x00, 0x21, 0x00]).encode();
        let out = s.receive(&reply, now);
        assert_eq!(out.writes, vec![Message::ack_for(0).encode()]);
        assert_eq!(out.updates, vec![Update::Battery(Battery { level: 33, charging: false })]);
    }

    #[test]
    fn keeps_only_the_latest_pending_noise_change() {
        let now = Instant::now();
        let mut s = Session::default();
        s.request(Request::Init, now);
        for level in [5, 10, 15] {
            let nc = NoiseControl { mode: NoiseMode::Ambient, focus_on_voice: false, ambient_level: level };
            s.request(Request::SetNoiseControl(nc), now);
        }
        let out = s.receive(&ack(1), now);
        assert_eq!(out.writes.len(), 1);
        assert_eq!(out.writes[0], Message::command(1, vec![0x68, 0x17, 0x01, 1, 1, 0, 15]).encode());
    }

    #[test]
    fn retries_then_moves_on() {
        let start = Instant::now();
        let mut s = Session::default();
        let init = s.request(Request::Init, start);
        s.request(Request::Battery, start);

        assert!(s.tick(start + Duration::from_millis(500)).is_empty());
        assert_eq!(s.tick(start + RETRY_AFTER), init);
        assert_eq!(s.tick(start + RETRY_AFTER * 2), init);
        // Out of retries: the next request goes out instead.
        assert_eq!(s.tick(start + RETRY_AFTER * 3), vec![Message::command(0, vec![0x22, 0x00]).encode()]);
    }
}
