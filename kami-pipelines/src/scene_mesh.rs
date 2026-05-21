//! `scene_mesh` — shared triangle-mesh adapter internals.
//!
//! Both `BimSceneAdapter` and `CadSceneAdapter` re-use this module to
//! upload pre-tessellated triangle batches and draw them with
//! `kami_render::scene_pipelines::VoxelPipeline` (pos3 + norm3 + col3,
//! TriangleList, back-face culling, depth write).
//!
//! We piggy-back on `VoxelPipeline` rather than adding a new
//! `MeshPipeline` in kami-render: the vertex layout and uniform shape
//! are identical, and the shader already handles sun / fog / flat
//! per-vertex colour — which is exactly what a Phase-1 CAD / BIM demo
//! needs. A bespoke pipeline can be added later if CAD requires edge
//! lines or BIM needs materials.
//!
//! Coordinate system: we render in model-local space and expect callers
//! to bake any world transform into the vertex positions at upload
//! time (`push_batch` applies `Mat4` once on CPU, uploading a single
//! world-space buffer per batch). This avoids per-draw uniform swaps
//! for the ~dozens of batches a typical storey / part contains.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4Swizzles};
use kami_app::Camera;
use kami_render::scene_pipelines::{VoxelPipeline, VoxelUniform};
use kami_render::RenderContext;
use wgpu::util::DeviceExt;

use crate::{fog_from_sun, sun_from_time};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct MeshVertex {
    pos: [f32; 3],
    norm: [f32; 3],
    col: [f32; 3],
}

pub(crate) struct MeshBatch {
    pub vb: wgpu::Buffer,
    pub ib: wgpu::Buffer,
    pub index_count: u32,
}

/// Shared mesh adapter. Owns a `VoxelPipeline` + a list of uploaded
/// batches. Both `BimSceneAdapter` and `CadSceneAdapter` embed one.
pub(crate) struct SceneMeshCore {
    pub(crate) pipeline: VoxelPipeline,
    pub(crate) batches: Vec<MeshBatch>,
    pub(crate) label: &'static str,
    pub(crate) fog_density: f32,
}

impl SceneMeshCore {
    pub(crate) fn new(ctx: &RenderContext, label: &'static str, fog_density: f32) -> Self {
        Self {
            pipeline: VoxelPipeline::new(&ctx.device, ctx.format),
            batches: Vec::new(),
            label,
            fog_density,
        }
    }

    /// Upload a triangle batch. `positions` / `normals` are in model-local
    /// space; `world` is applied on CPU so draw time only needs one
    /// uniform write (shared across all batches).
    pub(crate) fn push_batch(
        &mut self,
        ctx: &RenderContext,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
        base_color: [f32; 3],
        world: Mat4,
    ) {
        assert_eq!(positions.len(), normals.len(), "positions / normals length mismatch");
        if positions.is_empty() || indices.is_empty() {
            return;
        }
        // Bake the world transform into the vertices so the GPU pipeline
        // doesn't need a per-batch model matrix binding.
        let normal_mat = world.inverse().transpose();
        let verts: Vec<MeshVertex> = positions.iter().zip(normals.iter()).map(|(p, n)| {
            let wp = (world * glam::Vec4::new(p[0], p[1], p[2], 1.0)).xyz();
            let wn = (normal_mat * glam::Vec4::new(n[0], n[1], n[2], 0.0)).xyz().normalize_or_zero();
            MeshVertex {
                pos: [wp.x, wp.y, wp.z],
                norm: [wn.x, wn.y, wn.z],
                col: base_color,
            }
        }).collect();
        let vb = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(self.label),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(self.label),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.batches.push(MeshBatch { vb, ib, index_count: indices.len() as u32 });
    }

    /// Re-upload a previously-pushed batch with a new colour. Used by
    /// `Bim/CadSceneAdapter::set_highlighted` to flip selection colour
    /// without rebuilding every buffer in the scene.
    pub(crate) fn replace_batch_color(
        &mut self,
        device: &wgpu::Device,
        index: usize,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
        color: [f32; 3],
    ) {
        if index >= self.batches.len() {
            return;
        }
        let verts: Vec<MeshVertex> = positions.iter().zip(normals.iter()).map(|(p, n)| MeshVertex {
            pos: *p,
            norm: *n,
            col: color,
        }).collect();
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(self.label),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(self.label),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.batches[index] = MeshBatch { vb, ib, index_count: indices.len() as u32 };
    }

    pub(crate) fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
    ) {
        if self.batches.is_empty() {
            return;
        }
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let vp = proj * view_m;
        let sun_dir = sun_from_time(camera.time);
        let fog = fog_from_sun(sun_dir);
        let warmth = 1.0 - sun_dir.y.max(0.0);
        let sun_color = [1.0, 0.96 - warmth * 0.12, 0.88 - warmth * 0.28];
        let vu = VoxelUniform {
            view_proj: vp.to_cols_array(),
            cam_pos: u.position,
            _p0: 0.0,
            sun_dir: sun_dir.to_array(),
            _p1: 0.0,
            sun_color,
            fog_density: self.fog_density,
            fog_color: fog.to_array(),
            _p2: 0.0,
        };
        ctx.queue
            .write_buffer(&self.pipeline.uniform, 0, bytemuck::bytes_of(&vu));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.label),
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
        for b in &self.batches {
            pass.set_vertex_buffer(0, b.vb.slice(..));
            pass.set_index_buffer(b.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..b.index_count, 0, 0..1);
        }
    }
}

/// Generate a unit-box triangle mesh (positions, normals, indices)
/// centred at the origin, side length 1. Used by BIM slab / wall
/// demo builders and CAD primitive helpers.
pub fn unit_box() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    // 24 vertices (4 per face × 6 faces) so each face gets its own
    // normal. 12 triangles.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +X
        ([1.0, 0.0, 0.0], [[0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5], [0.5, -0.5, 0.5]]),
        // -X
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5], [-0.5, -0.5, -0.5]]),
        // +Y
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, -0.5], [-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5]]),
        // -Y
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, 0.5], [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5]]),
        // +Z
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        // -Z
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (i, (n, corners)) in faces.iter().enumerate() {
        let base = (i * 4) as u32;
        for c in corners {
            positions.push(*c);
            normals.push(*n);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let _ = Vec3::ZERO; // silence unused warning on some feature combos
    (positions, normals, indices)
}

/// Generate a unit cylinder (radius=0.5, height=1, axis = +Y) with
/// `segments` sides. Used by CAD demo for boss / pin features and BIM
/// demo for columns / pipes.
pub fn unit_cylinder(segments: u32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    assert!(segments >= 3, "cylinder needs >= 3 segments");
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let r = 0.5_f32;
    let h = 0.5_f32; // half-height
    // Side strip — two verts per segment (top + bottom), seamed (not
    // shared across the wrap) so normals point radially correct.
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let (s0, c0) = (a0.sin(), a0.cos());
        let (s1, c1) = (a1.sin(), a1.cos());
        let base = positions.len() as u32;
        // v0 bottom-0, v1 top-0, v2 top-1, v3 bottom-1
        positions.push([c0 * r, -h, s0 * r]);
        positions.push([c0 * r,  h, s0 * r]);
        positions.push([c1 * r,  h, s1 * r]);
        positions.push([c1 * r, -h, s1 * r]);
        normals.push([c0, 0.0, s0]);
        normals.push([c0, 0.0, s0]);
        normals.push([c1, 0.0, s1]);
        normals.push([c1, 0.0, s1]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    // Top cap (fan around centre, +Y normal).
    let centre_top = positions.len() as u32;
    positions.push([0.0, h, 0.0]);
    normals.push([0.0, 1.0, 0.0]);
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let v0 = positions.len() as u32;
        positions.push([a0.cos() * r, h, a0.sin() * r]);
        positions.push([a1.cos() * r, h, a1.sin() * r]);
        normals.push([0.0, 1.0, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        indices.extend_from_slice(&[centre_top, v0, v0 + 1]);
    }
    // Bottom cap (fan, -Y normal, reversed winding).
    let centre_bot = positions.len() as u32;
    positions.push([0.0, -h, 0.0]);
    normals.push([0.0, -1.0, 0.0]);
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let v0 = positions.len() as u32;
        positions.push([a0.cos() * r, -h, a0.sin() * r]);
        positions.push([a1.cos() * r, -h, a1.sin() * r]);
        normals.push([0.0, -1.0, 0.0]);
        normals.push([0.0, -1.0, 0.0]);
        indices.extend_from_slice(&[centre_bot, v0 + 1, v0]);
    }
    (positions, normals, indices)
}
