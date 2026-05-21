//! Signaling protocol for WebRTC negotiation.
//!
//! Signaling messages (SDP offer/answer, ICE candidates) are transported
//! over KNP ReliableOrdered channel. This module defines the wire format
//! and state machine for the signaling exchange.

use serde::{Deserialize, Serialize};

use crate::peer::PeerId;
use crate::room::RoomId;

/// Signaling message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    /// SDP offer from initiator.
    Offer,
    /// SDP answer from responder.
    Answer,
    /// ICE candidate trickle.
    IceCandidate,
    /// Peer joined room notification.
    Join,
    /// Peer left room notification.
    Leave,
    /// Position update for spatial audio.
    Position,
    /// Data channel message (cursor, annotation, reaction).
    Data,
}

/// Wire-format signaling message, serialized as JSON over KNP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMessage {
    /// Message type discriminator.
    #[serde(rename = "type")]
    pub signal_type: SignalType,
    /// Sender peer ID.
    pub from: PeerId,
    /// Target peer ID (empty = broadcast).
    pub to: PeerId,
    /// Room this message belongs to.
    pub room_id: RoomId,
    /// Payload (SDP string, ICE JSON, position array, or data blob).
    pub payload: String,
    /// Monotonic sequence for ordering.
    pub seq: u32,
}

impl SignalMessage {
    /// Create an SDP offer message.
    pub fn offer(from: PeerId, to: PeerId, room_id: RoomId, sdp: String, seq: u32) -> Self {
        Self {
            signal_type: SignalType::Offer,
            from,
            to,
            room_id,
            payload: sdp,
            seq,
        }
    }

    /// Create an SDP answer message.
    pub fn answer(from: PeerId, to: PeerId, room_id: RoomId, sdp: String, seq: u32) -> Self {
        Self {
            signal_type: SignalType::Answer,
            from,
            to,
            room_id,
            payload: sdp,
            seq,
        }
    }

    /// Create an ICE candidate message.
    pub fn ice_candidate(
        from: PeerId,
        to: PeerId,
        room_id: RoomId,
        candidate_json: String,
        seq: u32,
    ) -> Self {
        Self {
            signal_type: SignalType::IceCandidate,
            from,
            to,
            room_id,
            payload: candidate_json,
            seq,
        }
    }

    /// Create a join notification.
    pub fn join(from: PeerId, room_id: RoomId, display_name: &str, seq: u32) -> Self {
        Self {
            signal_type: SignalType::Join,
            from,
            to: String::new(),
            room_id,
            payload: display_name.to_string(),
            seq,
        }
    }

    /// Create a leave notification.
    pub fn leave(from: PeerId, room_id: RoomId, seq: u32) -> Self {
        Self {
            signal_type: SignalType::Leave,
            from,
            to: String::new(),
            room_id,
            payload: String::new(),
            seq,
        }
    }

    /// Create a position update for spatial audio.
    pub fn position(from: PeerId, room_id: RoomId, pos: [f32; 3], seq: u32) -> Self {
        let payload = serde_json::to_string(&pos).unwrap_or_default();
        Self {
            signal_type: SignalType::Position,
            from,
            to: String::new(),
            room_id,
            payload,
            seq,
        }
    }

    /// Create a data channel message (cursor, annotation, reaction).
    pub fn data(from: PeerId, room_id: RoomId, data_json: String, seq: u32) -> Self {
        Self {
            signal_type: SignalType::Data,
            from,
            to: String::new(),
            room_id,
            payload: data_json,
            seq,
        }
    }

    /// Serialize to JSON bytes for KNP transport.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Deserialize from JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// Check if this is a broadcast message (no specific target).
    pub fn is_broadcast(&self) -> bool {
        self.to.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_roundtrip() {
        let msg = SignalMessage::offer(
            "alice".into(),
            "bob".into(),
            "room-1".into(),
            "v=0\r\no=- ...".into(),
            1,
        );
        let bytes = msg.to_bytes();
        let msg2 = SignalMessage::from_bytes(&bytes).unwrap();
        assert_eq!(msg2.signal_type, SignalType::Offer);
        assert_eq!(msg2.from, "alice");
        assert_eq!(msg2.to, "bob");
        assert_eq!(msg2.seq, 1);
    }

    #[test]
    fn join_is_broadcast() {
        let msg = SignalMessage::join("alice".into(), "room-1".into(), "Alice", 0);
        assert!(msg.is_broadcast());
    }

    #[test]
    fn position_payload() {
        let msg = SignalMessage::position("bob".into(), "room-1".into(), [1.0, 2.0, 3.0], 5);
        let pos: [f32; 3] = serde_json::from_str(&msg.payload).unwrap();
        assert!((pos[0] - 1.0).abs() < f32::EPSILON);
        assert!((pos[2] - 3.0).abs() < f32::EPSILON);
    }
}
