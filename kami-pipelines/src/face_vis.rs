//! FaceField visualiser — renders 2-forms as billboard sprites at each
//! cell, with size + colour by magnitude. Mirrors `edge_vis` but reads
//! 3 face-normal components instead of 3 edges. Used for Maxwell B-field.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_dec::FaceField;
use kami_render::scene_pipelines::{ParticlePipeline, ParticleUniform};
use kami_render::RenderContext;
use std::cell::RefCell;
use std::rc::Rc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FaceInstance {
    pos: [f32; 3],
    col: [f32; 3],
    size: f32,
    age: f32,
    life: f32,
}

pub struct FaceVisAdapter {
    pipeline: ParticlePipeline,
    device: wgpu::Device,
    field: Rc<RefCell<FaceField>>,
    instance_vb: RefCell<wgpu::Buffer>,
    instance_count: RefCell<u32>,
    capacity: u32,
    pub max_mag: f32,
    pub min_mag: f32,
    pub sprite_size: f32,
    pub stride: i32,
    pub base_color: [f32; 3],
    pub lod_near: f32,
    pub lod_far: f32,
    cam_pos: RefCell<[f32; 3]>,
}

impl FaceVisAdapter {
    pub fn new(ctx: &RenderContext, field: Rc<RefCell<FaceField>>, capacity: u32) -> Self {
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("face_vis.instances"),
            size: (capacity as u64) * std::mem::size_of::<FaceInstance>() as u64,
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
            max_mag: 0.6,
            min_mag: 0.01,
            sprite_size: 0.2,
            stride: 1,
            base_color: [0.3, 0.9, 0.4], // green → distinguish from E (warm)
            lod_near: 8.0,
            lod_far: 20.0,
            cam_pos: RefCell::new([0.0, 0.0, 0.0]),
        }
    }

    fn rebuild_instances(&self) {
        let mut instances: Vec<FaceInstance> = Vec::new();
        let field = self.field.borrow();
        let inv_max = 1.0 / self.max_mag.max(1e-6);
        let cam = *self.cam_pos.borrow();
        let near2 = self.lod_near * self.lod_near;
        let far2 = self.lod_far * self.lod_far;
        for (&cc, cells) in &field.chunks {
            let bx = cc.0 * (kami_dec::CHUNK_SIZE as i32);
            let by = cc.1 * (kami_dec::CHUNK_SIZE as i32);
            let bz = cc.2 * (kami_dec::CHUNK_SIZE as i32);
            let cx = bx as f32 + kami_dec::CHUNK_SIZE as f32 * 0.5;
            let cy = by as f32 + kami_dec::CHUNK_SIZE as f32 * 0.5;
            let cz = bz as f32 + kami_dec::CHUNK_SIZE as f32 * 0.5;
            let dx = cx - cam[0]; let dy = cy - cam[1]; let dz = cz - cam[2];
            let d2 = dx*dx + dy*dy + dz*dz;
            let stride = if d2 < near2 { self.stride as usize }
                else if d2 < far2 { (self.stride as usize).max(1) * 2 }
                else { (self.stride as usize).max(1) * 4 };
            if stride == 0 { continue; }
            for lz in (0..kami_dec::CHUNK_SIZE).step_by(stride) {
                for ly in (0..kami_dec::CHUNK_SIZE).step_by(stride) {
                    for lx in (0..kami_dec::CHUNK_SIZE).step_by(stride) {
                        if instances.len() as u32 >= self.capacity { break; }
                        let i = lx + ly * kami_dec::CHUNK_SIZE + lz * kami_dec::CHUNK_SIZE * kami_dec::CHUNK_SIZE;
                        let v = Vec3::new(cells[i][0], cells[i][1], cells[i][2]);
                        let m = v.length();
                        if m < self.min_mag { continue; }
                        let intensity = (m * inv_max).clamp(0.0, 1.0);
                        let col = [
                            self.base_color[0] * intensity,
                            self.base_color[1] * intensity,
                            self.base_color[2] * intensity,
                        ];
                        let pos = Vec3::new(
                            (bx + lx as i32) as f32 + 0.5,
                            (by + ly as i32) as f32 + 0.5,
                            (bz + lz as i32) as f32 + 0.5,
                        );
                        instances.push(FaceInstance {
                            pos: pos.to_array(),
                            col,
                            size: self.sprite_size * (0.5 + 0.5 * intensity),
                            age: 0.0,
                            life: 1.0,
                        });
                    }
                }
            }
        }
        *self.instance_count.borrow_mut() = instances.len() as u32;
        if instances.is_empty() { return; }
        let buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("face_vis.instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        *self.instance_vb.borrow_mut() = buf;
    }
}

impl RenderPipeline for FaceVisAdapter {
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
        if count == 0 { return; }
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
        ctx.queue.write_buffer(&self.pipeline.uniform, 0, bytemuck::bytes_of(&pu));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("face_vis.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
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
