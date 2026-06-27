//! Audio — Web Audio synthesis driven by the beat grid.
//!
//! Per-track [`AudioPattern`]s describe the music **declaratively**:
//! a 16-step drum pattern + a bass line. The show emits [`AudioCue`]
//! events on each beat / eighth so the renderer's JS bridge can fire
//! `kamiPlayDrum` / `kamiPlaySynth` Web Audio calls.
//!
//! The project's audio rule (`Web Audio synthesis only, no audio files`)
//! is satisfied because nothing here references samples — every cue is
//! a (slot, midi, duration) triple the synth fabricates from oscillators.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrumSlot {
    Kick = 0,
    Snare = 1,
    ClosedHat = 2,
    OpenHat = 3,
    Clap = 4,
    Crash = 5,
    Tom = 6,
    Rim = 7,
}

impl DrumSlot {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Kick,
            1 => Self::Snare,
            2 => Self::ClosedHat,
            3 => Self::OpenHat,
            4 => Self::Clap,
            5 => Self::Crash,
            6 => Self::Tom,
            7 => Self::Rim,
            _ => return None,
        })
    }
}

/// 16-step drum pattern. One bar of 4/4 = 16 sixteenth-notes; we trigger
/// at the 8-th note resolution so even-indexed steps fire on the beat
/// fraction and odd indices fire on the off-beat.
///
/// `velocity` is 0..1 per step. Step velocity 0.0 = no hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrumPattern {
    pub steps: [[f32; 8]; 8], // [DrumSlot index][step in 8ths-per-bar]
}

impl DrumPattern {
    pub fn empty() -> Self {
        Self {
            steps: [[0.0; 8]; 8],
        }
    }

    pub fn set(&mut self, slot: DrumSlot, step: usize, velocity: f32) -> &mut Self {
        let s = step.min(7);
        self.steps[slot as usize][s] = velocity.clamp(0.0, 1.0);
        self
    }

    /// Four-on-floor kick + back-beat snare + 8-th hats. The standard
    /// "default groove".
    pub fn four_on_floor() -> Self {
        let mut p = Self::empty();
        for s in 0..8 {
            // Kicks on every quarter-note (steps 0/2/4/6 in 8th grid).
            if s % 2 == 0 {
                p.set(DrumSlot::Kick, s, 1.0);
            }
            // Hats on every 8th.
            p.set(DrumSlot::ClosedHat, s, 0.6);
        }
        // Snare on beats 2 and 4 (8th-step indices 2 and 6).
        p.set(DrumSlot::Snare, 2, 0.95);
        p.set(DrumSlot::Snare, 6, 0.95);
        p
    }

    /// Sparser ballad pattern: kick + clap on backbeat, no hats.
    pub fn ballad() -> Self {
        let mut p = Self::empty();
        p.set(DrumSlot::Kick, 0, 0.9);
        p.set(DrumSlot::Kick, 4, 0.9);
        p.set(DrumSlot::Clap, 2, 0.85);
        p.set(DrumSlot::Clap, 6, 0.85);
        p
    }

    /// Fast K-pop / EDM: kick on every step + open hat on offbeats.
    pub fn pumping() -> Self {
        let mut p = Self::empty();
        for s in 0..8 {
            if s % 2 == 0 {
                p.set(DrumSlot::Kick, s, 1.0);
            } else {
                p.set(DrumSlot::OpenHat, s, 0.7);
            }
        }
        p.set(DrumSlot::Snare, 2, 1.0);
        p.set(DrumSlot::Snare, 6, 1.0);
        p
    }

    /// Drum hit at a given 8th-step index (0..7), or None if silent.
    pub fn hits_at(&self, step: usize) -> impl Iterator<Item = (DrumSlot, f32)> + '_ {
        let s = step % 8;
        (0..8).filter_map(move |slot| {
            let v = self.steps[slot][s];
            if v > 0.0 {
                Some((DrumSlot::from_u8(slot as u8).unwrap(), v))
            } else {
                None
            }
        })
    }
}

/// One bass note. `pitch_midi` follows MIDI convention (60 = middle C).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BassNote {
    pub at_beat: u32,
    pub pitch_midi: u8,
    /// Length in beats (e.g. 0.5 = 8th note, 1.0 = quarter).
    pub length_beats: f32,
    /// 0..1 velocity.
    pub velocity: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BassLine {
    pub notes: Vec<BassNote>,
}

impl BassLine {
    /// Default I-V-vi-IV root pattern in C minor (root notes only,
    /// half-bar each). Suitable as a placeholder bass for any 4/4 track.
    pub fn root_pattern_c_minor() -> Self {
        // C2 = 36, G2 = 43, A♭2 = 44, F2 = 41 (Cm-Gm-Abmaj-Fm root motion).
        let pitches = [36u8, 43, 44, 41];
        let mut notes = Vec::with_capacity(16);
        for bar in 0..4u32 {
            // Each pitch held for one bar = 4 beats.
            notes.push(BassNote {
                at_beat: bar * 4,
                pitch_midi: pitches[bar as usize % pitches.len()],
                length_beats: 3.5,
                velocity: 0.85,
            });
        }
        Self { notes }
    }

    pub fn empty() -> Self {
        Self { notes: Vec::new() }
    }

    /// Notes whose `at_beat` falls within `(prev_beat, cur_beat]`.
    pub fn between(&self, prev_beat: u32, cur_beat: u32) -> impl Iterator<Item = &BassNote> {
        self.notes
            .iter()
            .filter(move |n| n.at_beat > prev_beat && n.at_beat <= cur_beat)
    }
}

/// Per-track audio program.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioPattern {
    pub drums: Option<DrumPattern>,
    pub bass: Option<BassLine>,
    /// Lead arpeggio MIDI notes; played one per quarter note, looping.
    pub lead_arp: Vec<u8>,
    /// Pad chord MIDI notes; held for the whole track.
    pub pad_chord: Vec<u8>,
}

impl AudioPattern {
    pub fn opener() -> Self {
        Self {
            drums: Some(DrumPattern::four_on_floor()),
            bass: Some(BassLine::root_pattern_c_minor()),
            // C minor arp: C-Eb-G-Bb (notes 60, 63, 67, 70).
            lead_arp: vec![60, 63, 67, 70],
            // Cm9 pad: C-Eb-G-Bb-D (60, 63, 67, 70, 74).
            pad_chord: vec![60, 63, 67, 70, 74],
        }
    }

    pub fn ballad() -> Self {
        Self {
            drums: Some(DrumPattern::ballad()),
            bass: Some(BassLine::root_pattern_c_minor()),
            lead_arp: vec![],
            pad_chord: vec![55, 60, 63, 67], // G-C-Eb-G
        }
    }

    pub fn encore() -> Self {
        Self {
            drums: Some(DrumPattern::pumping()),
            bass: Some(BassLine::root_pattern_c_minor()),
            lead_arp: vec![60, 67, 72, 67, 63, 67, 70, 67],
            pad_chord: vec![60, 63, 67, 70, 75],
        }
    }
}

/// Discrete audio events emitted from `LiveShow::tick`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AudioCue {
    /// A drum hit. `at_time` = show-time seconds.
    Drum {
        at_time: f32,
        slot: DrumSlot,
        velocity: f32,
    },
    /// A bass / melodic note.
    Note {
        at_time: f32,
        midi: u8,
        velocity: f32,
        duration_beats: f32,
    },
    /// Pad swap (chord change) — JS layer fades to the new chord.
    Pad { at_time: f32, midis: [u8; 5] },
    /// Stop everything (e.g. on track-end / mute).
    Stop { at_time: f32 },
}

/// Convert a MIDI note to Hz (12-TET, A4=440).
pub fn midi_to_hz(midi: u8) -> f32 {
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drum_pattern_four_on_floor_kicks_every_quarter() {
        let p = DrumPattern::four_on_floor();
        let kicks: Vec<usize> = (0..8)
            .filter(|&s| p.steps[DrumSlot::Kick as usize][s] > 0.0)
            .collect();
        assert_eq!(kicks, vec![0, 2, 4, 6]);
    }

    #[test]
    fn drum_pattern_back_beat_snare() {
        let p = DrumPattern::four_on_floor();
        assert!(p.steps[DrumSlot::Snare as usize][2] > 0.0);
        assert!(p.steps[DrumSlot::Snare as usize][6] > 0.0);
        assert_eq!(p.steps[DrumSlot::Snare as usize][0], 0.0);
    }

    #[test]
    fn drum_hits_at_returns_active_slots() {
        let p = DrumPattern::four_on_floor();
        let hits: Vec<DrumSlot> = p.hits_at(0).map(|(s, _)| s).collect();
        assert!(hits.contains(&DrumSlot::Kick));
        assert!(hits.contains(&DrumSlot::ClosedHat));
        assert!(!hits.contains(&DrumSlot::Snare));
    }

    #[test]
    fn bassline_between_is_open_closed() {
        let b = BassLine::root_pattern_c_minor();
        let in_window: Vec<u32> = b.between(3, 8).map(|n| n.at_beat).collect();
        // notes at beat 0, 4, 8, 12 → window (3, 8] catches 4 and 8.
        assert_eq!(in_window, vec![4, 8]);
    }

    #[test]
    fn midi_to_hz_a4_is_440() {
        assert!((midi_to_hz(69) - 440.0).abs() < 0.01);
        assert!((midi_to_hz(60) - 261.626).abs() < 0.1);
    }

    #[test]
    fn audio_pattern_presets_are_distinct() {
        let o = AudioPattern::opener();
        let b = AudioPattern::ballad();
        let e = AudioPattern::encore();
        assert!(o.drums.is_some() && b.drums.is_some() && e.drums.is_some());
        assert_ne!(o.lead_arp, e.lead_arp);
        assert_ne!(o.pad_chord, b.pad_chord);
    }
}

/// A Web-Audio cue recipe — the same shape as the EDN-driven `kami.audio` CLJS
/// player (`{:wave :freq :to :dur :gain}`): a tiny data recipe synthesised with
/// no asset files. The dance projects each [`AudioCue`] / `:sound` into one of
/// these so the browser plays the show with the same data-driven soundscape.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundCue {
    /// Oscillator waveform: `"sine"` / `"square"` / `"triangle"` / `"sawtooth"`.
    pub wave: String,
    /// Start frequency (Hz).
    pub freq: f32,
    /// Optional sweep-to frequency (Hz) over `dur`.
    pub to: Option<f32>,
    /// Duration (seconds).
    pub dur: f32,
    /// Peak gain.
    pub gain: f32,
}

impl SoundCue {
    fn new(wave: &str, freq: f32, to: Option<f32>, dur: f32, gain: f32) -> Self {
        Self { wave: wave.into(), freq, to, dur, gain }
    }
}

impl DrumSlot {
    /// The sound-bank entry name for this drum slot.
    pub fn bank_name(self) -> &'static str {
        match self {
            DrumSlot::Kick => "kick",
            DrumSlot::Snare => "snare",
            DrumSlot::ClosedHat => "closed-hat",
            DrumSlot::OpenHat => "open-hat",
            DrumSlot::Clap => "clap",
            DrumSlot::Crash => "crash",
            DrumSlot::Tom => "tom",
            DrumSlot::Rim => "rim",
        }
    }
}
