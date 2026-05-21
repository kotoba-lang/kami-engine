//! Scalar-field visualiser — renders `kami_dec::ScalarField` values as
//! camera-facing billboard sprites. Reuses `kami_render::ParticlePipeline`
//! (same 36-byte instance format: pos3 + col3 + size1 + age1 + life1).
//!
//! Each frame in `prepare`:
//!   1. Clear instance accumulator
//!   2. For each registered layer (ScalarField + color + max_value):
//!      iterate non-zero cells, emit one instance per cell with
//!      `color = base_color × min(value / max_value, 1.0)` and
//!      `age = 0, life = 1` (fade factor = 1.0, full alpha)
//!   3. Upload instance buffer
//!
//! Multiple layers compose via alpha-blend — heat (red) + moisture
//! (blue) show up as a purple gradient where they overlap.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_dec::ScalarField;
use kami_render::scene_pipelines::{ParticlePipeline, ParticleUniform};
use kami_render::RenderContext;
use std::cell::RefCell;
use std::rc::Rc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FieldInstance {
    pos: [f32; 3],
    col: [f32; 3],
    size: f32,
    age: f32,
    life: f32,
}

pub struct FieldLayer {
    pub field: Rc<RefCell<ScalarField>>,
    pub base_color: [f32; 3],
    /// Field value at which the sprite reaches full intensity.
    pub max_value: f32,
    /// Threshold below which cells are skipped.
    pub min_value: f32,
    /// Sprite size in meters.
    pub size: f32,
}

pub struct FieldVisAdapter {
    pipeline: ParticlePipeline,
    device: wgpu::Device,
    layers: Vec<FieldLayer>,
    instance_vb: RefCell<wgpu::Buffer>,
    instance_count: RefCell<u32>,
    capacity: u32,
    /// LOD: cells within this distance (m) emit at full density.
    pub lod_near: f32,
    /// LOD: cells beyond this distance emit at quarter density.
    pub lod_far: f32,
    /// Last-known camera position (captured in `prepare`, consumed
    /// in `rebuild_instances`).
    cam_pos: RefCell<[f32; 3]>,
}

impl FieldVisAdapter {
    pub fn new(ctx: &RenderContext, capacity: u32) -> Self {
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("field_vis.instances"),
            size: (capacity as u64) * std::mem::size_of::<FieldInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: ParticlePipeline::new(&ctx.device, ctx.format),
            device: ctx.device.clone(),
            layers: Vec::new(),
            instance_vb: RefCell::new(buf),
            instance_count: RefCell::new(0),
            capacity,
            lod_near: 8.0,
            lod_far: 20.0,
            cam_pos: RefCell::new([0.0, 0.0, 0.0]),
        }
    }

    pub fn add_layer(&mut self, layer: FieldLayer) {
        self.layers.push(layer);
    }

    fn rebuild_instances(&self) {
        let mut instances: Vec<FieldInstance> = Vec::new();
        let cam = *self.cam_pos.borrow();
        let near2 = self.lod_near * self.lod_near;
        let far2 = self.lod_far * self.lod_far;
        for layer in &self.layers {
            let field = layer.field.borrow();
            let col = layer.base_color;
            let max = layer.max_value.max(1e-6);
            field.for_each_nonzero(layer.min_value, |x, y, z, v| {
                if instances.len() as u32 >= self.capacity {
                    return;
                }
                // Distance-based LOD: near = full density, mid =
                // stride 2 (skip odd cells), far = stride 4. Since
                // missing cells are compensated by enlarging
                // surviving billboards, visual coverage is preserved.
                let dx = (x as f32 + 0.5) - cam[0];
                let dy = (y as f32 + 0.5) - cam[1];
                let dz = (z as f32 + 0.5) - cam[2];
                let d2 = dx*dx + dy*dy + dz*dz;
                let (stride, size_boost) = if d2 < near2 {
                    (1, 1.0)
                } else if d2 < far2 {
                    (2, 1.6)
                } else {
                    (4, 2.5)
                };
                if stride > 1 {
                    let parity = ((x & (stride - 1)) + (y & (stride - 1)) + (z & (stride - 1))) & (stride - 1);
                    if parity != 0 { return; }
                }
                let intensity = (v / max).clamp(0.0, 1.0);
                instances.push(FieldInstance {
                    pos: [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5],
                    col: [col[0] * intensity, col[1] * intensity, col[2] * intensity],
                    size: layer.size * (0.5 + 0.5 * intensity) * size_boost,
                    age: 0.0,
                    life: 1.0,
                });
            });
        }
        *self.instance_count.borrow_mut() = instances.len() as u32;
        if instances.is_empty() {
            return;
        }
        let buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("field_vis.instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
        *self.instance_vb.borrow_mut() = buf;
    }
}

impl RenderPipeline for FieldVisAdapter {
    fn prepare(&mut self, _ctx: &RenderContext, camera: &Camera, _world: &World) {
        *self.cam_pos.borrow_mut() = camera.as_render().uniform().position;
        self.rebuild_instances();
    }

    fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
        _world: &World,
    ) {
        let count = *self.instance_count.borrow();
        if count == 0 {
            return;
        }
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let vp = proj * view_m;
        let right = Vec3::new(view_m.x_axis.x, view_m.y_axis.x, view_m.z_axis.x);
        let up = Vec3::new(view_m.x_axis.y, view_m.y_axis.y, view_m.z_axis.y);
        let pu = ParticleUniform {
            view_proj: vp.to_cols_array(),
            cam_right: right.to_array(),
            _p0: 0.0,
            cam_up: up.to_array(),
            _p1: 0.0,
        };
        ctx.queue
            .write_buffer(&self.pipeline.uniform, 0, bytemuck::bytes_of(&pu));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("field_vis.pass"),
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
        pass.set_vertex_buffer(0, self.pipeline.quad_vb.slice(..));
        let vb = self.instance_vb.borrow();
        pass.set_vertex_buffer(1, vb.slice(..));
        pass.set_index_buffer(self.pipeline.quad_ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, 0, 0..count);
    }
}
