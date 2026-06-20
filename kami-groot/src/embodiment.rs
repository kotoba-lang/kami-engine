//! The embodiment-head seat: the GR00T "new-embodiment head" idea as a trait —
//! a small adapter mapping the shared policy latent to a specific robot's
//! action space. Swapping the head is how the same surface serves a native
//! policy or an out-of-tree checkpoint backend.

use kami_shugyo::LinearPolicy;

/// Maps a proprioceptive observation row to a **normalized** one-step action
/// (`[-1, 1]^n_dof`, the standard Isaac/GR00T squashed-action convention; the
/// policy rescales to joint limits downstream).
pub trait EmbodimentHead {
    /// Normalized one-step action for `obs_state` (`[state_dim]`).
    fn act(&self, obs_state: &[f32]) -> Vec<f32>;

    /// Actuated DOF count (length of the returned action).
    fn n_dof(&self) -> usize;
}

/// The shipped default backend: the `kami-shugyo` affine `LinearPolicy`
/// (`a = W·obs + b`) trained by its gradient-free `random_search`. Charter-clean
/// — no foundation-model weights. A zeros policy emits the joint-limit midpoint.
#[derive(Debug, Clone)]
pub struct NativeHead {
    policy: LinearPolicy,
    n_dof: usize,
}

impl NativeHead {
    /// A zero-initialized head (`state_dim → n_dof`).
    pub fn zeros(state_dim: usize, n_dof: usize) -> Self {
        NativeHead { policy: LinearPolicy::zeros(state_dim, n_dof), n_dof }
    }

    /// Wrap an already-built / already-trained `LinearPolicy`.
    pub fn from_policy(policy: LinearPolicy) -> Self {
        let n_dof = policy.act_dim;
        NativeHead { policy, n_dof }
    }

    /// Mutable access to the underlying policy (for the trainer to learn it).
    pub fn policy_mut(&mut self) -> &mut LinearPolicy {
        &mut self.policy
    }
}

impl EmbodimentHead for NativeHead {
    fn act(&self, obs_state: &[f32]) -> Vec<f32> {
        // Single-env evaluation, then squash to the [-1, 1] action convention.
        self.policy
            .act_batch(obs_state, 1)
            .into_iter()
            .map(|x| x.clamp(-1.0, 1.0))
            .collect()
    }

    fn n_dof(&self) -> usize {
        self.n_dof
    }
}
