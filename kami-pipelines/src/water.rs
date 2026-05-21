//! Water adapter — wraps `kami_render::scene_pipelines::WaterPipeline`
//! into the `RenderPipeline` trait.
//!
//! The wgpu pipeline + shader (`shaders/scene_water.wgsl`) live in
//! kami-render. This file holds only the per-frame uniform update
//! (view_proj / sun_dir / fog_color / time / water_y / base_col) and
//! the draw call against a `kami_terrain::generate_water_mesh` plane.

use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::scene_pipelines::{WaterPipeline, WaterUniform};
use kami_render::RenderContext;
use wgpu::util::DeviceExt;

use crate::{fog_from_sun, sun_from_time};

pub struct WaterAdapter {
    pipeline: WaterPipeline,
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    index_count: u32,
    water_y: f32,
    base_col: Vec3,
}

impl WaterAdapter {
    /// Build a water plane from `kami_terrain::generate_water_mesh`
    /// covering `extent × extent` centred at origin at height `water_y`.
    pub fn new(ctx: &RenderContext, extent: f32, water_y: f32) -> Self {
        let cfg = kami_terrain::WaterConfig {
            sea_level: water_y,
            extent,
            resolution: 64,
            waves: kami_terrain::default_waves(),
        };
        let (verts, idxs) = kami_terrain::generate_water_mesh(&cfg);
        let flat: Vec<f32> = verts
            .iter()
            .flat_map(|v| [v.position[0], v.position[1], v.position[2], v.uv[0], v.uv[1]])
            .collect();
        let vb = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water.vb"),
            contents: bytemuck::cast_slice(&flat),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water.ib"),
            contents: bytemuck::cast_slice(&idxs),
            usage: wgpu::BufferUsages::INDEX,
        });
        let pipeline = WaterPipeline::new(&ctx.device, ctx.format);
        Self {
            pipeline,
            vb,
            ib,
            index_count: idxs.len() as u32,
            water_y,
            base_col: Vec3::new(0.06, 0.22, 0.36),
        }
    }

    pub fn with_base_color(mut self, rgb: Vec3) -> Self {
        self.base_col = rgb;
        self
    }
}

impl RenderPipeline for WaterAdapter {
    fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
        _world: &World,
    ) {
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let vp = proj * view_m;
        let sun_dir = sun_from_time(camera.time);
        let fog = fog_from_sun(sun_dir);
        let wu = WaterUniform {
            view_proj: vp.to_cols_array(),
            cam_pos: u.position,
            time: camera.time,
            sun_dir: sun_dir.to_array(),
            water_y: self.water_y,
            fog_color: fog.to_array(),
            _p0: 0.0,
            base_col: self.base_col.to_array(),
            _p1: 0.0,
        };
        ctx.queue
            .write_buffer(&self.pipeline.uniform, 0, bytemuck::bytes_of(&wu));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline.pipeline);
        pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vb.slice(..));
        pass.set_index_buffer(self.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}
