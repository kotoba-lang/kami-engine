//! kami-rtc: WebRTC SDK for KAMI Engine.
//!
//! Provides room-based real-time communication with:
//!   - WebRTC peer connections (SDP offer/answer, ICE candidates)
//!   - Media track management (audio/video add/remove/mute)
//!   - Spatial audio integration (participant 3D positions → kami-audio)
//!   - KNP data channel for low-latency state sync (cursors, annotations, reactions)
//!   - Signaling protocol over KNP ReliableOrdered channel
//!
//! Architecture:
//!   Browser (WASM): web-sys RTCPeerConnection + MediaStream APIs
//!   State machine: platform-agnostic room/peer/track management
//!   KNP bridge: signaling + data sync over KAMI Network Protocol

pub mod media;
pub mod peer;
pub mod room;
pub mod signal;
pub mod spatial;

pub use media::{MediaKind, TrackId, TrackState};
pub use peer::{PeerId, PeerState};
pub use room::{Room, RoomConfig, RoomEvent, RoomId};
pub use signal::{SignalMessage, SignalType};
pub use spatial::SpatialMixer;
