// kami-mangaka-scene — headless 3D scene composition facade for mangaka.gftd.ai.
// ADR-2605141200. Composes existing kami-* crates; no engine-internal modifications.
// P0 skeleton: types + builder API. Renderer + simulation wiring lands in P1–P4.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod scene;
pub mod pose;
pub mod camera;
pub mod render;
pub mod sim;
pub mod lexicon;

#[cfg(not(target_family = "wasm"))]
pub mod renderer;

#[cfg(target_family = "wasm")]
pub mod web;

pub use camera::{CameraSpec, LightSpec, ShotGrammar};
pub use pose::{Expression, PoseSpec};
pub use render::{RenderOpts, RenderPasses, RenderResult};
pub use scene::{EnvironmentSpec, MangakaScene, Transform};
pub use sim::FxKind;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CharacterId(pub u32);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PropId(pub u32);

#[derive(Debug, Error)]
pub enum SceneError {
    #[error("vrm decode failed: {0}")]
    VrmDecode(String),
    #[error("gltf decode failed: {0}")]
    GltfDecode(String),
    #[error("render failed: {0}")]
    Render(String),
    #[error("jsonld roundtrip failed: {0}")]
    Jsonld(String),
}

pub type Result<T> = std::result::Result<T, SceneError>;

#[cfg(feature = "python")]
mod py;
