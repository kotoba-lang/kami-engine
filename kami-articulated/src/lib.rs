//! kami-articulated — URDF / MJCF / USD physics loader → kami-genesis articulation.
//!
//! R1.1 PoC scope (ADR-2605261800):
//!   - URDF parser supporting prismatic + revolute joints
//!   - link mass / inertia / axis extraction
//!   - hand-off to kami-genesis as an `ArticulatedSystem`
//!
//! Full URDF spec coverage (visual / collision meshes, mimic, transmission,
//! gazebo extensions) is deferred. Cartpole-class topologies only at R1.1.

pub const ADR: &str = "ADR-2605261800";
pub const PHASE: &str = "R1.1-cartpole-poc";
pub const KAMI_NAME: &str = "kami-articulated";
pub const NV_COMPAT_TARGET: &str = "isaacsim.core.prims.Articulation";
pub const SUPPORTED_FORMATS: &[&str] = &["urdf"];

mod urdf;

pub use urdf::{ArticulatedSystem, Inertia, Joint, JointKind, Link, ParseError, Pose, parse_urdf};
