//! kami-genesis — Genesis-compat physics backend for KAMI / e7m-sim.
//!
//! R1.1 PoC scope (ADR-2605261800):
//!   - closed-form Cartpole dynamics (Sutton & Barto 1983 formulation)
//!   - semi-implicit Euler integrator
//!   - PhysX-style `World` + `Articulation` API surface
//!
//! Full Genesis 5-solver Taichi → wgpu integration is deferred to R1.x
//! per ADR §D7. The R1.1 contract: `World::step()` produces results
//! within ±10% reward of Isaac Sim Cartpole-v1 baseline (G5 gate).
//!
//! API surface mirrors:
//!   - `isaacsim.core.api.{World, Articulation}` (Isaac Sim 4.x)
//!   - `PxScene` / `PxArticulationReducedCoordinate` (PhysX 5)
//! See `nv-compat/isaacsim` and `nv-compat/physx` for facade.

pub const ADR: &str = "ADR-2605261800";
pub const PHASE: &str = "R1.1-cartpole-poc";
pub const KAMI_NAME: &str = "kami-genesis";
pub const NV_COMPAT_TARGETS: &[&str] = &["isaacsim.core.api", "PhysX 5"];
pub const UPSTREAM_REPO: &str = "Genesis-Embodied-AI/Genesis";

pub const SOLVERS: &[&str] = &["rigid", "mpm", "sph", "fem", "pbd"];
pub const SOLVERS_IMPLEMENTED_R1_1: &[&str] = &["rigid (cartpole closed-form)"];

mod articulation3d;
mod batched;
mod cartpole;
mod ccd;
mod contact;
mod controllers;
mod convex;
mod double_pendulum;
mod ik;
mod isaac_api;
mod jacobian;
mod lqr;
mod mpm;
mod obb;
mod planar_chain;
mod spatial;
mod thermal;
mod trajectory;
mod vectorized;
mod world;

#[cfg(feature = "gpu")]
mod wgpu_backend;
#[cfg(feature = "gpu")]
mod wgpu_planar;

pub use articulation3d::{Articulation3dConfig, Articulation3dState, Body3d, JointType3d};
pub use cartpole::{CartpoleConfig, CartpoleState};
pub use contact::{Collider, ContactParams, ContactWorld, Obstacle};
pub use controllers::{ArticulationAction, ArticulationController};
pub use double_pendulum::{DoublePendulumConfig, DoublePendulumState};
pub use ik::{
    IkOptions, IkResult, TargetPose, solve_ik_cartpole, solve_ik_dp, solve_ik_planar_chain,
};
pub use isaac_api::{ArticulationView, ArticulationViewMut, IsaacWorld};
pub use jacobian::{
    Jacobian, cartpole_link_jacobian, dp_link_jacobian, planar_chain_link_jacobian,
};
pub use lqr::{LqrController, LqrWeights};
pub use planar_chain::{PlanarChainConfig, PlanarChainState};
pub use trajectory::{
    CubicPolynomialTrajectory, JointTrajectory, QuinticPolynomialTrajectory, WaypointTrajectory,
};
pub use vectorized::{WGSL_SOURCE, step_vectorized, step_vectorized_per_env};
pub use world::{Articulation, ArticulationHandle, LinkState, World};
// continuum + narrow-phase additions (R2 — close the PhysX/Isaac gap)
pub use batched::{ArticulationBatch, px};
pub use ccd::{conservative_advancement_toi, sphere_plane_toi};
pub use convex::{ConvexPoly, epa_penetration, gjk_closest_vec, gjk_distance, gjk_intersects};
pub use mpm::{MpmMaterial, MpmObstacle, MpmSolver};
pub use obb::{Manifold, Obb, obb_manifold, obb_sat};
pub use thermal::{Bc, ThermalField};

#[cfg(feature = "gpu")]
pub use wgpu_backend::WgpuBackend;
#[cfg(feature = "gpu")]
pub use wgpu_planar::PlanarChainGpu;

/// Solver coverage by R1 sub-phase.
pub fn solver_for_phase(phase: &str) -> Option<&'static str> {
    match phase {
        "R1.1" => Some("rigid"),
        "R1.8" => Some("mpm"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r1_1_uses_rigid_solver() {
        assert_eq!(solver_for_phase("R1.1"), Some("rigid"));
    }

    #[test]
    fn solvers_list_includes_all_five() {
        assert_eq!(SOLVERS.len(), 5);
    }
}
