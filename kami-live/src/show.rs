//! `LiveShow` — top-level façade that holds every subsystem.
//!
//! ```ignore
//! use kami_live::{LiveShow, StagePreset, Track, TrackId, CuePoint, CueKind};
//!
//! let mut show = LiveShow::builder()
//!     .stage(StagePreset::Hall)
//!     .bpm(128.0)
//!     .build();
//! show.setlist_mut().push(Track {
//!     id: TrackId(1),
//!     title: "Opener".into(),
//!     bpm: 128.0,
//!     length_beats: 128,
//!     cues: vec![CuePoint { at_beat: 32, kind: CueKind::Drop, tag: "drop".into() }],
//!     dance: Some("wota".into()),
//! });
//! show.start();
//!
//! loop {
//!     let events = show.tick(1.0 / 60.0);
//!     for e in events {
//!         /* dispatch to renderer / audio engine */
//!     }
//! }
//! ```

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::audio::AudioCue;
use crate::beat::{BeatEvent, BeatGrid, BeatPhase};
use crate::cheer::{CheerAggregate, CheerKind, CheerSample};
use crate::crowd::{Crowd, CrowdConfig, FanMood, FanSnapshot};
use crate::lighting::{LightingDesigner, LightingFrame};
use crate::performer::{DanceMove, DancePose, Performer};
use crate::setlist::{CueKind, CuePoint, Setlist};
use crate::stage::{Stage, StagePreset, StageZone};
use crate::vj::{VJDeck, VJFrame};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShowEvent {
    /// New track started at the given show-time.
    TrackChanged { index: usize, title: String, bpm: f32 },
    /// A cue point fired.
    Cue { track_index: usize, cue: CuePoint },
    /// Beat-grid event (re-emitted for downstream consumers).
    Beat(BeatEvent),
    /// Audio synthesis cue (drum hit / bass note / pad swap / stop).
    Audio(AudioCue),
    /// End of setlist.
    SetEnded,
}

/// Per-frame snapshot for the renderer.
#[derive(Debug, Clone)]
pub struct ShowSnapshot {
    pub phase: BeatPhase,
    pub current_track: Option<usize>,
    pub performer_pose: DancePose,
    pub lighting: Vec<LightingFrame>,
    pub vj: VJFrame,
    pub crowd: Vec<FanSnapshot>,
    /// Loudness from cheer aggregator. Use as bloom / particle multiplier.
    pub cheer_loudness: f32,
}

pub struct LiveShow {
    grid: BeatGrid,
    setlist: Setlist,
    stage: Stage,
    performer: Performer,
    lighting: LightingDesigner,
    vj: VJDeck,
    crowd: Crowd,
    cheers: CheerAggregate,
    /// Track index → last consumed cue beat.
    last_cue_beat: Vec<u32>,
    /// Track index → last consumed bass-note beat (track-local).
    last_bass_beat: Vec<u32>,
    /// Last reported track index for change detection.
    last_track: Option<usize>,
    started: bool,
    /// Master mute (e.g. while loading audio buffer).
    pub muted: bool,
}

impl LiveShow {
    pub fn builder() -> LiveShowBuilder {
        LiveShowBuilder::default()
    }

    pub fn setlist_mut(&mut self) -> &mut Setlist {
        &mut self.setlist
    }
    pub fn setlist(&self) -> &Setlist {
        &self.setlist
    }
    pub fn stage(&self) -> &Stage {
        &self.stage
    }
    pub fn performer(&self) -> &Performer {
        &self.performer
    }
    pub fn performer_mut(&mut self) -> &mut Performer {
        &mut self.performer
    }
    pub fn crowd(&self) -> &Crowd {
        &self.crowd
    }
    pub fn lighting_mut(&mut self) -> &mut LightingDesigner {
        &mut self.lighting
    }
    pub fn cheers_mut(&mut self) -> &mut CheerAggregate {
        &mut self.cheers
    }
    pub fn grid(&self) -> &BeatGrid {
        &self.grid
    }

    pub fn start(&mut self) {
        self.started = true;
        self.last_cue_beat = vec![0; self.setlist.tracks.len()];
        self.last_bass_beat = vec![0; self.setlist.tracks.len()];
        self.last_track = None;
    }

    /// Advance the show by `dt` seconds. Returns the discrete events that
    /// fired during the tick.
    pub fn tick(&mut self, dt: f32) -> Vec<ShowEvent> {
        if !self.started {
            return Vec::new();
        }
        let mut events = Vec::new();

        // 1) advance grid
        let prev_phase = self.grid.phase();
        self.grid.tick(dt);
        let phase = self.grid.phase();
        let drained = self.grid.drain_events();
        // Pre-resolve which track is active right now so audio cues
        // route to the correct AudioPattern.
        let active_idx = self.setlist.locate(phase.time).map(|(i, _)| i);
        for ev in drained {
            self.lighting.on_event(ev);
            events.push(ShowEvent::Beat(ev));
            if let Some(idx) = active_idx {
                if let Some(pat) = self.setlist.tracks[idx].audio.clone() {
                    self.emit_audio_for_event(idx, &pat, ev, &mut events);
                }
            }
        }

        // 2) track / cue dispatch
        let now = phase.time;
        match self.setlist.locate(now) {
            Some((idx, _local_seconds)) => {
                let track = &self.setlist.tracks[idx];
                if self.last_track != Some(idx) {
                    events.push(ShowEvent::TrackChanged {
                        index: idx,
                        title: track.title.clone(),
                        bpm: track.bpm,
                    });
                    if let Some(name) = &track.dance {
                        self.performer.set_move(DanceMove::by_name(name));
                    }
                    // On track change, emit a Pad swap so the JS synth
                    // can fade to the new chord. Empty chord = silence.
                    if let Some(pat) = &track.audio {
                        let mut chord = [0u8; 5];
                        for (i, m) in pat.pad_chord.iter().take(5).enumerate() {
                            chord[i] = *m;
                        }
                        events.push(ShowEvent::Audio(AudioCue::Pad {
                            at_time: now,
                            midis: chord,
                        }));
                    } else {
                        // Stop any synth that was running for the previous track.
                        events.push(ShowEvent::Audio(AudioCue::Stop { at_time: now }));
                    }
                    self.last_track = Some(idx);
                }
                let track_start_seconds: f32 = self.setlist.tracks[..idx]
                    .iter()
                    .map(|t| t.duration_seconds())
                    .sum();
                let local_t = now - track_start_seconds;
                let local_beat = (local_t * track.bpm / 60.0).max(0.0) as u32;
                let prev_local_beat = self.last_cue_beat[idx];
                let cues_owned: Vec<CuePoint> = self
                    .setlist
                    .cues_between(idx, prev_local_beat, local_beat)
                    .to_vec();
                for cue in cues_owned {
                    self.handle_cue(idx, &cue);
                    events.push(ShowEvent::Cue {
                        track_index: idx,
                        cue,
                    });
                }
                self.last_cue_beat[idx] = local_beat;
            }
            None => {
                if self.last_track.is_some() {
                    self.last_track = None;
                    events.push(ShowEvent::Audio(AudioCue::Stop { at_time: now }));
                    events.push(ShowEvent::SetEnded);
                }
            }
        }

        self.lighting.prune(phase.bar);
        let _ = prev_phase; // currently unused — reserved for future fade logic
        self.cheers.evict(now);
        events
    }

    fn emit_audio_for_event(
        &mut self,
        track_idx: usize,
        pat: &crate::audio::AudioPattern,
        ev: BeatEvent,
        out: &mut Vec<ShowEvent>,
    ) {
        match ev {
            BeatEvent::Eighth { time, eighth_index } => {
                if let Some(d) = &pat.drums {
                    let step = (eighth_index as usize) % 8;
                    for (slot, vel) in d.hits_at(step) {
                        out.push(ShowEvent::Audio(AudioCue::Drum {
                            at_time: time,
                            slot,
                            velocity: vel,
                        }));
                    }
                }
            }
            BeatEvent::Beat { time, beat_index } => {
                // Convert global beat to track-local for bass lookup.
                let track_start_seconds: f32 = self.setlist.tracks[..track_idx]
                    .iter()
                    .map(|t| t.duration_seconds())
                    .sum();
                let local_t = time - track_start_seconds;
                if local_t < 0.0 {
                    return;
                }
                let local_beat = (local_t * self.setlist.tracks[track_idx].bpm / 60.0) as u32;
                let prev = self.last_bass_beat[track_idx];
                if let Some(b) = &pat.bass {
                    for note in b.between(prev, local_beat) {
                        out.push(ShowEvent::Audio(AudioCue::Note {
                            at_time: time,
                            midi: note.pitch_midi,
                            velocity: note.velocity,
                            duration_beats: note.length_beats,
                        }));
                    }
                }
                // Lead arp: one note per beat, looping over `lead_arp`.
                if !pat.lead_arp.is_empty() {
                    let lead_idx = (beat_index as usize) % pat.lead_arp.len();
                    out.push(ShowEvent::Audio(AudioCue::Note {
                        at_time: time,
                        midi: pat.lead_arp[lead_idx],
                        velocity: 0.5,
                        duration_beats: 0.45,
                    }));
                }
                self.last_bass_beat[track_idx] = local_beat;
            }
            _ => {}
        }
    }

    fn handle_cue(&mut self, _track_idx: usize, cue: &CuePoint) {
        match cue.kind {
            CueKind::Drop => {
                self.crowd.set_mood_all(FanMood::Jump);
                let pit = self.stage.zone(StageZone::Pit).cloned();
                self.crowd.react(CheerKind::Jump, |p| {
                    pit.as_ref().map(|z| z.contains(p)).unwrap_or(false)
                });
            }
            CueKind::Breakdown => {
                self.crowd.set_mood_all(FanMood::Sway);
            }
            CueKind::Callout => {
                self.crowd.set_mood_all(FanMood::Hush);
            }
            CueKind::Custom => {}
        }
    }

    /// Build the per-frame snapshot. Also advances mutable subsystems
    /// (crowd snapshot, vj easing).
    pub fn snapshot(&mut self) -> ShowSnapshot {
        let phase = self.grid.phase();
        let pose = self.performer.pose(phase.beat_frac, phase.bar_frac);
        let lighting = self.lighting.resolve(phase);
        let loudness = self.cheers.loudness();
        let target = (loudness * 0.05).clamp(0.0, 1.0).max(0.4);
        let vj = self.vj.frame(phase, target);
        let crowd = self.crowd.snapshot(phase);
        ShowSnapshot {
            phase,
            current_track: self.last_track,
            performer_pose: pose,
            lighting,
            vj,
            crowd,
            cheer_loudness: loudness,
        }
    }

    /// Convenience: ingest a cheer from XRPC.
    pub fn ingest_cheer(&mut self, kind: CheerKind, weight: f32) {
        let now = self.grid.phase().time;
        self.cheers.push(CheerSample {
            at: now,
            kind,
            weight: weight.max(0.0),
        });
        let pit = self.stage.zone(StageZone::Pit).cloned();
        self.crowd.react(kind, |p| {
            pit.as_ref().map(|z| z.contains(p)).unwrap_or(false)
        });
    }
}

#[derive(Debug, Clone)]
pub struct LiveShowBuilder {
    bpm: f32,
    stage_preset: StagePreset,
    crowd_cfg: CrowdConfig,
    performer_name: String,
    program: Option<VJDeck>,
    swing: f32,
    beats_per_bar: u32,
    bars_per_phrase: u32,
}

impl Default for LiveShowBuilder {
    fn default() -> Self {
        Self {
            bpm: 128.0,
            stage_preset: StagePreset::Hall,
            crowd_cfg: CrowdConfig::default(),
            performer_name: "Mitama".into(),
            program: None,
            swing: 0.0,
            beats_per_bar: 4,
            bars_per_phrase: 8,
        }
    }
}

impl LiveShowBuilder {
    pub fn bpm(mut self, bpm: f32) -> Self {
        self.bpm = bpm;
        self
    }
    pub fn stage(mut self, p: StagePreset) -> Self {
        self.stage_preset = p;
        self
    }
    pub fn crowd(mut self, c: CrowdConfig) -> Self {
        self.crowd_cfg = c;
        self
    }
    pub fn performer_name(mut self, n: impl Into<String>) -> Self {
        self.performer_name = n.into();
        self
    }
    pub fn vj_deck(mut self, deck: VJDeck) -> Self {
        self.program = Some(deck);
        self
    }
    /// Swing factor in [-0.5, 0.5] for the master grid (groove on off-beat 8ths).
    pub fn swing(mut self, swing: f32) -> Self {
        self.swing = swing;
        self
    }
    /// Time signature: `beats_per_bar` (e.g. 4) and `bars_per_phrase` (e.g. 8).
    pub fn meter(mut self, beats_per_bar: u32, bars_per_phrase: u32) -> Self {
        self.beats_per_bar = beats_per_bar.max(1);
        self.bars_per_phrase = bars_per_phrase.max(1);
        self
    }
    pub fn build(self) -> LiveShow {
        let stage = self.stage_preset.build();
        let perf_home = stage
            .zone(StageZone::Performer)
            .map(|z| z.centre)
            .unwrap_or(Vec3::Y);
        let performer = Performer::new(self.performer_name, perf_home);
        let crowd = Crowd::new(self.crowd_cfg, &stage);
        let vj = self.program.unwrap_or_else(VJDeck::default_program);
        LiveShow {
            grid: BeatGrid::new(self.bpm)
                .with_meter(self.beats_per_bar, self.bars_per_phrase)
                .with_swing(self.swing),
            setlist: Setlist::new(),
            stage,
            performer,
            lighting: LightingDesigner::new(),
            vj,
            crowd,
            cheers: CheerAggregate::new(2.0),
            last_cue_beat: Vec::new(),
            last_bass_beat: Vec::new(),
            last_track: None,
            started: false,
            muted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::{Envelope, LightingCue, LightingFixture};
    use crate::setlist::{Track, TrackId};

    fn make_show() -> LiveShow {
        let mut s = LiveShow::builder()
            .bpm(120.0)
            .stage(StagePreset::Club)
            .crowd(CrowdConfig {
                fans_target: 50,
                ..CrowdConfig::default()
            })
            .build();
        s.setlist_mut().push(Track {
            id: TrackId(1),
            title: "Opener".into(),
            bpm: 120.0,
            length_beats: 64, // 32s
            cues: vec![
                CuePoint {
                    at_beat: 16,
                    kind: CueKind::Drop,
                    tag: "drop".into(),
                },
                CuePoint {
                    at_beat: 32,
                    kind: CueKind::Breakdown,
                    tag: "bd".into(),
                },
            ],
            dance: Some("wota".into()),
            audio: None,
        });
        s.lighting_mut().push(
            LightingCue {
                fixture: LightingFixture::FrontPar,
                color: [1.0, 0.5, 0.3],
                intensity: 0.9,
                envelope: Envelope::Hold,
                bars: 16,
            },
            0,
        );
        s.start();
        s
    }

    #[test]
    fn track_change_event_fires_once() {
        let mut s = make_show();
        let evs = s.tick(0.1);
        let first_change = evs
            .iter()
            .filter(|e| matches!(e, ShowEvent::TrackChanged { index: 0, .. }))
            .count();
        assert_eq!(first_change, 1);
        let evs2 = s.tick(0.1);
        let again = evs2
            .iter()
            .filter(|e| matches!(e, ShowEvent::TrackChanged { .. }))
            .count();
        assert_eq!(again, 0, "no further track-change for the same track");
    }

    #[test]
    fn cue_fires_at_correct_beat() {
        let mut s = make_show();
        // Drop is at beat 16 of a 120 BPM track => 8.0s.
        // Drive in 0.5s steps.
        let mut saw_drop = false;
        for _ in 0..30 {
            let evs = s.tick(0.5);
            if evs.iter().any(|e| matches!(e, ShowEvent::Cue { cue, .. } if matches!(cue.kind, CueKind::Drop))) {
                saw_drop = true;
                break;
            }
        }
        assert!(saw_drop);
    }

    #[test]
    fn snapshot_has_lighting_and_crowd() {
        let mut s = make_show();
        s.tick(0.1);
        let snap = s.snapshot();
        assert!(!snap.lighting.is_empty());
        assert!(!snap.crowd.is_empty());
        let front = snap
            .lighting
            .iter()
            .find(|l| matches!(l.fixture, LightingFixture::FrontPar))
            .unwrap();
        assert!(front.intensity > 0.5);
    }

    #[test]
    fn ingest_cheer_lifts_loudness() {
        let mut s = make_show();
        s.tick(0.1);
        let before = s.snapshot().cheer_loudness;
        for _ in 0..50 {
            s.ingest_cheer(CheerKind::Yell, 1.0);
        }
        let after = s.snapshot().cheer_loudness;
        assert!(after > before + 10.0);
    }

    #[test]
    fn set_ended_event_after_setlist_done() {
        let mut s = make_show();
        // 32s opener; tick past end.
        for _ in 0..70 {
            s.tick(0.5);
        }
        let evs = s.tick(0.5);
        let ended = evs.iter().any(|e| matches!(e, ShowEvent::SetEnded));
        // Will already have fired in earlier tick; only assert one show end exists overall.
        // Re-run from start to capture the boundary deterministically.
        let mut s2 = make_show();
        let mut found = false;
        for _ in 0..80 {
            let e = s2.tick(0.5);
            if e.iter().any(|x| matches!(x, ShowEvent::SetEnded)) {
                found = true;
                break;
            }
        }
        assert!(found || ended);
    }

    #[test]
    fn audio_pattern_emits_drums_and_bass() {
        use crate::audio::{AudioPattern, DrumSlot};
        let mut s = LiveShow::builder()
            .bpm(120.0)
            .stage(StagePreset::Club)
            .crowd(CrowdConfig {
                fans_target: 10,
                ..CrowdConfig::default()
            })
            .build();
        s.setlist_mut().push(Track {
            id: TrackId(1),
            title: "Audio Test".into(),
            bpm: 120.0,
            length_beats: 32,
            cues: vec![],
            dance: None,
            audio: Some(AudioPattern::opener()),
        });
        s.start();
        // Drive 1 second = 2 beats → expect kicks + hats + bass note (at beat 0).
        let mut drums = 0;
        let mut notes = 0;
        let mut pads = 0;
        let mut kick_seen = false;
        for _ in 0..30 {
            let evs = s.tick(1.0 / 30.0);
            for e in evs {
                match e {
                    ShowEvent::Audio(AudioCue::Drum { slot, .. }) => {
                        drums += 1;
                        if matches!(slot, DrumSlot::Kick) {
                            kick_seen = true;
                        }
                    }
                    ShowEvent::Audio(AudioCue::Note { .. }) => notes += 1,
                    ShowEvent::Audio(AudioCue::Pad { .. }) => pads += 1,
                    _ => {}
                }
            }
        }
        assert!(drums >= 4, "expected several drum hits, got {drums}");
        assert!(kick_seen, "expected kick drum from four-on-floor");
        assert!(notes >= 2, "expected bass + lead arp notes, got {notes}");
        assert_eq!(pads, 1, "expected exactly one pad swap on track-start");
    }

    #[test]
    fn track_dance_auto_selected() {
        let mut s = make_show();
        s.tick(0.05);
        // wota was set as the track's dance.
        assert!(matches!(s.performer().current, DanceMove::Wota));
    }
}
