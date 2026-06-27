//! Full-stack integration: physics (kami-genesis) → RL env (kami-shugyo) →
//! perception (kami-sensor-sim). A `VectorizedEeReachEnv` is driven to its
//! Cartesian goals; a `ContactSensor` (goal as a point obstacle, radius = a
//! success tolerance) then *perceives* task success from the env's end-effector
//! observation — the three Isaac-compat crates composed end-to-end.

use glam::Vec3;
use kami_sensor_sim::{ContactSensor, Primitive, Scene};
use kami_shugyo::{ReachCfg, VectorizedEeReachEnv};

const ARM2_URDF: &str = r#"<robot name="arm2">
<link name="base"><inertial><mass value="1"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="shoulder" type="revolute"><parent link="base"/><child link="upper"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="upper"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="elbow" type="revolute"><parent link="upper"/><child link="fore"/><origin xyz="0 0 -1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="fore"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

#[test]
fn contact_sensor_perceives_ee_reach_success() {
    let num_envs = 4;
    let mut env =
        VectorizedEeReachEnv::new(num_envs, ARM2_URDF, "fore", ReachCfg::default()).unwrap();
    env.reset_all(Some(7));
    let ndof = env.action_dim_per_env();
    let od = env.observation_dim_per_env();
    let goals = env.goals().to_vec(); // [num_envs * 3]

    // A ContactSensor whose collision sphere = an 8 cm success tolerance; the
    // goal is a point obstacle, so in_contact ⟺ ‖ee − goal‖ < 8 cm.
    let sensor = ContactSensor::new("touch", "/World/fore/touch", "fore", 0.08);
    let goal_scene = |e: usize| {
        let mut s = Scene::new();
        s.add(Primitive::Sphere {
            center: Vec3::new(goals[e * 3], goals[e * 3 + 1], goals[e * 3 + 2]),
            radius: 0.0,
        });
        s
    };
    let ee_from_obs = |obs: &[f32], e: usize| {
        let b = e * od + 2 * ndof; // obs = [q, q̇, ee_pos(3), goal−ee(3)]
        Vec3::new(obs[b], obs[b + 1], obs[b + 2])
    };

    // Before driving: the EE starts at the zero pose, away from the goal — at
    // least one env should read "no contact".
    let obs0 = env.observations_flat();
    let any_clear = (0..num_envs).any(|e| {
        !sensor
            .sample(ee_from_obs(&obs0, e), &goal_scene(e), 0.0)
            .in_contact
    });
    assert!(
        any_clear,
        "EE already at goal before any control — bad fixture"
    );

    // Drive every env to its goal with the reference (FK-derived) policy.
    let cmd = env.reference_joint_solution().to_vec();
    let mut last = env.step_all(&cmd);
    for _ in 0..299 {
        last = env.step_all(&cmd);
    }
    // The RL env reports success…
    assert!(
        last.iter().all(|s| s.terminated),
        "env did not reach goals: {last:?}"
    );

    // …and the ContactSensor independently *perceives* it: every EE is now in
    // contact with its goal region, with a finite contact normal.
    let obs = env.observations_flat();
    for e in 0..num_envs {
        let r = sensor.sample(ee_from_obs(&obs, e), &goal_scene(e), 0.0);
        assert!(
            r.in_contact,
            "env {e} EE not perceived at goal: {:?}",
            ee_from_obs(&obs, e)
        );
        assert!(r.contact_normal.is_finite());
    }
}
