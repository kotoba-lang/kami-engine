//! Setlist — the show's table of contents.
//!
//! A `Setlist` owns N `Track`s laid end to end on the show timeline.
//! Each track carries its own BPM and key, plus optional `CuePoint`s
//! (drop, breakdown, callout) that other modules subscribe to.

use serde::{Deserialize, Serialize};

use crate::audio::AudioPattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub u32);

/// What kind of moment is this cue?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CueKind {
    /// Big drop — strobe + confetti + crowd jump.
    Drop,
    /// Quiet section — dim PARs, audience swaying lightsticks.
    Breakdown,
    /// Performer addresses crowd — spotlight + duck VJ.
    Callout,
    /// Custom marker — userland routes by `tag`.
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuePoint {
    /// Beat offset from the start of the *track* (not the show).
    pub at_beat: u32,
    pub kind: CueKind,
    /// Free-form tag. e.g. "phrase-0:drop", "callout:hello-tokyo".
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub title: String,
    pub bpm: f32,
    /// Length in beats (4 beats = 1 bar). Used to schedule the next track.
    pub length_beats: u32,
    /// Sorted ascending by `at_beat`.
    pub cues: Vec<CuePoint>,
    /// Optional: dance preset name for performer auto-selection.
    pub dance: Option<String>,
    /// Optional: per-track audio synthesis program. Drives the Web
    /// Audio bridge; omit for tracks with externally-mixed audio.
    #[serde(default)]
    pub audio: Option<AudioPattern>,
}

impl Track {
    pub fn duration_seconds(&self) -> f32 {
        (self.length_beats as f32) * 60.0 / self.bpm
    }
}

/// Linear playlist + lookup helpers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Setlist {
    pub tracks: Vec<Track>,
}

impl Setlist {
    pub fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    pub fn push(&mut self, mut t: Track) {
        t.cues.sort_by_key(|c| c.at_beat);
        self.tracks.push(t);
    }

    /// Locate the track that contains show-time `t` seconds.
    /// Returns `(index, track_local_seconds)` or None if past end.
    pub fn locate(&self, t: f32) -> Option<(usize, f32)> {
        let mut acc = 0.0;
        for (i, tr) in self.tracks.iter().enumerate() {
            let d = tr.duration_seconds();
            if t < acc + d {
                return Some((i, t - acc));
            }
            acc += d;
        }
        None
    }

    /// Total show length in seconds.
    pub fn duration_seconds(&self) -> f32 {
        self.tracks.iter().map(|t| t.duration_seconds()).sum()
    }

    /// Cues that fall within `(prev_local_beat, cur_local_beat]` for a
    /// given track index. Caller is responsible for tracking `prev` per track.
    pub fn cues_between(&self, track_idx: usize, prev_beat: u32, cur_beat: u32) -> &[CuePoint] {
        let cues = &self.tracks[track_idx].cues;
        let lo = cues.partition_point(|c| c.at_beat <= prev_beat);
        let hi = cues.partition_point(|c| c.at_beat <= cur_beat);
        &cues[lo..hi]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: u32, bpm: f32, beats: u32, cues: Vec<(u32, CueKind)>) -> Track {
        Track {
            id: TrackId(id),
            title: format!("track-{id}"),
            bpm,
            length_beats: beats,
            cues: cues
                .into_iter()
                .map(|(b, k)| CuePoint {
                    at_beat: b,
                    kind: k,
                    tag: String::new(),
                })
                .collect(),
            dance: None,
            audio: None,
        }
    }

    #[test]
    fn locate_returns_correct_track() {
        let mut s = Setlist::new();
        s.push(t(1, 120.0, 64, vec![])); // 32s
        s.push(t(2, 120.0, 64, vec![])); // 32s
        s.push(t(3, 60.0, 64, vec![])); // 64s
        assert_eq!(s.locate(0.0).unwrap().0, 0);
        assert_eq!(s.locate(31.9).unwrap().0, 0);
        assert_eq!(s.locate(32.0).unwrap().0, 1);
        assert_eq!(s.locate(63.0).unwrap().0, 1);
        assert_eq!(s.locate(64.5).unwrap().0, 2);
        assert!(s.locate(999.0).is_none());
    }

    #[test]
    fn cues_between_is_open_closed() {
        let mut s = Setlist::new();
        s.push(t(
            1,
            120.0,
            128,
            vec![
                (16, CueKind::Drop),
                (32, CueKind::Breakdown),
                (48, CueKind::Drop),
            ],
        ));
        let c = s.cues_between(0, 0, 16);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].at_beat, 16);
        let c = s.cues_between(0, 16, 48);
        assert_eq!(c.len(), 2); // 32 + 48
    }

    #[test]
    fn cues_sorted_after_push() {
        let mut s = Setlist::new();
        s.push(t(
            1,
            120.0,
            128,
            vec![
                (48, CueKind::Drop),
                (16, CueKind::Drop),
                (32, CueKind::Breakdown),
            ],
        ));
        let beats: Vec<u32> = s.tracks[0].cues.iter().map(|c| c.at_beat).collect();
        assert_eq!(beats, vec![16, 32, 48]);
    }
}
