//! Media track types and state.
//!
//! Abstracts audio/video/screen-share tracks with mute/active lifecycle.

use serde::{Deserialize, Serialize};

/// Unique track identifier.
pub type TrackId = String;

/// Media track kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Audio,
    Video,
    ScreenShare,
}

/// Track lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackState {
    /// Track is sending/receiving data.
    Active,
    /// Track is muted (exists but not sending).
    Muted,
    /// Track ended (removed from connection).
    Ended,
}

/// Local media constraints for getUserMedia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConstraints {
    pub audio: bool,
    pub video: bool,
    /// Preferred video width (0 = default).
    pub video_width: u32,
    /// Preferred video height (0 = default).
    pub video_height: u32,
    /// Preferred frame rate (0 = default).
    pub frame_rate: u32,
}

impl Default for MediaConstraints {
    fn default() -> Self {
        Self {
            audio: true,
            video: true,
            video_width: 640,
            video_height: 480,
            frame_rate: 30,
        }
    }
}

impl MediaConstraints {
    /// Audio-only constraints (voice call).
    pub fn audio_only() -> Self {
        Self {
            audio: true,
            video: false,
            ..Default::default()
        }
    }

    /// HD video constraints.
    pub fn hd_video() -> Self {
        Self {
            audio: true,
            video: true,
            video_width: 1280,
            video_height: 720,
            frame_rate: 30,
        }
    }
}

/// Track statistics snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_lost: u32,
    pub jitter_ms: f32,
    pub round_trip_ms: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_constraints() {
        let c = MediaConstraints::default();
        assert!(c.audio);
        assert!(c.video);
        assert_eq!(c.video_width, 640);
    }

    #[test]
    fn audio_only_constraints() {
        let c = MediaConstraints::audio_only();
        assert!(c.audio);
        assert!(!c.video);
    }
}
