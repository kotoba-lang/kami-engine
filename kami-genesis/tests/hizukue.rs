//! Hizukue (日柄) 6-DoF panel-tracker servicer robot — end-to-end sim smoke test.
//!
//! Per ADR-2605261800 §G7 (PhysX NEVER) + §G8 (OptiX/RTX NEVER), this test
//! exercises the kami-genesis Apache-2.0 Genesis-5-solver physics stack via the
//! same `kami_articulated::parse_urdf` → `World::add_articulation` path used
//! by the cartpole + double-pendulum + planar-chain unit tests in
//! `kami-genesis/src/world.rs`.
//!
//! Pairs with `70-tools/e7m-sim/scenes/hikari-r1-solar-tracker-2km/scene.yaml`
//! (energy-themed sim scene introduced in ADR-2605263500 D1..D5 iteration 2;
//! Hizukue is the articulated_robot layer in that scene).
//!
//! Verifies:
//! - URDF parses (7 links + 6 joints — 3 prismatic mobile base + 3 revolute arm)
//! - System adds to the World without topology error
//! - Joint kind breakdown matches the documented kinematic chain
//! - Articulation appears in World handle registry

use kami_articulated::{JointKind, parse_urdf};
use kami_genesis::World;

const HIZUKUE_URDF: &str =
    include_str!("../../kami-articulated/urdf/hizukue.urdf");

#[test]
fn hizukue_urdf_parses_with_expected_topology() {
    let sys = parse_urdf(HIZUKUE_URDF).expect("hizukue.urdf must parse");

    assert_eq!(sys.name, "hizukue", "robot name");
    assert_eq!(sys.links.len(), 7, "world + 3 base + upper_arm + lower_arm + end_effector");
    assert_eq!(sys.joints.len(), 6, "3 prismatic mobile base + 3 revolute arm");

    let prismatic_count = sys.joints.iter().filter(|j| j.kind == JointKind::Prismatic).count();
    let revolute_count = sys.joints.iter().filter(|j| j.kind == JointKind::Revolute).count();

    assert_eq!(prismatic_count, 3, "mobile base XYZ");
    assert_eq!(revolute_count, 3, "shoulder + elbow + wrist");

    // Verify the kinematic chain: world → base_x → base_y → base_z → upper_arm → lower_arm → end_effector
    let chain: Vec<(&str, &str)> = sys
        .joints
        .iter()
        .map(|j| (j.parent.as_str(), j.child.as_str()))
        .collect();
    assert_eq!(
        chain,
        vec![
            ("world", "base_x"),
            ("base_x", "base_y"),
            ("base_y", "base_z"),
            ("base_z", "upper_arm"),
            ("upper_arm", "lower_arm"),
            ("lower_arm", "end_effector"),
        ],
    );
}

/// Hizukue is a 6-DoF (3 prismatic + 3 revolute) general-purpose serial chain.
///
/// Through ADR-2605311500 this was rejected as `UnsupportedTopology`: the
/// engine only had the closed-form / planar topologies (cartpole, double
/// pendulum, planar chain). ADR-2605311800 added the full 3-D spatial-vector
/// solver (`articulation3d`, Featherstone RNEA + CRBA) as the `Spatial3d`
/// fallback, so a mixed prismatic+revolute 3-D chain now loads and simulates.
///
/// This test was flipped (as its prior incarnation predicted it would be) from
/// a rejection assertion to a successful-load + stable-step assertion.
#[test]
fn hizukue_world_loads_as_spatial3d_and_steps() {
    let sys = parse_urdf(HIZUKUE_URDF).expect("hizukue.urdf must parse");
    let mut world = World::default();
    let h = world
        .add_articulation(sys)
        .expect("6-DoF general topology now supported via the 3-D spatial solver");

    // 6 DOF: 3 prismatic mobile base + 3 revolute arm.
    let q0 = world.get(h).unwrap().joint_positions();
    assert_eq!(q0.len(), 6, "3 prismatic + 3 revolute = 6 DOF");

    // Steps under gravity without blow-up (no NaN from the LDLᵀ solve).
    for _ in 0..120 {
        world.step();
    }
    assert!(
        world.get(h).unwrap().joint_positions().iter().all(|v| v.is_finite()),
        "hizukue state went non-finite"
    );
}
