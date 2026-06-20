//! kami-groot — clean-room NVIDIA Isaac GR00T N1.x foundation-policy compat.
//!
//! Mirrors the *public, documented* GR00T Vision-Language-Action surface
//! (model load → `obs → action` inference, embodiment/modality config,
//! action-chunking, teleop/imitation episode format) by **name and shape
//! only**. No NVIDIA library, header, binary, or model weight is linked,
//! vendored, or referenced (ADR-2605261800 §2(b) N1..N9 NEVER) — the seat is
//! `EmbodimentHead`-pluggable and the shipped default backend is KAMI-native
//! (the `kami-shugyo` gradient-free policy generalized to the VLA I/O shape),
//! so everything here builds, tests, and runs with **zero NVIDIA assets**.
//!
//! See `90-docs/adr/0037-kami-groot-foundation-policy-compat-surface.md`.

pub const ADR: &str = "ADR-0037";
pub const PHASE: &str = "R1.0-native-seat";
pub const KAMI_NAME: &str = "e7m-groot";
pub const NV_COMPAT_TARGET: &str = "gr00t.model.policy.Gr00tPolicy (Isaac GR00T N1.x)";

mod embodiment;
mod episode;
mod modality;
mod policy;
mod types;

pub use embodiment::{EmbodimentHead, NativeHead};
pub use episode::{Episode, EpisodeStep};
pub use modality::{EmbodimentConfig, Modality, ModalityConfig};
pub use policy::Gr00tPolicy;
pub use types::{Action, Frame, Observation};
