//! Beam — pairwise spring-damper between two nodes.
//!
//! A beam reproduces BeamNG semantics:
//!   F = -k * (L - L0_eff) - d * (dL/dt)
//!
//! where `L0_eff` is the *effective* rest length (drifts under plastic
//! deformation) and `k`, `d` are the spring stiffness and damping. Beams may
//! be `Normal` (always active), `Bounded` (active only between min/max ratio),
//! `Hydro` (length is a function of an external control like steering), or
//! `Pressured` (rest length increases with internal pressure — used for tire
//! sidewalls).
//!
//! Plastic deformation: when |strain| exceeds `deform_limit`, `L0_eff` follows
//! the current length (yielding). When the *stress* exceeds `break_stress`,
//! the beam snaps and is removed from the integration.

use crate::node::NodeId;
use serde::{Deserialize, Serialize};

pub type BeamId = u32;
pub type BreakGroup = u32;

/// Beam type discriminator. Each variant changes how the spring force is
/// computed at the current length.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BeamType {
    /// Always active two-way spring.
    Normal,
    /// Active only when current length / rest length lies in `[min, max]`.
    /// Outside the bounds the beam goes slack (force = 0). Used as a soft
    /// rebound stop on suspensions.
    Bounded { min_ratio: f32, max_ratio: f32 },
    /// Length is a linear function of `extension` (typ. wired to steering
    /// input in [-1, 1]). Effective rest length is
    /// `rest_length * (1 + factor * extension)`.
    Hydro { factor: f32, extension: f32 },
    /// Pressured beam (tire sidewall). Rest length is scaled by
    /// `1 + pressure_factor * (pressure - reference_pressure)`.
    Pressured {
        pressure_factor: f32,
        reference_pressure: f32,
    },
    /// Compression-only support beam (e.g. bump-stop).
    Support,
}

/// Plastic deformation parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeformParams {
    /// Strain threshold (|ΔL/L0|) at which plastic deformation starts.
    pub deform_limit: f32,
    /// Strain threshold above which the beam snaps.
    pub break_limit: f32,
    /// Cap on plastic strain accumulated over the beam's life.
    pub max_plastic_strain: f32,
}

impl Default for DeformParams {
    fn default() -> Self {
        Self {
            deform_limit: 0.10,
            break_limit: 0.45,
            max_plastic_strain: 0.40,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beam {
    pub id: BeamId,
    pub n1: NodeId,
    pub n2: NodeId,
    /// Original rest length (geometry).
    pub rest_length: f32,
    /// Effective rest length, modified by plastic deformation, hydro
    /// extension, or pressure changes.
    pub effective_length: f32,
    pub spring: f32,
    pub damping: f32,
    pub beam_type: BeamType,
    pub deform: DeformParams,
    /// Beams in the same break-group fail together (e.g. a door panel).
    pub break_group: Option<BreakGroup>,
    pub broken: bool,
    /// Accumulated plastic strain (for telemetry / wear).
    pub plastic_strain: f32,
    /// Last computed length (cached for telemetry, debug, & PBD-style solver).
    pub current_length: f32,
}

impl Beam {
    pub fn new(
        id: BeamId,
        n1: NodeId,
        n2: NodeId,
        rest_length: f32,
        spring: f32,
        damping: f32,
    ) -> Self {
        Self {
            id,
            n1,
            n2,
            rest_length,
            effective_length: rest_length,
            spring,
            damping,
            beam_type: BeamType::Normal,
            deform: DeformParams::default(),
            break_group: None,
            broken: false,
            plastic_strain: 0.0,
            current_length: rest_length,
        }
    }

    pub fn with_type(mut self, t: BeamType) -> Self {
        self.beam_type = t;
        self
    }

    pub fn with_deform(mut self, d: DeformParams) -> Self {
        self.deform = d;
        self
    }

    pub fn with_break_group(mut self, g: BreakGroup) -> Self {
        self.break_group = Some(g);
        self
    }

    /// Compute the rest length actually used by the spring evaluator at this
    /// instant. For `Hydro` and `Pressured`, this folds the external control
    /// signal into `effective_length`.
    pub fn live_rest_length(&self, pressure: f32) -> f32 {
        match self.beam_type {
            BeamType::Hydro { factor, extension } => {
                self.effective_length * (1.0 + factor * extension)
            }
            BeamType::Pressured {
                pressure_factor,
                reference_pressure,
            } => self.effective_length * (1.0 + pressure_factor * (pressure - reference_pressure)),
            _ => self.effective_length,
        }
    }

    /// Evaluate the spring scalar force in newtons (positive = pushing nodes
    /// apart, negative = pulling them together).
    ///
    /// `current_length` is the live geometric length, `rate` is the closing
    /// speed (dL/dt). `pressure` is required for `Pressured` beams; pass
    /// 0.0 for non-pressured beams.
    pub fn force_scalar(&self, current_length: f32, rate: f32, pressure: f32) -> f32 {
        if self.broken {
            return 0.0;
        }
        let l0 = self.live_rest_length(pressure);
        let strain = (current_length - l0) / l0.max(1e-6);
        match self.beam_type {
            BeamType::Bounded {
                min_ratio,
                max_ratio,
            } => {
                let ratio = current_length / l0.max(1e-6);
                if ratio < min_ratio || ratio > max_ratio {
                    -self.spring * (current_length - l0) - self.damping * rate
                } else {
                    0.0
                }
            }
            BeamType::Support => {
                if current_length < l0 {
                    -self.spring * (current_length - l0) - self.damping * rate
                } else {
                    0.0
                }
            }
            _ => -self.spring * strain * l0 - self.damping * rate,
        }
    }

    /// Update plastic deformation and break state given the current geometric
    /// length. Returns `true` if the beam just broke this call.
    pub fn update_plastic(&mut self, current_length: f32) -> bool {
        if self.broken {
            return false;
        }
        self.current_length = current_length;
        let l0 = self.effective_length.max(1e-6);
        let strain = (current_length - l0) / l0;
        let abs_strain = strain.abs();

        if abs_strain >= self.deform.break_limit {
            self.broken = true;
            return true;
        }
        if abs_strain > self.deform.deform_limit {
            // Plastic flow: yield surface drifts toward the current length.
            let excess = abs_strain - self.deform.deform_limit;
            let yield_step = excess.copysign(strain) * 0.5;
            let new_strain = self.plastic_strain + yield_step.abs();
            if new_strain <= self.deform.max_plastic_strain {
                self.plastic_strain = new_strain;
                self.effective_length = (l0 * (1.0 + yield_step)).max(1e-3);
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_beam_relaxed_has_zero_force() {
        let b = Beam::new(0, 0, 1, 1.0, 1000.0, 10.0);
        assert!(b.force_scalar(1.0, 0.0, 0.0).abs() < 1e-3);
    }

    #[test]
    fn stretched_beam_pulls_nodes_together() {
        let b = Beam::new(0, 0, 1, 1.0, 1000.0, 10.0);
        // length 1.10, rate 0  -> strain 0.10  -> F = -k*0.10*1.0 = -100 (pulls together)
        assert!((b.force_scalar(1.10, 0.0, 0.0) + 100.0).abs() < 1e-2);
    }

    #[test]
    fn bounded_beam_idle_inside_window() {
        let b = Beam::new(0, 0, 1, 1.0, 1000.0, 10.0).with_type(BeamType::Bounded {
            min_ratio: 0.8,
            max_ratio: 1.2,
        });
        assert_eq!(b.force_scalar(1.05, 0.0, 0.0), 0.0);
        // Past the upper bound the spring kicks in.
        assert!(b.force_scalar(1.30, 0.0, 0.0) < 0.0);
    }

    #[test]
    fn hydro_beam_extends_with_control() {
        let b = Beam::new(0, 0, 1, 1.0, 1000.0, 10.0).with_type(BeamType::Hydro {
            factor: 0.20,
            extension: 1.0,
        });
        // effective rest length = 1.20, current length = 1.20 -> zero force
        assert!(b.force_scalar(1.20, 0.0, 0.0).abs() < 1e-3);
    }

    #[test]
    fn beam_yields_past_deform_limit() {
        let mut b = Beam::new(0, 0, 1, 1.0, 1000.0, 10.0);
        // strain 0.20, deform_limit 0.10  -> plastic flow; effective_length grows.
        let broke = b.update_plastic(1.20);
        assert!(!broke);
        assert!(b.effective_length > 1.0);
    }

    #[test]
    fn beam_breaks_past_break_limit() {
        let mut b = Beam::new(0, 0, 1, 1.0, 1000.0, 10.0);
        let broke = b.update_plastic(1.50);
        assert!(broke);
        assert!(b.broken);
    }
}
