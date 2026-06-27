//! `VectorizedReachEnv` — a general articulated-arm joint-space reach task,
//! the manipulation counterpart to `VectorizedCartpoleEnv`.
//!
//! Mirrors `isaaclab.envs.ManagerBasedRLEnv` with `num_envs > 1`: it wraps the
//! matured `kami_genesis::ArticulationBatch` (any URDF arm, `[num_envs, n_dof]`
//! tensor I/O, implicit-PD actuator) into a Gym-style vectorized RL env. Each
//! env is given a per-env joint-space goal on reset and rewarded for driving its
//! joints to that goal — the smallest non-trivial manipulation task and the
//! scaffold the Franka pick-and-place reference (R1.5) slots into.
//!
//! Layout (env-major flat, like the Cartpole env):
//!   - action: `[num_envs, n_dof]` joint **position targets** (rad / m)
//!   - observation: `[num_envs, 3*n_dof]` = `[q, q̇, (q_goal − q)]`
//!   - reward: `−‖q − q_goal‖² − w·‖action‖²` (per env)
//!   - terminated: `‖q − q_goal‖∞ < goal_tol`; truncated: episode length cap

use crate::traits::{StepResult, VecRLEnv};
use kami_articulated::parse_urdf;
use kami_genesis::ArticulationBatch;

/// Minimal reproducible LCG (same constants as the Cartpole env) so resets are
/// seed-deterministic without pulling in an RNG crate. Shared with `ee_reach_env`.
pub(crate) struct Lcg(u64);
impl Lcg {
    pub(crate) fn new(seed: u64) -> Self {
        Lcg(seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }
    /// Next f32 in `[-1, 1)`.
    pub(crate) fn next_signed(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u = ((self.0 >> 40) as f32) / ((1u64 << 24) as f32); // [0,1)
        2.0 * u - 1.0
    }
}

/// Configuration for the reach task (sensible defaults via [`ReachCfg::default`]).
#[derive(Debug, Clone)]
pub struct ReachCfg {
    pub dt: f32,
    pub gravity_z: f32,
    /// PD stiffness / damping for the position-target actuator. In
    /// computed-torque mode these are acceleration gains (ω² and 2ζω).
    pub kp: f32,
    pub kd: f32,
    /// Use the computed-torque (feedback-linearizing) actuator — exact tracking
    /// under gravity/inertia, the right default for a manipulation reach task.
    pub computed_torque: bool,
    /// Interpret actions as normalized `[-1, 1]` and rescale to the joint
    /// position limits (the Isaac Lab squashed-policy convention). When false,
    /// actions are raw joint position targets.
    pub normalized_actions: bool,
    /// Touch-reach: radius (m) of the goal contact region sensed by an
    /// in-the-loop `ContactSensor`. `0` disables contact entirely (the EE-reach
    /// env stays a pure positioning task). Only used by `VectorizedEeReachEnv`.
    pub contact_radius: f32,
    /// Per-step reward bonus added when the end-effector is in contact with the
    /// goal region (requires `contact_radius > 0`).
    pub contact_bonus: f32,
    /// Append the contact flag (0/1) to the EE-reach observation so the policy
    /// can react to touch. Grows `observation_dim_per_env` by 1.
    pub observe_contact: bool,
    /// Tool-centre-point offset in the end-effector link frame (`VectorizedEe-
    /// ReachEnv` only). `[0,0,0]` controls the link frame origin; a non-zero
    /// offset (e.g. a gripper tip down the link) makes the reach target that
    /// tool point — observation, reward, goal sampling and IK all follow it.
    pub tool_offset: [f32; 3],
    /// Domain-randomisation actuator noise: each `step_all` perturbs the action
    /// by per-env uniform noise in `[-action_noise_std, action_noise_std]`
    /// before it drives the joints (Isaac Lab's action noise for sim-to-real
    /// robustness). `0` disables it. Seeded → reproducible.
    pub action_noise_std: f32,
    /// Per-env **gravity** domain randomisation: when `Some((low, high))`, every
    /// `reset_all` re-scales each env's gravity by a factor in `[low, high]`
    /// (physics DR for sim-to-real). `None` → all envs share the nominal gravity.
    pub gravity_dr: Option<(f32, f32)>,
    /// Per-env **mass/inertia** domain randomisation (scale factor range), applied
    /// each `reset_all` alongside `gravity_dr`. `None` → nominal masses.
    pub mass_dr: Option<(f32, f32)>,
    /// Observation (sensor) noise: per-element uniform noise in
    /// `[-obs_noise_std, obs_noise_std]` added to the observation the policy
    /// *receives* (the `StepResult`), while `observations_flat` stays the clean
    /// ground truth. `0` disables it. Seeded → reproducible.
    pub obs_noise_std: f32,
    /// Goals are sampled per joint uniformly in `[-goal_range, goal_range]`.
    pub goal_range: f32,
    /// `‖q − q_goal‖∞` below this terminates the episode (success).
    pub goal_tol: f32,
    /// Action-magnitude reward penalty weight.
    pub action_penalty: f32,
    /// Physics substeps per `step_all` (Isaac Lab `decimation`).
    pub decimation: usize,
    /// Episode length cap (control ticks) before truncation.
    pub max_steps: u32,
}

impl Default for ReachCfg {
    fn default() -> Self {
        ReachCfg {
            dt: 1.0 / 120.0,
            gravity_z: -9.81,
            kp: 100.0,
            kd: 20.0, // critically damped for ω=√kp=10 in computed-torque mode
            computed_torque: true,
            normalized_actions: false,
            contact_radius: 0.0,
            contact_bonus: 0.0,
            observe_contact: false,
            tool_offset: [0.0, 0.0, 0.0],
            action_noise_std: 0.0,
            gravity_dr: None,
            mass_dr: None,
            obs_noise_std: 0.0,
            goal_range: 0.8,
            goal_tol: 0.05,
            action_penalty: 0.001,
            decimation: 4,
            max_steps: 300,
        }
    }
}

/// Vectorized joint-space reach env over `num_envs` copies of one URDF arm.
pub struct VectorizedReachEnv {
    batch: ArticulationBatch,
    pub num_envs: usize,
    ndof: usize,
    cfg: ReachCfg,
    /// Per-env joint goals, `[num_envs * n_dof]` env-major.
    goals: Vec<f32>,
    /// Per-DOF `[lower, upper]` limits (for normalized-action rescaling).
    dof_limits: Vec<[f32; 2]>,
    steps: Vec<u32>,
    rngs: Vec<Lcg>,
    /// Separate RNG for action-noise DR (kept independent of goal sampling so
    /// reset's goal stream is unaffected by the noise setting).
    noise_rng: Lcg,
}

impl VectorizedReachEnv {
    /// Build `num_envs` arms from `urdf_text` with the given task config.
    pub fn new(num_envs: usize, urdf_text: &str, cfg: ReachCfg) -> Result<Self, String> {
        if num_envs == 0 {
            return Err("num_envs must be > 0".to_string());
        }
        let sys = parse_urdf(urdf_text).map_err(|e| format!("urdf parse: {e:?}"))?;
        let gravity = glam::Vec3::new(0.0, 0.0, cfg.gravity_z);
        let batch = ArticulationBatch::from_urdf(&sys, gravity, cfg.dt, num_envs);
        let ndof = batch.num_dof();
        if ndof == 0 {
            return Err("articulation has no actuated DOF".to_string());
        }
        let rngs = (0..num_envs).map(|i| Lcg::new(i as u64)).collect();
        let dof_limits = batch.get_dof_limits().to_vec();
        Ok(VectorizedReachEnv {
            batch,
            num_envs,
            ndof,
            cfg,
            goals: vec![0.0; num_envs * ndof],
            dof_limits,
            steps: vec![0; num_envs],
            rngs,
            noise_rng: Lcg::new(0x9E37_79B9),
        })
    }

    /// Per-DOF `[lower, upper]` position limits (from the URDF).
    pub fn dof_limits(&self) -> &[[f32; 2]] {
        &self.dof_limits
    }

    pub fn observation_dim_per_env(&self) -> usize {
        3 * self.ndof
    }

    pub fn action_dim_per_env(&self) -> usize {
        self.ndof
    }

    /// Ordered DOF (joint) names, as on the underlying batch view.
    pub fn dof_names(&self) -> &[String] {
        self.batch.dof_names()
    }

    /// Per-env joint goals, `[num_envs * n_dof]` env-major.
    pub fn goals(&self) -> &[f32] {
        &self.goals
    }

    /// Reset every env: zero the arm state, sample a fresh per-env joint goal
    /// (seeded by `base_seed`), and return the initial observation tensor.
    pub fn reset_all(&mut self, base_seed: Option<u64>) -> Vec<f32> {
        self.batch.reset();
        if let Some(s) = base_seed {
            self.noise_rng = Lcg::new(s ^ 0xA5A5_1234); // reproducible action noise
        }
        // Per-episode physics DR: re-randomise per-env gravity + mass each reset.
        let (g, m) = (self.cfg.gravity_dr, self.cfg.mass_dr);
        if g.is_some() || m.is_some() {
            self.batch.randomize_physics(
                base_seed.unwrap_or(0) ^ 0xD12,
                g.unwrap_or((1.0, 1.0)),
                m.unwrap_or((1.0, 1.0)),
            );
        } else {
            self.batch.clear_physics_randomization();
        }
        for e in 0..self.num_envs {
            if let Some(s) = base_seed {
                self.rngs[e] = Lcg::new(s.wrapping_add(e as u64));
            }
            for d in 0..self.ndof {
                self.goals[e * self.ndof + d] = self.rngs[e].next_signed() * self.cfg.goal_range;
            }
            self.steps[e] = 0;
        }
        self.observations_flat()
    }

    /// One control tick: interpret `actions` as `[num_envs, n_dof]` joint
    /// position targets, drive the implicit-PD actuator for `decimation`
    /// substeps, and return a per-env `StepResult`.
    pub fn step_all(&mut self, actions: &[f32]) -> Vec<StepResult> {
        assert_eq!(actions.len(), self.num_envs * self.ndof, "action shape");
        // Domain-randomisation actuator noise (uniform, per-env, reproducible).
        let noised;
        let actions: &[f32] = if self.cfg.action_noise_std > 0.0 {
            let std = self.cfg.action_noise_std;
            noised = actions
                .iter()
                .map(|a| a + self.noise_rng.next_signed() * std)
                .collect::<Vec<f32>>();
            &noised
        } else {
            actions
        };
        // Normalized actions ([-1,1]) are rescaled to the joint limits.
        let scaled;
        let targets: &[f32] = if self.cfg.normalized_actions {
            scaled = crate::policy::rescale_to_limits(actions, &self.dof_limits, self.num_envs);
            &scaled
        } else {
            actions
        };
        self.batch
            .set_joint_position_targets(targets, self.cfg.kp, self.cfg.kd);
        self.batch
            .set_computed_torque_control(self.cfg.computed_torque);
        for _ in 0..self.cfg.decimation {
            self.batch.step();
        }

        let q = self.batch.get_joint_positions();
        let mut out = Vec::with_capacity(self.num_envs);
        // Clean ground-truth obs; the policy-facing StepResult obs may carry
        // sensor noise (observations_flat itself stays clean).
        let mut obs_all = self.observations_flat();
        if self.cfg.obs_noise_std > 0.0 {
            let std = self.cfg.obs_noise_std;
            for v in obs_all.iter_mut() {
                *v += self.noise_rng.next_signed() * std;
            }
        }
        let od = self.observation_dim_per_env();
        for e in 0..self.num_envs {
            self.steps[e] += 1;
            let mut sq_err = 0.0f32;
            let mut max_err = 0.0f32;
            let mut act_sq = 0.0f32;
            for d in 0..self.ndof {
                let i = e * self.ndof + d;
                let err = q[i] - self.goals[i];
                sq_err += err * err;
                max_err = max_err.max(err.abs());
                act_sq += actions[i] * actions[i];
            }
            let reward = -sq_err - self.cfg.action_penalty * act_sq;
            let terminated = max_err < self.cfg.goal_tol;
            let truncated = self.steps[e] >= self.cfg.max_steps;
            out.push(StepResult {
                observation: obs_all[e * od..(e + 1) * od].to_vec(),
                reward,
                terminated,
                truncated,
            });
        }
        out
    }

    /// Per-env observations packed env-major: `[q, q̇, (q_goal − q)]` per env.
    pub fn observations_flat(&self) -> Vec<f32> {
        let q = self.batch.get_joint_positions();
        let qd = self.batch.get_joint_velocities();
        let mut out = Vec::with_capacity(self.num_envs * 3 * self.ndof);
        for e in 0..self.num_envs {
            let base = e * self.ndof;
            out.extend_from_slice(&q[base..base + self.ndof]);
            out.extend_from_slice(&qd[base..base + self.ndof]);
            for d in 0..self.ndof {
                out.push(self.goals[base + d] - q[base + d]);
            }
        }
        out
    }
}

impl VecRLEnv for VectorizedReachEnv {
    fn num_envs(&self) -> usize {
        self.num_envs
    }
    fn observation_dim_per_env(&self) -> usize {
        Self::observation_dim_per_env(self)
    }
    fn action_dim_per_env(&self) -> usize {
        Self::action_dim_per_env(self)
    }
    fn reset_all(&mut self, base_seed: Option<u64>) -> Vec<f32> {
        Self::reset_all(self, base_seed)
    }
    fn step_all(&mut self, actions: &[f32]) -> Vec<StepResult> {
        Self::step_all(self, actions)
    }
    fn observations_flat(&self) -> Vec<f32> {
        Self::observations_flat(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARM2_URDF: &str = r#"<robot name="arm2">
<link name="base"><inertial><mass value="1"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="shoulder" type="revolute"><parent link="base"/><child link="upper"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="upper"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="elbow" type="revolute"><parent link="upper"/><child link="fore"/><origin xyz="0 0 -1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="fore"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

    fn env(num_envs: usize) -> VectorizedReachEnv {
        VectorizedReachEnv::new(num_envs, ARM2_URDF, ReachCfg::default()).unwrap()
    }

    #[test]
    fn shapes_and_dof_names() {
        let mut e = env(4);
        assert_eq!(e.action_dim_per_env(), 2);
        assert_eq!(e.observation_dim_per_env(), 6);
        assert_eq!(
            e.dof_names(),
            &["shoulder".to_string(), "elbow".to_string()]
        );
        let obs = e.reset_all(Some(0));
        assert_eq!(obs.len(), 4 * 6);
        assert!(obs.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn reset_is_seed_deterministic() {
        let mut a = env(3);
        let mut b = env(3);
        let oa = a.reset_all(Some(42));
        let ob = b.reset_all(Some(42));
        assert_eq!(oa, ob, "same seed must give identical reset");
        let oc = a.reset_all(Some(7));
        assert_ne!(oa, oc, "different seed should differ");
        // Goals differ across envs (per-env sampling), not all identical.
        let g = a.goals();
        assert!(
            g[0..2] != g[2..4] || g[2..4] != g[4..6],
            "per-env goals identical"
        );
    }

    #[test]
    fn oracle_policy_reaches_goal_and_rewards_rise() {
        // The "oracle" policy commands the goal as the position target directly.
        // Reward (negative distance) must climb toward ~0 and the episode must
        // terminate (success) for every env within the step budget.
        let mut e = env(4);
        e.reset_all(Some(1));
        let goals = e.goals().to_vec();

        let first = e.step_all(&goals);
        let r0: f32 = first.iter().map(|s| s.reward).sum();

        let mut last = first;
        for _ in 0..299 {
            last = e.step_all(&goals);
        }
        let r1: f32 = last.iter().map(|s| s.reward).sum();
        assert!(r1 > r0, "reward did not improve: {r0} -> {r1}");
        assert!(
            last.iter().all(|s| s.terminated),
            "not all envs reached goal: {last:?}"
        );
        assert!(
            last.iter()
                .all(|s| s.observation.iter().all(|v| v.is_finite()))
        );
    }

    #[test]
    fn normalized_actions_rescale_to_limits_and_reach_goal() {
        // With normalized_actions, commanding the goal as a [-1,1] action
        // (mapped through the joint limits) must reach the joint-space goal.
        let cfg = ReachCfg {
            normalized_actions: true,
            ..Default::default()
        };
        let mut e = VectorizedReachEnv::new(4, ARM2_URDF, cfg).unwrap();
        e.reset_all(Some(5));
        let goals = e.goals().to_vec();
        let limits = e.dof_limits().to_vec();
        let n = e.num_envs;
        let ndof = e.action_dim_per_env();

        // Inverse of rescale_to_limits: goal (rad) → normalized action in [-1,1].
        let mut norm = vec![0.0f32; n * ndof];
        for env in 0..n {
            for d in 0..ndof {
                let g = goals[env * ndof + d];
                let [lo, hi] = limits[d];
                norm[env * ndof + d] = 2.0 * (g - lo) / (hi - lo) - 1.0;
                assert!(norm[env * ndof + d].abs() <= 1.0, "goal outside limits");
            }
        }
        let mut last = e.step_all(&norm);
        for _ in 0..299 {
            last = e.step_all(&norm);
        }
        assert!(
            last.iter().all(|s| s.terminated),
            "normalized control did not reach goal: {last:?}"
        );
    }

    #[test]
    fn gravity_dr_diverges_envs_and_is_reproducible() {
        // Passive arm (zero gains, no gravity comp) falling under gravity: with
        // per-env gravity DR the envs' joint angles diverge; without it they are
        // identical. The env-0 joint block is obs[0..ndof].
        let ndof = 2;
        let run = |dr: Option<(f32, f32)>, seed: u64| -> Vec<f32> {
            // Plain PD (no gravity comp) driven to a fixed target: the steady-
            // state droop g(q)/kp differs with per-env gravity → envs diverge.
            let cfg = ReachCfg {
                gravity_dr: dr,
                computed_torque: false,
                kp: 50.0,
                kd: 8.0,
                ..ReachCfg::default()
            };
            let mut e = VectorizedReachEnv::new(4, ARM2_URDF, cfg).unwrap();
            e.reset_all(Some(seed));
            let target = vec![0.5_f32; e.num_envs * e.action_dim_per_env()];
            for _ in 0..400 {
                e.step_all(&target);
            }
            e.observations_flat()
        };
        let od = 3 * ndof;
        let env_q = |obs: &[f32], e: usize| obs[e * od..e * od + ndof].to_vec();

        let with_dr = run(Some((0.3, 1.7)), 5);
        // Envs diverge under per-env gravity.
        assert!(
            (0..ndof).any(|d| (env_q(&with_dr, 0)[d] - env_q(&with_dr, 1)[d]).abs() > 1e-3),
            "gravity DR did not diverge the envs"
        );
        // Reproducible under a fixed seed.
        assert_eq!(
            with_dr,
            run(Some((0.3, 1.7)), 5),
            "gravity DR not reproducible"
        );
        // Without DR, all envs share gravity → identical joint evolution.
        let no_dr = run(None, 5);
        for e in 1..4 {
            assert!(
                (0..ndof).all(|d| (env_q(&no_dr, e)[d] - env_q(&no_dr, 0)[d]).abs() < 1e-6),
                "envs differ without gravity DR"
            );
        }
    }

    #[test]
    fn mass_dr_diverges_envs_under_fixed_torque() {
        // Mass DR exposed at the RL-env level: under a fixed (raw) action and no
        // gravity, heavier envs accelerate less → joint angles diverge. (Use
        // a near-zero-gravity-effect setup: drive via efforts by using a raw
        // position target with low gains so it acts like a steady push.)
        let ndof = 2;
        let run = |dr: Option<(f32, f32)>| -> Vec<f32> {
            let cfg = ReachCfg {
                mass_dr: dr,
                gravity_z: 0.0, // isolate mass from gravity
                computed_torque: false,
                kp: 40.0,
                kd: 4.0,
                ..ReachCfg::default()
            };
            let mut e = VectorizedReachEnv::new(4, ARM2_URDF, cfg).unwrap();
            e.reset_all(Some(2));
            let target = vec![0.6_f32; e.num_envs * e.action_dim_per_env()];
            for _ in 0..200 {
                e.step_all(&target);
            }
            e.observations_flat()
        };
        let od = 3 * ndof;
        let env_q = |obs: &[f32], e: usize| obs[e * od..e * od + ndof].to_vec();

        let with_dr = run(Some((0.4, 2.0)));
        assert!(
            (0..ndof).any(|d| (env_q(&with_dr, 0)[d] - env_q(&with_dr, 1)[d]).abs() > 1e-3),
            "mass DR did not diverge the envs"
        );
        let no_dr = run(None);
        for e in 1..4 {
            assert!(
                (0..ndof).all(|d| (env_q(&no_dr, e)[d] - env_q(&no_dr, 0)[d]).abs() < 1e-6),
                "envs differ without mass DR"
            );
        }
    }

    #[test]
    fn obs_noise_dr_noises_stepresult_but_keeps_ground_truth_clean() {
        // Observation noise appears in the StepResult the policy receives, while
        // observations_flat stays the clean ground truth. Reproducible by seed.
        let cfg = ReachCfg {
            obs_noise_std: 0.1,
            ..ReachCfg::default()
        };
        let mut e = VectorizedReachEnv::new(2, ARM2_URDF, cfg).unwrap();
        e.reset_all(Some(5));
        let cmd = vec![0.2_f32; e.num_envs * e.action_dim_per_env()];
        let res = e.step_all(&cmd);
        let clean = e.observations_flat(); // ground truth after the same step
        let od = e.observation_dim_per_env();
        // StepResult obs differs from the clean obs (noise added)…
        let noisy: Vec<f32> = res.iter().flat_map(|r| r.observation.clone()).collect();
        assert_eq!(noisy.len(), e.num_envs * od);
        assert!(
            (0..noisy.len()).any(|i| (noisy[i] - clean[i]).abs() > 1e-4),
            "obs noise did not perturb the StepResult"
        );
        // …but stays within the noise band of the truth (no blow-up).
        assert!((0..noisy.len()).all(|i| (noisy[i] - clean[i]).abs() < 0.11));

        // Reproducible: a fresh env, same seed + same action → identical noisy obs.
        let mut e2 = VectorizedReachEnv::new(
            2,
            ARM2_URDF,
            ReachCfg {
                obs_noise_std: 0.1,
                ..ReachCfg::default()
            },
        )
        .unwrap();
        e2.reset_all(Some(5));
        let res2 = e2.step_all(&cmd);
        let noisy2: Vec<f32> = res2.iter().flat_map(|r| r.observation.clone()).collect();
        assert_eq!(
            noisy, noisy2,
            "obs noise not reproducible under a fixed seed"
        );
    }

    #[test]
    fn action_noise_dr_is_reproducible_and_perturbs_the_trajectory() {
        // Action-noise DR must (a) be reproducible under a fixed seed, (b) change
        // the trajectory vs the no-noise run, and (c) stay finite/bounded.
        let run = |std: f32, seed: u64| -> Vec<f32> {
            let cfg = ReachCfg {
                action_noise_std: std,
                ..ReachCfg::default()
            };
            let mut e = VectorizedReachEnv::new(2, ARM2_URDF, cfg).unwrap();
            e.reset_all(Some(seed));
            let cmd = vec![0.3_f32; e.num_envs * e.action_dim_per_env()];
            for _ in 0..50 {
                e.step_all(&cmd);
            }
            e.observations_flat()
        };

        let a = run(0.2, 99);
        let b = run(0.2, 99);
        assert_eq!(a, b, "action-noise DR not reproducible under a fixed seed");
        let clean = run(0.0, 99);
        assert_ne!(a, clean, "action noise had no effect on the trajectory");
        assert!(
            a.iter().all(|v| v.is_finite()),
            "noisy rollout went non-finite"
        );
    }

    #[test]
    fn truncates_at_episode_cap_when_goal_unreachable() {
        // A short cap + a goal the (zero-gain-free) policy won't hold instantly:
        // command zeros while the goal is non-zero → never within tol → truncate.
        let cfg = ReachCfg {
            max_steps: 5,
            goal_range: 1.0,
            goal_tol: 1e-4,
            ..Default::default()
        };
        let mut e = VectorizedReachEnv::new(2, ARM2_URDF, cfg).unwrap();
        e.reset_all(Some(3));
        let zeros = vec![0.0; e.num_envs * e.action_dim_per_env()];
        let mut res = e.step_all(&zeros);
        for _ in 0..4 {
            res = e.step_all(&zeros);
        }
        assert!(
            res.iter().all(|s| s.truncated),
            "episode did not truncate at cap"
        );
    }
}
