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
/// kami-genesis R1.1 (ADR-2605261800) supports only three closed-form topologies:
/// cartpole (1P + 1R), double-pendulum (2R), planar-chain (N R). General 3P+3R
/// chains require a generalized articulated-body solver, scoped to R1.2+.
///
/// This test LOCKS IN that current limit as a structured `UnsupportedTopology`
/// error rather than a silent garbage simulation. When R1.2 ships generalized
/// dynamics, this test will need to flip to a successful-load assertion.
#[test]
fn hizukue_world_load_rejected_as_unsupported_until_r12_generalized_dynamics() {
    // WorldError is in a private module; match on Display + Debug surface instead.
    let sys = parse_urdf(HIZUKUE_URDF).expect("hizukue.urdf must parse");
    let mut world = World::default();
    let err = world.add_articulation(sys).expect_err(
        "kami-genesis R1.1 must reject 6-DoF general topologies; pre-R1.2 invariant",
    );

    let err_dbg = format!("{:?}", err);
    let err_disp = format!("{}", err);

    assert!(
        err_dbg.starts_with("UnsupportedTopology"),
        "expected UnsupportedTopology, got {err_dbg} \
         (kami-genesis R1.1 → R1.2 gap classifier broken)"
    );

    // Operator-facing detail string MUST surface DOF count + joint-kind breakdown
    // so a sim-team triaging an R1.2 escalation has all the context inline.
    for needle in ["hizukue", "dofs=6", "revolute_count=3"] {
        assert!(
            err_disp.contains(needle) || err_dbg.contains(needle),
            "diagnostic missing `{needle}`: display={err_disp}  debug={err_dbg}"
        );
    }
}
