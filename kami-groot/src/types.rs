//! GR00T-shaped observation / action tensors.

/// A single camera frame feeding the `video` modality. `pixels` is RGB8
/// row-major and may be empty in headless tests (the shape is what the policy
/// binds against; the codec is ours, no NVIDIA decoder).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub camera: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// GR00T policy input: proprioceptive `state` (`[state_dim]`), zero or more
/// camera `video` frames, and an optional `language` instruction.
#[derive(Debug, Clone, Default)]
pub struct Observation {
    /// Proprioception (e.g. joint pos/vel) — `[state_dim]`, env-major.
    pub state: Vec<f32>,
    /// One frame per bound camera stream.
    pub video: Vec<Frame>,
    /// Free-form instruction slot (native backend treats it as conditioning).
    pub language: Option<String>,
}

/// GR00T policy output: an **action chunk** — `horizon` future steps of
/// `n_dof` joint targets, row-major `[horizon, n_dof]`. Action-chunking is the
/// GR00T shape; the native backend emits a myopic plan (a repeated one-step
/// action), but the shape lets a real checkpoint emit a genuine H-step plan.
#[derive(Debug, Clone, Default)]
pub struct Action {
    pub horizon: usize,
    pub n_dof: usize,
    /// `[horizon, n_dof]` row-major joint targets.
    pub joint_targets: Vec<f32>,
}

impl Action {
    /// Joint targets for chunk step `i` (`0..horizon`).
    pub fn step(&self, i: usize) -> &[f32] {
        &self.joint_targets[i * self.n_dof..(i + 1) * self.n_dof]
    }

    /// The immediately-executed step (`step(0)`) — what a non-chunked control
    /// loop consumes.
    pub fn first(&self) -> &[f32] {
        self.step(0)
    }
}
