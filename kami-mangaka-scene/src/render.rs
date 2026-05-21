// Render options + result types. The actual GPU work lives in `renderer.rs`
// (gated on non-wasm targets). `MangakaScene::render*` delegate.

use serde::{Deserialize, Serialize};

use crate::{camera::CameraSpec, scene::MangakaScene, Result};
#[cfg(target_family = "wasm")]
use crate::SceneError;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct RenderPasses: u32 {
        const BASE    = 0b0001;
        const DEPTH   = 0b0010;
        const OUTLINE = 0b0100;
        const TONE    = 0b1000;
        const ALL     = Self::BASE.bits() | Self::DEPTH.bits() | Self::OUTLINE.bits() | Self::TONE.bits();
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderOpts {
    pub width: u32,
    pub height: u32,
    pub passes: RenderPasses,
    pub seed: u64,
}

impl Default for RenderOpts {
    fn default() -> Self {
        // Manga page aspect ~ 4:5.7 (B5). Default ≈ panel keyframe.
        Self {
            width: 1024,
            height: 1448,
            passes: RenderPasses::ALL,
            seed: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub base_png: Vec<u8>,
    pub depth_png: Option<Vec<u8>>,
    pub outline_png: Option<Vec<u8>>,
    pub toon_png: Option<Vec<u8>>,
    pub camera: CameraSpec,
}

impl MangakaScene {
    /// Render a single frame for the configured camera.
    #[cfg(not(target_family = "wasm"))]
    pub fn render(&self, opts: RenderOpts) -> Result<RenderResult> {
        let r = crate::renderer::MangakaRenderer::new()?;
        r.render(self, None, opts)
    }

    /// WASM stub — browser preview uses the same shader bundle but is driven
    /// by `wasm-pack` exports (lands in P5). Direct `render()` on wasm is a
    /// no-op so the crate stays cdylib-compatible.
    #[cfg(target_family = "wasm")]
    pub fn render(&self, _opts: RenderOpts) -> Result<RenderResult> {
        Err(SceneError::Render(
            "render() on wasm is driven by wasm-pack façade (P5).".into(),
        ))
    }

    pub fn render_multi(&self, angles: &[CameraSpec], opts: RenderOpts) -> Result<Vec<RenderResult>> {
        #[cfg(not(target_family = "wasm"))]
        {
            let r = crate::renderer::MangakaRenderer::new()?;
            let mut out = Vec::with_capacity(angles.len());
            for cam in angles {
                out.push(r.render(self, Some(*cam), opts)?);
            }
            Ok(out)
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (angles, opts);
            Err(SceneError::Render(
                "render_multi() on wasm is driven by wasm-pack façade (P5).".into(),
            ))
        }
    }
}
