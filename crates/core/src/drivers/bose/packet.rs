//! Bose's BMAP framing.
//!
//! ```text
//! block | function | flags | length | payload
//! ```
//!
//! The low four bits of `flags` are the operator. Layouts follow based-connect
//! and bosectl (MIT), which checked them against a QuietComfort 35 on
//! firmware 4.8.1.

/// A function of the headset: a block and a function within it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Function {
    pub block: u8,
    pub id: u8,
}

impl Function {
    /// Must be read first: the headset ignores everything else until then.
    pub const INIT: Self = Self { block: 0, id: 1 };
    /// Product ID (big-endian u16) and variant.
    pub const PRODUCT: Self = Self { block: 0, id: 3 };
    /// Firmware version, as ASCII.
    pub const FIRMWARE: Self = Self { block: 0, id: 5 };
    /// Voice prompts: bit 5 is on/off, the low five bits the language.
    pub const VOICE_PROMPTS: Self = Self { block: 1, id: 3 };
    /// Minutes before turning off when unused; 0 means never.
    pub const AUTO_OFF: Self = Self { block: 1, id: 4 };
    /// Noise cancelling ("active noise reduction") level.
    pub const NOISE_CANCELLING: Self = Self { block: 1, id: 6 };
    /// What a button does: the button, the press and the action, then which
    /// actions it can take.
    pub const BUTTONS: Self = Self { block: 1, id: 9 };
    /// Battery level, in percent.
    pub const BATTERY: Self = Self { block: 2, id: 2 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operator {
    Get,
    /// Writes a value and reads it back. Unlike a plain set, the headset
    /// accepts it without authentication.
    SetGet,
    /// The headset's answer to a get, or a change it reports by itself.
    Status,
    Error,
    /// An action finished.
    Result,
    /// An action is under way; its result comes later.
    Processing,
}

impl Operator {
    fn code(self) -> u8 {
        match self {
            Self::Get => 1,
            Self::SetGet => 2,
            Self::Status => 3,
            Self::Error => 4,
            Self::Result => 6,
            Self::Processing => 7,
        }
    }

    fn from_flags(flags: u8) -> Option<Self> {
        match flags & 0x0f {
            1 => Some(Self::Get),
            2 => Some(Self::SetGet),
            3 => Some(Self::Status),
            4 => Some(Self::Error),
            6 => Some(Self::Result),
            7 => Some(Self::Processing),
            _ => None,
        }
    }

    /// Whether this answers a request, so the next one can go out.
    pub fn is_answer(self) -> bool {
        matches!(self, Self::Status | Self::Error | Self::Result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Packet {
    pub function: Function,
    pub operator: Operator,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn get(function: Function) -> Self {
        Self { function, operator: Operator::Get, payload: Vec::new() }
    }

    pub fn set_get(function: Function, payload: Vec<u8>) -> Self {
        Self { function, operator: Operator::SetGet, payload }
    }

    /// Payloads are at most 255 bytes; the ones this driver sends are one.
    pub fn encode(&self) -> Vec<u8> {
        let length = u8::try_from(self.payload.len()).expect("BMAP payloads fit in 255 bytes");
        let mut bytes = vec![self.function.block, self.function.id, self.operator.code(), length];
        bytes.extend_from_slice(&self.payload);
        bytes
    }
}

/// Splits what the headset sends into packets. Several may arrive in one
/// read, and one may be split across reads.
#[derive(Default)]
pub(super) struct PacketReader {
    buf: Vec<u8>,
}

impl PacketReader {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Packet> {
        self.buf.extend_from_slice(bytes);
        let mut packets = Vec::new();
        let mut start = 0;
        while let Some(header) = self.buf.get(start..start + 4) {
            let end = start + 4 + usize::from(header[3]);
            let Some(payload) = self.buf.get(start + 4..end) else { break };
            // Operators this driver doesn't know are skipped, not misread.
            if let Some(operator) = Operator::from_flags(header[2]) {
                let function = Function { block: header[0], id: header[1] };
                packets.push(Packet { function, operator, payload: payload.to_vec() });
            }
            start = end;
        }
        self.buf.drain(..start);
        packets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_requests() {
        assert_eq!(Packet::get(Function::INIT).encode(), [0x00, 0x01, 0x01, 0x00]);
        assert_eq!(Packet::get(Function::BATTERY).encode(), [0x02, 0x02, 0x01, 0x00]);
        assert_eq!(Packet::set_get(Function::NOISE_CANCELLING, vec![3]).encode(), [0x01, 0x06, 0x02, 0x01, 0x03]);
    }

    #[test]
    fn splits_packets_that_arrive_together_or_in_pieces() {
        let mut reader = PacketReader::default();
        // A firmware answer, then the start of a battery one.
        let bytes = [0x00, 0x05, 0x03, 0x05, b'4', b'.', b'8', b'.', b'1', 0x02, 0x02, 0x03];
        let packets = reader.push(&bytes);
        assert_eq!(
            packets,
            [Packet { function: Function::FIRMWARE, operator: Operator::Status, payload: b"4.8.1".to_vec() }]
        );
        let packets = reader.push(&[0x01, 0x5a]);
        assert_eq!(packets, [Packet { function: Function::BATTERY, operator: Operator::Status, payload: vec![90] }]);
    }

    #[test]
    fn skips_unknown_operators() {
        let mut reader = PacketReader::default();
        let packets = reader.push(&[0x01, 0x06, 0x0f, 0x01, 0x00, 0x02, 0x02, 0x03, 0x01, 0x50]);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].function, Function::BATTERY);
    }
}
