//! KNP packet format. 5-byte header after session establishment.
//!
//! Pre-session: 7 bytes (magic "KN" + header)
//! Post-session: 5 bytes (no magic)
//!
//! Shannon η = 95% (post-session)

/// Channel type.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Position, rotation, animation. Loss-tolerant, no ordering.
    Unreliable = 0,
    /// Commands, chat, inventory. ACK + retransmit, ordered.
    ReliableOrdered = 1,
    /// Asset loading, spawn. ACK + retransmit, unordered.
    ReliableUnordered = 2,
    /// Voice (Opus). Unreliable + jitter buffer + FEC.
    Voice = 3,
}

bitflags::bitflags! {
    /// Packet flags (upper 4 bits of flags_channel byte).
    #[derive(Debug, Clone, Copy)]
    pub struct Flags: u8 {
        const RELIABLE  = 0b0001;
        const ORDERED   = 0b0010;
        const ENCRYPTED = 0b0100;
        const FRAGMENT  = 0b1000;
    }
}

/// Wire header (5 bytes post-session).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Header {
    /// Upper 4 bits: flags, lower 4 bits: channel.
    pub flags_channel: u8,
    /// Per-channel sequence number.
    pub sequence: u16,
    /// Piggybacked ACK (latest received seq from peer).
    pub ack: u16,
}

impl Header {
    pub const SIZE: usize = 5;
    pub const MAGIC: [u8; 2] = [0x4B, 0x4E]; // "KN"

    pub fn new(channel: Channel, flags: Flags, sequence: u16, ack: u16) -> Self {
        Self {
            flags_channel: (flags.bits() << 4) | (channel as u8),
            sequence: sequence.to_le(),
            ack: ack.to_le(),
        }
    }

    pub fn channel(&self) -> Channel {
        match self.flags_channel & 0x0F {
            0 => Channel::Unreliable,
            1 => Channel::ReliableOrdered,
            2 => Channel::ReliableUnordered,
            3 => Channel::Voice,
            _ => Channel::Unreliable,
        }
    }

    pub fn flags(&self) -> Flags {
        Flags::from_bits_truncate(self.flags_channel >> 4)
    }

    pub fn sequence(&self) -> u16 {
        u16::from_le(self.sequence)
    }

    pub fn ack(&self) -> u16 {
        u16::from_le(self.ack)
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let seq = self.sequence.to_le_bytes();
        let ack = self.ack.to_le_bytes();
        [self.flags_channel, seq[0], seq[1], ack[0], ack[1]]
    }

    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        Self {
            flags_channel: bytes[0],
            sequence: u16::from_le_bytes([bytes[1], bytes[2]]),
            ack: u16::from_le_bytes([bytes[3], bytes[4]]),
        }
    }
}

/// Complete packet: header + payload.
pub struct Packet {
    pub header: Header,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(channel: Channel, flags: Flags, seq: u16, ack: u16, payload: Vec<u8>) -> Self {
        Self {
            header: Header::new(channel, flags, seq, ack),
            payload,
        }
    }

    /// Serialize to wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Header::SIZE + self.payload.len());
        buf.extend_from_slice(&self.header.to_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserialize from wire bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Header::SIZE {
            return None;
        }
        let header = Header::from_bytes(bytes[..Header::SIZE].try_into().ok()?);
        let payload = bytes[Header::SIZE..].to_vec();
        Some(Self { header, payload })
    }

    /// Wire size in bytes.
    pub fn wire_size(&self) -> usize {
        Header::SIZE + self.payload.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let h = Header::new(
            Channel::ReliableOrdered,
            Flags::RELIABLE | Flags::ORDERED,
            42,
            41,
        );
        let bytes = h.to_bytes();
        let h2 = Header::from_bytes(&bytes);
        assert_eq!(h2.channel(), Channel::ReliableOrdered);
        assert!(h2.flags().contains(Flags::RELIABLE));
        assert!(h2.flags().contains(Flags::ORDERED));
        assert_eq!(h2.sequence(), 42);
        assert_eq!(h2.ack(), 41);
    }

    #[test]
    fn packet_roundtrip() {
        let pkt = Packet::new(
            Channel::Unreliable,
            Flags::empty(),
            100,
            99,
            vec![1, 2, 3, 4],
        );
        let bytes = pkt.to_bytes();
        assert_eq!(bytes.len(), 9); // 5 header + 4 payload
        let pkt2 = Packet::from_bytes(&bytes).unwrap();
        assert_eq!(pkt2.payload, vec![1, 2, 3, 4]);
        assert_eq!(pkt2.header.sequence(), 100);
    }
}
