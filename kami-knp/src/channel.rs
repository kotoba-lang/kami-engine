//! Reliable channel manager: ACK tracking, retransmit, ordering.

use crate::packet::{Channel, Flags, Packet};

const SEND_BUFFER_SIZE: usize = 256;
const ACK_TIMEOUT_MS: u64 = 100;

/// Per-channel state for reliable delivery.
struct ReliableChannel {
    send_seq: u16,
    recv_seq: u16,
    /// Unacknowledged packets (ring buffer, indexed by seq % SIZE).
    unacked: [Option<UnackedPacket>; SEND_BUFFER_SIZE],
}

struct UnackedPacket {
    data: Vec<u8>,
    sent_at_ms: u64,
    retransmit_count: u8,
}

impl ReliableChannel {
    fn new() -> Self {
        Self {
            send_seq: 0,
            recv_seq: 0,
            unacked: [const { None }; SEND_BUFFER_SIZE],
        }
    }

    fn next_seq(&mut self) -> u16 {
        let seq = self.send_seq;
        self.send_seq = self.send_seq.wrapping_add(1);
        seq
    }
}

/// Manages all 4 KNP channels.
pub struct ChannelManager {
    reliable_ordered: ReliableChannel,
    reliable_unordered: ReliableChannel,
    unreliable_seq: u16,
    voice_seq: u16,
    peer_ack: [u16; 4], // latest ACK per channel from peer
}

impl ChannelManager {
    pub fn new() -> Self {
        Self {
            reliable_ordered: ReliableChannel::new(),
            reliable_unordered: ReliableChannel::new(),
            unreliable_seq: 0,
            voice_seq: 0,
            peer_ack: [0; 4],
        }
    }

    /// Prepare a packet for sending. Returns wire bytes.
    pub fn send(&mut self, channel: Channel, payload: Vec<u8>) -> Vec<u8> {
        let (seq, flags) = match channel {
            Channel::Unreliable => {
                let seq = self.unreliable_seq;
                self.unreliable_seq = self.unreliable_seq.wrapping_add(1);
                (seq, Flags::empty())
            }
            Channel::ReliableOrdered => {
                let seq = self.reliable_ordered.next_seq();
                (seq, Flags::RELIABLE | Flags::ORDERED)
            }
            Channel::ReliableUnordered => {
                let seq = self.reliable_unordered.next_seq();
                (seq, Flags::RELIABLE)
            }
            Channel::Voice => {
                let seq = self.voice_seq;
                self.voice_seq = self.voice_seq.wrapping_add(1);
                (seq, Flags::empty())
            }
        };

        let ack = self.peer_ack[channel as usize];
        let pkt = Packet::new(channel, flags, seq, ack, payload);
        pkt.to_bytes()
    }

    /// Process a received packet. Returns payload if accepted.
    pub fn receive(&mut self, bytes: &[u8]) -> Option<(Channel, Vec<u8>)> {
        let pkt = Packet::from_bytes(bytes)?;
        let channel = pkt.header.channel();

        // Update our record of what peer has ACKed
        // (piggybacked ACK in their packets tells us what they received from us)

        // Update peer_ack (latest seq we received from peer)
        let seq = pkt.header.sequence();
        self.peer_ack[channel as usize] = seq;

        Some((channel, pkt.payload))
    }

    /// Get packets that need retransmission (called periodically).
    pub fn get_retransmits(&mut self, now_ms: u64) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        for slot in &mut self.reliable_ordered.unacked {
            if let Some(pkt) = slot {
                if now_ms - pkt.sent_at_ms > ACK_TIMEOUT_MS {
                    pkt.sent_at_ms = now_ms;
                    pkt.retransmit_count += 1;
                    result.push(pkt.data.clone());
                }
            }
        }

        result
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}
