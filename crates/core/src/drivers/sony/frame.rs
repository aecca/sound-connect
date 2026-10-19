//! Wire framing used by Sony headphones over RFCOMM.
//!
//! ```text
//! 0x3E | kind | seq | len (u32 BE) | payload | checksum | 0x3C
//! ```
//!
//! Inside the frame, the bytes 0x3C, 0x3D and 0x3E are escaped as
//! `0x3D, byte & 0xEF`. The checksum is the wrapping sum of every byte
//! between the start and checksum markers.

const START: u8 = 0x3e;
const END: u8 = 0x3c;
const ESCAPE: u8 = 0x3d;
const ESCAPE_MASK: u8 = 0xef;

/// Data types carried in the `kind` byte.
pub(super) mod kind {
    pub const ACK: u8 = 0x01;
    pub const COMMAND: u8 = 0x0c;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Message {
    pub kind: u8,
    pub seq: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FrameError {
    TooShort,
    BadChecksum,
    BadLength,
}

impl Message {
    pub fn command(seq: u8, payload: Vec<u8>) -> Self {
        Self { kind: kind::COMMAND, seq, payload }
    }

    /// The acknowledgement the receiver of `seq` must send back.
    pub fn ack_for(seq: u8) -> Self {
        Self { kind: kind::ACK, seq: 1u8.wrapping_sub(seq), payload: Vec::new() }
    }

    pub fn is_ack(&self) -> bool {
        self.kind == kind::ACK
    }

    pub fn encode(&self) -> Vec<u8> {
        let len = self.payload.len() as u32;
        let mut body = Vec::with_capacity(self.payload.len() + 7);
        body.push(self.kind);
        body.push(self.seq);
        body.extend_from_slice(&len.to_be_bytes());
        body.extend_from_slice(&self.payload);
        body.push(checksum(&body));

        let mut out = Vec::with_capacity(body.len() + 4);
        out.push(START);
        for b in body {
            if matches!(b, START | END | ESCAPE) {
                out.extend_from_slice(&[ESCAPE, b & ESCAPE_MASK]);
            } else {
                out.push(b);
            }
        }
        out.push(END);
        out
    }

    /// Decodes the bytes between (and excluding) the start and end markers.
    fn decode_body(escaped: &[u8]) -> Result<Self, FrameError> {
        let mut bytes = Vec::with_capacity(escaped.len());
        let mut iter = escaped.iter();
        while let Some(&b) = iter.next() {
            if b == ESCAPE {
                match iter.next() {
                    Some(&next) => bytes.push(next | !ESCAPE_MASK),
                    None => return Err(FrameError::TooShort),
                }
            } else {
                bytes.push(b);
            }
        }

        if bytes.len() < 7 {
            return Err(FrameError::TooShort);
        }
        let expected = bytes.pop().unwrap();
        if checksum(&bytes) != expected {
            return Err(FrameError::BadChecksum);
        }
        let len = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as usize;
        if bytes.len() != 6 + len {
            return Err(FrameError::BadLength);
        }
        Ok(Self { kind: bytes[0], seq: bytes[1], payload: bytes[6..].to_vec() })
    }
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}

/// Splits an incoming byte stream into messages. Bytes may arrive in
/// arbitrary chunks; partial frames are kept until the rest arrives.
#[derive(Default)]
pub(super) struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<Message, FrameError>> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(end) = self.buf.iter().position(|&b| b == END) {
            let frame: Vec<u8> = self.buf.drain(..=end).collect();
            // Anything before the last start marker is line noise.
            if let Some(start) = frame.iter().rposition(|&b| b == START) {
                out.push(Message::decode_body(&frame[start + 1..frame.len() - 1]));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Frames captured from a WH-1000XM5 (firmware 2.4.1).
    const INIT_REQUEST: [u8; 11] = [0x3e, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x0e, 0x3c];
    const BATTERY_REQUEST: [u8; 11] = [0x3e, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x22, 0x00, 0x30, 0x3c];
    const NOISE_REQUEST: [u8; 11] = [0x3e, 0x0c, 0x01, 0x00, 0x00, 0x00, 0x02, 0x66, 0x17, 0x8c, 0x3c];

    #[test]
    fn encodes_requests_like_the_headphones_expect() {
        assert_eq!(Message::command(0, vec![0x00, 0x00]).encode(), INIT_REQUEST);
        assert_eq!(Message::command(0, vec![0x22, 0x00]).encode(), BATTERY_REQUEST);
        assert_eq!(Message::command(1, vec![0x66, 0x17]).encode(), NOISE_REQUEST);
    }

    #[test]
    fn round_trips_through_the_reader() {
        let msg = Message::command(1, vec![0x67, 0x17, 0x01, 0x01, 0x01, 0x00, 0x14]);
        let mut reader = FrameReader::default();
        assert_eq!(reader.push(&msg.encode()), vec![Ok(msg)]);
    }

    #[test]
    fn escapes_marker_bytes() {
        let msg = Message::command(0, vec![0x3c, 0x3d, 0x3e]);
        let encoded = msg.encode();
        let inner = &encoded[1..encoded.len() - 1];
        assert!(!inner.iter().any(|&b| b == START || b == END));
        assert_eq!(FrameReader::default().push(&encoded), vec![Ok(msg)]);
    }

    #[test]
    fn reassembles_frames_split_across_reads() {
        let a = Message::ack_for(0);
        let b = Message::command(0, vec![0x23, 0x00, 0x21, 0x00]);
        let bytes = [a.encode(), b.encode()].concat();
        let mut reader = FrameReader::default();
        let (first, rest) = bytes.split_at(5);
        assert!(reader.push(first).is_empty());
        assert_eq!(reader.push(rest), vec![Ok(a), Ok(b)]);
    }

    #[test]
    fn rejects_corrupted_frames() {
        let mut bytes = Message::command(0, vec![0x22, 0x00]).encode();
        bytes[8] ^= 0x01;
        assert_eq!(FrameReader::default().push(&bytes), vec![Err(FrameError::BadChecksum)]);
    }

    #[test]
    fn acks_flip_the_sequence_number() {
        assert_eq!(Message::ack_for(0).seq, 1);
        assert_eq!(Message::ack_for(1).seq, 0);
    }
}
