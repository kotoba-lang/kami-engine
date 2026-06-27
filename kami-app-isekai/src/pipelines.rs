//! ISEKAI-local render pipelines.
//!
//! The full Sky / Terrain (+vegetation) pipeline stack migrated to
//! `kami-pipelines` in Phase 12 so other games (quarry-walk, future
//! test sandboxes) can reuse it. This file only keeps ISEKAI-specific
//! debug helpers; re-exports the shared adapters for ergonomic
//! `use pipelines::{SkyAdapter, TerrainAdapter}` on the game side.

pub use kami_pipelines::{SkyAdapter, TerrainAdapter, fog_from_sun, sun_from_time};

use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::RenderContext;

/// Breathing-gradient clear for bootstrap / debug. Proves `prepare` +
/// `record` are driven each frame without needing a full scene.
pub struct BreathingClear {
    tick: u64,
}

impl BreathingClear {
    pub fn new() -> Self {
        Self { tick: 0 }
    }
}

impl Default for BreathingClear {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderPipeline for BreathingClear {
    fn prepare(&mut self, _ctx: &RenderContext, _camera: &Camera, _world: &World) {
        self.tick = self.tick.wrapping_add(1);
    }

    fn record(
        &self,
        _ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        _depth_view: &wgpu::TextureView,
        _camera: &Camera,
        _world: &World,
    ) {
        let t = self.tick as f32 * 0.016;
        let r = ((t * 0.7).sin() * 0.5 + 0.5) * 0.25 + 0.04;
        let g = ((t * 0.9 + 2.1).sin() * 0.5 + 0.5) * 0.35 + 0.05;
        let b = ((t * 1.1 + 4.2).sin() * 0.5 + 0.5) * 0.45 + 0.08;
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("isekai-v2.breathing-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: r as f64,
                        g: g as f64,
                        b: b as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
}
