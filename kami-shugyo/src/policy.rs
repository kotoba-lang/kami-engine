//! Minimal gradient-free policy + trainer over any [`VecRLEnv`].
//!
//! The point: kami-shugyo is an RL *training* framework, so it must be able to
//! learn a policy, not just simulate. This provides a goal-conditioned
//! `LinearPolicy` (`a = W·obs + b`) and `random_search` — a hill-climbing /
//! Augmented-Random-Search-lite optimizer that perturbs the policy, re-evaluates
//! the vectorized return under a *fixed* goal distribution (same eval seed), and
//! keeps improvements. No autodiff, no external crates; deterministic given a
//! seed. The vectorized env is what makes each evaluation cheap (all envs scored
//! in lockstep).

use crate::reach_env::Lcg;
use crate::traits::VecRLEnv;

/// Affine policy `action = W·obs + b`, mapping a per-env observation row
/// (`obs_dim`) to an action row (`act_dim`). Row-major `w[a*obs_dim + o]`.
#[derive(Debug, Clone)]
pub struct LinearPolicy {
    pub obs_dim: usize,
    pub act_dim: usize,
    pub w: Vec<f32>,
    pub b: Vec<f32>,
}

impl LinearPolicy {
    pub fn zeros(obs_dim: usize, act_dim: usize) -> Self {
        LinearPolicy {
            obs_dim,
            act_dim,
            w: vec![0.0; obs_dim * act_dim],
            b: vec![0.0; act_dim],
        }
    }

    /// Map a `[num_envs, obs_dim]` observation tensor to a `[num_envs, act_dim]`
    /// action tensor (env-major flat).
    pub fn act_batch(&self, obs: &[f32], num_envs: usize) -> Vec<f32> {
        let mut out = vec![0.0_f32; num_envs * self.act_dim];
        for e in 0..num_envs {
            let o = &obs[e * self.obs_dim..(e + 1) * self.obs_dim];
            for a in 0..self.act_dim {
                let mut acc = self.b[a];
                let row = &self.w[a * self.obs_dim..(a + 1) * self.obs_dim];
                for (wi, oi) in row.iter().zip(o.iter()) {
                    acc += wi * oi;
                }
                out[e * self.act_dim + a] = acc;
            }
        }
        out
    }

    /// A gaussian-perturbed copy (Box-Muller noise scaled by `sigma`).
    fn perturbed(&self, rng: &mut Lcg, sigma: f32) -> LinearPolicy {
        let mut p = self.clone();
        for x in p.w.iter_mut().chain(p.b.iter_mut()) {
            *x += sigma * gaussian(rng);
        }
        p
    }
}

/// Map a `[num_envs, n_dof]` normalized action tensor in `[-1, 1]` to joint
/// targets in `[lower, upper]` per DOF — the standard Isaac Lab action pipeline
/// (a squashed policy outputs `[-1,1]`; the env rescales to the joint range).
/// `limits` is `[n_dof]` of `[lower, upper]`, broadcast across envs. A DOF with
/// a non-finite limit (unbounded joint) passes its action through unchanged.
pub fn rescale_to_limits(normalized: &[f32], limits: &[[f32; 2]], num_envs: usize) -> Vec<f32> {
    let ndof = limits.len();
    let mut out = vec![0.0_f32; num_envs * ndof];
    for e in 0..num_envs {
        for d in 0..ndof {
            let a = normalized[e * ndof + d].clamp(-1.0, 1.0);
            let [lo, hi] = limits[d];
            out[e * ndof + d] = if lo.is_finite() && hi.is_finite() {
                lo + (a * 0.5 + 0.5) * (hi - lo)
            } else {
                normalized[e * ndof + d]
            };
        }
    }
    out
}

/// Standard-normal sample from the uniform `Lcg` via Box-Muller.
fn gaussian(rng: &mut Lcg) -> f32 {
    // next_signed ∈ [-1,1) → map to (0,1] for the logs.
    let u1 = (rng.next_signed() * 0.5 + 0.5).max(1e-6);
    let u2 = rng.next_signed() * 0.5 + 0.5;
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Roll out `policy` on `env` for `episode_len` control ticks and return the
/// total reward summed over envs and ticks (higher = better). The `seed` fixes
/// the per-env goal distribution so two policies are compared on the same task.
pub fn evaluate<E: VecRLEnv>(
    env: &mut E,
    policy: &LinearPolicy,
    episode_len: usize,
    seed: u64,
) -> f32 {
    let mut obs = env.reset_all(Some(seed));
    let n = env.num_envs();
    let od = env.observation_dim_per_env();
    let mut total = 0.0_f32;
    for _ in 0..episode_len {
        let action = policy.act_batch(&obs, n);
        let results = env.step_all(&action);
        // The policy acts on what `step` returns (carrying any sensor/obs noise),
        // not the clean ground truth — so observation-noise DR actually shapes
        // training. Re-assemble the [num_envs, obs_dim] tensor from the rows.
        obs.clear();
        for r in &results {
            total += r.reward;
            obs.extend_from_slice(&r.observation);
        }
        debug_assert_eq!(obs.len(), n * od);
    }
    total
}

/// Hill-climbing random search (ARS-lite): start from `init`, perturb, keep the
/// candidate if it scores higher on the fixed-seed task, repeat for `iters`.
/// Returns the best policy found and the best-score-so-far history
/// (`iters + 1` entries, monotone non-decreasing).
pub fn random_search<E: VecRLEnv>(
    env: &mut E,
    init: LinearPolicy,
    iters: usize,
    sigma: f32,
    episode_len: usize,
    seed: u64,
) -> (LinearPolicy, Vec<f32>) {
    let eval_seed = seed ^ 0x5151_5151;
    let mut best = init;
    let mut best_score = evaluate(env, &best, episode_len, eval_seed);
    let mut history = vec![best_score];
    let mut rng = Lcg::new(seed);
    for _ in 0..iters {
        let cand = best.perturbed(&mut rng, sigma);
        let score = evaluate(env, &cand, episode_len, eval_seed);
        if score > best_score {
            best = cand;
            best_score = score;
        }
        history.push(best_score);
    }
    (best, history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reach_env::{ReachCfg, VectorizedReachEnv};

    const ARM2_URDF: &str = r#"<robot name="arm2">
<link name="base"><inertial><mass value="1"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="shoulder" type="revolute"><parent link="base"/><child link="upper"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="upper"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="elbow" type="revolute"><parent link="upper"/><child link="fore"/><origin xyz="0 0 -1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="80" velocity="10"/><dynamics damping="0"/></joint>
<link name="fore"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

    fn reach_env(n: usize) -> VectorizedReachEnv {
        VectorizedReachEnv::new(n, ARM2_URDF, ReachCfg::default()).unwrap()
    }

    /// The hand-built optimal policy for joint-space reach: command the goal,
    /// i.e. `a = q + (goal − q)`. obs = [q, q̇, goal−q] so a_d = obs[d] +
    /// obs[2*ndof + d]. This validates the linear policy class can solve the task.
    fn oracle_linear(ndof: usize) -> LinearPolicy {
        let obs_dim = 3 * ndof;
        let mut p = LinearPolicy::zeros(obs_dim, ndof);
        for d in 0..ndof {
            p.w[d * obs_dim + d] = 1.0; // + q_d
            p.w[d * obs_dim + (2 * ndof + d)] = 1.0; // + (goal-q)_d
        }
        p
    }

    #[test]
    fn linear_policy_class_can_express_the_reach_solver() {
        let mut env = reach_env(4);
        let ndof = 2;
        let zero = evaluate(&mut env, &LinearPolicy::zeros(3 * ndof, ndof), 120, 7);
        let oracle = evaluate(&mut env, &oracle_linear(ndof), 120, 7);
        // Both cumulative returns are negative (distance cost over the episode);
        // the oracle converges so it accrues only the start→goal transient and
        // must dramatically out-return the do-nothing policy (which pays the full
        // ‖goal‖² every tick). Expect the oracle's cost well under 1/4 of zero's.
        assert!(zero < 0.0 && oracle < 0.0);
        assert!(
            oracle > zero,
            "oracle {oracle} not better than zero-policy {zero}"
        );
        assert!(
            oracle > zero * 0.25,
            "oracle {oracle} not dramatically better than {zero}"
        );
    }

    #[test]
    fn rescale_maps_unit_interval_to_joint_limits() {
        let limits = [[-2.0, 2.0], [0.0, 1.0]];
        // 2 envs, 2 dof. -1→lower, 0→mid, +1→upper; out-of-range clamps.
        let norm = [
            -1.0, 0.0, // env0
            1.0, 2.0, // env1 (2.0 clamps to +1 → upper)
        ];
        let out = rescale_to_limits(&norm, &limits, 2);
        assert!((out[0] + 2.0).abs() < 1e-6, "-1→lower: {}", out[0]);
        assert!((out[1] - 0.5).abs() < 1e-6, "0→mid: {}", out[1]);
        assert!((out[2] - 2.0).abs() < 1e-6, "+1→upper: {}", out[2]);
        assert!((out[3] - 1.0).abs() < 1e-6, "clamp→upper: {}", out[3]);
        // Unbounded DOF passes through.
        let inf = rescale_to_limits(&[0.7], &[[f32::NEG_INFINITY, f32::INFINITY]], 1);
        assert!((inf[0] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn random_search_improves_return() {
        let mut env = reach_env(4);
        let ndof = 2;
        let init = LinearPolicy::zeros(3 * ndof, ndof);
        let (_best, history) = random_search(&mut env, init, 60, 0.3, 80, 123);
        // best-score history is monotone non-decreasing by construction…
        for w in history.windows(2) {
            assert!(w[1] >= w[0] - 1e-6, "history not monotone: {:?}", w);
        }
        // …and the search must actually find an improvement over the init.
        assert!(
            *history.last().unwrap() > history[0] + 1e-3,
            "no improvement: {} -> {}",
            history[0],
            history.last().unwrap()
        );
    }
}
