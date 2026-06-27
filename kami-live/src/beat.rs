//! Beat grid — the master clock that everything else syncs to.
//!
//! BPM → seconds per beat → bar/phrase phase. Deterministic so multiple
//! clients can render the same show in lock-step from a shared `(bpm, t0)`.

use serde::{Deserialize, Serialize};

/// One position on the grid.
///
/// `time` is wall-clock seconds since show start (`t0`).
/// `beat` increments by 1 per quarter-note. `bar` = 4 beats by default.
/// `phrase` = 8 bars (Western pop / J-pop song convention — drop on phrase 0).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatPhase {
    pub time: f32,
    pub beat: u32,
    pub bar: u32,
    pub phrase: u32,
    /// 0..1 fractional position within current beat (used by lighting LFOs / dance ease).
    pub beat_frac: f32,
    /// 0..1 fractional position within current bar.
    pub bar_frac: f32,
}

/// Discrete events emitted when crossing a grid line during `tick`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BeatEvent {
    /// Sub-beat 8th-note tick (used by hi-hat strobes).
    Eighth { time: f32, eighth_index: u32 },
    /// Quarter-note kick. The everyday "beat".
    Beat { time: f32, beat_index: u32 },
    /// Bar line (default every 4 beats). Lighting cues usually fire here.
    Bar { time: f32, bar_index: u32 },
    /// Phrase boundary (default every 8 bars). VJ pattern + camera switch.
    Phrase { time: f32, phrase_index: u32 },
}

/// Master clock.
#[derive(Debug, Clone)]
pub struct BeatGrid {
    pub bpm: f32,
    pub beats_per_bar: u32,
    pub bars_per_phrase: u32,
    /// Show-internal time in seconds. Advanced by `tick`.
    t: f32,
    /// Last reported phase (so `tick` can detect crossings).
    last: BeatPhase,
    /// Swing factor in [-0.5, 0.5]: shifts even 8ths back/forward for groove.
    pub swing: f32,
    /// Held events flushed on next `drain_events`.
    pending: Vec<BeatEvent>,
}

impl BeatGrid {
    pub fn new(bpm: f32) -> Self {
        assert!(bpm > 0.0, "bpm must be positive");
        Self {
            bpm,
            beats_per_bar: 4,
            bars_per_phrase: 8,
            t: 0.0,
            last: BeatPhase {
                time: 0.0,
                beat: 0,
                bar: 0,
                phrase: 0,
                beat_frac: 0.0,
                bar_frac: 0.0,
            },
            swing: 0.0,
            pending: Vec::new(),
        }
    }

    pub fn with_meter(mut self, beats_per_bar: u32, bars_per_phrase: u32) -> Self {
        assert!(beats_per_bar > 0 && bars_per_phrase > 0);
        self.beats_per_bar = beats_per_bar;
        self.bars_per_phrase = bars_per_phrase;
        self
    }

    pub fn with_swing(mut self, swing: f32) -> Self {
        self.swing = swing.clamp(-0.5, 0.5);
        self
    }

    /// Seconds per beat at current BPM.
    #[inline]
    pub fn beat_seconds(&self) -> f32 {
        60.0 / self.bpm
    }

    /// Current phase (computed from `t`).
    pub fn phase(&self) -> BeatPhase {
        self.compute_phase(self.t)
    }

    fn compute_phase(&self, t: f32) -> BeatPhase {
        let spb = self.beat_seconds();
        let total_beats_f = (t / spb).max(0.0);
        let beat = total_beats_f as u32;
        let beat_frac = total_beats_f - beat as f32;
        let bar = beat / self.beats_per_bar;
        let beat_in_bar = beat % self.beats_per_bar;
        let bar_frac = (beat_in_bar as f32 + beat_frac) / self.beats_per_bar as f32;
        let phrase = bar / self.bars_per_phrase;
        BeatPhase {
            time: t,
            beat,
            bar,
            phrase,
            beat_frac,
            bar_frac,
        }
    }

    /// Advance the clock by `dt` seconds. Records crossing events; drain
    /// with [`drain_events`].
    pub fn tick(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let prev = self.last;
        let next_t = self.t + dt;
        let next = self.compute_phase(next_t);

        // Eighth-note crossings within (prev.time, next.time].
        let spb = self.beat_seconds();
        let eighth = spb * 0.5;
        let mut e_idx = (prev.time / eighth).floor() as i64 + 1;
        loop {
            let mut t_e = e_idx as f32 * eighth;
            // Apply swing: even 8ths (e_idx odd within beat = the "and") shift forward.
            if e_idx > 0 && e_idx % 2 == 1 {
                t_e += self.swing * eighth;
            }
            if t_e <= prev.time || t_e > next_t {
                if t_e > next_t {
                    break;
                }
                e_idx += 1;
                continue;
            }
            self.pending.push(BeatEvent::Eighth {
                time: t_e,
                eighth_index: e_idx as u32,
            });
            e_idx += 1;
        }

        if next.beat > prev.beat {
            for b in (prev.beat + 1)..=next.beat {
                self.pending.push(BeatEvent::Beat {
                    time: b as f32 * spb,
                    beat_index: b,
                });
            }
        }
        if next.bar > prev.bar {
            for b in (prev.bar + 1)..=next.bar {
                self.pending.push(BeatEvent::Bar {
                    time: (b * self.beats_per_bar) as f32 * spb,
                    bar_index: b,
                });
            }
        }
        if next.phrase > prev.phrase {
            for p in (prev.phrase + 1)..=next.phrase {
                self.pending.push(BeatEvent::Phrase {
                    time: (p * self.bars_per_phrase * self.beats_per_bar) as f32 * spb,
                    phrase_index: p,
                });
            }
        }

        self.t = next_t;
        self.last = next;
    }

    pub fn drain_events(&mut self) -> Vec<BeatEvent> {
        std::mem::take(&mut self.pending)
    }

    /// Reset the grid to t=0. BPM/meter retained.
    pub fn rewind(&mut self) {
        self.t = 0.0;
        self.last = BeatPhase {
            time: 0.0,
            beat: 0,
            bar: 0,
            phrase: 0,
            beat_frac: 0.0,
            bar_frac: 0.0,
        };
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_at_120_bpm_is_half_second() {
        let g = BeatGrid::new(120.0);
        assert!((g.beat_seconds() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn tick_emits_one_beat_per_500ms_at_120() {
        let mut g = BeatGrid::new(120.0);
        g.tick(0.5);
        let evts = g.drain_events();
        assert!(matches!(evts[0], BeatEvent::Eighth { .. }));
        assert!(
            evts.iter()
                .any(|e| matches!(e, BeatEvent::Beat { beat_index: 1, .. }))
        );
    }

    #[test]
    fn bar_and_phrase_fire_on_boundary() {
        let mut g = BeatGrid::new(120.0); // 4/4, 8 bars/phrase → phrase = 16s
        g.tick(16.0);
        let evts = g.drain_events();
        let bars = evts
            .iter()
            .filter(|e| matches!(e, BeatEvent::Bar { .. }))
            .count();
        let phrases = evts
            .iter()
            .filter(|e| matches!(e, BeatEvent::Phrase { .. }))
            .count();
        assert_eq!(bars, 8, "8 bars in 16s at 120 bpm");
        assert_eq!(phrases, 1, "phrase fires at the 16s boundary");
    }

    #[test]
    fn deterministic_replay() {
        let mut a = BeatGrid::new(128.0);
        let mut b = BeatGrid::new(128.0);
        for _ in 0..600 {
            a.tick(1.0 / 60.0);
            b.tick(1.0 / 60.0);
        }
        assert_eq!(a.phase().beat, b.phase().beat);
        assert!((a.phase().bar_frac - b.phase().bar_frac).abs() < 1e-5);
    }

    #[test]
    fn swing_shifts_offbeat_eighths() {
        let mut straight = BeatGrid::new(120.0);
        let mut swung = BeatGrid::new(120.0).with_swing(0.25);
        // Drive a full beat. The "and-of-1" eighth should be later when swung.
        straight.tick(0.51);
        swung.tick(0.51);
        let s_e = straight
            .drain_events()
            .into_iter()
            .find_map(|e| match e {
                BeatEvent::Eighth {
                    time,
                    eighth_index: 1,
                } => Some(time),
                _ => None,
            })
            .unwrap();
        let w_e = swung
            .drain_events()
            .into_iter()
            .find_map(|e| match e {
                BeatEvent::Eighth {
                    time,
                    eighth_index: 1,
                } => Some(time),
                _ => None,
            })
            .unwrap();
        assert!(w_e > s_e, "swung eighth should arrive later");
    }
}
