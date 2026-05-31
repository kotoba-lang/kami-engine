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

        // Reverse gear pushes the vehicle backward (capped at 35 % of forward
        // top speed); the signed speed feeds the bicycle yaw kinematics so the
        // K-turn recovery reorients correctly.
        let accel = if cmd.reverse {
            -cmd.throttle * l.max_accel - self.drag * self.speed
        } else {
            cmd.throttle * l.max_accel - cmd.brake * l.max_decel - self.drag * self.speed
        };
        // Braking stops at zero; only a reverse command allows negative speed
        // (a slow, controlled maneuver ≈12 % of forward top speed).
        let min_speed = if cmd.reverse { -0.12 * l.max_speed } else { 0.0 };
        self.speed = (self.speed + accel * dt).clamp(min_speed, l.max_speed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::VehicleClass;

    fn car() -> BicycleModel {
        BicycleModel::new(Pose2::new(0.0, 0.0, 0.0), VehicleClass::Car.limits())
    }

    #[test]
    fn reverse_command_moves_backward() {
        let mut c = car();
        for _ in 0..60 {
            c.step(Command::reverse_with(1.0, 0.0), 1.0 / 30.0);
        }
        assert!(c.speed() < 0.0, "reverse → negative speed, got {}", c.speed());
        assert!(c.pose().x < -0.5, "should move backward along −heading (x={})", c.pose().x);
    }

    #[test]
    fn braking_stops_at_zero_not_reverse() {
        let mut c = car();
        for _ in 0..30 {
            c.step(Command { throttle: 1.0, ..Default::default() }, 1.0 / 30.0);
        }
        assert!(c.speed() > 0.0, "should be moving");
        for _ in 0..120 {
            c.step(Command::stop(), 1.0 / 30.0);
        }
        assert!(c.speed() >= 0.0, "braking must never reverse; speed {}", c.speed());
        assert!(c.speed() < 0.5, "should be ~stopped, speed {}", c.speed());
    }

    #[test]
    fn reverse_with_steer_reorients_heading() {
        let mut c = car();
        let yaw0 = c.pose().yaw;
        for _ in 0..60 {
            c.step(Command::reverse_with(1.0, 1.0), 1.0 / 30.0);
        }
        assert!((c.pose().yaw - yaw0).abs() > 0.1, "reverse + steer should change heading");
    }
}
