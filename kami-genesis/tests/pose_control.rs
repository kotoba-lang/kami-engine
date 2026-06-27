//! End-to-end pose control: pose-IK → computed-torque drive → reach a full
//! 6-DOF Cartesian **pose** (position + orientation) under gravity.
//!
//! Where `tests/isaac_lab_rl_loop.rs` does joint-space reaching, this closes the
//! operational-space pose loop on a real 6-DOF arm: a reachable pose goal is
//! solved with `solve_ik_pose`, then *dynamically* driven (not teleported) with
//! the gravity-aware computed-torque actuator, and the achieved end-effector
//! pose is checked against the target. All KAMI-native solver, no NVIDIA code.

use kami_articulated::parse_urdf;
use kami_genesis::ArticulationBatch;

const ARM6_URDF: &str = include_str!("../../fixtures/giemon_arm6/giemon_arm6.urdf");

#[test]
fn pose_ik_then_computed_torque_drive_reaches_the_target_pose() {
    const NUM_ENVS: usize = 2;
    let dt = 1.0 / 240.0;
    let sys = parse_urdf(ARM6_URDF).expect("arm6 urdf");
    let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::new(0.0, 0.0, -9.81), dt, NUM_ENVS);
    assert_eq!(b.num_dof(), 6);

    // Reachable per-env pose targets = FK of two distinct known configs.
    b.set_joint_positions(&[
        0.3, -0.4, 0.5, 0.2, -0.3, 0.4, // env0
        -0.2, 0.3, -0.4, 0.5, 0.2, -0.3, // env1
    ]);
    let (pos, quat) = b.get_world_poses("link6").unwrap();
    let targets_pos: Vec<f32> = pos.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let targets_quat: Vec<f32> = quat.iter().flat_map(|q| [q[0], q[1], q[2], q[3]]).collect();

    // Back to the zero pose; solve pose-IK toward the targets.
    b.reset();
    let sol = b
        .solve_ik_pose("link6", &targets_pos, &targets_quat, 500, 0.02)
        .expect("link6");

    // DYNAMICALLY drive to the IK solution with the gravity-aware computed-torque
    // actuator (not a teleport) — the controller must hold the pose under gravity.
    b.set_joint_position_targets(&sol, 100.0, 20.0);
    b.set_computed_torque_control(true);
    for _ in 0..3000 {
        b.step();
    }

    let (pos2, quat2) = b.get_world_poses("link6").unwrap();
    for e in 0..NUM_ENVS {
        let dpos = ((pos2[e][0] - targets_pos[e * 3]).powi(2)
            + (pos2[e][1] - targets_pos[e * 3 + 1]).powi(2)
            + (pos2[e][2] - targets_pos[e * 3 + 2]).powi(2))
        .sqrt();
        assert!(dpos < 0.05, "env {e} position not held: {dpos} m");
        // Orientation: |q · q_target| ≈ 1 (same rotation up to sign).
        let dot: f32 = (0..4).map(|i| quat2[e][i] * targets_quat[e * 4 + i]).sum();
        assert!(dot.abs() > 0.98, "env {e} orientation not held: dot {dot}");
        // Finite, settled.
        assert!(pos2[e].iter().all(|v| v.is_finite()));
    }
}
