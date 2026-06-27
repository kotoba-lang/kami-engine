//! Lighting designer.
//!
//! Stage lighting is a function of the beat. Fixtures live at fixed
//! truss positions; the [`LightingDesigner`] owns a stack of [`LightingCue`]s
//! and per-tick produces a [`LightingFrame`] (color + intensity + direction
//! per fixture) that the renderer consumes.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::beat::{BeatEvent, BeatPhase};

/// Where a luminaire physically lives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LightingFixture {
    /// Front wash (audience-facing PAR can / LED bar).
    FrontPar,
    /// Back wash behind the performer.
    BackPar,
    /// Top-down spot from truss above stage centre.
    Spot,
    /// Side blinder.
    Blinder,
    /// Moving-head laser. Direction derived from beat-driven sweep.
    Laser,
    /// Strobe (white flash).
    Strobe,
}

impl LightingFixture {
    pub fn all() -> [LightingFixture; 6] {
        [
            LightingFixture::FrontPar,
            LightingFixture::BackPar,
            LightingFixture::Spot,
            LightingFixture::Blinder,
            LightingFixture::Laser,
            LightingFixture::Strobe,
        ]
    }
}

/// Cue = "for the next N bars play this look on these fixtures".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingCue {
    pub fixture: LightingFixture,
    pub color: [f32; 3],
    /// Peak intensity in [0,1].
    pub intensity: f32,
    /// How the intensity moves with the beat.
    pub envelope: Envelope,
    /// Total duration in bars; cue auto-pops when expired.
    pub bars: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Envelope {
    /// Constant `intensity`.
    Hold,
    /// Sharp attack on each beat, decays exp over the beat.
    Pulse { decay: f32 },
    /// Sine breathing (1 cycle per bar).
    Breathe,
    /// Strobe — full on for `duty` of each 8th, off otherwise.
    Strobe { duty: f32 },
    /// Linear ramp from 0 to peak across `bars`.
    Ramp,
}

/// Per-fixture render-ready output for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingFrame {
    pub fixture: LightingFixture,
    pub color: [f32; 3],
    pub intensity: f32,
    /// Aim direction (for moving heads / lasers). Unit vector. World space.
    pub aim: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct LightingDesigner {
    /// Active cues (newest last; later cues override earlier on the
    /// same fixture). Index into start_bar tracks when the cue began.
    cues: Vec<(LightingCue, u32)>,
    laser_phase: f32,
}

impl LightingDesigner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a cue starting at the given bar index.
    pub fn push(&mut self, cue: LightingCue, start_bar: u32) {
        self.cues.push((cue, start_bar));
    }

    /// Advance internal state. The laser direction is driven by 8th-note events.
    pub fn on_event(&mut self, ev: BeatEvent) {
        if let BeatEvent::Eighth { eighth_index, .. } = ev {
            // Walk the laser sweep deterministically by eighth count so
            // every viewer sees the same beam direction.
            self.laser_phase = (eighth_index as f32 * 0.37) % std::f32::consts::TAU;
        }
    }

    /// Drop expired cues.
    pub fn prune(&mut self, current_bar: u32) {
        self.cues
            .retain(|(c, start)| current_bar < start.saturating_add(c.bars));
    }

    /// Resolve to a frame per fixture. Latest matching cue wins.
    pub fn resolve(&self, phase: BeatPhase) -> Vec<LightingFrame> {
        let mut out = Vec::with_capacity(LightingFixture::all().len());
        for f in LightingFixture::all() {
            let cue = self.cues.iter().rev().find(|(c, start)| {
                c.fixture == f && phase.bar >= *start && phase.bar < start + c.bars
            });
            let frame = match cue {
                None => LightingFrame {
                    fixture: f,
                    color: [0.05, 0.05, 0.07], // dim ambient
                    intensity: 0.05,
                    aim: default_aim(f, self.laser_phase),
                },
                Some((c, start)) => {
                    let env_amp = envelope_amp(c.envelope, phase, *start, c.bars);
                    LightingFrame {
                        fixture: f,
                        color: c.color,
                        intensity: c.intensity * env_amp,
                        aim: default_aim(f, self.laser_phase),
                    }
                }
            };
            out.push(frame);
        }
        out
    }
}

fn envelope_amp(env: Envelope, phase: BeatPhase, start_bar: u32, bars: u32) -> f32 {
    match env {
        Envelope::Hold => 1.0,
        Envelope::Pulse { decay } => {
            // Re-trigger every beat. amp = exp(-decay * beat_frac).
            (-decay * phase.beat_frac).exp()
        }
        Envelope::Breathe => {
            let t = phase.bar_frac * std::f32::consts::TAU;
            0.5 + 0.5 * t.sin()
        }
        Envelope::Strobe { duty } => {
            // 8 eighths per bar of 4 beats. fire if 8th-frac < duty.
            let frac = (phase.bar_frac * 8.0).fract();
            if frac < duty { 1.0 } else { 0.0 }
        }
        Envelope::Ramp => {
            let total = (bars.max(1)) as f32;
            let elapsed = (phase.bar.saturating_sub(start_bar)) as f32 + phase.bar_frac;
            (elapsed / total).clamp(0.0, 1.0)
        }
    }
}

fn default_aim(f: LightingFixture, laser_phase: f32) -> Vec3 {
    match f {
        LightingFixture::FrontPar => Vec3::new(0.0, -0.2, -1.0).normalize(),
        LightingFixture::BackPar => Vec3::new(0.0, -0.2, 1.0).normalize(),
        LightingFixture::Spot => Vec3::new(0.0, -1.0, 0.0),
        LightingFixture::Blinder => Vec3::new(0.0, 0.0, -1.0),
        LightingFixture::Strobe => Vec3::new(0.0, -0.5, -1.0).normalize(),
        LightingFixture::Laser => {
            let (s, c) = laser_phase.sin_cos();
            Vec3::new(s * 0.7, -0.6, -c).normalize()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat::BeatGrid;

    #[test]
    fn cue_active_only_in_window() {
        let mut d = LightingDesigner::new();
        d.push(
            LightingCue {
                fixture: LightingFixture::FrontPar,
                color: [1.0, 0.2, 0.3],
                intensity: 1.0,
                envelope: Envelope::Hold,
                bars: 4,
            },
            2, // starts at bar 2
        );
        // bar 1 → ambient
        let p = BeatPhase {
            time: 0.0,
            beat: 4,
            bar: 1,
            phrase: 0,
            beat_frac: 0.0,
            bar_frac: 0.0,
        };
        let f = d.resolve(p);
        let front = f
            .iter()
            .find(|x| x.fixture == LightingFixture::FrontPar)
            .unwrap();
        assert!(front.intensity < 0.1, "ambient before cue start");

        // bar 3 → in window
        let p2 = BeatPhase { bar: 3, ..p };
        let f2 = d.resolve(p2);
        let front2 = f2
            .iter()
            .find(|x| x.fixture == LightingFixture::FrontPar)
            .unwrap();
        assert!((front2.intensity - 1.0).abs() < 1e-5);

        // bar 6 → expired
        let p3 = BeatPhase { bar: 6, ..p };
        let f3 = d.resolve(p3);
        let front3 = f3
            .iter()
            .find(|x| x.fixture == LightingFixture::FrontPar)
            .unwrap();
        assert!(front3.intensity < 0.1, "expired");
    }

    #[test]
    fn pulse_envelope_resets_on_beat() {
        let mut d = LightingDesigner::new();
        d.push(
            LightingCue {
                fixture: LightingFixture::Strobe,
                color: [1.0; 3],
                intensity: 1.0,
                envelope: Envelope::Pulse { decay: 5.0 },
                bars: 4,
            },
            0,
        );
        let on_beat = BeatPhase {
            time: 0.0,
            beat: 0,
            bar: 0,
            phrase: 0,
            beat_frac: 0.0,
            bar_frac: 0.0,
        };
        let mid_beat = BeatPhase {
            beat_frac: 0.5,
            bar_frac: 0.125,
            ..on_beat
        };
        let f0 = d.resolve(on_beat);
        let f1 = d.resolve(mid_beat);
        let i0 = f0
            .iter()
            .find(|f| f.fixture == LightingFixture::Strobe)
            .unwrap()
            .intensity;
        let i1 = f1
            .iter()
            .find(|f| f.fixture == LightingFixture::Strobe)
            .unwrap()
            .intensity;
        assert!(i0 > i1, "pulse decays after beat");
    }

    #[test]
    fn laser_phase_advances_with_eighth_events() {
        let mut d = LightingDesigner::new();
        let p0 = d.laser_phase;
        d.on_event(BeatEvent::Eighth {
            time: 0.0,
            eighth_index: 7,
        });
        let p1 = d.laser_phase;
        assert!((p1 - p0).abs() > 1e-3);
    }

    #[test]
    fn prune_drops_expired() {
        let mut d = LightingDesigner::new();
        d.push(
            LightingCue {
                fixture: LightingFixture::Spot,
                color: [1.0; 3],
                intensity: 1.0,
                envelope: Envelope::Hold,
                bars: 2,
            },
            0,
        );
        assert_eq!(d.cues.len(), 1);
        d.prune(2);
        assert_eq!(d.cues.len(), 0);
    }

    #[test]
    fn integrates_with_beatgrid() {
        let mut g = BeatGrid::new(120.0);
        let mut d = LightingDesigner::new();
        d.push(
            LightingCue {
                fixture: LightingFixture::Laser,
                color: [0.0, 0.6, 1.0],
                intensity: 0.9,
                envelope: Envelope::Hold,
                bars: 8,
            },
            0,
        );
        g.tick(2.0);
        for ev in g.drain_events() {
            d.on_event(ev);
        }
        let frames = d.resolve(g.phase());
        let laser = frames
            .iter()
            .find(|f| f.fixture == LightingFixture::Laser)
            .unwrap();
        assert!((laser.intensity - 0.9).abs() < 1e-5);
    }
}
