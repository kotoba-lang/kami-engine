//! EdgeField visualiser — renders wind vectors as coloured billboard
//! arrows (a short oriented segment per cell). Reuses
//! `kami_render::ParticlePipeline` with one instance per sampled cell.
//!
//! Instead of rebuilding the full wgpu arrow-mesh pipeline, each wind
//! vector is expressed as a small billboard whose size and colour
//! encode magnitude, and position is offset along the wind direction
//! in world space on the CPU. Adequate for debug + P26 demo scene.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_dec::EdgeField;
use kami_render::RenderContext;
use kami_render::scene_pipelines::{ParticlePipeline, ParticleUniform};
use std::cell::RefCell;
use std::rc::Rc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ArrowInstance {
    pos: [f32; 3],
    col: [f32; 3],
    size: f32,
    age: f32,
    life: f32,
}

pub struct EdgeVisAdapter {
    pipeline: ParticlePipeline,
    device: wgpu::Device,
    field: Rc<RefCell<EdgeField>>,
    instance_vb: RefCell<wgpu::Buffer>,
    instance_count: RefCell<u32>,
    capacity: u32,
    /// Magnitude at which the arrow reaches full intensity.
    pub max_mag: f32,
    /// Minimum magnitude below which cells are skipped.
    pub min_mag: f32,
    /// Number of billboard samples along each arrow (head trail).
    pub samples_per_arrow: u32,
    /// Base arrow length in meters.
    pub arrow_length: f32,
    /// Billboard size in meters.
    pub sprite_size: f32,
    /// Skip every N-th cell to reduce visual clutter.
    pub stride: i32,
    /// Distance-based LOD thresholds (m).
    pub lod_near: f32,
    pub lod_far: f32,
    cam_pos: RefCell<[f32; 3]>,
}

impl EdgeVisAdapter {
    pub fn new(ctx: &RenderContext, field: Rc<RefCell<EdgeField>>, capacity: u32) -> Self {
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("edge_vis.instances"),
            size: (capacity as u64) * std::mem::size_of::<ArrowInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: ParticlePipeline::new(&ctx.device, ctx.format),
            device: ctx.device.clone(),
            field,
            instance_vb: RefCell::new(buf),
            instance_count: RefCell::new(0),
            capacity,
            max_mag: 1.5,
            min_mag: 0.05,
            samples_per_arrow: 4,
            arrow_length: 1.6,
            sprite_size: 0.18,
            stride: 2,
            lod_near: 8.0,
            lod_far: 20.0,
            cam_pos: RefCell::new([0.0, 0.0, 0.0]),
        }
    }

    fn rebuild_instances(&self) {
        let mut instances: Vec<ArrowInstance> = Vec::new();
        let field = self.field.borrow();
        let inv_max = 1.0 / self.max_mag.max(1e-6);
        let cam = *self.cam_pos.borrow();
        let near2 = self.lod_near * self.lod_near;
        let far2 = self.lod_far * self.lod_far;
        for (&cc, cells) in &field.chunks {
            let bx = cc.0 * (kami_dec::CHUNK_SIZE as i32);
            let by = cc.1 * (kami_dec::CHUNK_SIZE as i32);
            let bz = cc.2 * (kami_dec::CHUNK_SIZE as i32);
            // Chunk-level LOD: compute chunk centre vs camera, pick
            // effective stride per chunk.
            let cx = bx as f32 + kami_dec::CHUNK_SIZE as f32 * 0.5;
            let cy = by as f32 + kami_dec::CHUNK_SIZE as f32 * 0.5;
            let cz = bz as f32 + kami_dec::CHUNK_SIZE as f32 * 0.5;
            let dx = cx - cam[0];
            let dy = cy - cam[1];
            let dz = cz - cam[2];
            let d2 = dx * dx + dy * dy + dz * dz;
            let stride = if d2 < near2 {
                self.stride as usize
            } else if d2 < far2 {
                (self.stride as usize).max(2) * 2
            } else {
                (self.stride as usize).max(2) * 4
            };
            if stride == 0 {
                continue;
            }
            for lz in (0..kami_dec::CHUNK_SIZE).step_by(stride) {
                for ly in (0..kami_dec::CHUNK_SIZE).step_by(stride) {
                    for lx in (0..kami_dec::CHUNK_SIZE).step_by(stride) {
                        let i = lx
                            + ly * kami_dec::CHUNK_SIZE
                            + lz * kami_dec::CHUNK_SIZE * kami_dec::CHUNK_SIZE;
                        let v = Vec3::new(cells[i][0], cells[i][1], cells[i][2]);
                        let m = v.length();
                        if m < self.min_mag {
                            continue;
                        }
                        let intensity = (m * inv_max).clamp(0.0, 1.0);
                        let dir = v / m;
                        // Colour: cool (blue) → warm (red) by magnitude.
                        let col = [intensity, 0.4 * (1.0 - intensity) + 0.2, (1.0 - intensity)];
                        let origin = Vec3::new(
                            (bx + lx as i32) as f32 + 0.5,
                            (by + ly as i32) as f32 + 0.5,
                            (bz + lz as i32) as f32 + 0.5,
                        );
                        for k in 0..self.samples_per_arrow {
                            if instances.len() as u32 >= self.capacity {
                                *self.instance_count.borrow_mut() = instances.len() as u32;
                                let buf = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("edge_vis.instances"),
                                        contents: bytemuck::cast_slice(&instances),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                *self.instance_vb.borrow_mut() = buf;
                                return;
                            }
                            let t = (k as f32) / (self.samples_per_arrow as f32 - 1.0).max(1.0);
                            let pos = origin + dir * (t * self.arrow_length);
                            let head_boost = if k + 1 == self.samples_per_arrow {
                                1.6
                            } else {
                                1.0
                            };
                            instances.push(ArrowInstance {
                                pos: pos.to_array(),
                                col,
                                size: self.sprite_size * head_boost * (0.6 + 0.4 * intensity),
                                age: 0.0,
                                life: 1.0,
                            });
                        }
                    }
                }
            }
        }
        *self.instance_count.borrow_mut() = instances.len() as u32;
        if instances.is_empty() {
            return;
        }
        let buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("edge_vis.instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
        *self.instance_vb.borrow_mut() = buf;
    }
}

impl RenderPipeline for EdgeVisAdapter {
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
            label: Some("edge_vis.pass"),
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
