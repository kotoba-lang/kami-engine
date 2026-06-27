//! `VecRLEnv` polymorphism test — one generic harness exercised against every
//! vectorized env, proving a trainer written to the trait runs over any task.

use kami_shugyo::{
    ReachCfg, VecRLEnv, VectorizedEeReachEnv, VectorizedReachEnv, run_zero_action_rollout,
};

const ARM2_URDF: &str = r#"<robot name="arm2">
<link name="base"><inertial><mass value="1"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="shoulder" type="revolute"><parent link="base"/><child link="upper"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="upper"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="elbow" type="revolute"><parent link="upper"/><child link="fore"/><origin xyz="0 0 -1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="fore"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

/// Generic contract every `VecRLEnv` must satisfy — exercised polymorphically.
fn assert_vec_rl_contract<E: VecRLEnv>(env: &mut E) {
    let n = env.num_envs();
    let od = env.observation_dim_per_env();
    let ad = env.action_dim_per_env();
    assert!(n > 0 && od > 0 && ad > 0);

    // reset → [num_envs, obs_dim], finite.
    let obs = env.reset_all(Some(123));
    assert_eq!(obs.len(), n * od, "reset obs shape");
    assert!(obs.iter().all(|v| v.is_finite()));
    // observations_flat agrees with reset's shape.
    assert_eq!(env.observations_flat().len(), n * od);

    // step → one StepResult per env, each obs row the right width, all finite.
    let action = vec![0.05_f32; n * ad];
    let results = env.step_all(&action);
    assert_eq!(results.len(), n, "one StepResult per env");
    for r in &results {
        assert_eq!(r.observation.len(), od, "per-env obs width");
        assert!(r.reward.is_finite());
        assert!(r.observation.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn joint_reach_satisfies_vec_rl_contract() {
    let mut e = VectorizedReachEnv::new(6, ARM2_URDF, ReachCfg::default()).unwrap();
    assert_vec_rl_contract(&mut e);
}

#[test]
fn ee_reach_satisfies_vec_rl_contract() {
    let mut e = VectorizedEeReachEnv::new(6, ARM2_URDF, "fore", ReachCfg::default()).unwrap();
    assert_vec_rl_contract(&mut e);
}

#[test]
fn generic_rollout_runs_over_any_env() {
    // The trainer-agnostic harness compiles and runs against different env types
    // through the trait alone, returning a finite total reward.
    let mut joint = VectorizedReachEnv::new(4, ARM2_URDF, ReachCfg::default()).unwrap();
    let mut ee = VectorizedEeReachEnv::new(4, ARM2_URDF, "fore", ReachCfg::default()).unwrap();

    let rj = run_zero_action_rollout(&mut joint, 20, Some(1));
    let re = run_zero_action_rollout(&mut ee, 20, Some(1));
    assert!(rj.is_finite() && re.is_finite());
    // Reward is negative distance-based, so a non-converged zero-action rollout
    // accrues strictly negative reward.
    assert!(
        rj < 0.0 && re < 0.0,
        "expected negative shaping reward: {rj}, {re}"
    );
}
