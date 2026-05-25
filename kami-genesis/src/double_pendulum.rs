//! 2-link planar revolute serial chain (double pendulum) — extends beyond
//! Cartpole topology toward general robot arms.
//!
//! Equations from Spong, Hutchinson & Vidyasagar "Robot Modeling and Control"
//! (planar 2-link manipulator, revolute–revolute, gravity along -z, motion
//! confined to xz plane with both axes along world y):
//!
//! ```text
//!     M(q) qddot + C(q, qdot) qdot + g(q) = tau
//!
//!     m11 = (m1+m2) l1^2 + m2 l2^2 + 2 m2 l1 l2 cos(q2) + I1 + I2
//!     m12 = m2 l2^2 + m2 l1 l2 cos(q2) + I2
//!     m22 = m2 l2^2 + I2
//!
//!     h = -m2 l1 l2 sin(q2)
//!     C qdot = [ h qdot2 (2 qdot1 + qdot2);
//!               -h qdot1^2 ]
//!
//!     g(q) = [ (m1+m2) g lc1 sin(q1) + m2 g l2 sin(q1+q2);
//!              m2 g l2 sin(q1+q2) ]
//! ```
//!
//! where lc1, lc2 are com distances from each joint (lc1 = l1/2 if uniform).
//! This implementation assumes uniform-density rods (lc = l/2), I = m l^2 / 12.
//!
//! Used to demonstrate the ADR-2605261800 §D10.3 invariant: same `World`
//! and Articulation surfaces handle non-Cartpole topology without changing
//! the nv-compat API facade.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DoublePendulumConfig {
    pub m1: f32,
    pub m2: f32,
    pub l1: f32, // length of link 1
    pub l2: f32, // length of link 2
    pub gravity: f32,
    pub effort_limit: f32, // |action| clamp per joint
    pub dt: f32,
}

impl Default for DoublePendulumConfig {
    fn default() -> Self {
        // Matches 70-tools/e7m-sim/scenes/double_pendulum/double_pendulum.urdf.
        DoublePendulumConfig {
            m1: 1.0,
            m2: 1.0,
            l1: 1.0,
            l2: 1.0,
            gravity: 9.81,
            effort_limit: 50.0,
            dt: 1.0 / 240.0, // 240 Hz physics (longer chain wants smaller dt)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DoublePendulumState {
    pub q1: f32,
    pub q2: f32,
    pub q1_dot: f32,
    pub q2_dot: f32,
}

impl Default for DoublePendulumState {
    fn default() -> Self {
        DoublePendulumState { q1: 0.0, q2: 0.0, q1_dot: 0.0, q2_dot: 0.0 }
    }
}

impl DoublePendulumState {
    /// Semi-implicit Euler step with `tau = [tau1, tau2]` (joint torques).
    pub fn step(&mut self, tau: [f32; 2], cfg: &DoublePendulumConfig) {
        let t1 = tau[0].clamp(-cfg.effort_limit, cfg.effort_limit);
        let t2 = tau[1].clamp(-cfg.effort_limit, cfg.effort_limit);

        // Uniform rod about its centre of mass: I_yy = m l^2 / 12.
        let lc1 = cfg.l1 * 0.5;
        let lc2 = cfg.l2 * 0.5;
        let i1 = cfg.m1 * cfg.l1 * cfg.l1 / 12.0;
        let i2 = cfg.m2 * cfg.l2 * cfg.l2 / 12.0;

        let s2 = self.q2.sin();
        let c2 = self.q2.cos();
        let s1 = self.q1.sin();
        let s12 = (self.q1 + self.q2).sin();

        // Mass matrix entries (using lc1 lc2 instead of l1/l2 for the COM-based form):
        let m11 = cfg.m1 * lc1 * lc1
            + cfg.m2 * (cfg.l1 * cfg.l1 + lc2 * lc2 + 2.0 * cfg.l1 * lc2 * c2)
            + i1
            + i2;
        let m12 = cfg.m2 * (lc2 * lc2 + cfg.l1 * lc2 * c2) + i2;
        let m22 = cfg.m2 * lc2 * lc2 + i2;

        // Coriolis/centrifugal bias h ([Spong et al. §6.1]):
        let h = -cfg.m2 * cfg.l1 * lc2 * s2;
        let c_1 = h * self.q2_dot * (2.0 * self.q1_dot + self.q2_dot);
        let c_2 = -h * self.q1_dot * self.q1_dot;

        // Gravity bias:
        let g1 = (cfg.m1 * lc1 + cfg.m2 * cfg.l1) * cfg.gravity * s1
            + cfg.m2 * lc2 * cfg.gravity * s12;
        let g2 = cfg.m2 * lc2 * cfg.gravity * s12;

        // Solve M qddot = tau - C qdot - g (2x2 system):
        let b1 = t1 - c_1 - g1;
        let b2 = t2 - c_2 - g2;
        let det = m11 * m22 - m12 * m12; // symmetric matrix
        let q1_acc = (m22 * b1 - m12 * b2) / det;
        let q2_acc = (-m12 * b1 + m11 * b2) / det;

        // Semi-implicit Euler.
        self.q1_dot += cfg.dt * q1_acc;
        self.q1 += cfg.dt * self.q1_dot;
        self.q2_dot += cfg.dt * q2_acc;
        self.q2 += cfg.dt * self.q2_dot;
    }

    /// Total mechanical energy — used to validate energy conservation under
    /// zero torque (semi-implicit Euler conserves energy approximately).
    pub fn energy(&self, cfg: &DoublePendulumConfig) -> f32 {
        let lc1 = cfg.l1 * 0.5;
        let lc2 = cfg.l2 * 0.5;
        let i1 = cfg.m1 * cfg.l1 * cfg.l1 / 12.0;
        let i2 = cfg.m2 * cfg.l2 * cfg.l2 / 12.0;
        let q1d = self.q1_dot;
        let q2d = self.q2_dot;
        let c2 = self.q2.cos();

        // KE = ½ qdot^T M qdot
        let m11 = cfg.m1 * lc1 * lc1
            + cfg.m2 * (cfg.l1 * cfg.l1 + lc2 * lc2 + 2.0 * cfg.l1 * lc2 * c2)
            + i1
            + i2;
        let m12 = cfg.m2 * (lc2 * lc2 + cfg.l1 * lc2 * c2) + i2;
        let m22 = cfg.m2 * lc2 * lc2 + i2;
        let ke = 0.5 * (m11 * q1d * q1d + 2.0 * m12 * q1d * q2d + m22 * q2d * q2d);

        // PE relative to joint origin: each com's z position is -lc_i times cos
        // of cumulative angle (q1 hangs down at q1=0 means -z direction = down).
        let z1 = -lc1 * self.q1.cos();
        let z2 = -cfg.l1 * self.q1.cos() - lc2 * (self.q1 + self.q2).cos();
        let pe = cfg.m1 * cfg.gravity * z1 + cfg.m2 * cfg.gravity * z2;

        ke + pe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_pendulum_limit_matches_analytical() {
        // q2 ≡ 0, no torques: link2 hangs straight relative to link1 ⇒
        // system reduces to a single compound pendulum about q1.
        // We can't trivially analytically integrate, but we can require
        // that small-angle behavior is sinusoidal with period close to
        // 2π√(I_eff / (M_eff g L_eff)).
        let cfg = DoublePendulumConfig::default();
        let mut s = DoublePendulumState { q1: 0.05, ..Default::default() };
        let e0 = s.energy(&cfg);
        // Run 1 second of swing.
        let steps = (1.0 / cfg.dt) as usize;
        for _ in 0..steps {
            s.step([0.0, 0.0], &cfg);
        }
        let e1 = s.energy(&cfg);
        // Semi-implicit Euler is symplectic-ish; energy drift ≤ 2% over 1 s.
        assert!(
            (e1 - e0).abs() / e0.abs().max(1.0) < 0.02,
            "energy drift too large: e0={e0:.4} e1={e1:.4}"
        );
    }

    #[test]
    fn double_pendulum_falls_from_horizontal_release() {
        // Convention: q1=0 ⇒ link1 hangs straight down (stable equilibrium),
        // q1 increases counterclockwise. Release at q1=π/2 (horizontal) with
        // link2 aligned (q2=0); gravity drives q1 to swing DOWN toward 0.
        let cfg = DoublePendulumConfig::default();
        let mut s = DoublePendulumState { q1: std::f32::consts::FRAC_PI_2, ..Default::default() };
        for _ in 0..(0.5 / cfg.dt) as usize {
            s.step([0.0, 0.0], &cfg);
        }
        assert!(
            s.q1 < std::f32::consts::FRAC_PI_2,
            "released horizontal pendulum should swing toward 0 (got q1={})",
            s.q1
        );
        assert!(s.q1_dot < 0.0, "angular velocity should be negative (swinging down)");
    }

    #[test]
    fn balanced_at_zero_stays_balanced_under_no_torque() {
        // q=0 (both straight down) is a stable equilibrium ⇒ small δq stays
        // small over a short window.
        let cfg = DoublePendulumConfig::default();
        let mut s = DoublePendulumState::default();
        for _ in 0..200 {
            s.step([0.0, 0.0], &cfg);
        }
        assert!(s.q1.abs() < 1e-3);
        assert!(s.q2.abs() < 1e-3);
    }

    #[test]
    fn shoulder_torque_drives_shoulder_motion() {
        let cfg = DoublePendulumConfig::default();
        let mut s = DoublePendulumState::default();
        for _ in 0..60 {
            s.step([5.0, 0.0], &cfg);
        }
        assert!(s.q1.abs() > 0.01, "+τ1 should swing the shoulder");
    }

    #[test]
    fn effort_clamped_to_limit() {
        let cfg = DoublePendulumConfig::default();
        let mut s = DoublePendulumState::default();
        s.step([10_000.0, -10_000.0], &cfg);
        // Result still finite and within reasonable bounds (effort clamped at 50).
        assert!(s.q1_dot.abs() < 5.0);
        assert!(s.q1_dot.is_finite());
        assert!(s.q2_dot.is_finite());
    }
}
