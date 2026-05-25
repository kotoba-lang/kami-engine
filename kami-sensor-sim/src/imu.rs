//! IMUSensor — body-frame inertial measurement unit.
//!
//! Mirrors `isaacsim.sensors.IMUSensor` (Isaac Sim 4.x) at the public API
//! level. Attached to a named link of an articulation; samples at each step
//! to expose:
//!   - linear acceleration in body frame (finite difference of linear velocity)
//!   - angular velocity in body frame (rotated from world)
//!   - orientation as a quaternion (world)
//!
//! Convention: linear acceleration is **proper acceleration** = inertial
//! acceleration − gravity in body frame. A static body in free fall reads
//! zero; a body resting on the ground reads +g in the "up" body direction
//! (matches real accelerometers).

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImuReading {
    /// Body-frame proper acceleration (m/s² – gravity).
    pub linear_acceleration: Vec3,
    /// Body-frame angular velocity (rad/s).
    pub angular_velocity: Vec3,
    /// Body orientation in world frame.
    pub orientation: Quat,
    /// Sample time, monotonic seconds since start.
    pub time: f32,
}

impl Default for ImuReading {
    fn default() -> Self {
        ImuReading {
            linear_acceleration: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            time: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Imu {
    pub name: String,
    pub prim_path: String,
    /// Link of the parent articulation this IMU is attached to.
    pub link_name: String,
    /// World-frame gravity vector (default: 9.81 along -z).
    pub gravity: Vec3,

    // Sampling state (last world-frame linear velocity + last time).
    last_lin_vel_world: Vec3,
    last_time: f32,
    has_previous: bool,
}

impl Imu {
    pub fn new(name: impl Into<String>, prim_path: impl Into<String>, link_name: impl Into<String>) -> Self {
        Imu {
            name: name.into(),
            prim_path: prim_path.into(),
            link_name: link_name.into(),
            gravity: Vec3::new(0.0, 0.0, -9.81),
            last_lin_vel_world: Vec3::ZERO,
            last_time: 0.0,
            has_previous: false,
        }
    }

    /// Override gravity (e.g. zero gravity for free-fall sanity tests).
    pub fn set_gravity(&mut self, g: Vec3) {
        self.gravity = g;
    }

    /// Reset the running state (forget previous sample). Call before each
    /// new episode.
    pub fn reset(&mut self) {
        self.has_previous = false;
        self.last_lin_vel_world = Vec3::ZERO;
        self.last_time = 0.0;
    }

    /// Sample the IMU given the link's world-frame state and the current sim time.
    /// `lin_vel_world` and `ang_vel_world` come from
    /// `kami_genesis::Articulation::link_state(link_name)`. Returns an
    /// ImuReading; on the very first call (no previous velocity), the
    /// linear_acceleration is reported as the negative-gravity reading
    /// (consistent with a body at rest in gravity).
    pub fn sample(
        &mut self,
        lin_vel_world: Vec3,
        ang_vel_world: Vec3,
        orientation: Quat,
        time: f32,
    ) -> ImuReading {
        // Inertial acceleration in world frame.
        let inertial_accel_world = if self.has_previous && time > self.last_time {
            (lin_vel_world - self.last_lin_vel_world) / (time - self.last_time)
        } else {
            Vec3::ZERO
        };

        // Proper acceleration (world) = inertial - gravity.
        // A body at rest reads (0, 0, 0) - (0, 0, -g) = (0, 0, +g) in world,
        // which is "up", matching a real accelerometer at rest.
        let proper_world = inertial_accel_world - self.gravity;

        // Rotate world quantities into body frame using orientation^-1.
        // (orientation rotates body → world, so its inverse takes world → body.)
        let inv = orientation.inverse();
        let lin_accel_body = inv * proper_world;
        let ang_vel_body = inv * ang_vel_world;

        // Update sampling state.
        self.last_lin_vel_world = lin_vel_world;
        self.last_time = time;
        self.has_previous = true;

        ImuReading {
            linear_acceleration: lin_accel_body,
            angular_velocity: ang_vel_body,
            orientation,
            time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_body_reads_plus_g_along_body_up() {
        // Body at rest at world origin with identity orientation. Real
        // accelerometer reading should be (0, 0, +9.81) ("up").
        let mut imu = Imu::new("imu", "/W/cart/imu", "cart");
        // First sample: no previous velocity, so report -gravity ("up").
        let r0 = imu.sample(Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY, 0.0);
        assert!((r0.linear_acceleration.z - 9.81).abs() < 1e-3);

        // Steady-state: still zero velocity, time advanced. Reading still +g up.
        let r1 = imu.sample(Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY, 0.01);
        assert!((r1.linear_acceleration.z - 9.81).abs() < 1e-3);
    }

    #[test]
    fn freefall_reads_zero() {
        // Body in freefall (lin_vel decreasing along -z over time).
        let mut imu = Imu::new("imu", "/W/imu", "any");
        imu.reset();
        // first sample to set the baseline:
        imu.sample(Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY, 0.0);
        // After dt = 0.01 s in freefall, world velocity = -g*dt along z.
        let dt = 0.01f32;
        let v_fall = Vec3::new(0.0, 0.0, -9.81 * dt);
        let r = imu.sample(v_fall, Vec3::ZERO, Quat::IDENTITY, dt);
        // Proper accel = (v - 0) / dt - gravity = (0, 0, -9.81) - (0,0,-9.81) = 0
        assert!(r.linear_acceleration.length() < 1e-2, "got {:?}", r.linear_acceleration);
    }

    #[test]
    fn rotating_body_reports_body_frame_ang_vel() {
        // Body rotating at 1 rad/s about world +y, oriented at 90° about y.
        // Body-frame +z (originally world +z) maps to world +x after rotation.
        // So world ang_vel (0, 1, 0) → body ang_vel (0, 1, 0) (y axis is shared
        // by both orientation and rotation).
        let mut imu = Imu::new("imu", "/W/imu", "any");
        imu.reset();
        let q = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2);
        let r = imu.sample(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), q, 0.0);
        // y-axis rotation: world y == body y.
        assert!((r.angular_velocity.y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn reading_carries_orientation_and_time() {
        let mut imu = Imu::new("imu", "/W/imu", "any");
        let q = Quat::from_axis_angle(Vec3::Y, 0.3);
        let r = imu.sample(Vec3::ZERO, Vec3::ZERO, q, 1.234);
        assert!((r.time - 1.234).abs() < 1e-6);
        assert!((r.orientation.y - q.y).abs() < 1e-6);
    }
}
