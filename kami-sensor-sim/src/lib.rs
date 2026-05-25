//! kami-sensor-sim — camera / lidar / IMU / contact sensor synth.
//!
//! R1.1 scope (ADR-2605261800): minimum viable PinholeCamera (intrinsics +
//! extrinsics + 3D-point projection + depth image). Future R1.x adds:
//!   - LidarRtx via wgpu ray-query against scene BVH
//!   - IMU from kami-genesis articulation state
//!   - ContactSensor from kami-genesis collision events
//!
//! API surface mirrors `isaacsim.sensors.{Camera, LidarRtx, IMUSensor, ContactSensor}`
//! per Isaac Sim 4.x docs. Implementations route to kami-render (raster) and
//! kami-rt (ray-query) at R1.2+.

pub const ADR: &str = "ADR-2605261800";
pub const PHASE: &str = "R1.1-pinhole-camera";
pub const KAMI_NAME: &str = "kami-sensor-sim";
pub const NV_COMPAT_TARGET: &str = "isaacsim.sensors";
pub const SUPPORTED_SENSORS_R1_1: &[&str] = &["camera", "lidar", "imu", "contact"];

mod camera;
mod contact;
mod imu;
mod lidar;

pub use camera::{Camera, CameraIntrinsics, DepthImage, Projection};
pub use contact::{ContactReading, ContactSensor};
pub use imu::{Imu, ImuReading};
pub use lidar::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};
