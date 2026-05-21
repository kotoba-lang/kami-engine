//! Nintendo-style animation system with smooth, bouncy, juicy motions.
//! Supports bobbing, spinning, squash/stretch, wobble, pop-in, head bob, and pulse glow.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Combined output from one or more animation clips.
#[derive(Debug, Clone, Copy)]
pub struct AnimationOutput {
    pub position_offset: Vec3,
    pub rotation_offset: Quat,
    pub scale_multiplier: Vec3,
}

impl Default for AnimationOutput {
    fn default() -> Self {
        Self {
            position_offset: Vec3::ZERO,
            rotation_offset: Quat::IDENTITY,
            scale_multiplier: Vec3::ONE,
        }
    }
}

impl AnimationOutput {
    /// Combine two outputs: additive position, multiplicative rotation, multiplicative scale.
    pub fn combine(self, other: &AnimationOutput) -> Self {
        Self {
            position_offset: self.position_offset + other.position_offset,
            rotation_offset: self.rotation_offset * other.rotation_offset,
            scale_multiplier: self.scale_multiplier * other.scale_multiplier,
        }
    }
}

/// Phase state for HeadBob animation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeadBobPhase {
    Rise,
    Hold,
    Drop,
    Wait,
}

/// Individual animation clip. Each variant holds its own state and parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnimationClip {
    Bobbing {
        amplitude: f32,
        frequency: f32,
        phase: f32,
    },
    Spinning {
        speed: f32,
        angle: f32,
    },
    SquashStretch {
        squash_scale: [f32; 3],
        stretch_scale: [f32; 3],
        duration: f32,
        timer: f32,
        active: bool,
    },
    Wobble {
        intensity: f32,
        speed: f32,
        phase: f32,
    },
    PopIn {
        target_scale: [f32; 3],
        duration: f32,
        timer: f32,
        overshoot: f32,
    },
    HeadBob {
        rise_height: f32,
        rise_time: f32,
        hold_time: f32,
        drop_time: f32,
        wait_time: f32,
        timer: f32,
        phase: HeadBobPhase,
    },
    PulseGlow {
        min_scale: f32,
        max_scale: f32,
        speed: f32,
        phase: f32,
    },
    /// Ohio-style glitch: random micro-jitter.
    Glitch {
        interval: f32,
        timer: f32,
        intensity: f32,
        seed: u32,
    },
}

impl AnimationClip {
    /// Advance animation by `dt` seconds and return the output transform.
    pub fn tick(&mut self, dt: f32) -> AnimationOutput {
        let mut out = AnimationOutput::default();
        match self {
            AnimationClip::Bobbing {
                amplitude,
                frequency,
                phase,
            } => {
                *phase += dt * *frequency * 2.0 * PI;
                out.position_offset.y = *amplitude * phase.sin();
            }
            AnimationClip::Spinning { speed, angle } => {
                *angle += dt * *speed;
                out.rotation_offset = Quat::from_rotation_y(*angle);
            }
            AnimationClip::SquashStretch {
                squash_scale,
                stretch_scale,
                duration,
                timer,
                active,
            } => {
                if *active {
                    *timer += dt;
                    if *timer >= *duration {
                        *active = false;
                        *timer = 0.0;
                    } else {
                        let t = *timer / *duration;
                        // Sine ease: squash in first half, stretch in second half
                        let phase = (t * PI).sin();
                        let s = if t < 0.5 {
                            // Squash phase
                            let f = phase;
                            Vec3::new(
                                1.0 + (squash_scale[0] - 1.0) * f,
                                1.0 + (squash_scale[1] - 1.0) * f,
                                1.0 + (squash_scale[2] - 1.0) * f,
                            )
                        } else {
                            // Stretch phase
                            let f = phase;
                            Vec3::new(
                                1.0 + (stretch_scale[0] - 1.0) * f,
                                1.0 + (stretch_scale[1] - 1.0) * f,
                                1.0 + (stretch_scale[2] - 1.0) * f,
                            )
                        };
                        out.scale_multiplier = s;
                    }
                }
            }
            AnimationClip::Wobble {
                intensity,
                speed,
                phase,
            } => {
                *phase += dt * *speed * 2.0 * PI;
                let p = *phase;
                out.scale_multiplier = Vec3::new(
                    1.0 + *intensity * p.sin(),
                    1.0 + *intensity * (p * 1.3 + 0.5).sin(),
                    1.0 + *intensity * (p * 0.7 + 1.0).sin(),
                );
            }
            AnimationClip::PopIn {
                target_scale,
                duration,
                timer,
                overshoot,
            } => {
                *timer += dt;
                let t = (*timer / *duration).min(1.0);
                // Elastic ease out: overshoot then settle
                let elastic = if t >= 1.0 {
                    1.0
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    let t1 = t - 1.0;
                    *overshoot * (2.0_f32).powf(-10.0 * t) * ((t1 - s) * 2.0 * PI / p).sin() + 1.0
                };
                out.scale_multiplier = Vec3::new(
                    target_scale[0] * elastic,
                    target_scale[1] * elastic,
                    target_scale[2] * elastic,
                );
            }
            AnimationClip::HeadBob {
                rise_height,
                rise_time,
                hold_time,
                drop_time,
                wait_time,
                timer,
                phase,
            } => {
                *timer += dt;
                match phase {
                    HeadBobPhase::Rise => {
                        if *timer >= *rise_time {
                            *timer = 0.0;
                            *phase = HeadBobPhase::Hold;
                            out.position_offset.y = *rise_height;
                        } else {
                            let t = *timer / *rise_time;
                            // Smooth sine ease in
                            out.position_offset.y = *rise_height * (t * PI * 0.5).sin();
                        }
                    }
                    HeadBobPhase::Hold => {
                        out.position_offset.y = *rise_height;
                        if *timer >= *hold_time {
                            *timer = 0.0;
                            *phase = HeadBobPhase::Drop;
                        }
                    }
                    HeadBobPhase::Drop => {
                        if *timer >= *drop_time {
                            *timer = 0.0;
                            *phase = HeadBobPhase::Wait;
                        } else {
                            let t = 1.0 - *timer / *drop_time;
                            out.position_offset.y = *rise_height * t;
                        }
                    }
                    HeadBobPhase::Wait => {
                        if *timer >= *wait_time {
                            *timer = 0.0;
                            *phase = HeadBobPhase::Rise;
                        }
                    }
                }
            }
            AnimationClip::PulseGlow {
                min_scale,
                max_scale,
                speed,
                phase,
            } => {
                *phase += dt * *speed * 2.0 * PI;
                let t = (*phase).sin() * 0.5 + 0.5; // 0..1
                let s = *min_scale + (*max_scale - *min_scale) * t;
                out.scale_multiplier = Vec3::splat(s);
            }
            AnimationClip::Glitch {
                interval,
                timer,
                intensity,
                seed,
            } => {
                *timer += dt;
                if *timer >= *interval {
                    *timer = 0.0;
                    *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                }
                // Simple hash-based pseudo-random offset
                let s = *seed;
                let fx = ((s & 0xFF) as f32 / 255.0 - 0.5) * 2.0 * *intensity;
                let fy = (((s >> 8) & 0xFF) as f32 / 255.0 - 0.5) * 2.0 * *intensity;
                let fz = (((s >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 2.0 * *intensity;
                out.position_offset = Vec3::new(fx, fy, fz);
            }
        }
        out
    }
}

/// Holds multiple animation clips and combines their outputs each tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationState {
    pub animations: Vec<AnimationClip>,
}

impl AnimationState {
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
        }
    }

    pub fn with(mut self, clip: AnimationClip) -> Self {
        self.animations.push(clip);
        self
    }

    /// Advance all animations and combine outputs.
    pub fn tick(&mut self, dt: f32) -> AnimationOutput {
        let mut result = AnimationOutput::default();
        for clip in &mut self.animations {
            let out = clip.tick(dt);
            result = result.combine(&out);
        }
        result
    }

    /// Trigger squash/stretch animations (for landing, bouncing).
    pub fn trigger_squash_stretch(&mut self) {
        for clip in &mut self.animations {
            if let AnimationClip::SquashStretch { active, timer, .. } = clip {
                *active = true;
                *timer = 0.0;
            }
        }
    }

    // =========================================================================
    // Preset factories
    // =========================================================================

    /// Skibidi idle: periodic head rise-drop + spin during rise.
    pub fn skibidi_idle() -> Self {
        Self {
            animations: vec![
                AnimationClip::HeadBob {
                    rise_height: 2.0,
                    rise_time: 1.0,
                    hold_time: 0.5,
                    drop_time: 0.5,
                    wait_time: 2.0,
                    timer: 0.0,
                    phase: HeadBobPhase::Wait,
                },
                AnimationClip::Spinning {
                    speed: 3.0,
                    angle: 0.0,
                },
            ],
        }
    }

    /// Grimace wobble: organic blob motion + gentle bobbing.
    pub fn grimace_wobble() -> Self {
        Self {
            animations: vec![
                AnimationClip::Wobble {
                    intensity: 0.05,
                    speed: 2.0,
                    phase: 0.0,
                },
                AnimationClip::Bobbing {
                    amplitude: 0.2,
                    frequency: 0.5,
                    phase: 0.0,
                },
            ],
        }
    }

    /// Item pickup: bobbing + spinning + pulse glow.
    pub fn item_pickup() -> Self {
        Self {
            animations: vec![
                AnimationClip::Bobbing {
                    amplitude: 0.3,
                    frequency: 1.5,
                    phase: 0.0,
                },
                AnimationClip::Spinning {
                    speed: 2.0,
                    angle: 0.0,
                },
                AnimationClip::PulseGlow {
                    min_scale: 0.9,
                    max_scale: 1.1,
                    speed: 2.0,
                    phase: 0.0,
                },
            ],
        }
    }

    /// Sigma idle: completely still. That is the point.
    pub fn sigma_idle() -> Self {
        Self { animations: vec![] }
    }

    /// Ohio glitch: random position micro-jitter every 0.1s.
    pub fn ohio_glitch() -> Self {
        Self {
            animations: vec![AnimationClip::Glitch {
                interval: 0.1,
                timer: 0.0,
                intensity: 0.15,
                seed: 42,
            }],
        }
    }

    /// Pop spawn: elastic scale from 0 to full with overshoot.
    pub fn pop_spawn() -> Self {
        Self {
            animations: vec![AnimationClip::PopIn {
                target_scale: [1.0, 1.0, 1.0],
                duration: 0.3,
                timer: 0.0,
                overshoot: 1.3,
            }],
        }
    }

    // =========================================================================
    // Emote preset factories (gftd:kami/emote animation-preset mapping)
    // =========================================================================

    /// Emote: wave greeting — bobbing + slight Y rotation back and forth.
    pub fn emote_wave() -> Self {
        Self {
            animations: vec![
                AnimationClip::Bobbing {
                    amplitude: 0.15,
                    frequency: 2.0,
                    phase: 0.0,
                },
                AnimationClip::Spinning {
                    speed: 0.5,
                    angle: 0.0,
                },
            ],
        }
    }

    /// Emote: dance — rhythmic bobbing + spinning + wobble.
    pub fn emote_dance() -> Self {
        Self {
            animations: vec![
                AnimationClip::Bobbing {
                    amplitude: 0.4,
                    frequency: 3.0,
                    phase: 0.0,
                },
                AnimationClip::Spinning {
                    speed: 4.0,
                    angle: 0.0,
                },
                AnimationClip::Wobble {
                    intensity: 0.06,
                    speed: 3.0,
                    phase: 0.0,
                },
            ],
        }
    }

    /// Emote: taunt — aggressive pop-in + glitch.
    pub fn emote_taunt() -> Self {
        Self {
            animations: vec![
                AnimationClip::PopIn {
                    target_scale: [1.2, 1.2, 1.2],
                    duration: 0.2,
                    timer: 0.0,
                    overshoot: 1.5,
                },
                AnimationClip::Glitch {
                    interval: 0.08,
                    timer: 0.0,
                    intensity: 0.1,
                    seed: 77,
                },
            ],
        }
    }

    /// Emote: celebrate — victory pop-in + spinning + pulse.
    pub fn emote_celebrate() -> Self {
        Self {
            animations: vec![
                AnimationClip::PopIn {
                    target_scale: [1.3, 1.3, 1.3],
                    duration: 0.3,
                    timer: 0.0,
                    overshoot: 1.4,
                },
                AnimationClip::Spinning {
                    speed: 6.0,
                    angle: 0.0,
                },
                AnimationClip::PulseGlow {
                    min_scale: 0.85,
                    max_scale: 1.15,
                    speed: 3.0,
                    phase: 0.0,
                },
            ],
        }
    }

    /// Emote: sad — slow droop bobbing + scale shrink.
    pub fn emote_sad() -> Self {
        Self {
            animations: vec![
                AnimationClip::Bobbing {
                    amplitude: 0.05,
                    frequency: 0.3,
                    phase: 0.0,
                },
                AnimationClip::PulseGlow {
                    min_scale: 0.85,
                    max_scale: 0.95,
                    speed: 0.5,
                    phase: 0.0,
                },
            ],
        }
    }

    /// Emote: rage — rapid glitch + squash/stretch.
    pub fn emote_rage() -> Self {
        Self {
            animations: vec![
                AnimationClip::Glitch {
                    interval: 0.05,
                    timer: 0.0,
                    intensity: 0.25,
                    seed: 99,
                },
                AnimationClip::SquashStretch {
                    squash_scale: [1.4, 0.6, 1.4],
                    stretch_scale: [0.7, 1.4, 0.7],
                    duration: 0.2,
                    timer: 0.0,
                    active: true,
                },
            ],
        }
    }

    /// Create emote AnimationState from WIT animation-preset string.
    pub fn from_emote_preset(preset: &str) -> Self {
        match preset {
            "idle" => Self::sigma_idle(),
            "bobbing" => Self::new().with(AnimationClip::Bobbing {
                amplitude: 0.3,
                frequency: 1.0,
                phase: 0.0,
            }),
            "spinning" => Self::new().with(AnimationClip::Spinning {
                speed: 3.0,
                angle: 0.0,
            }),
            "wobble" => Self::grimace_wobble(),
            "pop-in" => Self::pop_spawn(),
            "pulse-glow" => Self::new().with(AnimationClip::PulseGlow {
                min_scale: 0.8,
                max_scale: 1.2,
                speed: 2.0,
                phase: 0.0,
            }),
            "glitch" => Self::ohio_glitch(),
            "head-bob" => Self::skibidi_idle(),
            "squash-stretch" => Self::nintendo_bounce(),
            "wave" => Self::emote_wave(),
            "dance" => Self::emote_dance(),
            "taunt" => Self::emote_taunt(),
            "celebrate" => Self::emote_celebrate(),
            "sad" => Self::emote_sad(),
            "rage" => Self::emote_rage(),
            _ => Self::new(), // unknown preset → no animation
        }
    }

    /// Nintendo bounce: squash/stretch for cartoon landing.
    pub fn nintendo_bounce() -> Self {
        Self {
            animations: vec![AnimationClip::SquashStretch {
                squash_scale: [1.3, 0.7, 1.3],
                stretch_scale: [0.85, 1.2, 0.85],
                duration: 0.4,
                timer: 0.0,
                active: false,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bobbing_oscillates() {
        let mut state = AnimationState::new().with(AnimationClip::Bobbing {
            amplitude: 1.0,
            frequency: 1.0,
            phase: 0.0,
        });
        let mut values = Vec::new();
        for _ in 0..20 {
            let out = state.tick(0.05);
            values.push(out.position_offset.y);
        }
        // Should have positive and negative values (oscillation)
        let has_pos = values.iter().any(|&v| v > 0.1);
        let has_neg = values.iter().any(|&v| v < -0.1);
        assert!(has_pos, "bobbing should have positive y offsets");
        assert!(has_neg, "bobbing should have negative y offsets");
    }

    #[test]
    fn spinning_accumulates() {
        let mut state = AnimationState::new().with(AnimationClip::Spinning {
            speed: 1.0,
            angle: 0.0,
        });
        let out1 = state.tick(0.5);
        let out2 = state.tick(0.5);
        // Rotation should be non-identity and increasing
        let angle1 = out1.rotation_offset.to_axis_angle().1;
        let angle2 = out2.rotation_offset.to_axis_angle().1;
        assert!(angle1 > 0.0, "spinning should produce rotation");
        assert!(angle2 > angle1, "spinning should accumulate");
    }

    #[test]
    fn squash_stretch_returns_to_identity() {
        let mut state = AnimationState::new().with(AnimationClip::SquashStretch {
            squash_scale: [1.3, 0.7, 1.3],
            stretch_scale: [0.85, 1.2, 0.85],
            duration: 0.4,
            timer: 0.0,
            active: true,
        });
        // Run past the duration
        for _ in 0..20 {
            state.tick(0.05);
        }
        let out = state.tick(0.05);
        // After completion, scale should be identity
        assert!((out.scale_multiplier.x - 1.0).abs() < 0.01);
        assert!((out.scale_multiplier.y - 1.0).abs() < 0.01);
        assert!((out.scale_multiplier.z - 1.0).abs() < 0.01);
    }

    #[test]
    fn pop_in_reaches_target() {
        let mut state = AnimationState::new().with(AnimationClip::PopIn {
            target_scale: [2.0, 2.0, 2.0],
            duration: 0.3,
            timer: 0.0,
            overshoot: 1.3,
        });
        // Run well past duration to settle
        for _ in 0..50 {
            state.tick(0.02);
        }
        let out = state.tick(0.01);
        // Should converge near target_scale
        assert!(
            (out.scale_multiplier.x - 2.0).abs() < 0.3,
            "pop_in x: {}",
            out.scale_multiplier.x
        );
        assert!(
            (out.scale_multiplier.y - 2.0).abs() < 0.3,
            "pop_in y: {}",
            out.scale_multiplier.y
        );
    }

    #[test]
    fn combined_animations_valid() {
        let mut state = AnimationState::new()
            .with(AnimationClip::Bobbing {
                amplitude: 0.5,
                frequency: 1.0,
                phase: 0.0,
            })
            .with(AnimationClip::Spinning {
                speed: 2.0,
                angle: 0.0,
            })
            .with(AnimationClip::PulseGlow {
                min_scale: 0.9,
                max_scale: 1.1,
                speed: 1.0,
                phase: 0.0,
            });

        let out = state.tick(0.1);
        // Position should have y offset from bobbing
        // Rotation should be non-identity from spinning
        // Scale should be from pulse glow
        let angle = out.rotation_offset.to_axis_angle().1;
        assert!(angle > 0.0, "combined: spinning should work");
        assert!(
            out.scale_multiplier.x > 0.0,
            "combined: scale should be positive"
        );
    }

    #[test]
    fn preset_skibidi_non_empty() {
        let state = AnimationState::skibidi_idle();
        assert!(!state.animations.is_empty());
    }

    #[test]
    fn preset_grimace_non_empty() {
        let state = AnimationState::grimace_wobble();
        assert!(!state.animations.is_empty());
    }

    #[test]
    fn preset_item_pickup_non_empty() {
        let state = AnimationState::item_pickup();
        assert!(!state.animations.is_empty());
    }

    #[test]
    fn preset_sigma_is_empty() {
        let state = AnimationState::sigma_idle();
        assert!(state.animations.is_empty(), "sigma should be still");
        let mut s = state;
        let out = s.tick(1.0);
        assert_eq!(out.position_offset, Vec3::ZERO);
        assert_eq!(out.scale_multiplier, Vec3::ONE);
    }

    #[test]
    fn preset_ohio_glitch_produces_jitter() {
        let mut state = AnimationState::ohio_glitch();
        let mut offsets = Vec::new();
        for _ in 0..20 {
            let out = state.tick(0.05);
            offsets.push(out.position_offset);
        }
        // Glitch should produce varying offsets (not all identical)
        let first = offsets[0];
        let has_different = offsets.iter().any(|o| (*o - first).length() > 0.001);
        assert!(has_different, "ohio glitch should produce varying offsets");
    }

    #[test]
    fn preset_pop_spawn_non_empty() {
        let state = AnimationState::pop_spawn();
        assert!(!state.animations.is_empty());
    }

    #[test]
    fn preset_nintendo_bounce_trigger() {
        let mut state = AnimationState::nintendo_bounce();
        // Initially not active, output should be identity
        let out = state.tick(0.01);
        assert!((out.scale_multiplier.x - 1.0).abs() < 0.01);
        // Trigger it
        state.trigger_squash_stretch();
        let out = state.tick(0.05);
        // Should now have non-identity scale
        assert!(
            (out.scale_multiplier.y - 1.0).abs() > 0.01,
            "triggered bounce should squash"
        );
    }

    #[test]
    fn head_bob_cycles_through_phases() {
        let mut state = AnimationState::skibidi_idle();
        // Run through a full cycle (wait + rise + hold + drop = 4.0s)
        let mut max_y: f32 = 0.0;
        for _ in 0..200 {
            let out = state.tick(0.05);
            if out.position_offset.y > max_y {
                max_y = out.position_offset.y;
            }
        }
        assert!(max_y > 1.0, "head bob should rise to ~2.0, got {max_y}");
    }

    #[test]
    fn emote_wave_produces_motion() {
        let mut state = AnimationState::emote_wave();
        assert_eq!(state.animations.len(), 2);
        let out = state.tick(0.5);
        // Should have both y offset (bobbing) and rotation (spinning)
        let angle = out.rotation_offset.to_axis_angle().1;
        assert!(angle > 0.0, "wave emote should produce rotation");
    }

    #[test]
    fn emote_dance_has_three_clips() {
        let state = AnimationState::emote_dance();
        assert_eq!(state.animations.len(), 3);
    }

    #[test]
    fn emote_taunt_glitches() {
        let mut state = AnimationState::emote_taunt();
        let mut offsets = Vec::new();
        for _ in 0..20 {
            let out = state.tick(0.05);
            offsets.push(out.position_offset);
        }
        let first = offsets[0];
        let has_different = offsets.iter().any(|o| (*o - first).length() > 0.001);
        assert!(has_different, "taunt emote should produce glitch jitter");
    }

    #[test]
    fn emote_celebrate_spins_and_pulses() {
        let mut state = AnimationState::emote_celebrate();
        assert_eq!(state.animations.len(), 3);
        let out = state.tick(0.5);
        let angle = out.rotation_offset.to_axis_angle().1;
        assert!(angle > 0.0, "celebrate should spin");
    }

    #[test]
    fn emote_sad_shrinks() {
        let mut state = AnimationState::emote_sad();
        state.tick(0.5);
        let out = state.tick(0.1);
        // PulseGlow range is 0.85-0.95, so scale should be < 1.0
        assert!(
            out.scale_multiplier.x < 1.0,
            "sad emote should shrink, got {}",
            out.scale_multiplier.x
        );
    }

    #[test]
    fn emote_rage_is_aggressive() {
        let mut state = AnimationState::emote_rage();
        assert_eq!(state.animations.len(), 2);
        let out = state.tick(0.05);
        // Glitch should produce position offset
        assert!(
            out.position_offset.length() > 0.0,
            "rage should have glitch offset"
        );
    }

    #[test]
    fn from_emote_preset_all_valid() {
        let presets = [
            "idle",
            "bobbing",
            "spinning",
            "wobble",
            "pop-in",
            "pulse-glow",
            "glitch",
            "head-bob",
            "squash-stretch",
            "wave",
            "dance",
            "taunt",
            "celebrate",
            "sad",
            "rage",
            "unknown-fallback",
        ];
        for preset in &presets {
            let state = AnimationState::from_emote_preset(preset);
            // Should not panic for any preset
            let _ = state.animations.len();
        }
    }

    #[test]
    fn from_emote_preset_maps_correctly() {
        // "dance" should have 3 clips (bobbing + spinning + wobble)
        let dance = AnimationState::from_emote_preset("dance");
        assert_eq!(dance.animations.len(), 3);
        // "idle" (sigma) should have 0 clips
        let idle = AnimationState::from_emote_preset("idle");
        assert_eq!(idle.animations.len(), 0);
        // "head-bob" (skibidi) should have 2 clips
        let headbob = AnimationState::from_emote_preset("head-bob");
        assert_eq!(headbob.animations.len(), 2);
    }

    #[test]
    fn wobble_produces_non_uniform_scale() {
        let mut state = AnimationState::grimace_wobble();
        state.tick(0.5);
        let out = state.tick(0.1);
        // Scale axes should differ due to phase offsets
        let sx = out.scale_multiplier.x;
        let sy = out.scale_multiplier.y;
        let sz = out.scale_multiplier.z;
        let max_diff = (sx - sy).abs().max((sy - sz).abs()).max((sx - sz).abs());
        assert!(
            max_diff > 0.001,
            "wobble should have non-uniform scale, got {sx}/{sy}/{sz}"
        );
    }
}
