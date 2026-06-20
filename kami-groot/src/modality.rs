//! Embodiment + modality configuration — binds a robot's DOF order, camera
//! streams, and language slot onto the policy's typed I/O heads (à la GR00T's
//! `ModalityConfig` / embodiment data-config).

/// The four GR00T modality channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    State,
    Action,
    Video,
    Language,
}

/// Which modalities the policy consumes/produces and at what width.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModalityConfig {
    /// Proprioceptive observation width.
    pub state_dim: usize,
    /// Action width (per chunk step) — normally `n_dof`.
    pub action_dim: usize,
    /// Bound camera stream names.
    pub cameras: Vec<String>,
    /// Whether the language instruction slot is active.
    pub language: bool,
}

/// Maps a concrete robot onto the GR00T I/O heads. Built from URDF-derived
/// actuated-joint names + limits (e.g. kami-genesis `dof_names` /
/// `get_dof_limits`) plus the sensor-rig camera list — not hardcoded.
#[derive(Debug, Clone, Default)]
pub struct EmbodimentConfig {
    pub name: String,
    /// Ordered actuated-joint names (defines the action/state DOF order).
    pub dof_names: Vec<String>,
    /// Per-DOF `[lower, upper]` joint limits (RL action rescaling).
    pub dof_limits: Vec<[f32; 2]>,
    /// Bound camera streams (fill the `video` modality).
    pub cameras: Vec<String>,
    /// Action-chunk horizon (`>= 1`).
    pub action_horizon: usize,
    pub modality: ModalityConfig,
}

impl EmbodimentConfig {
    /// Build from a robot's actuated-joint names + limits and a camera list.
    /// `state_dim` defaults to the DOF count (joint-position proprioception);
    /// callers wanting pos+vel set `modality.state_dim` themselves afterwards.
    pub fn from_robot(
        name: impl Into<String>,
        dof_names: Vec<String>,
        dof_limits: Vec<[f32; 2]>,
        cameras: Vec<String>,
        action_horizon: usize,
    ) -> Self {
        let n_dof = dof_names.len();
        assert_eq!(dof_limits.len(), n_dof, "dof_limits must be [n_dof]");
        let modality = ModalityConfig {
            state_dim: n_dof,
            action_dim: n_dof,
            cameras: cameras.clone(),
            language: true,
        };
        EmbodimentConfig {
            name: name.into(),
            dof_names,
            dof_limits,
            cameras,
            action_horizon: action_horizon.max(1),
            modality,
        }
    }

    /// Number of actuated DOFs.
    pub fn n_dof(&self) -> usize {
        self.dof_names.len()
    }
}
