//! Peer connection state machine.
//!
//! Tracks per-peer lifecycle: connecting → connected → disconnected.
//! Each peer has media tracks (audio/video) and a spatial position.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::media::{MediaKind, TrackId, TrackState};

/// Unique peer identifier (maps to DID or session ID).
pub type PeerId = String;

/// Peer connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerState {
    /// ICE/SDP negotiation in progress.
    Connecting,
    /// Media flowing, connection established.
    Connected,
    /// Connection lost or intentionally closed.
    Disconnected,
}

/// Remote media track attached to a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerTrack {
    pub id: TrackId,
    pub kind: MediaKind,
    pub state: TrackState,
}

/// A remote participant in the room.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: PeerId,
    pub display_name: String,
    pub state: PeerState,
    pub position: [f32; 3],
    pub tracks: Vec<PeerTrack>,
    /// Whether this peer's audio is spatially positioned.
    pub spatial_audio: bool,
}

impl Peer {
    /// Create a new peer in connecting state.
    pub fn new(id: PeerId, display_name: String) -> Self {
        Self {
            id,
            display_name,
            state: PeerState::Connecting,
            position: [0.0, 0.0, 0.0],
            tracks: Vec::new(),
            spatial_audio: true,
        }
    }

    /// Get the peer's 3D position as a Vec3.
    pub fn position_vec3(&self) -> Vec3 {
        Vec3::from_array(self.position)
    }

    /// Update peer position (for spatial audio).
    pub fn set_position(&mut self, pos: [f32; 3]) {
        self.position = pos;
    }

    /// Add a media track from this peer.
    pub fn add_track(&mut self, id: TrackId, kind: MediaKind) {
        self.tracks.push(PeerTrack {
            id,
            kind,
            state: TrackState::Active,
        });
    }

    /// Remove a media track.
    pub fn remove_track(&mut self, id: &str) -> Option<PeerTrack> {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id) {
            Some(self.tracks.remove(idx))
        } else {
            None
        }
    }

    /// Mute/unmute a track.
    pub fn set_track_state(&mut self, id: &str, state: TrackState) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            track.state = state;
        }
    }

    /// Check if peer has an active audio track.
    pub fn has_audio(&self) -> bool {
        self.tracks
            .iter()
            .any(|t| t.kind == MediaKind::Audio && t.state == TrackState::Active)
    }

    /// Check if peer has an active video track.
    pub fn has_video(&self) -> bool {
        self.tracks
            .iter()
            .any(|t| t.kind == MediaKind::Video && t.state == TrackState::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_lifecycle() {
        let mut peer = Peer::new("peer-1".into(), "Alice".into());
        assert_eq!(peer.state, PeerState::Connecting);
        assert!(!peer.has_audio());

        peer.add_track("audio-0".into(), MediaKind::Audio);
        peer.add_track("video-0".into(), MediaKind::Video);
        assert!(peer.has_audio());
        assert!(peer.has_video());

        peer.set_track_state("audio-0", TrackState::Muted);
        assert!(!peer.has_audio());

        peer.remove_track("video-0");
        assert!(!peer.has_video());
        assert_eq!(peer.tracks.len(), 1);
    }

    #[test]
    fn peer_spatial_position() {
        let mut peer = Peer::new("peer-2".into(), "Bob".into());
        peer.set_position([5.0, 0.0, -3.0]);
        let pos = peer.position_vec3();
        assert!((pos.x - 5.0).abs() < f32::EPSILON);
        assert!((pos.z - (-3.0)).abs() < f32::EPSILON);
    }
}
