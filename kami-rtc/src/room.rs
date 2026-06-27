//! Room management: create, join, leave, peer lifecycle.
//!
//! A Room is a named session containing multiple peers.
//! Signaling messages are routed through KNP ReliableOrdered channel.
//! Media is exchanged via WebRTC peer connections (mesh topology).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::media::MediaConstraints;
use crate::peer::{Peer, PeerId, PeerState};
use crate::signal::{SignalMessage, SignalType};
use crate::spatial::SpatialMixer;

/// Unique room identifier.
pub type RoomId = String;

/// Room topology for peer connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Topology {
    /// Full mesh: every peer connects to every other peer.
    /// Best for small rooms (2-8 participants).
    Mesh,
    /// Selective forwarding: peers connect to a central SFU.
    /// Best for large rooms (8+ participants).
    Sfu,
}

/// Room configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    /// Room display name.
    pub name: String,
    /// Maximum number of participants (0 = unlimited).
    pub max_participants: u32,
    /// Enable spatial audio for this room.
    pub spatial_audio: bool,
    /// Connection topology.
    pub topology: Topology,
    /// Default media constraints for joining.
    pub media_constraints: MediaConstraints,
    /// Enable KNP data channel for state sync.
    pub data_channel: bool,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            max_participants: 16,
            spatial_audio: true,
            topology: Topology::Mesh,
            media_constraints: MediaConstraints::default(),
            data_channel: true,
        }
    }
}

/// Events emitted by the room state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoomEvent {
    /// A peer joined the room.
    PeerJoined {
        peer_id: PeerId,
        display_name: String,
    },
    /// A peer left the room.
    PeerLeft { peer_id: PeerId },
    /// A peer's connection state changed.
    PeerStateChanged { peer_id: PeerId, state: PeerState },
    /// Received an SDP offer (need to create answer).
    OfferReceived { from: PeerId, sdp: String },
    /// Received an SDP answer.
    AnswerReceived { from: PeerId, sdp: String },
    /// Received an ICE candidate.
    IceCandidateReceived { from: PeerId, candidate: String },
    /// Peer position updated (spatial audio).
    PositionUpdated { peer_id: PeerId, position: [f32; 3] },
    /// Data channel message received.
    DataReceived { from: PeerId, data: String },
}

/// Room state machine. Platform-agnostic peer and signaling management.
pub struct Room {
    pub id: RoomId,
    pub config: RoomConfig,
    pub local_peer_id: PeerId,
    peers: HashMap<PeerId, Peer>,
    signal_seq: u32,
    spatial_mixer: SpatialMixer,
}

impl Room {
    /// Create a new room with configuration.
    pub fn new(id: RoomId, local_peer_id: PeerId, config: RoomConfig) -> Self {
        Self {
            id,
            config,
            local_peer_id,
            peers: HashMap::new(),
            signal_seq: 0,
            spatial_mixer: SpatialMixer::new(),
        }
    }

    /// Get next signaling sequence number.
    fn next_seq(&mut self) -> u32 {
        let seq = self.signal_seq;
        self.signal_seq += 1;
        seq
    }

    /// Generate a join signal to broadcast to the room.
    pub fn join(&mut self, display_name: &str) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::join(
            self.local_peer_id.clone(),
            self.id.clone(),
            display_name,
            seq,
        )
    }

    /// Generate a leave signal.
    pub fn leave(&mut self) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::leave(self.local_peer_id.clone(), self.id.clone(), seq)
    }

    /// Process an incoming signaling message. Returns events to handle.
    pub fn process_signal(&mut self, msg: &SignalMessage) -> Vec<RoomEvent> {
        if msg.room_id != self.id {
            return Vec::new();
        }

        let mut events = Vec::new();

        match msg.signal_type {
            SignalType::Join => {
                let peer = Peer::new(msg.from.clone(), msg.payload.clone());
                self.peers.insert(msg.from.clone(), peer);
                events.push(RoomEvent::PeerJoined {
                    peer_id: msg.from.clone(),
                    display_name: msg.payload.clone(),
                });
            }
            SignalType::Leave => {
                self.peers.remove(&msg.from);
                events.push(RoomEvent::PeerLeft {
                    peer_id: msg.from.clone(),
                });
            }
            SignalType::Offer => {
                if let Some(peer) = self.peers.get_mut(&msg.from) {
                    peer.state = PeerState::Connecting;
                }
                events.push(RoomEvent::OfferReceived {
                    from: msg.from.clone(),
                    sdp: msg.payload.clone(),
                });
            }
            SignalType::Answer => {
                if let Some(peer) = self.peers.get_mut(&msg.from) {
                    peer.state = PeerState::Connected;
                    events.push(RoomEvent::PeerStateChanged {
                        peer_id: msg.from.clone(),
                        state: PeerState::Connected,
                    });
                }
                events.push(RoomEvent::AnswerReceived {
                    from: msg.from.clone(),
                    sdp: msg.payload.clone(),
                });
            }
            SignalType::IceCandidate => {
                events.push(RoomEvent::IceCandidateReceived {
                    from: msg.from.clone(),
                    candidate: msg.payload.clone(),
                });
            }
            SignalType::Position => {
                if let Ok(pos) = serde_json::from_str::<[f32; 3]>(&msg.payload) {
                    if let Some(peer) = self.peers.get_mut(&msg.from) {
                        peer.set_position(pos);
                    }
                    events.push(RoomEvent::PositionUpdated {
                        peer_id: msg.from.clone(),
                        position: pos,
                    });
                }
            }
            SignalType::Data => {
                events.push(RoomEvent::DataReceived {
                    from: msg.from.clone(),
                    data: msg.payload.clone(),
                });
            }
        }

        events
    }

    /// Create an SDP offer for a specific peer.
    pub fn create_offer(&mut self, to: PeerId, sdp: String) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::offer(self.local_peer_id.clone(), to, self.id.clone(), sdp, seq)
    }

    /// Create an SDP answer for a specific peer.
    pub fn create_answer(&mut self, to: PeerId, sdp: String) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::answer(self.local_peer_id.clone(), to, self.id.clone(), sdp, seq)
    }

    /// Create an ICE candidate message for a specific peer.
    pub fn create_ice_candidate(&mut self, to: PeerId, candidate: String) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::ice_candidate(
            self.local_peer_id.clone(),
            to,
            self.id.clone(),
            candidate,
            seq,
        )
    }

    /// Broadcast local position for spatial audio.
    pub fn update_position(&mut self, pos: [f32; 3]) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::position(self.local_peer_id.clone(), self.id.clone(), pos, seq)
    }

    /// Send data to all peers (cursor, annotation, reaction).
    pub fn send_data(&mut self, data_json: String) -> SignalMessage {
        let seq = self.next_seq();
        SignalMessage::data(self.local_peer_id.clone(), self.id.clone(), data_json, seq)
    }

    /// Get a peer by ID.
    pub fn peer(&self, id: &str) -> Option<&Peer> {
        self.peers.get(id)
    }

    /// Get mutable peer by ID.
    pub fn peer_mut(&mut self, id: &str) -> Option<&mut Peer> {
        self.peers.get_mut(id)
    }

    /// Get all peers.
    pub fn peers(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    /// Number of remote peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// IDs of all connected peers (for mesh offer initiation).
    pub fn connected_peer_ids(&self) -> Vec<PeerId> {
        self.peers
            .values()
            .filter(|p| p.state == PeerState::Connected)
            .map(|p| p.id.clone())
            .collect()
    }

    /// Run spatial audio spatialization for all peers.
    /// Returns (peer_id, left_vol, right_vol, pan) tuples for JS.
    pub fn spatialize(&mut self) -> Vec<(String, f32, f32, f32)> {
        if !self.config.spatial_audio {
            return Vec::new();
        }

        let peers: Vec<Peer> = self.peers.values().cloned().collect();
        self.spatial_mixer
            .spatialize_peers(&peers)
            .into_iter()
            .map(|(id, r)| (id.to_string(), r.left, r.right, r.pan))
            .collect()
    }

    /// Access the spatial mixer for configuration.
    pub fn spatial_mixer_mut(&mut self) -> &mut SpatialMixer {
        &mut self.spatial_mixer
    }

    /// Room summary as JSON (for /_app/meta or health endpoint).
    pub fn summary_json(&self) -> String {
        serde_json::to_string(&serde_json::json!({
            "room_id": self.id,
            "name": self.config.name,
            "peer_count": self.peer_count(),
            "topology": self.config.topology,
            "spatial_audio": self.config.spatial_audio,
            "local_peer": self.local_peer_id,
        }))
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_room() -> Room {
        Room::new(
            "test-room".into(),
            "local".into(),
            RoomConfig {
                name: "Test Room".into(),
                ..Default::default()
            },
        )
    }

    #[test]
    fn join_and_leave() {
        let mut room = test_room();

        // Simulate peer joining
        let join_msg = SignalMessage::join("alice".into(), "test-room".into(), "Alice", 0);
        let events = room.process_signal(&join_msg);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], RoomEvent::PeerJoined { .. }));
        assert_eq!(room.peer_count(), 1);

        // Simulate peer leaving
        let leave_msg = SignalMessage::leave("alice".into(), "test-room".into(), 1);
        let events = room.process_signal(&leave_msg);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], RoomEvent::PeerLeft { .. }));
        assert_eq!(room.peer_count(), 0);
    }

    #[test]
    fn sdp_exchange() {
        let mut room = test_room();

        // Alice joins
        let join_msg = SignalMessage::join("alice".into(), "test-room".into(), "Alice", 0);
        room.process_signal(&join_msg);

        // Alice sends offer
        let offer_msg = SignalMessage::offer(
            "alice".into(),
            "local".into(),
            "test-room".into(),
            "sdp-offer".into(),
            1,
        );
        let events = room.process_signal(&offer_msg);
        assert!(matches!(events[0], RoomEvent::OfferReceived { .. }));

        // Local creates answer
        let answer = room.create_answer("alice".into(), "sdp-answer".into());
        assert_eq!(answer.signal_type, SignalType::Answer);
        assert_eq!(answer.to, "alice");
    }

    #[test]
    fn position_update() {
        let mut room = test_room();

        // Alice joins
        let join_msg = SignalMessage::join("alice".into(), "test-room".into(), "Alice", 0);
        room.process_signal(&join_msg);

        // Alice updates position
        let pos_msg =
            SignalMessage::position("alice".into(), "test-room".into(), [3.0, 0.0, -2.0], 1);
        let events = room.process_signal(&pos_msg);
        assert!(matches!(events[0], RoomEvent::PositionUpdated { .. }));

        let alice = room.peer("alice").unwrap();
        assert!((alice.position[0] - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn wrong_room_ignored() {
        let mut room = test_room();
        let msg = SignalMessage::join("alice".into(), "other-room".into(), "Alice", 0);
        let events = room.process_signal(&msg);
        assert!(events.is_empty());
    }

    #[test]
    fn signal_seq_increments() {
        let mut room = test_room();
        let m1 = room.join("Local");
        let m2 = room.leave();
        assert_eq!(m1.seq, 0);
        assert_eq!(m2.seq, 1);
    }

    #[test]
    fn summary_json() {
        let room = test_room();
        let json = room.summary_json();
        assert!(json.contains("test-room"));
        assert!(json.contains("Test Room"));
    }
}
