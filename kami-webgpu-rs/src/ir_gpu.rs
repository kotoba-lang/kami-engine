//! ir_gpu — the render-IR GPU executor (ADR-0044 phase 6).
//!
//! Where [`crate::Renderer`] (v1) draws a single hardcoded sun with Reinhard tonemap,
//! `IrRenderer` consumes the parsed [`RenderIr`] and actually *renders on the GPU* the
//! features that were previously data-layer-only:
//!   • **multiple lights** (directional/point/spot, attenuation + spot cone) — `lit_ir.wgsl`
//!   • **IBL** (hemisphere irradiance + procedural env specular) — `lit_ir.wgsl`
//!   • **post chain** bloom + ACES tonemap + vignette — `post.wgsl`
//!   • **anti-aliasing** MSAA (sample_count) + FXAA — `post.wgsl`
//!
//! Pipeline: shadow depth → lit HDR (Rgba16Float, optional MSAA resolve) → bright-pass →
//! separable blur (H,V) → composite(+bloom, ACES, vignette) → FXAA → color target.
//! v1 [`crate::Renderer`] and its golden frames are untouched; this is purely additive.

use super::*;
use glam::{Mat4, Vec3};

const LIT_IR_WGSL: &str = include_str!("lit_ir.wgsl");
const POST_WGSL: &str = include_str!("post.wgsl");
const SHADOW_IR_WGSL: &str = include_str!("shadow_ir.wgsl");
const POINT_SHADOW_WGSL: &str = include_str!("point_shadow.wgsl");

/// Max lights consumed by the shader (must match `MAX_LIGHTS` in lit_ir.wgsl).
pub const MAX_LIGHTS: usize = 8;
/// Max 2D-atlas shadow casters (directional/spot) — must match `MAX_SHADOWS` in lit_ir.wgsl.
pub const MAX_SHADOWS: usize = 4;
/// Sentinel `spot.w` layer marking a light that uses the point-light shadow cube.
pub const POINT_LAYER: i32 = -2;
/// Shadow atlas layer resolution (square).
const SHADOW_RES: u32 = 2048;
/// Point-shadow cube face resolution.
const POINT_RES: u32 = 1024;

/// The 6 cube-face view-projections for a point light at `pos` with `range`
/// (WebGPU cube face order +X,-X,+Y,-Y,+Z,-Z, 90° fov). Pure → unit-testable.
pub fn cube_face_views(pos: Vec3, range: f32) -> [Mat4; 6] {
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.2, range.max(1.0));
    let faces = [
        (Vec3::X, -Vec3::Y), (-Vec3::X, -Vec3::Y),
        (Vec3::Y, Vec3::Z), (-Vec3::Y, -Vec3::Z),
        (Vec3::Z, -Vec3::Y), (-Vec3::Z, -Vec3::Y),
    ];
    faces.map(|(f, up)| proj * Mat4::look_at_rh(pos, pos + f, up))
}
/// Floats of the lit-IR uniform: vp(16) + shadow_vp[4](64) + point_vp[6](96) + 5 vec4 (20)
/// + 8×Lgt(128) = 324. Layout must match `struct G` in lit_ir.wgsl.
const POINT_VP_BASE: usize = 16 + 16 * MAX_SHADOWS;
const VEC4_BASE: usize = POINT_VP_BASE + 16 * 6;        // start of eye/amb/ground/sky/tune
const LIGHTS_BASE: usize = VEC4_BASE + 20;
const GIR_FLOATS: usize = LIGHTS_BASE + 16 * MAX_LIGHTS;
const GIR_BYTES: usize = GIR_FLOATS * 4;

/// Decide which lights cast shadows and build each caster's view-projection.
///
/// Pure (no GPU) so the assignment logic is unit-testable. Directional casters get an
/// orthographic frustum framed on `centroid`; spot casters get a perspective frustum
/// from their position along their direction (cone outer angle → fov); the first
/// shadow-casting **point** light gets the omnidirectional shadow cube (`layer_of` =
/// `POINT_LAYER`, `point` = its index). Returns the per-layer matrices, the per-light
/// atlas layer (`-1` = none, `POINT_LAYER` = cube), active 2D-atlas layer count (capped at
/// MAX_SHADOWS), and the point-shadow light index if any.
pub fn assign_shadows(ir: &RenderIr, centroid: Vec3) -> ([Mat4; MAX_SHADOWS], [i32; MAX_LIGHTS], usize, Option<usize>) {
    let mut mats = [Mat4::IDENTITY; MAX_SHADOWS];
    let mut layer_of = [-1i32; MAX_LIGHTS];
    let mut next = 0usize;
    let mut point = None;
    for (i, l) in ir.lights.iter().take(MAX_LIGHTS).enumerate() {
        if !l.cast_shadow { continue; }
        match l.kind {
            LightKind::Point => {
                if point.is_none() {
                    point = Some(i);
                    layer_of[i] = POINT_LAYER;
                }
            }
            LightKind::Directional if next < MAX_SHADOWS => {
                let d = Vec3::from(l.dir).normalize_or_zero();
                let leye = centroid - d * 200.0;
                mats[next] = Mat4::orthographic_rh(-130.0, 130.0, -130.0, 130.0, 1.0, 420.0)
                    * Mat4::look_at_rh(leye, centroid, Vec3::Y);
                layer_of[i] = next as i32;
                next += 1;
            }
            LightKind::Spot if next < MAX_SHADOWS => {
                let pos = Vec3::from(l.pos);
                let d = Vec3::from(l.dir).normalize_or_zero();
                let fov = (l.spot_outer.max(0.05) * 2.0).min(2.8);
                let far = l.range.max(1.0);
                let up = if d.abs_diff_eq(Vec3::Y, 0.01) || d.abs_diff_eq(-Vec3::Y, 0.01) { Vec3::Z } else { Vec3::Y };
                mats[next] = Mat4::perspective_rh(fov, 1.0, 0.2, far) * Mat4::look_at_rh(pos, pos + d, up);
                layer_of[i] = next as i32;
                next += 1;
            }
            _ => {}
        }
    }
    (mats, layer_of, next, point)
}

/// Pack the lit-IR uniform from camera + lights + env + shadow assignment.
/// Returned as a flat float array matching the std140 `G` layout in lit_ir.wgsl.
pub fn pack_gir(ir: &RenderIr, eye: [f32; 3], vp: &Mat4, shadow_vp: &[Mat4; MAX_SHADOWS], point_vp: &[Mat4; 6], layer_of: &[i32; MAX_LIGHTS]) -> [f32; GIR_FLOATS] {
    let mut g = [0f32; GIR_FLOATS];
    g[0..16].copy_from_slice(&vp.to_cols_array());
    for (k, m) in shadow_vp.iter().enumerate() {
        let b = 16 + 16 * k;
        g[b..b + 16].copy_from_slice(&m.to_cols_array());
    }
    for (k, m) in point_vp.iter().enumerate() {
        let b = POINT_VP_BASE + 16 * k;
        g[b..b + 16].copy_from_slice(&m.to_cols_array());
    }
    let s = VEC4_BASE; // start of the 5 vec4 block
    let n = ir.lights.len().min(MAX_LIGHTS) as f32;
    g[s..s + 4].copy_from_slice(&[eye[0], eye[1], eye[2], n]);
    // amb: ambient rgb + ibl_intensity ; ground: ground rgb + sky-mix weight ; sky: horizon rgb
    let af = ambient_floor(ir);
    g[s + 4..s + 8].copy_from_slice(&[af[0], af[1], af[2], ir.env.ibl_intensity]);
    g[s + 8..s + 12].copy_from_slice(&[ir.env.ground[0], ir.env.ground[1], ir.env.ground[2], 0.5]);
    g[s + 12..s + 16].copy_from_slice(&[ir.globals.horizon[0], ir.globals.horizon[1], ir.globals.horizon[2], 0.0]);
    // tune: specStr, shininess, shadow_bias, texel
    g[s + 16..s + 20].copy_from_slice(&[0.7, 256.0, 0.0025, 1.0 / SHADOW_RES as f32]);
    for (k, l) in ir.lights.iter().take(MAX_LIGHTS).enumerate() {
        let base = LIGHTS_BASE + k * 16;
        let kind = match l.kind {
            LightKind::Directional => 0.0,
            LightKind::Point => 1.0,
            LightKind::Spot => 2.0,
        };
        g[base..base + 4].copy_from_slice(&[l.color[0], l.color[1], l.color[2], l.intensity]);
        g[base + 4..base + 8].copy_from_slice(&[l.pos[0], l.pos[1], l.pos[2], l.range]);
        let d = Vec3::from(l.dir).normalize_or_zero();
        g[base + 8..base + 12].copy_from_slice(&[d.x, d.y, d.z, kind]);
        // spot: cos_inner, cos_outer, _, shadow_layer (-1 = none)
        g[base + 12..base + 16].copy_from_slice(&[l.spot_inner.cos(), l.spot_outer.cos(), 0.0, layer_of[k] as f32]);
    }
    g
}

/// Draw order for correct transparency: opaque instances first (any order), then the
/// alpha-blended ones (`alpha < ~1`) sorted **back-to-front** from `eye`. Returns the
/// index permutation and the opaque count `n_op` (the blend pass draws `[n_op..]`). Pure
/// → unit-testable.
pub fn instance_draw_order(instances: &[Instance], eye: [f32; 3]) -> (Vec<usize>, u32) {
    let eye_v = Vec3::from(eye);
    let opaque = |i: &Instance| i.alpha >= 0.999;
    let mut order: Vec<usize> = (0..instances.len()).collect();
    order.sort_by(|&a, &b| {
        opaque(&instances[b]).cmp(&opaque(&instances[a])).then_with(|| {
            let da = (Vec3::from(instances[a].pos) - eye_v).length_squared();
            let db = (Vec3::from(instances[b].pos) - eye_v).length_squared();
            db.total_cmp(&da) // farther first within the transparent group
        })
    });
    let n_op = instances.iter().filter(|i| opaque(i)).count() as u32;
    (order, n_op)
}

/// A small ambient floor so unlit scenes aren't pitch black (mirrors v1 light_a).
fn ambient_floor(ir: &RenderIr) -> [f32; 3] {
    let a = ir.env.ambient;
    [a[0] * 0.25 + 0.04, a[1] * 0.25 + 0.04, a[2] * 0.25 + 0.05]
}

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ENV_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// IEEE-754 binary32 → binary16 bit pattern. Subnormals flush to zero, overflow → inf,
/// mantissa truncated — ample for an HDR env map (and keeps the crate dependency-free).
pub fn f32_to_f16(f: f32) -> u16 {
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32 - 127 + 15;
    let mant = x & 0x007f_ffff;
    if exp <= 0 {
        sign // too small → ±0
    } else if exp >= 0x1f {
        sign | 0x7c00 // overflow / inf / nan → inf
    } else {
        sign | ((exp as u16) << 10) | ((mant >> 13) as u16)
    }
}

/// Build the mip chain for an equirect env map from RGBA-f32 level-0 pixels (`w*h*4`),
/// box-downsampling to 1×1. Each level is returned as `(w, h, rgba_f16_bits)` for upload.
pub fn build_env_mips(w: u32, h: u32, level0: &[f32]) -> Vec<(u32, u32, Vec<u16>)> {
    let mut levels = Vec::new();
    let mut cur = level0.to_vec();
    let (mut cw, mut ch) = (w.max(1), h.max(1));
    loop {
        levels.push((cw, ch, cur.iter().map(|&x| f32_to_f16(x)).collect()));
        if cw == 1 && ch == 1 { break; }
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![0f32; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let (x0, x1) = ((2 * x).min(cw - 1), (2 * x + 1).min(cw - 1));
                let (y0, y1) = ((2 * y).min(ch - 1), (2 * y + 1).min(ch - 1));
                for c in 0..4 {
                    let at = |px: u32, py: u32| cur[((py * cw + px) * 4 + c) as usize];
                    next[((y * nw + x) * 4 + c) as usize] = (at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1)) * 0.25;
                }
            }
        }
        cur = next; cw = nw; ch = nh;
    }
    levels
}

fn tex2d(device: &wgpu::Device, w: u32, h: u32, fmt: wgpu::TextureFormat, samples: u32, extra: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2,
        format: fmt, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | extra, view_formats: &[],
    })
}

/// GPU executor for the full render-IR (lights/IBL/post/AA).
pub struct IrRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    w: u32,
    h: u32,
    msaa: u32,

    // geometry
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    inst: wgpu::Buffer,
    idx_count: u32,

    // shadow atlas (depth 2D-array, one layer per directional/spot caster)
    shadow_layer_views: Vec<wgpu::TextureView>,
    shadow_pipe: wgpu::RenderPipeline,
    shadow_binds: Vec<wgpu::BindGroup>,
    shadow_ubufs: Vec<wgpu::Buffer>,

    // point-light shadow cube (R16Float distance, 6 faces)
    point_face_views: [wgpu::TextureView; 6],
    point_depth_view: wgpu::TextureView,
    point_pipe: wgpu::RenderPipeline,
    point_binds: Vec<wgpu::BindGroup>,
    point_ubufs: Vec<wgpu::Buffer>,

    // lit (opaque + alpha-blended variants share the bind group)
    lit_pipe: wgpu::RenderPipeline,
    lit_blend_pipe: wgpu::RenderPipeline,
    lit_bind: wgpu::BindGroup,
    gir: wgpu::Buffer,

    // resources kept so lit_bind can be rebuilt when the env map changes
    lit_bgl: wgpu::BindGroupLayout,
    shadow_array_view: wgpu::TextureView,
    shadow_samp: wgpu::Sampler,
    point_cube_view: wgpu::TextureView,
    point_samp: wgpu::Sampler,
    env_samp: wgpu::Sampler,
    env_view: wgpu::TextureView,
    has_env: bool,

    // post
    sampler: wgpu::Sampler,
    bright_pipe: wgpu::RenderPipeline,
    blur_pipe: wgpu::RenderPipeline,
    composite_pipe: wgpu::RenderPipeline,
    fxaa_pipe: wgpu::RenderPipeline,

    // size-dependent targets + bind groups (rebuilt on resize)
    t: Targets,
}

/// All size-dependent GPU resources — rebuilt whenever the surface resizes.
struct Targets {
    depth_view: wgpu::TextureView,
    scene_ms_view: Option<wgpu::TextureView>,
    scene_view: wgpu::TextureView,
    bloom_a_view: wgpu::TextureView,
    bloom_b_view: wgpu::TextureView,
    ldr_view: wgpu::TextureView,
    bright_bind: wgpu::BindGroup,
    blur_h_bind: wgpu::BindGroup,
    blur_v_bind: wgpu::BindGroup,
    comp_bind0: wgpu::BindGroup,
    comp_bind1: wgpu::BindGroup,
    fxaa_bind: wgpu::BindGroup,
    // params kept alive (referenced by the bind groups above)
    _params: [wgpu::Buffer; 5],
}

impl IrRenderer {
    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }

    /// Build the executor for `color_format` at `w`×`h` with `msaa` samples (1 = off, 4 = MSAA 4×).
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, color_format: wgpu::TextureFormat, w: u32, h: u32, msaa: u32) -> Self {
        let msaa = if msaa >= 4 { 4 } else { 1 };
        let (verts, idx) = cube();
        let vbuf = make_buf(&device, &queue, bytemuck::cast_slice(&verts), wgpu::BufferUsages::VERTEX);
        let ibuf = make_buf(&device, &queue, bytemuck::cast_slice(&idx), wgpu::BufferUsages::INDEX);
        let inst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: (MAX_INST * 96) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });

        // vertex layout shared by shadow + lit (cube + instance), identical to v1.
        let va = |fmt, off, loc| wgpu::VertexAttribute { format: fmt, offset: off, shader_location: loc };
        let cube_attrs = [va(wgpu::VertexFormat::Float32x3, 0, 0), va(wgpu::VertexFormat::Float32x3, 12, 1)];
        let inst_attrs = [
            va(wgpu::VertexFormat::Float32x4, 0, 2), va(wgpu::VertexFormat::Float32x4, 16, 3),
            va(wgpu::VertexFormat::Float32x4, 32, 4), va(wgpu::VertexFormat::Float32x4, 48, 5),
            va(wgpu::VertexFormat::Float32x4, 64, 6), va(wgpu::VertexFormat::Float32x4, 80, 7),
        ];
        let vlayout = [
            wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &cube_attrs },
            wgpu::VertexBufferLayout { array_stride: 96, step_mode: wgpu::VertexStepMode::Instance, attributes: &inst_attrs },
        ];

        // ── shadow atlas (depth 2D-array, one layer per shadow-casting light) ──
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("shadow-ir"), source: wgpu::ShaderSource::Wgsl(SHADOW_IR_WGSL.into()) });
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-atlas"),
            size: wgpu::Extent3d { width: SHADOW_RES, height: SHADOW_RES, depth_or_array_layers: MAX_SHADOWS as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let shadow_array_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let shadow_layer_views: Vec<_> = (0..MAX_SHADOWS as u32).map(|k| {
            shadow_tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: k, array_layer_count: Some(1), ..Default::default()
            })
        }).collect();
        let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-ir"), layout: None,
            vertex: wgpu::VertexState { module: &shadow_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
            fragment: None,
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        let shadow_ubufs: Vec<_> = (0..MAX_SHADOWS).map(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow-vp"), size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        })).collect();
        let shadow_binds: Vec<_> = shadow_ubufs.iter().map(|b| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &shadow_pipe.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }],
        })).collect();

        // ── point-light shadow faces (R16Float linear distance, 6-layer 2D-array) ──
        let point_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("point-shadow"), source: wgpu::ShaderSource::Wgsl(POINT_SHADOW_WGSL.into()) });
        let point_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("point-faces"),
            size: wgpu::Extent3d { width: POINT_RES, height: POINT_RES, depth_or_array_layers: 6 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let point_cube_view = point_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let point_face_views: [wgpu::TextureView; 6] = std::array::from_fn(|k| point_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: k as u32, array_layer_count: Some(1), ..Default::default()
        }));
        let point_depth_view = tex2d(&device, POINT_RES, POINT_RES, wgpu::TextureFormat::Depth32Float, 1, wgpu::TextureUsages::empty()).create_view(&Default::default());
        let point_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge, address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let point_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("point-shadow"), layout: None,
            vertex: wgpu::VertexState { module: &point_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
            fragment: Some(wgpu::FragmentState { module: &point_module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::R16Float, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        let point_ubufs: Vec<_> = (0..6).map(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("point-vp"), size: 80, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        })).collect();
        let point_binds: Vec<_> = point_ubufs.iter().map(|b| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &point_pipe.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }],
        })).collect();

        // ── lit HDR pass ──
        let lit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("lit-ir"), source: wgpu::ShaderSource::Wgsl(LIT_IR_WGSL.into()) });
        let gir = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gir"), size: GIR_BYTES as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        // Explicit bind-group layout shared by the opaque + blend pipelines so one `lit_bind`
        // is compatible with both (auto-derived layouts are treated as exclusive per pipeline).
        let fs = wgpu::ShaderStages::FRAGMENT;
        let lit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lit-ir-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: fs, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: fs, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: fs, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 4, visibility: fs, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 5, visibility: fs, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 6, visibility: fs, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let lit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("lit-ir-pl"), bind_group_layouts: &[&lit_bgl], push_constant_ranges: &[] });
        let lit_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit-ir"), layout: Some(&lit_pl),
            vertex: wgpu::VertexState { module: &lit_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
            fragment: Some(wgpu::FragmentState { module: &lit_module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual, stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState { count: msaa, ..Default::default() }, multiview: None, cache: None,
        });
        // alpha-blended variant: src-over blending, depth-tested but no depth write (transparency)
        let lit_blend_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit-ir-blend"), layout: Some(&lit_pl),
            vertex: wgpu::VertexState { module: &lit_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
            fragment: Some(wgpu::FragmentState { module: &lit_module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::LessEqual, stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState { count: msaa, ..Default::default() }, multiview: None, cache: None,
        });
        // default 1×1 env map (procedural fallback active until set_env_map uploads a real one)
        let env_tex0 = tex2d(&device, 1, 1, ENV_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &env_tex0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&[f32_to_f16(0.2), f32_to_f16(0.2), f32_to_f16(0.25), f32_to_f16(1.0)]),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(8), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let env_view = env_tex0.create_view(&Default::default());
        let env_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let lit_bind = make_lit_bind(&device, &lit_bgl, &gir, &shadow_array_view, &shadow_samp, &point_cube_view, &point_samp, &env_view, &env_samp);

        // ── post pipelines (fullscreen triangle) ──
        let post_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("post"), source: wgpu::ShaderSource::Wgsl(POST_WGSL.into()) });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let post_pipe = |entry: &str, fmt: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry), layout: None,
                vertex: wgpu::VertexState { module: &post_module, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState { module: &post_module, entry_point: Some(entry), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: fmt, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None, multisample: Default::default(), multiview: None, cache: None,
            })
        };
        let bright_pipe = post_pipe("fs_bright", HDR_FORMAT);
        let blur_pipe = post_pipe("fs_blur", HDR_FORMAT);
        let composite_pipe = post_pipe("fs_composite", LDR_FORMAT);
        let fxaa_pipe = post_pipe("fs_fxaa", color_format);

        let t = build_targets(&device, &queue, &sampler, &bright_pipe, &blur_pipe, &composite_pipe, &fxaa_pipe, w, h, msaa);
        IrRenderer {
            device, queue, w, h, msaa,
            vbuf, ibuf, inst, idx_count: idx.len() as u32,
            shadow_layer_views, shadow_pipe, shadow_binds, shadow_ubufs,
            point_face_views, point_depth_view, point_pipe, point_binds, point_ubufs,
            lit_pipe, lit_blend_pipe, lit_bind, gir,
            lit_bgl, shadow_array_view, shadow_samp, point_cube_view, point_samp, env_samp, env_view, has_env: false,
            sampler, bright_pipe, blur_pipe, composite_pipe, fxaa_pipe, t,
        }
    }

    /// Upload an equirectangular HDR env map (RGBA-f32 level-0 pixels, `w*h*4`) for
    /// image-based IBL — diffuse from the blurred top mip, specular from a roughness-blurred
    /// mip. Replaces the procedural gradient. Pass it once after `new()` (the IR `:env :ibl
    /// :url` is a host-loaded reference: decode the `.hdr` host-side and hand the pixels here).
    pub fn set_env_map(&mut self, w: u32, h: u32, pixels: &[f32]) {
        let levels = build_env_mips(w, h, pixels);
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("env-map"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: levels.len() as u32, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: ENV_FORMAT, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        for (lvl, (lw, lh, data)) in levels.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: lvl as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(data),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(lw * 8), rows_per_image: Some(*lh) },
                wgpu::Extent3d { width: *lw, height: *lh, depth_or_array_layers: 1 },
            );
        }
        self.env_view = tex.create_view(&Default::default());
        self.has_env = true;
        self.lit_bind = make_lit_bind(&self.device, &self.lit_bgl, &self.gir, &self.shadow_array_view, &self.shadow_samp, &self.point_cube_view, &self.point_samp, &self.env_view, &self.env_samp);
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.w = w; self.h = h;
        self.t = build_targets(&self.device, &self.queue, &self.sampler, &self.bright_pipe, &self.blur_pipe, &self.composite_pipe, &self.fxaa_pipe, w, h, self.msaa);
    }

    /// Render the render-IR into `color_view` (one submit): shadow → lit HDR → bloom → composite → FXAA.
    pub fn draw(&self, color_view: &wgpu::TextureView, ir: &RenderIr) {
        let (w, h) = (self.w, self.h);
        let aspect = w as f32 / h.max(1) as f32;

        // camera: IR camera if present, else globals eye/target, else framed on centroid
        let centroid = ir.instances.iter().fold([0.0f32, 0.0], |a, i| [a[0] + i.pos[0], a[1] + i.pos[2]]);
        let cn = ir.instances.len().max(1) as f32;
        let (cx, cz) = (centroid[0] / cn, centroid[1] / cn);
        let (eye, target, fov, near, far) = match &ir.camera {
            Some(c) => (c.eye, c.target, c.fov_y, c.near, c.far),
            None => (
                ir.globals.eye.unwrap_or([cx + 60.0, 80.0, cz + 60.0]),
                ir.globals.target.unwrap_or([cx, 0.0, cz]),
                60f32.to_radians(), 0.5, 4000.0,
            ),
        };
        let vp = Mat4::perspective_rh(fov, aspect, near, far) * Mat4::look_at_rh(Vec3::from(eye), Vec3::from(target), Vec3::Y);

        // shadow assignment: 2D atlas (directional ortho / spot perspective) + optional point faces
        let (shadow_vp, layer_of, n_shadows, point) = assign_shadows(ir, Vec3::new(cx, 0.0, cz));
        // point shadow: the 6 face view-projections (identity when no point caster)
        let point_vp = match point {
            Some(p) => cube_face_views(Vec3::from(ir.lights[p].pos), ir.lights[p].range),
            None => [Mat4::IDENTITY; 6],
        };

        // upload uniforms (sky.w flags whether a real env map is bound → image-based IBL)
        let mut gir = pack_gir(ir, eye, &vp, &shadow_vp, &point_vp, &layer_of);
        gir[VEC4_BASE + 15] = if self.has_env { 1.0 } else { 0.0 }; // sky.w
        self.queue.write_buffer(&self.gir, 0, bytemuck::cast_slice(&gir));
        for (k, m) in shadow_vp.iter().enumerate().take(n_shadows) {
            self.queue.write_buffer(&self.shadow_ubufs[k], 0, bytemuck::cast_slice(&m.to_cols_array()));
        }
        // point shadow: write each face's view-projection + the light pos/range
        if let Some(p) = point {
            let l = &ir.lights[p];
            for (k, fvp) in point_vp.iter().enumerate() {
                let mut buf = [0f32; 20]; // mat4 (16) + light vec4 (xyz pos, range)
                buf[0..16].copy_from_slice(&fvp.to_cols_array());
                buf[16..20].copy_from_slice(&[l.pos[0], l.pos[1], l.pos[2], l.range]);
                self.queue.write_buffer(&self.point_ubufs[k], 0, bytemuck::cast_slice(&buf));
            }
        }

        // instances: opaque first, then alpha-blended sorted back-to-front from the eye.
        // color.w carries opacity (1.0 opaque). The opaque + shadow passes draw [0..n_op);
        // the blend pass draws [n_op..n_inst).
        let n_inst = ir.instances.len().min(MAX_INST as usize);
        let (order, n_op) = instance_draw_order(&ir.instances[..n_inst], eye);
        let mut idata: Vec<f32> = Vec::with_capacity(n_inst * 24);
        for &idx in &order {
            let i = &ir.instances[idx];
            idata.extend_from_slice(&model_mat(i).to_cols_array());
            idata.extend_from_slice(&[i.color[0], i.color[1], i.color[2], i.alpha.clamp(0.0, 1.0)]);
            idata.extend_from_slice(&[i.metallic, i.roughness, i.emissive, 0.0]);
        }
        if !idata.is_empty() { self.queue.write_buffer(&self.inst, 0, bytemuck::cast_slice(&idata)); }

        let mut enc = self.device.create_command_encoder(&Default::default());
        // draw instances [first..last) with `pipe` (range lets us split opaque vs blended)
        let geom = |rp: &mut wgpu::RenderPass, pipe: &wgpu::RenderPipeline, bnd: &wgpu::BindGroup, range: std::ops::Range<u32>| {
            if range.start < range.end {
                rp.set_pipeline(pipe);
                rp.set_bind_group(0, bnd, &[]);
                rp.set_vertex_buffer(0, self.vbuf.slice(..));
                rp.set_vertex_buffer(1, self.inst.slice(..));
                rp.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint16);
                rp.draw_indexed(0..self.idx_count, 0, range);
            }
        };
        let full = |rp: &mut wgpu::RenderPass, pipe: &wgpu::RenderPipeline, b0: &wgpu::BindGroup, b1: Option<&wgpu::BindGroup>| {
            rp.set_pipeline(pipe);
            rp.set_bind_group(0, b0, &[]);
            if let Some(b1) = b1 { rp.set_bind_group(1, b1, &[]); }
            rp.draw(0..3, 0..1);
        };

        // PASS 0 — point-light shadow cube: render distance into each of the 6 faces (if any)
        if point.is_some() {
            for k in 0..6 {
                let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("point-shadow"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.point_face_views[k], resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.point_depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None, occlusion_query_set: None,
                });
                geom(&mut sp, &self.point_pipe, &self.point_binds[k], 0..n_op);
            }
        }
        // PASS 1 — shadow atlas: clear every layer; render instances into each active caster
        for k in 0..MAX_SHADOWS {
            let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow-ir"), color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_layer_views[k],
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            if k < n_shadows {
                geom(&mut sp, &self.shadow_pipe, &self.shadow_binds[k], 0..n_op);
            }
        }
        // PASS 2 — lit HDR (+ optional MSAA resolve into scene_view)
        {
            let (view, resolve) = match &self.t.scene_ms_view {
                Some(ms) => (ms, Some(&self.t.scene_view)),
                None => (&self.t.scene_view, None),
            };
            let bg = ir.globals.horizon;
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lit-ir"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view, resolve_target: resolve,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: bg[0] as f64, g: bg[1] as f64, b: bg[2] as f64, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.t.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            geom(&mut rp, &self.lit_pipe, &self.lit_bind, 0..n_op);          // opaque
            geom(&mut rp, &self.lit_blend_pipe, &self.lit_bind, n_op..n_inst as u32); // alpha-blended, back-to-front
        }
        // PASS 3 — bloom bright-pass (scene → bloom_a)
        post_pass(&mut enc, "bright", &self.t.bloom_a_view, |rp| full(rp, &self.bright_pipe, &self.t.bright_bind, None));
        // PASS 4 — blur H (bloom_a → bloom_b)
        post_pass(&mut enc, "blur-h", &self.t.bloom_b_view, |rp| full(rp, &self.blur_pipe, &self.t.blur_h_bind, None));
        // PASS 5 — blur V (bloom_b → bloom_a)
        post_pass(&mut enc, "blur-v", &self.t.bloom_a_view, |rp| full(rp, &self.blur_pipe, &self.t.blur_v_bind, None));
        // PASS 6 — composite scene + bloom → LDR
        post_pass(&mut enc, "composite", &self.t.ldr_view, |rp| full(rp, &self.composite_pipe, &self.t.comp_bind0, Some(&self.t.comp_bind1)));
        // PASS 7 — FXAA → final color target
        post_pass(&mut enc, "fxaa", color_view, |rp| full(rp, &self.fxaa_pipe, &self.t.fxaa_bind, None));

        self.queue.submit([enc.finish()]);
    }
}

/// Build the lit-pass bind group (shadow atlas + point faces + env map). Rebuilt by
/// `set_env_map` when the environment texture changes.
#[allow(clippy::too_many_arguments)]
fn make_lit_bind(
    device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, gir: &wgpu::Buffer,
    shadow_array_view: &wgpu::TextureView, shadow_samp: &wgpu::Sampler,
    point_cube_view: &wgpu::TextureView, point_samp: &wgpu::Sampler,
    env_view: &wgpu::TextureView, env_samp: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lit-bind"), layout: bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: gir.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(shadow_array_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(shadow_samp) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(point_cube_view) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(point_samp) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(env_view) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(env_samp) },
        ],
    })
}

/// Begin a fullscreen color pass over `view`, run `record`, and end it.
fn post_pass(enc: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, record: impl FnOnce(&mut wgpu::RenderPass)) {
    let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view, resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
    });
    record(&mut rp);
}

/// Build the size-dependent textures, post bind groups, and param buffers for `w`×`h`.
#[allow(clippy::too_many_arguments)]
fn build_targets(
    device: &wgpu::Device, queue: &wgpu::Queue, sampler: &wgpu::Sampler,
    bright_pipe: &wgpu::RenderPipeline, blur_pipe: &wgpu::RenderPipeline,
    composite_pipe: &wgpu::RenderPipeline, fxaa_pipe: &wgpu::RenderPipeline,
    w: u32, h: u32, msaa: u32,
) -> Targets {
    let (w, h) = (w.max(1), h.max(1));
    let (bw, bh) = ((w / 2).max(1), (h / 2).max(1));

    let depth_view = tex2d(device, w, h, wgpu::TextureFormat::Depth24Plus, msaa, wgpu::TextureUsages::empty()).create_view(&Default::default());
    let scene_view = tex2d(device, w, h, HDR_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC).create_view(&Default::default());
    let scene_ms_view = if msaa > 1 {
        Some(tex2d(device, w, h, HDR_FORMAT, msaa, wgpu::TextureUsages::empty()).create_view(&Default::default()))
    } else { None };
    let bloom_a_view = tex2d(device, bw, bh, HDR_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
    let bloom_b_view = tex2d(device, bw, bh, HDR_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
    let ldr_view = tex2d(device, w, h, LDR_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());

    // params: bright(thr,knee) ; blur(dir.xy, texel.xy) ; composite(exposure,bloom,vignette,gamma) ; fxaa(texel)
    let u = wgpu::BufferUsages::UNIFORM;
    let p_bright = make_buf(device, queue, bytemuck::cast_slice(&[1.3f32, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]), u);
    let p_blur_h = make_buf(device, queue, bytemuck::cast_slice(&[1.0f32, 0.0, 1.0 / bw as f32, 1.0 / bh as f32, 0.0, 0.0, 0.0, 0.0]), u);
    let p_blur_v = make_buf(device, queue, bytemuck::cast_slice(&[0.0f32, 1.0, 1.0 / bw as f32, 1.0 / bh as f32, 0.0, 0.0, 0.0, 0.0]), u);
    // composite: exposure, bloom_strength, vignette, gamma
    let p_comp = make_buf(device, queue, bytemuck::cast_slice(&[0.95f32, 0.45, 0.35, 2.2, 0.0, 0.0, 0.0, 0.0]), u);
    let p_fxaa = make_buf(device, queue, bytemuck::cast_slice(&[1.0 / w as f32, 1.0 / h as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]), u);

    let tex_bind = |layout: wgpu::BindGroupLayout, view: &wgpu::TextureView, p: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: p.as_entire_binding() },
            ],
        })
    };
    let bright_bind = tex_bind(bright_pipe.get_bind_group_layout(0), &scene_view, &p_bright);
    let blur_h_bind = tex_bind(blur_pipe.get_bind_group_layout(0), &bloom_a_view, &p_blur_h); // A → B
    let blur_v_bind = tex_bind(blur_pipe.get_bind_group_layout(0), &bloom_b_view, &p_blur_v); // B → A
    let comp_bind0 = tex_bind(composite_pipe.get_bind_group_layout(0), &scene_view, &p_comp);
    let comp_bind1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None, layout: &composite_pipe.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&bloom_a_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    });
    let fxaa_bind = tex_bind(fxaa_pipe.get_bind_group_layout(0), &ldr_view, &p_fxaa);

    Targets {
        depth_view, scene_ms_view, scene_view, bloom_a_view, bloom_b_view, ldr_view,
        bright_bind, blur_h_bind, blur_v_bind, comp_bind0, comp_bind1, fxaa_bind,
        _params: [p_bright, p_blur_h, p_blur_v, p_comp, p_fxaa],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = "{:lights [{:kind :directional :color [1 0.9 0.8] :intensity 1.2 :dir [-0.4 -0.8 -0.3] :cast-shadow true}
              {:kind :point :color [1 0.5 0.2] :intensity 3.0 :pos [2 3 0] :range 12.0}
              {:kind :spot :color [0.6 0.8 1] :pos [0 5 0] :dir [0 -1 0] :range 20.0 :inner 0.3 :outer 0.6}]
     :env {:ambient [0.2 0.2 0.25] :ground [0.1 0.1 0.1] :ibl {:intensity 0.8}}
     :instances [{:pos [0 0 0] :color [1 1 1] :size [1 1]}]}";

    const LB: usize = LIGHTS_BASE;     // start of the lights array (floats)
    const SB: usize = VEC4_BASE;       // start of the 5-vec4 block (eye/amb/...)

    #[test]
    fn pack_gir_lays_out_lights_and_env() {
        let ir = parse_render_ir(SCENE);
        let (svp, layer_of, _, _) = assign_shadows(&ir, Vec3::ZERO);
        let g = pack_gir(&ir, [5.0, 3.0, 8.0], &Mat4::IDENTITY, &svp, &[Mat4::IDENTITY; 6], &layer_of);

        assert_eq!(g[SB + 3], 3.0, "n_lights packed into eye.w");
        assert!((g[SB + 7] - 0.8).abs() < 1e-6, "ibl intensity into amb.w");

        // light kinds encoded in dir.w of each 16-float block (+11)
        assert_eq!(g[LB + 11], 0.0, "light0 directional");
        assert_eq!(g[LB + 16 + 11], 1.0, "light1 point");
        assert_eq!(g[LB + 32 + 11], 2.0, "light2 spot");

        // point light range packed into pos.w (+7)
        assert!((g[LB + 16 + 7] - 12.0).abs() < 1e-6, "point range");
        // spot cone stored as cosines: inner 0.3rad, outer 0.6rad
        assert!((g[LB + 32 + 12] - 0.3f32.cos()).abs() < 1e-6, "spot cos_inner");
        assert!((g[LB + 32 + 13] - 0.6f32.cos()).abs() < 1e-6, "spot cos_outer");
        // only the directional light casts here → its shadow layer (spot.w, +15) is 0, others -1
        assert_eq!(g[LB + 15], 0.0, "light0 shadow layer 0");
        assert_eq!(g[LB + 16 + 15], -1.0, "light1 (point) no shadow");
        assert_eq!(g[LB + 32 + 15], -1.0, "light2 (spot, no cast-shadow) none");
    }

    #[test]
    fn pack_gir_clamps_to_max_lights() {
        let many: String = (0..10).map(|_| "{:kind :point :pos [0 1 0] :range 5.0}".to_string()).collect::<Vec<_>>().join(" ");
        let ir = parse_render_ir(&format!("{{:lights [{many}] :instances []}}"));
        let (svp, layer_of, _, _) = assign_shadows(&ir, Vec3::ZERO);
        let g = pack_gir(&ir, [0.0, 0.0, 0.0], &Mat4::IDENTITY, &svp, &[Mat4::IDENTITY; 6], &layer_of);
        assert_eq!(g[SB + 3], MAX_LIGHTS as f32, "clamped to MAX_LIGHTS");
    }

    #[test]
    fn assign_shadows_layers_directional_spot_and_point_cube() {
        let ir = parse_render_ir(
            "{:lights [{:kind :directional :dir [-0.4 -0.8 -0.3] :cast-shadow true}
                       {:kind :point :pos [2 3 0] :range 12.0 :cast-shadow true}
                       {:kind :spot :pos [0 5 0] :dir [0 -1 0] :range 20.0 :outer 0.6 :cast-shadow true}]
             :instances []}");
        let (mats, layer_of, n, point) = assign_shadows(&ir, Vec3::ZERO);
        assert_eq!(n, 2, "directional + spot use the 2D atlas");
        assert_eq!(layer_of[0], 0, "directional → atlas layer 0");
        assert_eq!(layer_of[1], POINT_LAYER, "point → cube (POINT_LAYER)");
        assert_eq!(layer_of[2], 1, "spot → atlas layer 1");
        assert_eq!(point, Some(1), "point caster is light 1");
        assert_ne!(mats[0], Mat4::IDENTITY, "directional matrix built");
        assert_ne!(mats[1], Mat4::IDENTITY, "spot matrix built");
    }

    #[test]
    fn assign_shadows_caps_at_max_shadows() {
        let many: String = (0..(MAX_SHADOWS + 3)).map(|_| "{:kind :directional :dir [0 -1 0] :cast-shadow true}".to_string()).collect::<Vec<_>>().join(" ");
        let ir = parse_render_ir(&format!("{{:lights [{many}] :instances []}}"));
        let (_, _, n, _) = assign_shadows(&ir, Vec3::ZERO);
        assert_eq!(n, MAX_SHADOWS, "active layers capped at MAX_SHADOWS");
    }

    #[test]
    fn draw_order_opaque_first_then_back_to_front() {
        let ir = parse_render_ir(
            "{:instances [{:pos [0 0 -2] :alpha 0.5}
                          {:pos [0 0 0]  :alpha 1.0}
                          {:pos [0 0 -8] :alpha 0.5}
                          {:pos [0 0 -5] :alpha 0.3}]}");
        // eye at +Z looking toward -Z
        let (order, n_op) = instance_draw_order(&ir.instances, [0.0, 0.0, 10.0]);
        assert_eq!(n_op, 1, "one opaque instance");
        assert_eq!(order[0], 1, "opaque instance drawn first");
        // transparent tail sorted farthest→nearest: idx2 (-8), idx3 (-5), idx0 (-2)
        assert_eq!(&order[1..], &[2, 3, 0], "transparent back-to-front");
    }

    #[test]
    fn cube_face_views_are_six_distinct() {
        let f = cube_face_views(Vec3::new(1.0, 2.0, 3.0), 20.0);
        assert_eq!(f.len(), 6);
        for i in 0..6 {
            for j in (i + 1)..6 {
                assert_ne!(f[i], f[j], "face {i} and {j} differ");
            }
        }
    }

    #[test]
    fn only_first_point_light_gets_the_cube() {
        let ir = parse_render_ir(
            "{:lights [{:kind :point :pos [0 1 0] :range 8 :cast-shadow true}
                       {:kind :point :pos [4 1 0] :range 8 :cast-shadow true}]
             :instances []}");
        let (_, layer_of, _, point) = assign_shadows(&ir, Vec3::ZERO);
        assert_eq!(point, Some(0));
        assert_eq!(layer_of[0], POINT_LAYER, "first point → cube");
        assert_eq!(layer_of[1], -1, "second point → no shadow (one cube slot)");
    }

    #[test]
    fn f32_to_f16_known_values() {
        assert_eq!(f32_to_f16(0.0), 0x0000);
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(2.0), 0x4000);
        assert_eq!(f32_to_f16(-1.0), 0xbc00);
        assert_eq!(f32_to_f16(1e30) & 0x7c00, 0x7c00, "overflow → inf");
    }

    #[test]
    fn build_env_mips_full_chain_to_1x1() {
        // 4×2 equirect → mips 4×2, 2×1, 1×1 (3 levels)
        let px = vec![1.0f32; 4 * 2 * 4];
        let mips = build_env_mips(4, 2, &px);
        let dims: Vec<_> = mips.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(dims, vec![(4, 2), (2, 1), (1, 1)]);
        // box average of all-ones stays 1.0 → f16 1.0 = 0x3c00
        assert!(mips.last().unwrap().2.iter().all(|&b| b == 0x3c00));
    }
}

/// Headless: render the EDN render-IR and read back RGBA8 pixels (w*h*4, top row first).
pub fn render_ir_to_pixels(ir_edn: &str, w: u32, h: u32, msaa: u32) -> Vec<u8> {
    let ir = parse_render_ir(ir_edn);
    pollster::block_on(render_ir_async(&ir, w, h, msaa, None))
}

/// Like [`render_ir_to_pixels`] but with an equirectangular HDR env map for image-based IBL
/// (`env` = `(width, height, rgba_f32_pixels)` — the host-decoded `.hdr`).
pub fn render_ir_to_pixels_env(ir_edn: &str, w: u32, h: u32, msaa: u32, env: (u32, u32, Vec<f32>)) -> Vec<u8> {
    let ir = parse_render_ir(ir_edn);
    pollster::block_on(render_ir_async(&ir, w, h, msaa, Some(env)))
}

async fn render_ir_async(ir: &RenderIr, w: u32, h: u32, msaa: u32, env: Option<(u32, u32, Vec<f32>)>) -> Vec<u8> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no GPU adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.expect("no device");
    let fmt = wgpu::TextureFormat::Rgba8Unorm;
    let mut r = IrRenderer::new(device, queue, fmt, w, h, msaa);
    if let Some((ew, eh, px)) = env { r.set_env_map(ew, eh, &px); }
    let color = tex2d(r.device(), w, h, fmt, 1, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC);
    let color_view = color.create_view(&Default::default());
    r.draw(&color_view, ir);

    let bpr = align256(w * 4);
    let rb = r.device().create_buffer(&wgpu::BufferDescriptor {
        label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
    });
    let mut enc = r.device().create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &rb, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    r.queue().submit([enc.finish()]);
    let slice = rb.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    r.device().poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * bpr) as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    out
}
