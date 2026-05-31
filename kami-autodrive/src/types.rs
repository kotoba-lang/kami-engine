//! Core geometric + command types for the autonomy stack.
//!
//! Frame convention (ROS REP-105, z-up): the planar ground frame is `(x, y)`
//! with `x` east, `y` north, and `yaw` measured counter-clockwise from `+x`.
//! A vehicle at `yaw = 0` faces `+x`. This matches `kami-sensor-sim`'s lidar
//! sensor frame, so lidar returns drop in without a frame flip.

use glam::Vec2;

/// Planar pose: position on the ground plane plus heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose2 {
    pub x: f32,
    pub y: f32,
    /// Heading in radians, CCW from `+x`.
    pub yaw: f32,
}

impl Pose2 {
    pub fn new(x: f32, y: f32, yaw: f32) -> Self {
        Self { x, y, yaw }
    }

    pub fn pos(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    /// Unit forward (heading) vector.
    pub fn forward(&self) -> Vec2 {
        Vec2::new(self.yaw.cos(), self.yaw.sin())
    }

    /// Unit left vector (90° CCW from forward).
    pub fn left(&self) -> Vec2 {
        Vec2::new(-self.yaw.sin(), self.yaw.cos())
    }

    /// Express a world point in the body frame: `+x` forward, `+y` left.
    pub fn to_local(&self, world: Vec2) -> Vec2 {
        let d = world - self.pos();
        let (s, c) = self.yaw.sin_cos();
        Vec2::new(c * d.x + s * d.y, -s * d.x + c * d.y)
    }

    /// Express a body-frame point (`+x` forward, `+y` left) in world coords.
    pub fn to_world(&self, local: Vec2) -> Vec2 {
        let (s, c) = self.yaw.sin_cos();
        self.pos() + Vec2::new(c * local.x - s * local.y, s * local.x + c * local.y)
    }
}

/// Normalised actuator command, mirroring `kami_vehicle::Controls`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Command {
    /// `[0, 1]` accelerator.
    pub throttle: f32,
    /// `[0, 1]` foot brake.
    pub brake: f32,
    /// `[-1, 1]` steering (positive = left/CCW).
    pub steer: f32,
    /// `[0, 1]` handbrake.
    pub handbrake: f32,
    /// Reverse gear: when `true`, `throttle` drives the vehicle backward (used
    /// by the autopilot's stuck-recovery K-turn). Plants without a reverse gear
    /// ignore it.
    pub reverse: bool,
}

impl Default for Command {
    fn default() -> Self {
        Self { throttle: 0.0, brake: 0.0, steer: 0.0, handbrake: 0.0, reverse: false }
    }
}

impl Command {
    pub fn coast() -> Self {
        Self::default()
    }

    /// Full-brake, wheels-straight stop.
    pub fn stop() -> Self {
        Self { throttle: 0.0, brake: 1.0, steer: 0.0, handbrake: 0.0, reverse: false }
    }

    /// Reverse at `throttle` with `steer` (for K-turn recovery).
    pub fn reverse_with(throttle: f32, steer: f32) -> Self {
        Self { throttle, brake: 0.0, steer, handbrake: 0.0, reverse: true }
    }

    pub fn clamp(&mut self) {
        self.throttle = self.throttle.clamp(0.0, 1.0);
        self.brake = self.brake.clamp(0.0, 1.0);
        self.handbrake = self.handbrake.clamp(0.0, 1.0);
        self.steer = self.steer.clamp(-1.0, 1.0);
    }
}

/// A circular obstacle on the ground plane (post-clustering perception output).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstacle {
    pub center: Vec2,
    pub radius: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_and_left_are_orthonormal() {
        let p = Pose2::new(3.0, -1.0, 0.9);
        assert!((p.forward().length() - 1.0).abs() < 1e-6);
        assert!((p.left().length() - 1.0).abs() < 1e-6);
        assert!(p.forward().dot(p.left()).abs() < 1e-6);
    }

    #[test]
    fn local_world_round_trip() {
        let p = Pose2::new(2.0, 5.0, 0.7);
        let w = Vec2::new(9.0, -4.0);
        let back = p.to_world(p.to_local(w));
        assert!(back.distance(w) < 1e-5, "round-trip drift {back:?}");
    }

    #[test]
    fn point_dead_ahead_is_positive_x_local() {
        let p = Pose2::new(0.0, 0.0, std::f32::consts::FRAC_PI_2); // facing +y
        let local = p.to_local(Vec2::new(0.0, 4.0)); // 4 m ahead
        assert!(local.x > 3.99 && local.y.abs() < 1e-5, "{local:?}");
    }

    #[test]
    fn point_to_the_left_has_positive_y_local() {
        let p = Pose2::new(0.0, 0.0, 0.0); // facing +x
        let local = p.to_local(Vec2::new(0.0, 2.0)); // 2 m to the left (+y)
        assert!(local.y > 1.99, "{local:?}");
    }

    #[test]
    fn command_clamp_saturates() {
        let mut c = Command { throttle: 2.0, brake: -1.0, steer: -3.0, handbrake: 5.0, reverse: false };
        c.clamp();
        assert_eq!(c.throttle, 1.0);
        assert_eq!(c.brake, 0.0);
        assert_eq!(c.steer, -1.0);
        assert_eq!(c.handbrake, 1.0);
    }
}
