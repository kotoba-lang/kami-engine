//! Nintendo-style procedural sprite atlas adapter.
//!
//! 1 GPU pipeline, 16 shape kinds (flame / water / sparkle / shock
//! wave / wind swirl / arrow trail / …). Callers emit sprites by
//! slot ID and the fragment shader picks the right procedural shape.
//!
//! Life / motion are handled CPU-side per frame (similar to
//! ParticleAdapter) so the same adapter can host static billboards,
//! animated spring-bouncy flames, short-lived spark VFX, and
//! advected flow markers.

use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::scene_pipelines::{AtlasInstance, AtlasPipeline, AtlasUniform};
use kami_render::RenderContext;
use std::cell::RefCell;
use std::rc::Rc;
use wgpu::util::DeviceExt;

pub use kami_render::scene_pipelines::atlas_slot;

#[derive(Debug, Clone, Copy)]
pub struct AtlasSprite {
    pub pos:   Vec3,
    pub vel:   Vec3,
    pub tint:  [f32; 3],
    pub size:  f32,
    pub slot:  u32,
    pub rot:   f32,
    pub rot_vel: f32,
    pub age:   f32,
    pub life:  f32,
    /// When true, integrate gravity (9.8 m/s² downward).
    pub gravity: bool,
    /// Vertical bob amplitude (m) — sin wobble on Y.
    pub bob_amp: f32,
    /// Bob angular frequency (rad/s).
    pub bob_w:   f32,
    /// Phase — desync a cluster so flames don't pulse in lock-step.
    pub bob_phase: f32,
    /// Scale pulse amplitude (fraction of `size`). 0.0 = off.
    pub pulse_amp: f32,
    /// Scale pulse angular frequency.
    pub pulse_w:   f32,
    /// Rotation wiggle amplitude (rad).
    pub wiggle_amp: f32,
    /// Rotation wiggle angular frequency.
    pub wiggle_w:   f32,
    /// "Pop in" flag: scale eases 0 → 1.2 → 1.0 over `pop_ease_t`
    /// seconds (Splatoon / Switch bounce). 0.0 = off.
    pub pop_ease_t: f32,
}

struct AtlasShared {
    pipeline: AtlasPipeline,
    device: wgpu::Device,
    sprites: RefCell<Vec<AtlasSprite>>,
    instance_vb: RefCell<wgpu::Buffer>,
    instance_count: RefCell<u32>,
    capacity: u32,
    elapsed: RefCell<f32>,
    cam_pos: RefCell<[f32; 3]>,
    /// N6 LOD: beyond this distance, slot collapses to sparkle_star.
    lod_sparkle: f32,
    /// N6 LOD: beyond this distance, the sprite is culled entirely.
    lod_cull: f32,
}

#[derive(Clone)]
pub struct AtlasVisAdapter {
    inner: Rc<AtlasShared>,
}

impl AtlasVisAdapter {
    pub fn new(ctx: &RenderContext, capacity: u32) -> Self {
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atlas_vis.instances"),
            size: (capacity as u64) * std::mem::size_of::<AtlasInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            inner: Rc::new(AtlasShared {
                pipeline: AtlasPipeline::new(&ctx.device, ctx.format),
                device: ctx.device.clone(),
                sprites: RefCell::new(Vec::new()),
                instance_vb: RefCell::new(buf),
                instance_count: RefCell::new(0),
                capacity,
                elapsed: RefCell::new(0.0),
                cam_pos: RefCell::new([0.0, 0.0, 0.0]),
                lod_sparkle: 15.0,
                lod_cull: 40.0,
            }),
        }
    }

    /// Emit a sprite with full control.
    pub fn emit(&self, s: AtlasSprite) {
        let mut v = self.inner.sprites.borrow_mut();
        if v.len() as u32 >= self.inner.capacity { return; }
        v.push(s);
    }

    /// Convenience: static (no velocity, no gravity) sprite.
    pub fn emit_static(&self, pos: Vec3, slot: u32, tint: [f32; 3], size: f32, life: f32) {
        self.emit(AtlasSprite {
            pos, vel: Vec3::ZERO, tint, size, slot, rot: 0.0, rot_vel: 0.0,
            age: 0.0, life, gravity: false,
            bob_amp: 0.0, bob_w: 0.0, bob_phase: 0.0,
            pulse_amp: 0.0, pulse_w: 0.0,
            wiggle_amp: 0.0, wiggle_w: 0.0,
            pop_ease_t: 0.0,
        });
    }

    /// Convenience: flame / plume with Nintendo wobble (bob + scale
    /// pulse + rotation wiggle). Per-instance phase avoids lock-step.
    pub fn emit_bobbing(&self, pos: Vec3, slot: u32, tint: [f32; 3], size: f32, life: f32, phase: f32) {
        self.emit(AtlasSprite {
            pos, vel: Vec3::ZERO, tint, size, slot, rot: 0.0, rot_vel: 0.0,
            age: 0.0, life, gravity: false,
            bob_amp: size * 0.12, bob_w: 5.0, bob_phase: phase,
            pulse_amp: 0.15, pulse_w: 7.0,
            wiggle_amp: 0.05, wiggle_w: 6.0,
            pop_ease_t: 0.0,
        });
    }

    /// Convenience: "pop in" — sprite eases from size 0 to 1.2×size
    /// and settles at 1.0×size over the first `ease_t` seconds
    /// (Nintendo "item get" / Switch pop feel). After that it fades
    /// out via alpha. Use for sparkle bursts, splash rings, etc.
    pub fn emit_pop(&self, pos: Vec3, slot: u32, tint: [f32; 3], size: f32, life: f32, ease_t: f32) {
        self.emit(AtlasSprite {
            pos, vel: Vec3::ZERO, tint, size, slot, rot: 0.0, rot_vel: 0.0,
            age: 0.0, life, gravity: false,
            bob_amp: 0.0, bob_w: 0.0, bob_phase: 0.0,
            pulse_amp: 0.0, pulse_w: 0.0,
            wiggle_amp: 0.0, wiggle_w: 0.0,
            pop_ease_t: ease_t,
        });
    }

    fn tick_and_upload(&self, dt: f32) -> u32 {
        const GRAVITY: f32 = 9.8;
        let mut elapsed = self.inner.elapsed.borrow_mut();
        *elapsed += dt;
        let t = *elapsed;
        drop(elapsed);

        let mut sprites = self.inner.sprites.borrow_mut();
        sprites.retain_mut(|s| {
            s.age += dt;
            if s.age >= s.life { return false; }
            if s.gravity { s.vel.y -= GRAVITY * dt; }
            s.pos += s.vel * dt;
            s.rot += s.rot_vel * dt;
            true
        });

        if sprites.is_empty() {
            *self.inner.instance_count.borrow_mut() = 0;
            return 0;
        }
        let cam = *self.inner.cam_pos.borrow();
        let sparkle2 = self.inner.lod_sparkle * self.inner.lod_sparkle;
        let cull2 = self.inner.lod_cull * self.inner.lod_cull;
        let instances: Vec<AtlasInstance> = sprites
            .iter()
            .filter_map(|s| {
                // N6 LOD: cull very far, collapse mid-far to sparkle.
                let dx = s.pos.x - cam[0];
                let dy = s.pos.y - cam[1];
                let dz = s.pos.z - cam[2];
                let d2 = dx*dx + dy*dy + dz*dz;
                if d2 > cull2 { return None; }
                let far_slot = d2 > sparkle2;
                Some((s, far_slot))
            })
            .take(self.inner.capacity as usize)
            .map(|(s, far_slot)| {
                let bob = if s.bob_amp > 1e-4 {
                    s.bob_amp * (t * s.bob_w + s.bob_phase).sin()
                } else { 0.0 };
                // Scale pulse (breathing / throb) around base size.
                let pulse = if s.pulse_amp > 1e-4 {
                    1.0 + s.pulse_amp * (t * s.pulse_w + s.bob_phase).sin()
                } else { 1.0 };
                // Pop-in ease: scale 0 → 1.2 → 1.0 over pop_ease_t.
                // Uses cubic overshoot (Nintendo Switch button pop).
                let pop_scale = if s.pop_ease_t > 1e-4 {
                    let k = (s.age / s.pop_ease_t).clamp(0.0, 1.0);
                    // easeOutBack-like: 2.7·k - 1.7·k² (peak ~1.2)
                    let e = 2.7 * k - 1.7 * k * k;
                    e.max(0.01)
                } else { 1.0 };
                let size = s.size * pulse * pop_scale;
                // Rotation wiggle + per-sprite phase keeps sparkles
                // from rotating in unison.
                let wiggle = if s.wiggle_amp > 1e-4 {
                    s.wiggle_amp * (t * s.wiggle_w + s.bob_phase).sin()
                } else { 0.0 };
                let rot = s.rot + wiggle;
                // Alpha fade with pop-in gate: during pop_ease_t the
                // sprite is fully opaque; after that linear fade.
                let raw_fade = (1.0 - s.age / s.life.max(1e-3)).clamp(0.0, 1.0);
                let alpha = if s.pop_ease_t > 1e-4 && s.age < s.pop_ease_t {
                    1.0
                } else { raw_fade };
                // Far sprites collapse to SPARKLE_STAR so distant
                // activity reads as ambient "twinkle" rather than
                // illegible pixel noise. Tint shifts warmer so the
                // sparkle still carries fire-vs-water cue.
                let (out_slot, out_size) = if far_slot {
                    (kami_render::scene_pipelines::atlas_slot::SPARKLE_STAR,
                     size * 0.6)
                } else {
                    (s.slot, size)
                };
                AtlasInstance {
                    pos: [s.pos.x, s.pos.y + bob, s.pos.z],
                    tint: s.tint, size: out_size, slot: out_slot, rot, alpha,
                }
            })
            .collect();
        *self.inner.instance_count.borrow_mut() = instances.len() as u32;
        let buf = self.inner.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("atlas_vis.instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            },
        );
        *self.inner.instance_vb.borrow_mut() = buf;
        instances.len() as u32
    }
}

impl RenderPipeline for AtlasVisAdapter {
    fn prepare(&mut self, _ctx: &RenderContext, camera: &Camera, _world: &World) {
        *self.inner.cam_pos.borrow_mut() = camera.as_render().uniform().position;
        self.tick_and_upload(1.0 / 60.0);
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
        let count = *self.inner.instance_count.borrow();
        if count == 0 { return; }
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let vp = proj * view_m;
        let right = Vec3::new(view_m.x_axis.x, view_m.y_axis.x, view_m.z_axis.x);
        let up = Vec3::new(view_m.x_axis.y, view_m.y_axis.y, view_m.z_axis.y);
        let au = AtlasUniform {
            view_proj: vp.to_cols_array(),
            cam_right: right.to_array(),
            _p0: 0.0,
            cam_up: up.to_array(),
            _p1: 0.0,
        };
        ctx.queue.write_buffer(&self.inner.pipeline.uniform, 0, bytemuck::bytes_of(&au));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("atlas_vis.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view, resolve_target: None,
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
        pass.set_pipeline(&self.inner.pipeline.pipeline);
        pass.set_bind_group(0, &self.inner.pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, self.inner.pipeline.quad_vb.slice(..));
        let vb = self.inner.instance_vb.borrow();
        pass.set_vertex_buffer(1, vb.slice(..));
        pass.set_index_buffer(self.inner.pipeline.quad_ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, 0, 0..count);
    }
}
