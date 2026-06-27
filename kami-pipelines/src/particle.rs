//! Billboard particle system adapter.
//!
//! Shared handle (`Rc<ParticleShared>`) so game tick hooks can emit
//! bursts from anywhere. Particles integrate with constant velocity +
//! gravity CPU-side; the per-frame instance buffer is rebuilt each
//! `prepare()` tick.
//!
//! Usage:
//! ```ignore
//! let particles = ParticleAdapter::new(ctx, 4096);
//! let particles_fx = particles.clone();
//! app.on_update(move |_, cam, _| {
//!     if cam.consume_action() {
//!         particles_fx.burst(cam.eye() + cam.forward() * 3.0, 12, [1.0, 0.8, 0.3]);
//!     }
//! }).with_pipeline(particles);
//! ```

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::RenderContext;
use kami_render::scene_pipelines::{ParticlePipeline, ParticleUniform};
use std::cell::RefCell;
use std::rc::Rc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ParticleInstance {
    pos: [f32; 3],
    col: [f32; 3],
    size: f32,
    age: f32,
    life: f32,
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    col: [f32; 3],
    size: f32,
    age: f32,
    life: f32,
    /// When true, skip gravity integration. Used for
    /// streamline-style flow visualisation where the particle is a
    /// static marker along a traced path, not a physical debris.
    no_gravity: bool,
}

struct ParticleShared {
    pipeline: ParticlePipeline,
    device: wgpu::Device,
    /// CPU-side live particles. Drained on tick when age >= life.
    particles: RefCell<Vec<Particle>>,
    /// GPU instance buffer. Rebuilt each `prepare()`.
    instance_vb: RefCell<wgpu::Buffer>,
    capacity: u32,
}

#[derive(Clone)]
pub struct ParticleAdapter {
    inner: Rc<ParticleShared>,
}

impl ParticleAdapter {
    pub fn new(ctx: &RenderContext, capacity: u32) -> Self {
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle.instances"),
            size: (capacity as u64) * std::mem::size_of::<ParticleInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            inner: Rc::new(ParticleShared {
                pipeline: ParticlePipeline::new(&ctx.device, ctx.format),
                device: ctx.device.clone(),
                particles: RefCell::new(Vec::new()),
                instance_vb: RefCell::new(buf),
                capacity,
            }),
        }
    }

    /// Emit a single particle with explicit motion.
    pub fn emit(&self, pos: Vec3, vel: Vec3, col: [f32; 3], size: f32, life: f32) {
        let mut p = self.inner.particles.borrow_mut();
        if p.len() as u32 >= self.inner.capacity {
            return; // buffer full, drop
        }
        p.push(Particle {
            pos,
            vel,
            col,
            size,
            age: 0.0,
            life,
            no_gravity: false,
        });
    }

    /// Emit a gravity-free particle for streamline / flow-marker
    /// visualisation. The particle stays at `pos` (no velocity, no
    /// gravity) and fades over `life`.
    pub fn emit_flow(&self, pos: Vec3, col: [f32; 3], size: f32, life: f32) {
        let mut p = self.inner.particles.borrow_mut();
        if p.len() as u32 >= self.inner.capacity {
            return;
        }
        p.push(Particle {
            pos,
            vel: Vec3::ZERO,
            col,
            size,
            age: 0.0,
            life,
            no_gravity: true,
        });
    }

    /// Burst: emit `count` particles outward from `pos` with slight
    /// upward bias + randomized horizontal spread. Good for mining /
    /// block-break effects. RNG is deterministic per burst via a
    /// fnv-ish xorshift over pos; this keeps the API simple.
    pub fn burst(&self, pos: Vec3, count: u32, base_col: [f32; 3]) {
        let mut seed =
            (pos.x * 73856093.0) as u32 ^ (pos.y * 19349663.0) as u32 ^ (pos.z * 83492791.0) as u32;
        let mut next = || -> f32 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed & 0x7fffffff) as f32 / 0x7fffffff as f32
        };
        for _ in 0..count {
            let theta = next() * std::f32::consts::TAU;
            let up = 2.0 + next() * 4.0; // 2-6 m/s upward
            let r = 2.0 + next() * 3.0; // 2-5 m/s horizontal
            let vel = Vec3::new(theta.cos() * r, up, theta.sin() * r);
            let tint = 0.85 + next() * 0.3;
            let col = [base_col[0] * tint, base_col[1] * tint, base_col[2] * tint];
            let life = 0.6 + next() * 0.4;
            let size = 0.15 + next() * 0.1;
            self.emit(pos, vel, col, size, life);
        }
    }

    fn tick_and_upload(&self, dt: f32) -> u32 {
        // Integrate motion + age, cull dead.
        const GRAVITY: f32 = 9.8;
        let mut particles = self.inner.particles.borrow_mut();
        particles.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.life {
                return false;
            }
            if !p.no_gravity {
                p.vel.y -= GRAVITY * dt;
                p.pos += p.vel * dt;
            }
            true
        });

        // Upload instance buffer.
        if particles.is_empty() {
            return 0;
        }
        let instances: Vec<ParticleInstance> = particles
            .iter()
            .take(self.inner.capacity as usize)
            .map(|p| ParticleInstance {
                pos: p.pos.to_array(),
                col: p.col,
                size: p.size,
                age: p.age,
                life: p.life,
            })
            .collect();
        let bytes = bytemuck::cast_slice(&instances);
        let buf = self
            .inner
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("particle.instances"),
                contents: bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
        *self.inner.instance_vb.borrow_mut() = buf;
        instances.len() as u32
    }
}

impl RenderPipeline for ParticleAdapter {
    fn prepare(&mut self, _ctx: &RenderContext, _camera: &Camera, _world: &World) {
        // Integration fixed at 60 Hz for particle stability. Callers
        // at higher rates will see slightly long-lived particles but
        // the look is preserved.
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
        let count = self.inner.particles.borrow().len() as u32;
        if count == 0 {
            return;
        }
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let vp = proj * view_m;
        // Extract camera right/up from view matrix (row-major → cols).
        // View matrix columns: right=row0, up=row1, forward=row2 (in
        // a typical LH convention). With glam's right-handed LookAt,
        // basis vectors are the view matrix rows.
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
            .write_buffer(&self.inner.pipeline.uniform, 0, bytemuck::bytes_of(&pu));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particle.pass"),
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
        pass.set_pipeline(&self.inner.pipeline.pipeline);
        pass.set_bind_group(0, &self.inner.pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, self.inner.pipeline.quad_vb.slice(..));
        let vb = self.inner.instance_vb.borrow();
        pass.set_vertex_buffer(1, vb.slice(..));
        pass.set_index_buffer(
            self.inner.pipeline.quad_ib.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(0..6, 0, 0..count);
    }
}
