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
}

impl Default for Command {
    fn default() -> Self {
        Self { throttle: 0.0, brake: 0.0, steer: 0.0, handbrake: 0.0 }
    }
}

impl Command {
    pub fn coast() -> Self {
        Self::default()
    }

    /// Full-brake, wheels-straight stop.
    pub fn stop() -> Self {
        Self { throttle: 0.0, brake: 1.0, steer: 0.0, handbrake: 0.0 }
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
