//! Closed-form Cartpole dynamics (Sutton & Barto 1983, §10.2).
//!
//! State: (x, x_dot, theta, theta_dot)
//!   x       — cart position [m]
//!   x_dot   — cart velocity [m/s]
//!   theta   — pole angle from vertical (positive = pole tilts +x direction) [rad]
//!   theta_dot — pole angular velocity [rad/s]
//!
//! Action: force on cart [N] (continuous, clamped to ±effort_limit).
//!
//! Convention matches OpenAI Gym CartPole-v1 / Isaac Lab Cartpole-Direct-v0:
//!   gravity points -z; pole is balanced upright at theta = 0.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CartpoleConfig {
    pub cart_mass: f32,
    pub pole_mass: f32,
    pub pole_half_length: f32, // = pole_length / 2
    pub gravity: f32,
    pub force_mag: f32, // |action| ≤ force_mag
    pub dt: f32,
}

impl Default for CartpoleConfig {
    fn default() -> Self {
        // Matches kami-engine fixtures/cartpole/cartpole.urdf
        // and OpenAI Gym CartPole-v1 reference.
        CartpoleConfig {
            cart_mass: 1.0,
            pole_mass: 0.1,
            pole_half_length: 0.25, // 0.5 m total
            gravity: 9.81,
            force_mag: 100.0, // matches urdf effort limit
            dt: 1.0 / 60.0,   // 60 Hz physics step (decimation = 2 with 30 Hz control)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CartpoleState {
    pub x: f32,
    pub x_dot: f32,
    pub theta: f32,
    pub theta_dot: f32,
}

impl Default for CartpoleState {
    fn default() -> Self {
        CartpoleState {
            x: 0.0,
            x_dot: 0.0,
            theta: 0.0,
            theta_dot: 0.0,
        }
    }
}

impl CartpoleState {
    /// Apply one semi-implicit Euler integration step under `action` (force on cart).
    pub fn step(&mut self, action: f32, cfg: &CartpoleConfig) {
        let force = action.clamp(-cfg.force_mag, cfg.force_mag);
        let sin_t = self.theta.sin();
        let cos_t = self.theta.cos();
        let total_mass = cfg.cart_mass + cfg.pole_mass;
        let pole_mass_length = cfg.pole_mass * cfg.pole_half_length;

        // Standard cartpole equations of motion (Sutton & Barto 1983):
        //   theta_acc = (g sin(theta) - cos(theta) * temp) /
        //               (l * (4/3 - m_pole cos²(theta) / total_mass))
        //   x_acc     = temp - pole_mass_length * theta_acc * cos(theta) / total_mass
        //   where    temp = (force + pole_mass_length * theta_dot² * sin(theta)) / total_mass
        let temp =
            (force + pole_mass_length * self.theta_dot * self.theta_dot * sin_t) / total_mass;
        let theta_acc = (cfg.gravity * sin_t - cos_t * temp)
            / (cfg.pole_half_length * (4.0 / 3.0 - cfg.pole_mass * cos_t * cos_t / total_mass));
        let x_acc = temp - pole_mass_length * theta_acc * cos_t / total_mass;

        // Semi-implicit Euler: advance velocity first, then position with new velocity.
        self.x_dot += cfg.dt * x_acc;
        self.x += cfg.dt * self.x_dot;
        self.theta_dot += cfg.dt * theta_acc;
        self.theta += cfg.dt * self.theta_dot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pendulum_falls_from_small_initial_tilt() {
        // No control force; pole tilted 0.1 rad should fall (theta grows).
        let cfg = CartpoleConfig::default();
        let mut s = CartpoleState {
            theta: 0.1,
            ..Default::default()
        };
        for _ in 0..120 {
            s.step(0.0, &cfg);
        }
        assert!(
            s.theta.abs() > 0.1,
            "pole should fall from tilt under gravity"
        );
    }

    #[test]
    fn rightward_force_moves_cart_right() {
        let cfg = CartpoleConfig::default();
        let mut s = CartpoleState::default();
        for _ in 0..60 {
            s.step(10.0, &cfg);
        }
        assert!(s.x > 0.0, "positive force should move cart in +x");
        assert!(
            s.x_dot > 0.0,
            "positive force should give positive velocity"
        );
    }

    #[test]
    fn balanced_at_rest_stays_balanced() {
        // theta = 0, theta_dot = 0, no force: pole stays balanced (numerical noise only).
        let cfg = CartpoleConfig::default();
        let mut s = CartpoleState::default();
        for _ in 0..60 {
            s.step(0.0, &cfg);
        }
        assert!(
            s.theta.abs() < 1e-3,
            "perfectly balanced pole stays balanced"
        );
    }

    #[test]
    fn force_clamped_to_effort_limit() {
        // Action 10000 N is clamped to ±100 N; check that integration still finite.
        let cfg = CartpoleConfig::default();
        let mut s = CartpoleState::default();
        s.step(10000.0, &cfg);
        // velocity from one step under 100 N clamped force on 1.1 kg system ≈ 1.5 m/s
        assert!(
            s.x_dot.abs() < 2.0,
            "force should be clamped, not 10000 N applied"
        );
        assert!(s.x_dot.is_finite());
    }
}
