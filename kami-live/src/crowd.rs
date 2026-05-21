//! Crowd — instanced fan agents that react to the show.
//!
//! Lightweight CPU sim. Each fan has a position, a baseline mood, and
//! a y-offset driven by the beat (jumps on Pit/Floor/drop). The renderer
//! consumes [`FanSnapshot`] (position + body height + colour) per tick.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::beat::BeatPhase;
use crate::cheer::CheerKind;
use crate::stage::{Stage, StageZone, ZoneBox};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FanMood {
    /// Default — head-bob.
    Bob,
    /// Drop — full jump on the beat.
    Jump,
    /// Breakdown — sway side-to-side, lightstick raised.
    Sway,
    /// Listening intently — minimal motion.
    Hush,
}

#[derive(Debug, Clone, Copy)]
pub struct Fan {
    pub home: Vec3,
    /// 0..1 phase offset so adjacent fans don't move in identical lockstep.
    pub phase_offset: f32,
    /// Lightstick colour (palette pick).
    pub stick_color: [f32; 3],
    /// 0..1 reactive level — peaked by cheers.
    pub energy: f32,
    pub mood: FanMood,
}

#[derive(Debug, Clone, Copy)]
pub struct FanSnapshot {
    pub position: Vec3,
    pub stick_color: [f32; 3],
    pub stick_raised: bool,
    pub body_height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CrowdConfig {
    /// Total fans across all zones. Will be clamped to `cap`.
    pub fans_target: usize,
    pub cap: usize,
    /// Density bias: how many in pit vs floor (0=floor only, 1=pit only).
    pub pit_bias: f32,
    /// rng seed for deterministic placement.
    pub seed: u32,
}

impl Default for CrowdConfig {
    fn default() -> Self {
        Self {
            fans_target: 600,
            cap: 4096,
            pit_bias: 0.65,
            seed: 1,
        }
    }
}

/// Instanced crowd.
#[derive(Debug, Clone)]
pub struct Crowd {
    pub fans: Vec<Fan>,
    cfg: CrowdConfig,
    /// Energy decays each tick toward baseline 0.2.
    decay: f32,
}

impl Crowd {
    pub fn new(cfg: CrowdConfig, stage: &Stage) -> Self {
        let mut fans = Vec::with_capacity(cfg.fans_target.min(cfg.cap));
        let mut rng = SplitMix64::new(cfg.seed as u64);
        let pit = stage.zone(StageZone::Pit);
        let floor = stage.zone(StageZone::Floor);
        let palette: [[f32; 3]; 6] = [
            [1.0, 0.4, 0.5],
            [0.4, 0.7, 1.0],
            [1.0, 0.85, 0.3],
            [0.5, 1.0, 0.6],
            [0.85, 0.5, 1.0],
            [1.0, 1.0, 1.0],
        ];
        for _ in 0..cfg.fans_target.min(cfg.cap) {
            let pick = rng.next_f32();
            let zone = match (pit, floor) {
                (Some(p), Some(f)) => {
                    if pick < cfg.pit_bias {
                        p
                    } else {
                        f
                    }
                }
                (Some(p), None) => p,
                (None, Some(f)) => f,
                (None, None) => continue,
            };
            let home = sample_point(zone, &mut rng);
            fans.push(Fan {
                home,
                phase_offset: rng.next_f32(),
                stick_color: palette[(rng.next_u32() as usize) % palette.len()],
                energy: 0.25,
                mood: FanMood::Bob,
            });
        }
        Self {
            fans,
            cfg,
            decay: 0.95,
        }
    }

    pub fn config(&self) -> &CrowdConfig {
        &self.cfg
    }

    /// Switch every fan to a new mood (e.g. on Drop / Breakdown cue).
    pub fn set_mood_all(&mut self, mood: FanMood) {
        for f in self.fans.iter_mut() {
            f.mood = mood;
        }
    }

    /// Apply a discrete cheer event to every fan in `zones`.
    pub fn react(&mut self, kind: CheerKind, zones_of: impl Fn(Vec3) -> bool) {
        let bump = match kind {
            CheerKind::Clap => 0.15,
            CheerKind::Yell => 0.30,
            CheerKind::LightStick => 0.10,
            CheerKind::Jump => 0.50,
        };
        for f in self.fans.iter_mut() {
            if zones_of(f.home) {
                f.energy = (f.energy + bump).min(1.0);
                if matches!(kind, CheerKind::Jump) {
                    f.mood = FanMood::Jump;
                }
            }
        }
    }

    /// Per-frame render snapshot. Pure function of `(fan, phase)`.
    pub fn snapshot(&mut self, phase: BeatPhase) -> Vec<FanSnapshot> {
        // Decay energy toward baseline after each snapshot call.
        let mut out = Vec::with_capacity(self.fans.len());
        for f in self.fans.iter_mut() {
            // Phase-shifted beat fraction so fans don't all move identically.
            let p = (phase.beat_frac + f.phase_offset).fract();
            let height = match f.mood {
                FanMood::Bob => 0.05 * (p * std::f32::consts::TAU).sin().abs(),
                FanMood::Jump => {
                    // Short jump on the beat: 0..0.4 ramps up + ease-out.
                    if p < 0.4 {
                        let q = p / 0.4;
                        0.6 * (1.0 - (1.0 - q).powi(2))
                    } else {
                        0.0
                    }
                }
                FanMood::Sway => {
                    // Side-to-side: small Y oscillation, big phase shift expected on x.
                    0.02 * (p * std::f32::consts::TAU).cos()
                }
                FanMood::Hush => 0.0,
            };
            out.push(FanSnapshot {
                position: f.home + Vec3::Y * height,
                stick_color: f.stick_color,
                stick_raised: matches!(f.mood, FanMood::Sway | FanMood::Jump) || f.energy > 0.6,
                body_height: 1.6,
            });
            f.energy = (f.energy * self.decay).max(0.2);
        }
        out
    }
}

fn sample_point(b: &ZoneBox, rng: &mut SplitMix64) -> Vec3 {
    let dx = (rng.next_f32() * 2.0 - 1.0) * b.half_size.x;
    let dz = (rng.next_f32() * 2.0 - 1.0) * b.half_size.z;
    Vec3::new(b.centre.x + dx, b.centre.y, b.centre.z + dz)
}

/// SplitMix64 — small, fast, fully deterministic. Used so the same
/// seed yields the same crowd layout across clients.
#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }
    fn next_u64(&mut self) -> u64 {
        let mut z = self.state;
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::StagePreset;

    #[test]
    fn deterministic_layout() {
        let stage = StagePreset::Hall.build();
        let cfg = CrowdConfig {
            fans_target: 200,
            cap: 4096,
            pit_bias: 0.5,
            seed: 42,
        };
        let a = Crowd::new(cfg, &stage);
        let b = Crowd::new(cfg, &stage);
        assert_eq!(a.fans.len(), b.fans.len());
        for (fa, fb) in a.fans.iter().zip(b.fans.iter()) {
            assert!((fa.home - fb.home).length() < 1e-5);
        }
    }

    #[test]
    fn cap_clamps_fan_count() {
        let stage = StagePreset::Festival.build();
        let cfg = CrowdConfig {
            fans_target: 99_999,
            cap: 100,
            pit_bias: 0.5,
            seed: 1,
        };
        let c = Crowd::new(cfg, &stage);
        assert_eq!(c.fans.len(), 100);
    }

    #[test]
    fn snapshot_height_varies_in_jump_mood() {
        let stage = StagePreset::Club.build();
        let mut c = Crowd::new(
            CrowdConfig {
                fans_target: 30,
                ..CrowdConfig::default()
            },
            &stage,
        );
        c.set_mood_all(FanMood::Jump);
        let phase = BeatPhase {
            time: 0.0,
            beat: 0,
            bar: 0,
            phrase: 0,
            beat_frac: 0.1,
            bar_frac: 0.025,
        };
        let snap = c.snapshot(phase);
        let max_h = snap.iter().map(|s| s.position.y).fold(f32::MIN, f32::max);
        assert!(max_h > 0.0, "at least one fan in mid-jump");
    }

    #[test]
    fn react_pit_only_pumps_pit_fans() {
        let stage = StagePreset::Hall.build();
        let mut c = Crowd::new(CrowdConfig::default(), &stage);
        let pit = stage.zone(StageZone::Pit).cloned().unwrap();
        let before: Vec<f32> = c.fans.iter().map(|f| f.energy).collect();
        c.react(CheerKind::Yell, |p| pit.contains(p));
        let after: Vec<f32> = c.fans.iter().map(|f| f.energy).collect();
        let mut bumped = 0;
        for (i, f) in c.fans.iter().enumerate() {
            if pit.contains(f.home) && after[i] > before[i] {
                bumped += 1;
            }
        }
        assert!(bumped > 0, "pit fans should have been bumped");
    }
}
