//! kami-autodrive — vehicle-class-agnostic autonomy (GNC) layer.
//!
//! This is the missing **guidance / navigation / control** stack that sits on
//! top of the kami-engine simulation primitives, analogous to how an AV stack
//! sits on top of NVIDIA Isaac Sim / DRIVE Sim: the simulator provides the
//! plant and sensors, the autonomy stack provides perception, planning, and
//! control. The kami pieces already existed in isolation —
//!
//!   * **plant** — `kami-vehicle` (BeamNG-grade soft-body car), `kami-genesis`
//!     (rigid/articulated bodies),
//!   * **sensors** — `kami-sensor-sim` (lidar / camera / IMU / contact,
//!     Isaac-Sim-API compatible),
//!   * **search** — `kami-pathfind` (A* grid + NavMesh).
//!
//! `kami-autodrive` is the wiring that closes the loop:
//!
//! ```text
//!   lidar sweep ─▶ perception (occupancy grid) ─▶ planner (A*) ─▶
//!   pure-pursuit + PID control ─▶ Command ─▶ plant ─▶ (new pose) ─▶ …
//! ```
//!
//! The [`Autopilot`] is plant-agnostic: the same loop drives the kinematic
//! [`BicycleModel`], the high-fidelity [`ShipHydro`] (Fossen hydrodynamics),
//! [`FixedWing`] (aerodynamics: lift/drag/stall/bank-to-turn), and
//! [`Multirotor`] (rotor thrust-vectoring + aero drag) plants, or — behind the
//! `soft-body-car` feature — a real `kami_vehicle::Vehicle` (the car, full
//! fidelity). See [`classes`] and [`dynamics`] for the per-class fidelity map.
//!
//! Related: ADR-2606010600 (kami-autodrive GNC layer), nv-compat target
//! `isaacsim` (Isaac Sim 4.x). Constitutional note: per ADR-2605242000
//! (wadachi), any real-world deployment is SAE L4-ceiling, Transparent Force
//! gated; this crate is a simulation/design substrate, not a fielded controller.

pub mod autopilot;
pub mod classes;
pub mod control;
pub mod dynamics;
pub mod perception;
pub mod planner;
pub mod plant;
pub mod types;

#[cfg(feature = "soft-body-car")]
pub mod vehicle_adapter;

pub use autopilot::{Autopilot, AutopilotConfig, DriveState};
pub use classes::{VehicleClass, VehicleLimits};
pub use control::{PurePursuit, SpeedController};
pub use dynamics::{FixedWing, Multirotor, ShipHydro};
pub use perception::OccupancyGrid;
pub use plant::{BicycleModel, Plant};
pub use types::{Command, Obstacle, Pose2};

/// ADR that introduces this crate.
pub const ADR: &str = "ADR-2606010600";
/// nv-compat reference surface.
pub const NV_COMPAT_TARGET: &str = "isaacsim";
