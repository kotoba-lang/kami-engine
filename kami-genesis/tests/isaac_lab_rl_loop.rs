//! End-to-end Isaac Lab vectorized RL-loop integration test.
//!
//! Documentation-as-test: exercises the *whole* `ArticulationBatch` surface the
//! way an Isaac Lab `ManagerBasedRLEnv` would, proving the pieces compose into a
//! realistic vectorized rollout — no NVIDIA code, all KAMI-native solver:
//!
//!   1. build a `[num_envs, n_dof]` view from one URDF (`from_urdf`)
//!   2. per-env "reset to a sampled start state" (`set_joint_positions`/`_velocities`)
//!   3. an implicit-PD reach policy with gravity comp (`set_joint_position_targets`
//!      + `set_gravity_compensation`)
//!   4. step the batch and read observations every control tick
//!      (`get_joint_positions`/`_velocities`, `get_world_poses`, `get_jacobians`)
//!   5. name→DOF indexing for the action/observation layout (`dof_names`)
//!
//! It asserts the rollout invariants Isaac RL relies on: each env converges to
//! its *own* target, the envs stay independent (no cross-env contamination),
//! everything is finite, and the end-effector observation diverges per env.

use kami_articulated::parse_urdf;
use kami_genesis::ArticulationBatch;

/// A 2-DOF planar arm: enough that the distal link's pose/Jacobian depend on the
/// proximal joint, so per-env divergence is observable.
const ARM2_URDF: &str = r#"<robot name="arm2">
<link name="base"><inertial><mass value="1"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="shoulder" type="revolute"><parent link="base"/><child link="upper"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="upper"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="elbow" type="revolute"><parent link="upper"/><child link="fore"/><origin xyz="0 0 -1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="fore"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

/// Deterministic per-env spread so the test is reproducible without an RNG —
/// stands in for Isaac Lab's randomized reset/command sampling.
fn per_env_target(env: usize, num_envs: usize) -> [f32; 2] {
    let t = env as f32 / (num_envs as f32 - 1.0); // 0..1
    [(-0.6 + 1.2 * t), (0.5 - 0.8 * t)] // shoulder sweeps -0.6..0.6, elbow 0.5..-0.3
}

#[test]
fn vectorized_reach_rollout_converges_per_env_and_stays_independent() {
    const NUM_ENVS: usize = 8;
    const NDOF: usize = 2;
    let dt = 1.0 / 240.0;
    let gravity = glam::Vec3::new(0.0, 0.0, -9.81);

    let sys = parse_urdf(ARM2_URDF).expect("urdf parses");
    let mut envs = ArticulationBatch::from_urdf(&sys, gravity, dt, NUM_ENVS);

    // The action/observation layout is keyed by joint name, as in Isaac Lab.
    assert_eq!(
        envs.dof_names(),
        &["shoulder".to_string(), "elbow".to_string()]
    );
    let shoulder = envs.get_dof_index("shoulder").unwrap();
    let elbow = envs.get_dof_index("elbow").unwrap();
    assert_eq!((shoulder, elbow), (0, 1));

    // --- reset(): sample a per-env start state (small offset from each target) ---
    envs.reset();
    let mut start = vec![0.0_f32; NUM_ENVS * NDOF];
    for e in 0..NUM_ENVS {
        let tgt = per_env_target(e, NUM_ENVS);
        start[e * NDOF + shoulder] = tgt[0] - 0.3;
        start[e * NDOF + elbow] = tgt[1] + 0.2;
    }
    envs.set_joint_positions(&start);
    envs.set_joint_velocities(&vec![0.0; NUM_ENVS * NDOF]); // rest start

    // --- policy: drive each env to its own target with gravity-comped PD ---
    let mut targets = vec![0.0_f32; NUM_ENVS * NDOF];
    for e in 0..NUM_ENVS {
        let tgt = per_env_target(e, NUM_ENVS);
        targets[e * NDOF + shoulder] = tgt[0];
        targets[e * NDOF + elbow] = tgt[1];
    }
    envs.set_joint_position_targets_with_gains(&targets, &[120.0, 90.0], &[18.0, 12.0]);
    envs.set_gravity_compensation(true);

    // --- rollout: step and read observations each control tick ---
    let control_ticks = 40;
    let substeps = 100; // 40*100 = 4000 physics steps ≈ 16.7 s
    for _ in 0..control_ticks {
        for _ in 0..substeps {
            envs.step();
        }
        // Observation tensors must keep their Isaac shapes and stay finite.
        let q = envs.get_joint_positions();
        let qd = envs.get_joint_velocities();
        assert_eq!(q.len(), NUM_ENVS * NDOF);
        assert_eq!(qd.len(), NUM_ENVS * NDOF);
        assert!(
            q.iter().chain(qd.iter()).all(|v| v.is_finite()),
            "non-finite state"
        );

        // End-effector pose + Jacobian observations, one row per env.
        let (pos, quat) = envs.get_world_poses("fore").expect("ee link");
        let jac = envs.get_jacobians("fore").expect("ee jacobian");
        assert_eq!(pos.len(), NUM_ENVS);
        assert_eq!(quat.len(), NUM_ENVS);
        assert_eq!(jac.len(), NUM_ENVS);
        for j in &jac {
            assert_eq!(j.rows.len(), 6);
            assert_eq!(j.cols(), NDOF);
            assert!(j.rows.iter().flatten().all(|v| v.is_finite()));
        }
    }

    // --- terminal invariants ---
    let q = envs.get_joint_positions();
    let qd = envs.get_joint_velocities();
    for e in 0..NUM_ENVS {
        let tgt = per_env_target(e, NUM_ENVS);
        let es = (q[e * NDOF + shoulder] - tgt[0]).abs();
        let ee = (q[e * NDOF + elbow] - tgt[1]).abs();
        assert!(
            es < 0.03 && ee < 0.03,
            "env {e} did not reach target: q={:?} tgt={tgt:?}",
            &q[e * NDOF..e * NDOF + NDOF]
        );
        // Settled (near rest) — a converged reach.
        assert!(
            qd[e * NDOF + shoulder].abs() < 0.05 && qd[e * NDOF + elbow].abs() < 0.05,
            "env {e} not settled"
        );
    }

    // Envs are genuinely independent: distinct targets → distinct end-effector
    // world positions (no shared/aliased state across the batch).
    let (pos, _q) = envs.get_world_poses("fore").unwrap();
    let spread = pos
        .iter()
        .map(|p| p[0]) // end-effector x
        .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    assert!(
        spread.1 - spread.0 > 0.5,
        "end-effector positions did not diverge across envs"
    );
}

#[test]
fn reset_midway_returns_every_env_to_zero() {
    // Isaac `world.reset()` semantics on the batch: after driving away, a reset
    // returns all envs to the zero pose and clears the active PD drive.
    let sys = parse_urdf(ARM2_URDF).expect("urdf");
    let mut envs =
        ArticulationBatch::from_urdf(&sys, glam::Vec3::new(0.0, 0.0, -9.81), 1.0 / 240.0, 4);
    envs.set_joint_position_targets(&vec![0.5; 4 * 2], 100.0, 15.0);
    for _ in 0..500 {
        envs.step();
    }
    assert!(
        envs.get_joint_positions().iter().any(|&v| v.abs() > 0.05),
        "did not move"
    );

    envs.reset();
    let q = envs.get_joint_positions();
    let qd = envs.get_joint_velocities();
    assert!(q.iter().all(|&v| v == 0.0), "reset q not zero: {q:?}");
    assert!(qd.iter().all(|&v| v == 0.0), "reset qdot not zero");
    // Drive cleared: a plain step from zero with no command stays put (gravity
    // pulls slightly, but the old 0.5 target must not spring it back).
    envs.step();
    let q2 = envs.get_joint_positions();
    assert!(
        q2.iter().all(|v| v.is_finite()) && q2.iter().all(|&v| v.abs() < 0.05),
        "drive not cleared: {q2:?}"
    );
}
