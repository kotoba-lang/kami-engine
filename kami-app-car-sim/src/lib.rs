//! kami-app-car-sim — public car-sim demo (`driver.etzhayyim.com`).
//!
//! Loads a `kami-vehicle` BeamNG-grade soft-body car (vehicle kind +
//! paint colour selected from JS globals at boot), renders the chassis
//! beams as a wireframe and the body panels as filled, Lambert-shaded
//! triangles, drives it from JS-side `window.__carsim_*` controls,
//! publishes telemetry to `window.__carsim_hud`.

use std::cell::RefCell;
use std::rc::Rc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use kami_app::pipeline::RenderPipeline;
use kami_app::{Camera, CameraMode, InputMode, KamiApp};
use kami_render::RenderContext;
use kami_vehicle::{
    ground::{FlatGround, MapGround, SurfaceKind},
    models::garage::{build, VehicleKind},
    triangle::TriangleGroup,
    IntegratorMode, Vehicle,
};
#[cfg(target_family = "wasm")]
use log::Level;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    cam_pos: [f32; 4],
    color: [f32; 4],
    light_dir: [f32; 4],
    /// `params.x` = time (seconds, animated by RAF loop).
    /// `params.y` = flake density (0..1, 0.10 default).
    /// `params.z` = flake scale (m^-1, 60.0 default).
    /// `params.w` = clear-coat strength (0..1, 0.85 default).
    params: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct LineVertex {
    pos: [f32; 3],
    col: [f32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct TriVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    col: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GroundVertex {
    pos: [f32; 3],
    col: [f32; 3],
    surface_id: f32,
}

const MAX_LINES: usize = 4096;
const MAX_TRIS: usize = 4096;

fn surface_id(s: SurfaceKind) -> f32 {
    match s {
        SurfaceKind::AsphaltDry => 0.0,
        SurfaceKind::AsphaltWet => 1.0,
        SurfaceKind::Gravel => 2.0,
        SurfaceKind::Sand => 3.0,
        SurfaceKind::Snow => 4.0,
        SurfaceKind::Ice => 5.0,
        SurfaceKind::Mud => 6.0,
        SurfaceKind::Grass => 7.0,
    }
}

struct CarSimPipeline {
    line_pipeline: wgpu::RenderPipeline,
    tri_pipeline: wgpu::RenderPipeline,
    ground_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    line_buf: wgpu::Buffer,
    line_count: u32,
    tri_buf: wgpu::Buffer,
    tri_count: u32,
    ground_buf: wgpu::Buffer,
    ground_count: u32,
    vehicle: Rc<RefCell<Vehicle>>,
    map: MapGround,
    paint: [f32; 3],
    sky: SkyState,
}

impl CarSimPipeline {
    fn new(ctx: &RenderContext, vehicle: Rc<RefCell<Vehicle>>, paint: [f32; 3], map: MapGround) -> Self {
        let device = &ctx.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("car-sim/shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("car-sim/uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Sky cubemap driven by `kami-atmosphere` (Rayleigh sky model
        // + day/night sun colour). Initial bake at morning, clear
        // weather; the pipeline rebakes in `prepare()` whenever
        // `__carsim_time_of_day` or `__carsim_weather` JS globals
        // change.
        let init_time = js_get_f32_or("__carsim_time_of_day", 0.35);
        let init_overcast = js_weather_is_overcast();
        let sky = build_sky_cubemap(ctx, init_time, init_overcast);

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("car-sim/bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("car-sim/bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sky.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sky.sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("car-sim/pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let depth_stencil = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("car-sim/line-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(depth_stencil.clone()),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let tri_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("car-sim/tri-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_tri"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TriVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None, // double-sided so deformed panels stay visible
                ..Default::default()
            },
            depth_stencil: Some(depth_stencil),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_tri"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        // Ground tile pipeline (textured by surface_id in fragment shader).
        let ground_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("car-sim/ground-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ground"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GroundVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32 },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ground"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let line_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("car-sim/lines"),
            size: (MAX_LINES * 2 * std::mem::size_of::<LineVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tri_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("car-sim/tris"),
            size: (MAX_TRIS * 3 * std::mem::size_of::<TriVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Ground: ~100 zones max × 6 verts (2 triangles per quad) = 600.
        let ground_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("car-sim/ground"),
            size: (256 * 6 * std::mem::size_of::<GroundVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut p = Self {
            line_pipeline,
            tri_pipeline,
            ground_pipeline,
            bind_group,
            uniform_buf,
            line_buf,
            line_count: 0,
            tri_buf,
            tri_count: 0,
            ground_buf,
            ground_count: 0,
            vehicle,
            map,
            paint,
            sky,
        };
        // Build the ground geometry once (zones are static).
        p.upload_ground(ctx);
        p
    }

    fn upload_ground(&mut self, ctx: &RenderContext) {
        // Quad order so triangle normal points up: (xmin,zmin) (xmax,zmin) (xmax,zmax) (xmin,zmin) (xmax,zmax) (xmin,zmax)
        let mut verts: Vec<GroundVertex> = Vec::new();
        // First, the DEFAULT (background) covering a large area.
        let bg = self.map.default;
        let bg_id = surface_id(bg);
        let bg_col = bg.tint();
        let bg_size = 200.0;
        for &(x, z) in &[
            (-bg_size, -bg_size), (bg_size, -bg_size), (bg_size, bg_size),
            (-bg_size, -bg_size), (bg_size, bg_size), (-bg_size, bg_size),
        ] {
            verts.push(GroundVertex {
                pos: [x, 0.0, z],
                col: bg_col,
                surface_id: bg_id,
            });
        }
        // Each zone overlays its rectangle on top.
        for zone in &self.map.zones {
            let id = surface_id(zone.surface);
            let col = zone.surface.tint();
            // Slightly above the background so it draws on top.
            let y = 0.001;
            let (x0, x1, z0, z1) = (zone.x_min, zone.x_max, zone.z_min, zone.z_max);
            for &(x, z) in &[
                (x0, z0), (x1, z0), (x1, z1),
                (x0, z0), (x1, z1), (x0, z1),
            ] {
                verts.push(GroundVertex {
                    pos: [x, y, z],
                    col,
                    surface_id: id,
                });
            }
        }
        self.ground_count = (verts.len() / 3) as u32;
        ctx.queue.write_buffer(&self.ground_buf, 0, bytemuck::cast_slice(&verts));
    }

    fn rebuild_geometry(&mut self, ctx: &RenderContext) {
        let v = self.vehicle.borrow();

        // ── Lines: ground grid + structural beams + tire ring ──
        let mut lines: Vec<LineVertex> = Vec::with_capacity(v.beams.len() * 2 + 256);
        let grid_col = [0.16, 0.20, 0.26];
        for i in -25..=25 {
            let f = i as f32 * 2.0;
            lines.push(LineVertex { pos: [-50.0, 0.0, f], col: grid_col });
            lines.push(LineVertex { pos: [ 50.0, 0.0, f], col: grid_col });
            lines.push(LineVertex { pos: [f, 0.0, -50.0], col: grid_col });
            lines.push(LineVertex { pos: [f, 0.0,  50.0], col: grid_col });
        }
        for b in v.beams.iter() {
            if b.broken {
                continue;
            }
            let n1 = match v.nodes.iter().find(|n| n.id == b.n1) {
                Some(n) => n, None => continue,
            };
            let n2 = match v.nodes.iter().find(|n| n.id == b.n2) {
                Some(n) => n, None => continue,
            };
            // Colour beams by node group: tire ring = bright rubber-grey,
            // hub spokes = silver, structural = stress-tinted.
            use kami_vehicle::NodeGroup as NG;
            let col = match (n1.group, n2.group) {
                // Tire tread (ring-to-ring) — bright outline so the wheel is
                // visible against the dark scene.
                (NG::WheelTire, NG::WheelTire) => [0.85, 0.85, 0.88],
                // Sidewall spokes (ring-to-hub).
                (NG::WheelTire, _) | (_, NG::WheelTire) => [0.55, 0.55, 0.58],
                (NG::WheelHub, _) | (_, NG::WheelHub) => [0.70, 0.72, 0.78],
                _ => {
                    let l = (n2.position - n1.position).length();
                    let l0 = b.effective_length.max(1e-3);
                    let strain = (l / l0 - 1.0).abs();
                    let stress = (strain / b.deform.break_limit.max(1e-3)).clamp(0.0, 1.0);
                    [0.30 + stress * 0.7, 0.40 - stress * 0.3, 0.30 - stress * 0.3]
                }
            };
            lines.push(LineVertex { pos: n1.position.into(), col });
            lines.push(LineVertex { pos: n2.position.into(), col });
            if lines.len() >= MAX_LINES * 2 - 2 {
                break;
            }
        }
        self.line_count = (lines.len() / 2) as u32;
        ctx.queue.write_buffer(&self.line_buf, 0, bytemuck::cast_slice(&lines));

        // ── Triangles: filled body panels with per-group colouring ──
        let mut tris: Vec<TriVertex> = Vec::with_capacity(v.triangles.len() * 3);
        for t in v.triangles.iter() {
            if tris.len() >= MAX_TRIS * 3 - 3 {
                break;
            }
            let p1 = match v.nodes.iter().find(|n| n.id == t.n1) {
                Some(n) => n.position, None => continue,
            };
            let p2 = match v.nodes.iter().find(|n| n.id == t.n2) {
                Some(n) => n.position, None => continue,
            };
            let p3 = match v.nodes.iter().find(|n| n.id == t.n3) {
                Some(n) => n.position, None => continue,
            };
            let normal = (p2 - p1).cross(p3 - p1).normalize_or_zero();
            let (rgb, alpha) = match t.group {
                TriangleGroup::Body | TriangleGroup::Wing => (self.paint, 0.92),
                TriangleGroup::Window => ([0.20, 0.30, 0.42], 0.55),
                TriangleGroup::Underbody => ([0.10, 0.11, 0.13], 0.95),
            };
            for &p in &[p1, p2, p3] {
                tris.push(TriVertex {
                    pos: p.into(),
                    normal: normal.into(),
                    col: [rgb[0], rgb[1], rgb[2], alpha],
                });
            }
        }

        // ── Wheel discs: triangulate each tire ring around its hub axle ──
        // For each wheel, fan-triangulate ring nodes around the axle
        // mid-point, on both sides of the wheel (inner + outer face).
        for w in v.wheels.iter() {
            if w.tire_nodes.len() < 3 {
                continue;
            }
            if tris.len() >= MAX_TRIS * 3 - 12 {
                break;
            }
            let n_axle1 = match v.nodes.iter().find(|n| n.id == w.axle_n1) {
                Some(n) => n, None => continue,
            };
            let n_axle2 = match v.nodes.iter().find(|n| n.id == w.axle_n2) {
                Some(n) => n, None => continue,
            };
            // Tire side colour — dark rubber.
            let tire_rgb = [0.16, 0.16, 0.18];
            let tire_alpha = 0.95;
            // Inner face: fan from axle_n1 to ring nodes.
            let count = w.tire_nodes.len();
            for i in 0..count {
                let a = match v.nodes.iter().find(|n| n.id == w.tire_nodes[i]) {
                    Some(n) => n.position, None => continue,
                };
                let b = match v.nodes.iter().find(|n| n.id == w.tire_nodes[(i + 1) % count]) {
                    Some(n) => n.position, None => continue,
                };
                // Inner sidewall (around axle_n1).
                let normal = (a - n_axle1.position).cross(b - n_axle1.position).normalize_or_zero();
                for &p in &[n_axle1.position, a, b] {
                    tris.push(TriVertex {
                        pos: p.into(),
                        normal: normal.into(),
                        col: [tire_rgb[0], tire_rgb[1], tire_rgb[2], tire_alpha],
                    });
                }
                // Outer sidewall (around axle_n2).
                let normal = (b - n_axle2.position).cross(a - n_axle2.position).normalize_or_zero();
                for &p in &[n_axle2.position, b, a] {
                    tris.push(TriVertex {
                        pos: p.into(),
                        normal: normal.into(),
                        col: [tire_rgb[0], tire_rgb[1], tire_rgb[2], tire_alpha],
                    });
                }
                // Tread band (ring-i → ring-(i+1) → axle-mid). Connect
                // axle_n1 and axle_n2 via a tread quad.
                let tread_rgb = [0.10, 0.10, 0.12];
                let tread_n = (b - a).cross(n_axle2.position - a).normalize_or_zero();
                // Two triangles for the quad (a, b, a_outer_edge, b_outer_edge).
                // Approximate "outer edge" by interpolating to axle_n2 side.
                let a_out = a + (n_axle2.position - n_axle1.position);
                let b_out = b + (n_axle2.position - n_axle1.position);
                for &p in &[a, b, b_out] {
                    tris.push(TriVertex {
                        pos: p.into(),
                        normal: tread_n.into(),
                        col: [tread_rgb[0], tread_rgb[1], tread_rgb[2], tire_alpha],
                    });
                }
                for &p in &[a, b_out, a_out] {
                    tris.push(TriVertex {
                        pos: p.into(),
                        normal: tread_n.into(),
                        col: [tread_rgb[0], tread_rgb[1], tread_rgb[2], tire_alpha],
                    });
                }
                if tris.len() >= MAX_TRIS * 3 - 12 {
                    break;
                }
            }
        }

        self.tri_count = (tris.len() / 3) as u32;
        ctx.queue.write_buffer(&self.tri_buf, 0, bytemuck::cast_slice(&tris));
    }

    fn upload_uniforms(&self, ctx: &RenderContext, camera: &Camera, time_s: f32) {
        let view_proj = camera.view_projection();
        let cam_pos = camera.eye();
        let light_dir = Vec3::new(-0.4, 1.0, 0.6).normalize();
        // Defaults tuned to look like a metallic clear-coat over the
        // existing kami-vehicle paint colour. JS can override via
        // `window.__carsim_paint_*` if a UI exposes the knobs.
        let flake_density = js_get_f32_or("__carsim_paint_flake_density", 0.10);
        let flake_scale = js_get_f32_or("__carsim_paint_flake_scale", 60.0);
        let clearcoat = js_get_f32_or("__carsim_paint_clearcoat", 0.85);
        let u = Uniforms {
            view_proj: view_proj.to_cols_array(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            params: [time_s, flake_density, flake_scale, clearcoat],
        };
        ctx.queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
    }
}

impl RenderPipeline for CarSimPipeline {
    fn prepare(
        &mut self,
        ctx: &RenderContext,
        camera: &Camera,
        _world: &hecs::World,
    ) {
        // Cheap per-frame check — `bake_sky_data` is a no-op when neither
        // time-of-day nor weather changed. JS clients can drive these via
        // `window.__carsim_time_of_day` (0..1) and `window.__carsim_weather`
        // ("clear" | "overcast").
        let time = js_get_f32_or("__carsim_time_of_day", 0.35);
        let overcast = js_weather_is_overcast();
        bake_sky_data(ctx, &mut self.sky, time, overcast);

        self.rebuild_geometry(ctx);
        self.upload_uniforms(ctx, camera, now_seconds());
    }

    fn record(
        &self,
        _ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        _camera: &Camera,
        _world: &hecs::World,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("car-sim/pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.04, g: 0.05, b: 0.08, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);

        // 1. Ground tiles (back-most).
        if self.ground_count > 0 {
            pass.set_pipeline(&self.ground_pipeline);
            pass.set_vertex_buffer(0, self.ground_buf.slice(..));
            pass.draw(0..self.ground_count * 3, 0..1);
        }
        // 2. Filled body panels.
        if self.tri_count > 0 {
            pass.set_pipeline(&self.tri_pipeline);
            pass.set_vertex_buffer(0, self.tri_buf.slice(..));
            pass.draw(0..self.tri_count * 3, 0..1);
        }
        // 3. Wireframe overlay.
        if self.line_count > 0 {
            pass.set_pipeline(&self.line_pipeline);
            pass.set_vertex_buffer(0, self.line_buf.slice(..));
            pass.draw(0..self.line_count * 2, 0..1);
        }
    }
}

// ── JS bridge ──

/// Sky-cubemap state — the wgpu texture + view + sampler are created
/// once at init; the *contents* get rebaked on demand whenever
/// time-of-day or weather change. Re-using the same `Texture` /
/// `TextureView` means we don't have to recreate the bind group on
/// rebake — `queue.write_texture` updates the contents in place.
struct SkyState {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    last_time: f32,
    last_overcast: bool,
}

const SKY_SIZE: u32 = 64;

fn build_sky_cubemap(ctx: &RenderContext, time: f32, overcast: bool) -> SkyState {
    let device = &ctx.device;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("car-sim/sky-cube"),
        size: wgpu::Extent3d {
            width: SKY_SIZE,
            height: SKY_SIZE,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("car-sim/sky-cube-view"),
        // The view format must match the texture's HDR format. WGSL
        // binds it as `texture_cube<f32>` and the GPU handles the
        // f16→f32 expansion. The Reinhard tone-map at the bottom of
        // `fs_tri` then compresses HDR values back into [0,1] before
        // output.
        format: Some(wgpu::TextureFormat::Rgba16Float),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        usage: None,
        aspect: wgpu::TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: Some(1),
        base_array_layer: 0,
        array_layer_count: Some(6),
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("car-sim/sky-cube-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let mut state = SkyState {
        texture,
        view,
        sampler,
        last_time: f32::NAN,
        last_overcast: !overcast, // force initial bake
    };
    bake_sky_data(ctx, &mut state, time, overcast);
    state
}

/// Standard wgpu cubemap face → world direction lookup.
fn cube_face_dir(face: u32, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
    .normalize()
}

/// Rebake the cubemap contents from `kami-atmosphere`'s Rayleigh sky
/// model + day/night sun colour. No-op when params haven't changed.
fn bake_sky_data(ctx: &RenderContext, state: &mut SkyState, time: f32, overcast: bool) {
    if (time - state.last_time).abs() < 1e-3 && overcast == state.last_overcast {
        return;
    }
    state.last_time = time;
    state.last_overcast = overcast;

    let cycle = kami_atmosphere::DayNightCycle { time, period: 600.0 };
    let sun_dir = cycle.sun_direction();
    let sun_col = cycle.sun_color();

    let queue = &ctx.queue;
    // Per-pixel: 4 channels × 2 bytes (f16) = 8 bytes.
    const BYTES_PER_PIXEL: u32 = 8;
    for face in 0..6u32 {
        let mut buf = vec![0u8; (SKY_SIZE * SKY_SIZE * BYTES_PER_PIXEL) as usize];
        for j in 0..SKY_SIZE {
            for i in 0..SKY_SIZE {
                let u = (i as f32 + 0.5) / SKY_SIZE as f32 * 2.0 - 1.0;
                let v = (j as f32 + 0.5) / SKY_SIZE as f32 * 2.0 - 1.0;
                let dir = cube_face_dir(face, u, v);
                let mut sky = kami_atmosphere::rayleigh_sky_color(dir, sun_dir, sun_col);
                // HDR sun glare — when the cube cell looks straight at
                // the sun (cos θ > 0.99) we pump up the brightness ×5
                // so the paint clear-coat picks up a real, blinding
                // highlight. Reinhard tone-map at the end of fs_tri
                // compresses this back into displayable range.
                let cos_sun = dir.dot(sun_dir).clamp(0.0, 1.0);
                if cos_sun > 0.985 {
                    let edge = ((cos_sun - 0.985) / 0.015).clamp(0.0, 1.0);
                    sky += sun_col * 5.0 * edge;
                }
                if overcast {
                    sky = sky.lerp(Vec3::splat(0.55), 0.65);
                }
                let off = ((j * SKY_SIZE + i) * BYTES_PER_PIXEL) as usize;
                let pixel = [
                    f32_to_f16_bits(sky.x.max(0.0)),
                    f32_to_f16_bits(sky.y.max(0.0)),
                    f32_to_f16_bits(sky.z.max(0.0)),
                    f32_to_f16_bits(1.0),
                ];
                for (k, half_bits) in pixel.iter().enumerate() {
                    let b = half_bits.to_le_bytes();
                    buf[off + k * 2] = b[0];
                    buf[off + k * 2 + 1] = b[1];
                }
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &state.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: face },
                aspect: wgpu::TextureAspect::All,
            },
            &buf,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SKY_SIZE * BYTES_PER_PIXEL),
                rows_per_image: Some(SKY_SIZE),
            },
            wgpu::Extent3d {
                width: SKY_SIZE,
                height: SKY_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Convert an IEEE 754 binary32 float to binary16 (half) bit pattern.
/// Handles overflow → ±Inf, underflow → ±0, otherwise exponent shift +
/// 13-bit mantissa truncation. Sufficient for HDR colour values where
/// we don't need denormal precision.
fn f32_to_f16_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 31) & 1) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7fffff;
    if exp == 0 {
        // ±0 (or denormal — flush to 0).
        return sign << 15;
    }
    if exp == 0xff {
        // Inf / NaN.
        let h_mant = if mant != 0 { 0x200u16 } else { 0 };
        return (sign << 15) | (0x1f << 10) | h_mant;
    }
    let new_exp = exp - 127 + 15;
    if new_exp >= 0x1f {
        // Overflow → ±Inf.
        return (sign << 15) | (0x1f << 10);
    }
    if new_exp <= 0 {
        // Underflow → 0 (don't bother with denormals — colour values
        // never need that resolution).
        return sign << 15;
    }
    let new_mant = (mant >> 13) as u16;
    (sign << 15) | ((new_exp as u16) << 10) | new_mant
}

#[cfg(test)]
mod f16_tests {
    use super::f32_to_f16_bits;

    fn round_trip(v: f32) -> f32 {
        let bits = f32_to_f16_bits(v);
        let sign = (bits >> 15) & 1;
        let exp = (bits >> 10) & 0x1f;
        let mant = bits & 0x3ff;
        if exp == 0 {
            return if sign == 1 { -0.0 } else { 0.0 };
        }
        if exp == 0x1f {
            return if mant == 0 {
                if sign == 1 { f32::NEG_INFINITY } else { f32::INFINITY }
            } else {
                f32::NAN
            };
        }
        let f_exp = (exp as i32 - 15 + 127) as u32;
        let f_mant = (mant as u32) << 13;
        let f_bits = ((sign as u32) << 31) | (f_exp << 23) | f_mant;
        f32::from_bits(f_bits)
    }

    #[test]
    fn round_trips_zero_one_and_typical_colour_values() {
        for &v in &[0.0_f32, 0.5, 1.0, 1.5, 5.0, 10.0] {
            let r = round_trip(v);
            assert!((r - v).abs() < 0.01, "v={v} round={r}");
        }
    }

    #[test]
    fn overflow_clamps_to_inf() {
        let bits = f32_to_f16_bits(70_000.0);
        assert_eq!(bits & 0x7fff, 0x7c00, "got 0x{bits:04x}");
    }
}

#[cfg(target_family = "wasm")]
fn js_get_f32(name: &str) -> f32 {
    let win = match web_sys::window() {
        Some(w) => w, None => return 0.0,
    };
    js_sys::Reflect::get(&win, &JsValue::from_str(name))
        .unwrap_or(JsValue::from_f64(0.0))
        .as_f64()
        .unwrap_or(0.0) as f32
}

#[cfg(target_family = "wasm")]
fn js_get_f32_or(name: &str, default: f32) -> f32 {
    let win = match web_sys::window() {
        Some(w) => w, None => return default,
    };
    let v = js_sys::Reflect::get(&win, &JsValue::from_str(name))
        .unwrap_or(JsValue::UNDEFINED);
    if v.is_undefined() || v.is_null() {
        return default;
    }
    v.as_f64().map(|f| f as f32).unwrap_or(default)
}

#[cfg(not(target_family = "wasm"))]
fn js_get_f32_or(_name: &str, default: f32) -> f32 {
    default
}

#[cfg(target_family = "wasm")]
fn js_weather_is_overcast() -> bool {
    js_get_str("__carsim_weather").trim().eq_ignore_ascii_case("overcast")
}

#[cfg(not(target_family = "wasm"))]
fn js_weather_is_overcast() -> bool {
    false
}

#[cfg(target_family = "wasm")]
fn now_seconds() -> f32 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| (p.now() / 1000.0) as f32)
        .unwrap_or(0.0)
}

#[cfg(not(target_family = "wasm"))]
fn now_seconds() -> f32 {
    0.0
}

#[cfg(target_family = "wasm")]
fn js_get_str(name: &str) -> String {
    let win = match web_sys::window() {
        Some(w) => w, None => return String::new(),
    };
    js_sys::Reflect::get(&win, &JsValue::from_str(name))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

#[cfg(target_family = "wasm")]
fn js_set_str(name: &str, value: &str) {
    let win = match web_sys::window() {
        Some(w) => w, None => return,
    };
    let _ = js_sys::Reflect::set(&win, &JsValue::from_str(name), &JsValue::from_str(value));
}

#[cfg(target_family = "wasm")]
fn js_set_obj(name: &str, props: &[(&str, f64)]) {
    let win = match web_sys::window() {
        Some(w) => w, None => return,
    };
    let obj = js_sys::Object::new();
    for (k, v) in props {
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), &JsValue::from_f64(*v));
    }
    let _ = js_sys::Reflect::set(&win, &JsValue::from_str(name), &obj);
}

fn parse_color_hex(s: &str) -> [f32; 3] {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return [0.85, 0.20, 0.25]; // default red
    }
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(218) as f32 / 255.0;
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(51) as f32 / 255.0;
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(64) as f32 / 255.0;
    [r, g, b]
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_car_sim(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(Level::Info);

    // Vehicle kind + paint from JS globals (defaults: sedan, red).
    let kind_id = js_get_str("__carsim_vehicle");
    let kind = if kind_id.is_empty() {
        VehicleKind::Sedan
    } else {
        VehicleKind::from_id(&kind_id)
    };
    let paint_hex = js_get_str("__carsim_paint");
    let paint = if paint_hex.is_empty() {
        [0.85, 0.20, 0.25]
    } else {
        parse_color_hex(&paint_hex)
    };
    log::info!("[car-sim] kind={} paint={:?}", kind.id(), paint);
    // The VEHICLE is now DATA: build it from kami-vehicle-scene's garage.edn (the
    // canonical, parity-tested source of truth — behaviourally identical to
    // build_vehicle(kind)). Fall back to the compiled-in builder only if the
    // shipped EDN ever fails to parse / resolve.
    let mut car = kami_vehicle_scene::build_from_edn(kind.id()).unwrap_or_else(|e| {
        log::warn!("[car-sim] garage.edn build failed ({e}); using builtin build({})", kind.id());
        build(kind)
    });
    // Force XPBD — Implicit mode is still being stabilised (its
    // stiffness-matrix sign needs more work; in the meantime the
    // XPBD + rigid-chassis combo is the better-behaved default).
    car.set_integrator_mode(IntegratorMode::Xpbd);
    car.powertrain.gearbox.current_gear = 1;
    car.powertrain.gearbox.shift_progress = 1.0;
    // Pre-warm so the suspension settles before the user takes control —
    // avoids the chassis-falls-into-ground impulse on frame 1.
    let warm_ground = FlatGround::new(0.0);
    car.settle(&warm_ground, 0.8);

    let vehicle = Rc::new(RefCell::new(car));

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("car-sim")
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 0.7, 1.4),
            distance: 7.5,
            yaw: 0.6,
            pitch: 0.30,
        })
        .with_input(InputMode::OrbitMouse);

    // The circuit is now DATA: load the demo-circuit zones from kami-vehicle-scene's
    // ground.edn (the canonical, parity-tested source of truth). Fall back to the
    // compiled-in builder only if the shipped EDN ever fails to parse.
    let map = kami_vehicle_scene::shipped_demo_circuit().unwrap_or_else(|e| {
        log::warn!("[car-sim] ground.edn load failed ({e}); using builtin demo_circuit");
        MapGround::demo_circuit()
    });
    let pipeline = CarSimPipeline::new(app.render_context(), vehicle.clone(), paint, map.clone());

    let vh = vehicle.clone();
    let app = app.with_pipeline(pipeline).on_update(move |_world, cam, dt| {
        let mut v = vh.borrow_mut();
        v.controls.throttle = js_get_f32("__carsim_throttle").clamp(0.0, 1.0);
        v.controls.brake = js_get_f32("__carsim_brake").clamp(0.0, 1.0);
        v.controls.handbrake = js_get_f32("__carsim_handbrake").clamp(0.0, 1.0);
        v.controls.steer = js_get_f32("__carsim_steer").clamp(-1.0, 1.0);
        let req = js_get_f32("__carsim_gear");
        if req != 0.0 {
            let g = req as i32;
            if g != v.powertrain.gearbox.current_gear {
                v.powertrain.gearbox.current_gear = g;
                v.powertrain.gearbox.shift_progress = 1.0;
            }
        }

        // Multi-surface map provides per-position grip / friction. The
        // car automatically experiences different surfaces as it drives
        // over them.
        let ground = map.clone();

        // One-shot detach / repair commands. JS sets the integer to a
        // break-group ID; we consume it (set back to 0).
        let detach_req = js_get_f32("__carsim_detach") as i32;
        if detach_req > 0 {
            v.break_group(detach_req as u32);
            js_set_obj("__carsim_detach", &[]);
            // (clearing is best-effort — JS resets to 0 after reading)
        }
        let repair_req = js_get_f32("__carsim_repair");
        if repair_req as i32 > 0 {
            if repair_req as i32 == 999 {
                v.repair_all();
            } else {
                v.repair_group(repair_req as u32);
            }
        }

        v.step(dt.min(1.0 / 30.0), &ground);

        let com = v.center_of_mass();
        let speed_kmh = v.speed() * 3.6;
        let rpm = v.engine_rpm();
        let broken = v.beams.iter().filter(|b| b.broken).count() as f64;
        let grounded = v.wheels.iter().filter(|w| w.grounded).count() as f64;
        let surface_under_car = map.surface_at(com.x, com.z);

        cam.as_render_mut().target = com + Vec3::Y * 0.7;

        js_set_obj(
            "__carsim_hud",
            &[
                ("speed_kmh", speed_kmh as f64),
                ("rpm", rpm as f64),
                ("gear", v.powertrain.gearbox.current_gear as f64),
                ("throttle", v.controls.throttle as f64),
                ("brake", v.controls.brake as f64),
                ("steer", v.controls.steer as f64),
                ("com_x", com.x as f64),
                ("com_y", com.y as f64),
                ("com_z", com.z as f64),
                ("broken_beams", broken),
                ("grounded_wheels", grounded),
                ("nodes", v.nodes.len() as f64),
                ("beams", v.beams.len() as f64),
                ("triangles", v.triangles.len() as f64),
                ("surface_id", surface_id(surface_under_car) as f64),
            ],
        );
        // Push the current surface NAME via a dedicated string global.
        js_set_str("__carsim_current_surface", surface_under_car.id());
    });

    log::info!("[car-sim] backend={:?}", app.backend());
    app.run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
