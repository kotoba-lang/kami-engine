//! `Gr00tPolicy` — the clean-room mirror of GR00T's policy lifecycle
//! (`from_pretrained` / `reset` / `get_action`).

use kami_shugyo::rescale_to_limits;

use crate::embodiment::{EmbodimentHead, NativeHead};
use crate::modality::EmbodimentConfig;
use crate::types::{Action, Observation};

/// A foundation-policy-shaped controller. The backend is an [`EmbodimentHead`]
/// trait object, so the same surface serves the native default policy or an
/// out-of-tree checkpoint backend.
pub struct Gr00tPolicy {
    pub embodiment: EmbodimentConfig,
    backend: Box<dyn EmbodimentHead + Send>,
    loaded_checkpoint: Option<String>,
}

impl Gr00tPolicy {
    /// Mirror `Gr00tPolicy.from_pretrained(path, embodiment)`. With no weights
    /// at `path` (the only charter-clean case), instantiate the KAMI-native
    /// default backend and log honestly that no checkpoint was loaded — so the
    /// surface is exercisable with **zero NVIDIA assets**. Loading an actual
    /// checkpoint is an optional out-of-tree backend via [`with_backend`].
    ///
    /// [`with_backend`]: Gr00tPolicy::with_backend
    pub fn from_pretrained(path: impl Into<String>, embodiment: EmbodimentConfig) -> Self {
        let path = path.into();
        let head = NativeHead::zeros(embodiment.modality.state_dim, embodiment.n_dof());
        log::info!(
            "kami-groot: native backend for embodiment {:?} (no checkpoint loaded from {:?})",
            embodiment.name,
            path
        );
        Gr00tPolicy { embodiment, backend: Box::new(head), loaded_checkpoint: None }
    }

    /// Explicit native seat (`from_pretrained` with an empty path).
    pub fn native(embodiment: EmbodimentConfig) -> Self {
        Self::from_pretrained(String::new(), embodiment)
    }

    /// Install a caller-supplied backend (e.g. a trained `NativeHead`, or an
    /// out-of-tree checkpoint adapter). `checkpoint` is recorded for provenance.
    pub fn with_backend(
        embodiment: EmbodimentConfig,
        backend: Box<dyn EmbodimentHead + Send>,
        checkpoint: Option<String>,
    ) -> Self {
        Gr00tPolicy { embodiment, backend, loaded_checkpoint: checkpoint }
    }

    /// Reset per-episode policy state. The native backend is stateless, so this
    /// is a no-op; the method exists to mirror the GR00T surface.
    pub fn reset(&mut self) {}

    /// Mirror `policy.get_action(obs)`. Runs the backend on the proprioceptive
    /// state, rescales the normalized `[-1, 1]` action to the embodiment's joint
    /// limits, and tiles it across the action horizon as a constant chunk (the
    /// native backend is myopic; the `[horizon, n_dof]` shape is preserved so a
    /// real checkpoint can emit a genuine plan).
    pub fn get_action(&self, obs: &Observation) -> Action {
        let n_dof = self.embodiment.n_dof();
        let horizon = self.embodiment.action_horizon.max(1);

        let normalized = self.backend.act(&obs.state);
        let one = rescale_to_limits(&normalized, &self.embodiment.dof_limits, 1);

        let mut joint_targets = Vec::with_capacity(horizon * n_dof);
        for _ in 0..horizon {
            joint_targets.extend_from_slice(&one);
        }
        Action { horizon, n_dof, joint_targets }
    }

    /// The provenance of the loaded checkpoint, or `None` for the native seat.
    pub fn checkpoint(&self) -> Option<&str> {
        self.loaded_checkpoint.as_deref()
    }
}
