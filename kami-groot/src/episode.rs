//! Clean-room LeRobot-shaped episode / teleop record format for imitation
//! fine-tuning. The schema is mirrored; the codec is ours — no NVIDIA or
//! HuggingFace dataset binary is required.

/// One recorded control tick: the observation state, the executed action, and
/// the active instruction.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EpisodeStep {
    pub state: Vec<f32>,
    /// The executed `[n_dof]` joint targets (chunk step 0).
    pub action: Vec<f32>,
    pub language: Option<String>,
}

/// A teleop / imitation episode over one embodiment.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Episode {
    pub embodiment: String,
    pub steps: Vec<EpisodeStep>,
}

impl Episode {
    pub fn new(embodiment: impl Into<String>) -> Self {
        Episode { embodiment: embodiment.into(), steps: Vec::new() }
    }

    /// Append a recorded step.
    pub fn push(&mut self, state: Vec<f32>, action: Vec<f32>, language: Option<String>) {
        self.steps.push(EpisodeStep { state, action, language });
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}
