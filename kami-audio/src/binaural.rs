//! Binaural spatialization — the native arm of `kami.binaural` (clj).
//!
//! The clj/edn side (`kami-engine-sdk-clj/src/kami/binaural.cljc`) is the brain:
//! it authors a listener + positioned sources as EDN and computes the same
//! parameters this module computes. This is the **native executor** that the
//! host (`kami-script-runtime`, iOS/Metal · Android · desktop) feeds with the
//! `set-listener!` pose + the `play-at` source queue.
//!
//! Physically-grounded *spherical-head* model — identical math to the clj side
//! so a recipe spatializes the same on web (Web Audio) and native:
//!   * ITD — Woodworth `itd = (a/c)(θ + sin θ)` on the lateral angle
//!     (front/back symmetric, elevation-aware).
//!   * ILD — frequency-independent head-shadow: the contralateral ear is
//!     attenuated by |ILD|.
//!   * distance — inverse / linear / exponential / none rolloff.

use crate::Listener;
use glam::Vec3;

/// Speed of sound in dry air ~20°C (m/s).
pub const SPEED_OF_SOUND: f32 = 343.0;
/// Standard adult head radius (m, ~KEMAR).
pub const DEFAULT_HEAD_RADIUS: f32 = 0.0875;

/// Head model parameters (matches clj `:binaural/hrtf`).
#[derive(Debug, Clone, Copy)]
pub struct Hrtf {
    pub head_radius: f32,
    pub max_ild_db: f32,
}

impl Default for Hrtf {
    fn default() -> Self {
        Self {
            head_radius: DEFAULT_HEAD_RADIUS,
            max_ild_db: 12.0,
        }
    }
}

/// Distance rolloff (matches clj `:binaural/rolloff`, OpenAL semantics).
#[derive(Debug, Clone, Copy)]
pub struct Rolloff {
    pub kind: RolloffKind,
    pub reference: f32,
    pub max: f32,
    pub factor: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloffKind {
    None,
    Inverse,
    Linear,
    Exponential,
}

impl Default for Rolloff {
    fn default() -> Self {
        Self {
            kind: RolloffKind::Inverse,
            reference: 1.0,
            max: 100.0,
            factor: 1.0,
        }
    }
}

impl Rolloff {
    /// Linear gain ∈ [0,1] for `dist`.
    pub fn gain(&self, dist: f32) -> f32 {
        let d = dist.clamp(self.reference, self.max);
        match self.kind {
            RolloffKind::None => 1.0,
            RolloffKind::Inverse => {
                self.reference / (self.reference + self.factor * (d - self.reference))
            }
            RolloffKind::Linear => (1.0
                - self.factor * (d - self.reference) / (self.max - self.reference).max(1e-6))
            .clamp(0.0, 1.0),
            RolloffKind::Exponential => (d / self.reference).powf(-self.factor),
        }
    }
}

/// Per-source binaural parameters (matches the clj `:spatial` map).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinauralParams {
    pub distance: f32,
    pub azimuth: f32,
    pub elevation: f32,
    pub lateral: f32,
    pub itd_s: f32,
    pub ild_db: f32,
    pub gain_l: f32,
    pub gain_r: f32,
    pub delay_l_s: f32,
    pub delay_r_s: f32,
}

impl BinauralParams {
    /// Integer ITD sample delays at `sample_rate` (the `:native` emit form).
    pub fn sample_delays(&self, sample_rate: u32) -> (u32, u32) {
        let sr = sample_rate as f32;
        (
            (self.delay_l_s * sr).round() as u32,
            (self.delay_r_s * sr).round() as u32,
        )
    }
}

/// Spatialize one source position against the listener — the core of the model.
/// `source_gain` folds in the source's own volume (∈ [0,1]).
pub fn spatialize(
    listener: &Listener,
    hrtf: &Hrtf,
    rolloff: &Rolloff,
    source_pos: Vec3,
    source_gain: f32,
) -> BinauralParams {
    // Orthonormal listener basis (robust to a non-orthogonal authored up).
    let forward = listener.forward.normalize_or_zero();
    let right = forward.cross(listener.up).normalize_or_zero();
    let up = right.cross(forward);

    let rel = source_pos - listener.position;
    let distance = rel.length();
    let dir = if distance > 1e-6 {
        rel / distance
    } else {
        Vec3::ZERO
    };

    let lateral = dir.dot(right).clamp(-1.0, 1.0); // +right
    let front = dir.dot(forward);
    let vert = dir.dot(up).clamp(-1.0, 1.0);

    let azimuth = lateral.atan2(front);
    let elevation = vert.asin();
    let theta = lateral.asin(); // lateral angle, front/back symmetric

    let itd_s = (hrtf.head_radius / SPEED_OF_SOUND) * (theta + theta.sin()); // +→right leads
    let ild_db = hrtf.max_ild_db * lateral; // +→right louder

    let dgain = rolloff.gain(distance) * source_gain;
    let shadow = 10f32.powf(-ild_db.abs() / 20.0); // contralateral attenuation
    let right_side = lateral >= 0.0;
    let gain_l = dgain * if right_side { shadow } else { 1.0 };
    let gain_r = dgain * if right_side { 1.0 } else { shadow };
    let delay_l_s = if itd_s >= 0.0 { itd_s } else { 0.0 }; // right leads → delay left
    let delay_r_s = if itd_s < 0.0 { -itd_s } else { 0.0 };

    BinauralParams {
        distance,
        azimuth,
        elevation,
        lateral,
        itd_s,
        ild_db,
        gain_l,
        gain_r,
        delay_l_s,
        delay_r_s,
    }
}

/// A spatialized voice ready to mix: a mono signal placed in 3D via `params`.
pub struct Voice<'a> {
    pub params: BinauralParams,
    pub mono: &'a [f32],
}

/// Software-mix spatialized `voices` into one interleaved stereo f32 block of
/// `frames` frames (`out[2i]` = left, `out[2i+1]` = right). Each voice's mono
/// signal is offset by its per-ear ITD sample delay and scaled by its per-ear
/// gain; samples past the block are dropped. This is the PCM the host hands to
/// the audio device (cpal / Web Audio / console mixer) — the device sink itself
/// is host wiring, kept out of this pure DSP so it stays unit-testable.
pub fn mix_stereo(voices: &[Voice], sample_rate: u32, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; frames * 2];
    for v in voices {
        let (dl, dr) = v.params.sample_delays(sample_rate);
        let (dl, dr) = (dl as usize, dr as usize);
        for (i, &s) in v.mono.iter().enumerate() {
            let li = i + dl;
            if li < frames {
                out[li * 2] += s * v.params.gain_l;
            }
            let ri = i + dr;
            if ri < frames {
                out[ri * 2 + 1] += s * v.params.gain_r;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_listener() -> Listener {
        Listener::default() // at origin, facing -Z, up +Y → +X is right
    }

    #[test]
    fn source_on_the_right_leads_and_is_louder() {
        let p = spatialize(
            &default_listener(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(5.0, 0.0, 0.0),
            1.0,
        );
        assert!(
            p.itd_s > 0.0,
            "right source → right ear leads (positive ITD)"
        );
        assert!(p.delay_l_s > 0.0 && p.delay_r_s == 0.0, "left ear delayed");
        assert!(p.ild_db > 0.0 && p.gain_r > p.gain_l, "right louder");
        assert!((p.azimuth - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn left_mirrors_right() {
        let l = default_listener();
        let r = spatialize(
            &l,
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(5.0, 0.0, 0.0),
            1.0,
        );
        let lft = spatialize(
            &l,
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(-5.0, 0.0, 0.0),
            1.0,
        );
        assert!((lft.itd_s + r.itd_s).abs() < 1e-6);
        assert!((lft.gain_l - r.gain_r).abs() < 1e-6);
        assert!((lft.gain_r - r.gain_l).abs() < 1e-6);
    }

    #[test]
    fn dead_ahead_is_centered() {
        let p = spatialize(
            &default_listener(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(0.0, 0.0, -5.0),
            1.0,
        );
        assert!(p.itd_s.abs() < 1e-6 && p.ild_db.abs() < 1e-6);
        assert!((p.gain_l - p.gain_r).abs() < 1e-6);
    }

    #[test]
    fn itd_within_physical_bound() {
        // ≈ a/c·(π/2+1) ≈ 0.66 ms for a 0.0875 m head.
        let p = spatialize(
            &default_listener(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(100.0, 0.0, 0.0),
            1.0,
        );
        assert!(p.itd_s > 5.0e-4 && p.itd_s < 7.0e-4);
    }

    #[test]
    fn sample_delays_are_integers_at_rate() {
        let p = spatialize(
            &default_listener(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(5.0, 0.0, 0.0),
            1.0,
        );
        let (dl, dr) = p.sample_delays(48_000);
        assert!(dl > 0 && dr == 0);
    }

    #[test]
    fn mix_places_impulse_per_ear_with_itd_and_gain() {
        // Right source: right ear at frame 0 (no delay), left ear delayed by ITD.
        let p = spatialize(
            &default_listener(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(5.0, 0.0, 0.0),
            1.0,
        );
        let (dl, dr) = p.sample_delays(48_000);
        assert!(dl > 0 && dr == 0);

        let impulse = [1.0f32];
        let out = mix_stereo(
            &[Voice {
                params: p,
                mono: &impulse,
            }],
            48_000,
            64,
        );

        // Right channel: impulse at frame 0 scaled by gain_r.
        assert!((out[1] - p.gain_r).abs() < 1e-6);
        // Left channel: impulse appears at the ITD-delayed frame scaled by gain_l.
        assert!((out[dl as usize * 2] - p.gain_l).abs() < 1e-6);
        // Left channel frame 0 is silent (it was delayed).
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn mix_sums_multiple_voices() {
        let p = spatialize(
            &default_listener(),
            &Hrtf::default(),
            &Rolloff::default(),
            Vec3::new(0.0, 0.0, -5.0),
            1.0,
        ); // dead ahead, centered, no delay
        let a = [0.5f32];
        let b = [0.25f32];
        let out = mix_stereo(
            &[
                Voice {
                    params: p,
                    mono: &a,
                },
                Voice {
                    params: p,
                    mono: &b,
                },
            ],
            48_000,
            8,
        );
        // Both centered & undelayed → frame-0 left = (0.5+0.25)*gain_l.
        assert!((out[0] - 0.75 * p.gain_l).abs() < 1e-6);
    }

    #[test]
    fn rolloff_is_unit_at_reference_and_decreasing() {
        for kind in [
            RolloffKind::Inverse,
            RolloffKind::Linear,
            RolloffKind::Exponential,
        ] {
            let r = Rolloff {
                kind,
                ..Default::default()
            };
            assert!((r.gain(1.0) - 1.0).abs() < 1e-6);
            assert!(r.gain(1.0) > r.gain(10.0));
        }
        assert!(
            (Rolloff {
                kind: RolloffKind::None,
                ..Default::default()
            }
            .gain(99.0)
                - 1.0)
                .abs()
                < 1e-6
        );
    }
}
