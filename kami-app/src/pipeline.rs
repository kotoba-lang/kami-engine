//! `RenderPipeline` trait — pluggable draw stage.
//!
//! Each frame, `KamiApp::tick_once` calls `prepare()` (upload uniforms,
//! rebuild dirty buffers) then `record()` (emit draw calls into the
//! frame's `CommandEncoder`). Pipelines compose: a voxel-pbr pipeline +
//! an sdf-character pipeline + a sky-atmosphere pipeline all write into
//! the same surface view in the order they were registered.
//!
//! The shared `depth_view` is a `Depth24Plus` attachment owned by
//! `KamiApp::DepthTarget`, resized with the surface. Pipelines that
//! don't need depth may pass `depth_stencil_attachment: None` and ignore
//! the argument.

use crate::camera::Camera;
use hecs::World;
use kami_render::RenderContext;

pub trait RenderPipeline: 'static {
    fn prepare(&mut self, _ctx: &RenderContext, _camera: &Camera, _world: &World) {}

    fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
        world: &World,
    );
}
