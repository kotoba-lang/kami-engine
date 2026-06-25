//! Performer — the dancer on stage.
//!
//! Holds a small library of [`DanceMove`]s indexed by name. Each move
//! is a deterministic pose function f(beat_frac) -> pose. The actual
//! VRM skeleton drive lives in `kami-vrm` / `kami-skeleton`; this module
//! emits target [`DancePose`]s that the renderer can blend into the rig.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DancePose {
    /// World-space root translation (e.g. side-step, footwork drift).
    pub root_translation: Vec3,
    /// Root rotation around Y axis in radians.
    pub root_yaw: f32,
    /// Crouch / jump bias in metres. Positive = up.
    pub vertical_bob: f32,
    /// Arms-up amount in [0,1]. 0 = at-side, 1 = fully up.
    pub arms_up: f32,
    /// Spine sway in radians (Z-axis tilt).
    pub spine_sway: f32,
}

impl DancePose {
    pub fn rest() -> Self {
        Self {
            root_translation: Vec3::ZERO,
            root_yaw: 0.0,
            vertical_bob: 0.0,
            arms_up: 0.0,
            spine_sway: 0.0,
        }
    }

    /// Convert to a quaternion for the root yaw.
    pub fn root_quat(&self) -> Quat {
        Quat::from_rotation_y(self.root_yaw)
    }
}

/// A dance move = a pose-over-time pure function.
#[derive(Debug, Clone, Copy)]
pub enum DanceMove {
    /// Hands at side, gentle bob.
    Idle,
    /// 4-on-the-floor stomp on every beat.
    FourOnFloor,
    /// Arms up on bar 0, side-step on 2-4 (J-pop wota-style).
    Wota,
    /// K-pop point-choreo: arm flicks on quarter notes.
    KpopPoint,
    /// Two-step shuffle.
    Shuffle,
    /// Hold pose (used during ballad / breakdown).
    Hold,
    /// Springy two-feet bounce on every beat.
    Bounce,
    /// Side-to-side body sway (ballad groove).
    Sway,
    /// Continuous turn — one full rotation per bar.
    Spin,
    /// Sharp downward nod on each beat (rock head-bang).
    Headbang,
    /// Rhythmic arms-up clap on the off-beats.
    Clap,
}

impl DanceMove {
    /// Lookup by name. Unknown name → Idle.
    pub fn by_name(name: &str) -> Self {
        match name {
            "four-on-floor" => DanceMove::FourOnFloor,
            "wota" => DanceMove::Wota,
            "kpop-point" => DanceMove::KpopPoint,
            "shuffle" => DanceMove::Shuffle,
            "hold" => DanceMove::Hold,
            "bounce" => DanceMove::Bounce,
            "sway" => DanceMove::Sway,
            "spin" => DanceMove::Spin,
            "headbang" => DanceMove::Headbang,
            "clap" => DanceMove::Clap,
            _ => DanceMove::Idle,
        }
    }

    pub fn pose_at(&self, beat_frac: f32, bar_frac: f32) -> DancePose {
        let bf = beat_frac.clamp(0.0, 1.0);
        match self {
            DanceMove::Idle => DancePose {
                vertical_bob: 0.04 * (bf * std::f32::consts::TAU).sin(),
                ..DancePose::rest()
            },
            DanceMove::FourOnFloor => DancePose {
                vertical_bob: -0.12 * (1.0 - (1.0 - bf).powi(2)),
                arms_up: 0.2,
                ..DancePose::rest()
            },
            DanceMove::Wota => DancePose {
                vertical_bob: 0.05 * (bar_frac * std::f32::consts::TAU * 2.0).sin(),
                arms_up: if bar_frac < 0.25 { 0.95 } else { 0.4 },
                root_translation: Vec3::new(
                    0.15 * (bar_frac * std::f32::consts::TAU).sin(),
                    0.0,
                    0.0,
                ),
                ..DancePose::rest()
            },
            DanceMove::KpopPoint => {
                let arms = if bf < 0.4 { 0.9 } else { 0.4 };
                let yaw = 0.3 * (bar_frac * std::f32::consts::TAU).sin();
                DancePose {
                    arms_up: arms,
                    root_yaw: yaw,
                    spine_sway: 0.05 * (bf * std::f32::consts::TAU).cos(),
                    ..DancePose::rest()
                }
            }
            DanceMove::Shuffle => DancePose {
                root_translation: Vec3::new(
                    0.4 * (bar_frac * std::f32::consts::TAU).sin(),
                    0.0,
                    0.0,
                ),
                vertical_bob: 0.03 * (bf * std::f32::consts::TAU * 2.0).sin(),
                ..DancePose::rest()
            },
            DanceMove::Hold => DancePose::rest(),
            DanceMove::Bounce => DancePose {
                // crouch-and-spring: dips down then pops up each beat.
                vertical_bob: -0.12 * (bf * std::f32::consts::PI).sin().abs(),
                arms_up: 0.3,
                ..DancePose::rest()
            },
            DanceMove::Sway => DancePose {
                root_translation: Vec3::new(0.22 * (bar_frac * std::f32::consts::TAU).sin(), 0.0, 0.0),
                spine_sway: 0.16 * (bar_frac * std::f32::consts::TAU).sin(),
                vertical_bob: 0.03 * (bf * std::f32::consts::TAU).sin(),
                ..DancePose::rest()
            },
            DanceMove::Spin => DancePose {
                root_yaw: bar_frac * std::f32::consts::TAU, // one full turn per bar
                arms_up: 0.5,
                vertical_bob: 0.04 * (bf * std::f32::consts::TAU).sin(),
                ..DancePose::rest()
            },
            DanceMove::Headbang => DancePose {
                // sharp downward attack on the beat, recovering through it.
                vertical_bob: -0.16 * (1.0 - (1.0 - bf).powi(3)),
                spine_sway: 0.0,
                arms_up: 0.15,
                ..DancePose::rest()
            },
            DanceMove::Clap => DancePose {
                arms_up: 0.5 + 0.45 * (bf * std::f32::consts::TAU * 2.0).sin().abs(),
                vertical_bob: 0.03 * (bf * std::f32::consts::TAU).sin(),
                ..DancePose::rest()
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Performer {
    pub name: String,
    /// Currently active move.
    pub current: DanceMove,
    /// Where the performer stands (centre of the stage riser typically).
    pub home: Vec3,
}

impl Performer {
    pub fn new(name: impl Into<String>, home: Vec3) -> Self {
        Self {
            name: name.into(),
            current: DanceMove::Idle,
            home,
        }
    }

    /// Switch the active dance move.
    pub fn set_move(&mut self, m: DanceMove) {
        self.current = m;
    }

    /// Resolve the pose for a phase. Adds `home` translation.
    pub fn pose(&self, beat_frac: f32, bar_frac: f32) -> DancePose {
        let mut p = self.current.pose_at(beat_frac, bar_frac);
        p.root_translation += self.home;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_pose_is_zero() {
        let p = DancePose::rest();
        assert!(p.root_translation.length() < 1e-6);
        assert_eq!(p.arms_up, 0.0);
    }

    #[test]
    fn new_presets_resolve_and_pose() {
        // the added moves resolve by name and produce non-rest, distinct poses.
        for name in ["bounce", "sway", "spin", "headbang", "clap"] {
            let m = DanceMove::by_name(name);
            assert!(!matches!(m, DanceMove::Idle), "{name} resolves to its own move");
        }
        // spin turns through the bar; sway translates sideways; they differ.
        let spin = DanceMove::Spin.pose_at(0.5, 0.5);
        let sway = DanceMove::Sway.pose_at(0.5, 0.25);
        assert!(spin.root_yaw.abs() > 0.1, "spin yaws");
        assert!(sway.root_translation.x.abs() > 0.05, "sway steps sideways");
    }

    #[test]
    fn wota_lifts_arms_on_bar_start() {
        let m = DanceMove::Wota;
        let p_start = m.pose_at(0.0, 0.0);
        let p_mid = m.pose_at(0.5, 0.5);
        assert!(p_start.arms_up > p_mid.arms_up);
    }

    #[test]
    fn unknown_move_defaults_to_idle() {
        let p_unknown = DanceMove::by_name("???").pose_at(0.5, 0.5);
        let p_idle = DanceMove::Idle.pose_at(0.5, 0.5);
        assert!((p_unknown.vertical_bob - p_idle.vertical_bob).abs() < 1e-6);
    }

    #[test]
    fn performer_translates_pose_to_home() {
        let mut perf = Performer::new("test", Vec3::new(0.0, 1.2, 0.0));
        perf.set_move(DanceMove::Idle);
        let p = perf.pose(0.0, 0.0);
        assert!((p.root_translation.y - 1.2).abs() < 1e-5);
    }
}
