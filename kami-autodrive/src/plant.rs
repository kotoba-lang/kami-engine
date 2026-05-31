//! Dynamics plants the autonomy loop can drive.
//!
//! The [`Plant`] trait is the seam between the GNC stack and a vehicle body.
//! [`BicycleModel`] is the shared kinematic plant used by ship/drone/aircraft
//! (and as a fast reference for the car). The high-fidelity car plant lives in
//! `crate::vehicle_adapter` behind the `soft-body-car` feature.

use crate::classes::VehicleLimits;
use crate::types::{Command, Pose2};

/// Anything the [`Autopilot`](crate::autopilot::Autopilot) can sense + actuate.
pub trait Plant {
    fn pose(&self) -> Pose2;
    fn speed(&self) -> f32;
    /// Advance the plant by `dt` under `cmd`.
    fn step(&mut self, cmd: Command, dt: f32);
}

/// Kinematic bicycle model.
///
/// `x' = v cosψ`, `y' = v sinψ`, `ψ' = (v / L) tanδ`, with `δ = steer·δ_max`
/// and longitudinal accel `a = throttle·a_max − brake·d_max − drag·v`.
#[derive(Debug, Clone)]
pub struct BicycleModel {
    pub pose: Pose2,
    pub speed: f32,
    pub limits: VehicleLimits,
    /// Linear drag coefficient (1/s).
    pub drag: f32,
}

impl BicycleModel {
    pub fn new(pose: Pose2, limits: VehicleLimits) -> Self {
        Self { pose, speed: 0.0, limits, drag: 0.05 }
    }
}

impl Plant for BicycleModel {
    fn pose(&self) -> Pose2 {
        self.pose
    }

    fn speed(&self) -> f32 {
        self.speed
    }

    fn step(&mut self, mut cmd: Command, dt: f32) {
        cmd.clamp();
        let l = self.limits;

        let accel =
            cmd.throttle * l.max_accel - cmd.brake * l.max_decel - self.drag * self.speed;
        self.speed = (self.speed + accel * dt).clamp(0.0, l.max_speed);
        if cmd.handbrake > 0.5 {
            self.speed *= 1.0 - (0.9 * dt).min(1.0);
        }

        let delta = cmd.steer * l.max_steer;
        let yaw_rate = if l.wheelbase > 1e-3 {
            self.speed / l.wheelbase * delta.tan()
        } else {
            0.0
        };
        self.pose.yaw += yaw_rate * dt;
        let (s, c) = self.pose.yaw.sin_cos();
        self.pose.x += self.speed * c * dt;
        self.pose.y += self.speed * s * dt;
    }
}
