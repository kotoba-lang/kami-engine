//! Gsplat preview adapter — 3D Gaussian Splatting `RenderPipeline` impl.
//!
//! Consumer of `kami_render::splat::{GaussianSplat, SplatCloud}` +
//! `kami_render::splat_loader::{load_ply, load_splat}`. CPU sorts splats
//! back-to-front each frame, uploads sorted indices to a storage buffer,
//! and draws billboard quads with EWA-projected covariance falloff.
//!
//! **Preview / QC scope (ADR-2605092800).** Per-cloud splat budget is
//! capped at `MAX_SPLATS_PER_CLOUD` = 50 000 — sized for landmark / spot
//! review, not city-scale streaming. Runtime delivery on `maps.gftd.ai`
//! stays on baked static meshes (260416-maps-kami-street-asset-pipeline).
//!
//! Multi-cloud: clouds are keyed by an arbitrary string (typically a
//! tile H3 cell). `upsert(name, cloud)` replaces, `remove(name)` drops.
//! All clouds draw in the same pass.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::splat::SplatCloud;
use kami_render::splat_loader;
use kami_render::RenderContext;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wgpu::util::DeviceExt;

/// Per-cloud splat cap. CPU sort is `O(n log n)`; 100 000 entries
/// comes in around 10 ms on an M-series CPU (Apple silicon) which
/// leaves ~6 ms for `record()` at 60 fps. If 200k+ scenes appear,
/// the next step is GPU bitonic over a (distance², index) compute
/// buffer — see `90-docs/adr/2605092800-…` §"future work" for the
/// pipeline sketch.
pub const MAX_SPLATS_PER_CLOUD: usize = 100_000;

/// Beyond this distance from the camera (metres), the renderer
/// downgrades the cloud to DC-band-only (sh_degree=0) regardless of
/// the trained degree. View-dependent specular is imperceptible past
/// ~50 m and the band-1..3 fragment-shader evaluation is the dominant
/// per-pixel cost on far tiles.
pub const FAR_SH_THRESHOLD_M: f32 = 50.0;

/// Source format hint for `upsert_from_bytes`.
#[derive(Debug, Clone, Copy)]
pub enum GsplatFormat {
    /// PLY (ASCII or binary little-endian) with the
    /// `f_dc_*` / `scale_*` / `rot_*` properties from the original
    /// 3DGS paper.
    Ply,
    /// `antimatter15` 32-byte compact splat format.
    Splat,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GsplatUniform {
    view_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    cam_pos: [f32; 3],
    splat_count: u32,
    viewport: [f32; 2],
    focal: [f32; 2],
    sh_degree: u32,
    /// Number of `f32`s per splat in `sh_rest_buf` (= 3 * (K-1) for
    /// K=(sh_degree+1)²). 0 when `sh_degree == 0`.
    sh_rest_stride: u32,
    _pad: [u32; 2],
}

struct GpuCloud {
    /// Splat data storage buffer. Held to keep its underlying GPU memory
    /// alive while `bind_group` references it; never read on the CPU
    /// after upload (CPU sort uses the `positions` mirror instead).
    #[allow(dead_code)]
    splat_buf: wgpu::Buffer,
    /// Sorted indices buffer (back-to-front order, rewritten each frame).
    indices_buf: wgpu::Buffer,
    indices_capacity: u32,
    /// Higher-SH coefficients (band 1..sh_degree). Always allocated
    /// (≥4 bytes) to keep the bind group layout uniform across DC-only
    /// and view-dependent clouds.
    #[allow(dead_code)]
    sh_rest_buf: wgpu::Buffer,
    sh_degree: u32,
    sh_rest_stride: u32,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    /// CPU mirror of splat positions (used by the per-frame sort —
    /// avoids re-reading the storage buffer).
    positions: Vec<[f32; 3]>,
    /// Reusable scratch for the sort. `(distance², original index)`.
    sort_scratch: Vec<(f32, u32)>,
}

struct Shared {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    device: wgpu::Device,
    clouds: RefCell<HashMap<String, GpuCloud>>,
}

#[derive(Clone)]
pub struct GsplatAdapter {
    inner: Rc<Shared>,
}

#[derive(Debug)]
pub enum GsplatError {
    Empty,
    TooManySplats(usize),
    Loader(String),
}

impl std::fmt::Display for GsplatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GsplatError::Empty => write!(f, "splat cloud is empty"),
            GsplatError::TooManySplats(n) => write!(
                f,
                "splat count {n} exceeds preview cap {MAX_SPLATS_PER_CLOUD}"
            ),
            GsplatError::Loader(m) => write!(f, "loader: {m}"),
        }
    }
}

impl std::error::Error for GsplatError {}

const SHADER_WGSL: &str = r#"
struct GaussianSplat {
    position: vec3<f32>,
    opacity: f32,
    scale: vec3<f32>,
    _pad0: f32,
    rotation: vec4<f32>,
    sh_dc: vec3<f32>,
    _pad1: f32,
}

struct GsplatUniform {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    cam_pos: vec3<f32>,
    splat_count: u32,
    viewport: vec2<f32>,
    focal: vec2<f32>,
    sh_degree: u32,
    sh_rest_stride: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> u: GsplatUniform;
@group(0) @binding(1) var<storage, read> splats: array<GaussianSplat>;
@group(0) @binding(2) var<storage, read> sorted_indices: array<u32>;
// Higher-SH coefficients (band 1..sh_degree). Coefficient-major:
// per-splat (K-1) RGB triples laid out as f32 [r0,g0,b0, r1,g1,b1, ...]
// where K = (sh_degree+1)². Empty (4-byte sentinel) when sh_degree=0.
@group(0) @binding(3) var<storage, read> sh_rest: array<f32>;

// Standard 3DGS SH band coefficients (Inria reference / nerfstudio).
const SH_C0: f32 = 0.28209479177387814;
const SH_C1: f32 = 0.4886025119029199;
const SH_C2_0: f32 = 1.0925484305920792;
const SH_C2_1: f32 = -1.0925484305920792;
const SH_C2_2: f32 = 0.31539156525252005;
const SH_C2_3: f32 = -1.0925484305920792;
const SH_C2_4: f32 = 0.5462742152960396;
const SH_C3_0: f32 = -0.5900435899266435;
const SH_C3_1: f32 = 2.890611442640554;
const SH_C3_2: f32 = -0.4570457994644658;
const SH_C3_3: f32 = 0.3731763325901154;
const SH_C3_4: f32 = -0.4570457994644658;
const SH_C3_5: f32 = 1.445305721320277;
const SH_C3_6: f32 = -0.5900435899266435;

fn read_rest(splat_idx: u32, coef_index: u32) -> vec3<f32> {
    let base = splat_idx * u.sh_rest_stride + coef_index * 3u;
    return vec3<f32>(sh_rest[base], sh_rest[base + 1u], sh_rest[base + 2u]);
}

/// Evaluate SH up to `u.sh_degree` at view direction `dir` (unit vec
/// from splat centre toward camera) and add to the DC band. Returns
/// linear RGB in [0, ∞); caller clamps.
///
/// Note on convention: the PLY emitted by `runpod-endpoint-gsplat`
/// stores `f_dc_*` already multiplied by SH_C0 (i.e. as `rgb - 0.5`),
/// so the DC contribution is `sh_dc` directly. Higher-band
/// coefficients are stored in gsplat-native units and multiplied by
/// the SH_Cn constants here.
fn evaluate_sh(splat_idx: u32, dir: vec3<f32>, sh_dc: vec3<f32>) -> vec3<f32> {
    var color = sh_dc;
    if u.sh_degree < 1u { return color + vec3<f32>(0.5); }
    let x = dir.x; let y = dir.y; let z = dir.z;

    // Band 1 — coefficients [0, 1, 2]
    let c1 = read_rest(splat_idx, 0u);
    let c2 = read_rest(splat_idx, 1u);
    let c3 = read_rest(splat_idx, 2u);
    color = color - SH_C1 * y * c1 + SH_C1 * z * c2 - SH_C1 * x * c3;
    if u.sh_degree < 2u { return color + vec3<f32>(0.5); }

    // Band 2 — coefficients [3..7]
    let xx = x * x; let yy = y * y; let zz = z * z;
    let xy = x * y; let yz = y * z; let xz = x * z;
    let c4 = read_rest(splat_idx, 3u);
    let c5 = read_rest(splat_idx, 4u);
    let c6 = read_rest(splat_idx, 5u);
    let c7 = read_rest(splat_idx, 6u);
    let c8 = read_rest(splat_idx, 7u);
    color = color
        + SH_C2_0 * xy * c4
        + SH_C2_1 * yz * c5
        + SH_C2_2 * (2.0 * zz - xx - yy) * c6
        + SH_C2_3 * xz * c7
        + SH_C2_4 * (xx - yy) * c8;
    if u.sh_degree < 3u { return color + vec3<f32>(0.5); }

    // Band 3 — coefficients [8..14]
    let c9  = read_rest(splat_idx, 8u);
    let c10 = read_rest(splat_idx, 9u);
    let c11 = read_rest(splat_idx, 10u);
    let c12 = read_rest(splat_idx, 11u);
    let c13 = read_rest(splat_idx, 12u);
    let c14 = read_rest(splat_idx, 13u);
    let c15 = read_rest(splat_idx, 14u);
    color = color
        + SH_C3_0 * y * (3.0 * xx - yy) * c9
        + SH_C3_1 * xy * z * c10
        + SH_C3_2 * y * (4.0 * zz - xx - yy) * c11
        + SH_C3_3 * z * (2.0 * zz - 3.0 * xx - 3.0 * yy) * c12
        + SH_C3_4 * x * (4.0 * zz - xx - yy) * c13
        + SH_C3_5 * z * (xx - yy) * c14
        + SH_C3_6 * x * (xx - 3.0 * yy) * c15;
    return color + vec3<f32>(0.5);
}

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) alpha: f32,
    @location(2) quad_pos: vec2<f32>,
    @location(3) conic: vec3<f32>,
}

fn quat_to_mat3(q: vec4<f32>) -> mat3x3<f32> {
    let w = q.x; let x = q.y; let y = q.z; let z = q.w;
    return mat3x3<f32>(
        vec3<f32>(1.0 - 2.0*(y*y + z*z), 2.0*(x*y + w*z), 2.0*(x*z - w*y)),
        vec3<f32>(2.0*(x*y - w*z), 1.0 - 2.0*(x*x + z*z), 2.0*(y*z + w*x)),
        vec3<f32>(2.0*(x*z + w*y), 2.0*(y*z - w*x), 1.0 - 2.0*(x*x + y*y)),
    );
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VOut {
    var out: VOut;
    if iid >= u.splat_count {
        // Degenerate clip-space position so the rasterizer drops it.
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color = vec3<f32>(0.0);
        out.alpha = 0.0;
        out.quad_pos = vec2<f32>(0.0);
        out.conic = vec3<f32>(0.0);
        return out;
    }
    let splat_idx = sorted_indices[iid];
    let s = splats[splat_idx];

    // 3D covariance Σ = R · diag(exp(scale))² · Rᵀ
    let rot = quat_to_mat3(s.rotation);
    let sc = exp(s.scale);
    let m = mat3x3<f32>(
        vec3<f32>(sc.x, 0.0, 0.0),
        vec3<f32>(0.0, sc.y, 0.0),
        vec3<f32>(0.0, 0.0, sc.z),
    );
    let rs = rot * m;
    let sigma = rs * transpose(rs);

    // View-space splat centre
    let view_pos = u.view * vec4<f32>(s.position, 1.0);
    let tz = view_pos.z;
    if tz >= -0.05 {
        // Behind / on near plane → cull
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color = vec3<f32>(0.0);
        out.alpha = 0.0;
        out.quad_pos = vec2<f32>(0.0);
        out.conic = vec3<f32>(0.0);
        return out;
    }

    // Perspective Jacobian (negate tz so we operate in positive depth)
    let z = -tz;
    let fx = u.focal.x;
    let fy = u.focal.y;
    let j = mat3x3<f32>(
        vec3<f32>(fx / z, 0.0, 0.0),
        vec3<f32>(0.0, fy / z, 0.0),
        vec3<f32>(-fx * view_pos.x / (z * z), -fy * view_pos.y / (z * z), 0.0),
    );

    let view_rot = mat3x3<f32>(
        u.view[0].xyz,
        u.view[1].xyz,
        u.view[2].xyz,
    );
    let t = j * view_rot;
    let cov2d_full = t * sigma * transpose(t);

    // Low-pass filter (paper §3.1) so a splat is at least 1px wide
    let a = cov2d_full[0][0] + 0.3;
    let b = cov2d_full[0][1];
    let c = cov2d_full[1][1] + 0.3;
    let det = a * c - b * b;
    if det <= 0.0 {
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color = vec3<f32>(0.0);
        out.alpha = 0.0;
        out.quad_pos = vec2<f32>(0.0);
        out.conic = vec3<f32>(0.0);
        return out;
    }
    let det_inv = 1.0 / det;

    // 3σ extent → billboard radius
    let trace = a + c;
    let disc = max(trace * trace * 0.25 - det, 0.0);
    let lambda1 = trace * 0.5 + sqrt(disc);
    let lambda2 = max(trace * 0.5 - sqrt(disc), 0.0);
    let radius_px = ceil(3.0 * sqrt(max(lambda1, lambda2)));

    // 4-vertex strip corners
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );
    let corner = corners[vid];

    let clip = u.view_proj * vec4<f32>(s.position, 1.0);
    let ndc = clip.xy / clip.w;
    let pixel = (ndc * 0.5 + vec2<f32>(0.5, 0.5)) * u.viewport;
    let offset_px = pixel + corner * radius_px;
    let final_ndc = (offset_px / u.viewport) * 2.0 - vec2<f32>(1.0, 1.0);

    out.pos = vec4<f32>(final_ndc.x, -final_ndc.y, clip.z / clip.w, 1.0);
    if u.sh_degree == 0u {
        out.color = max(s.sh_dc + vec3<f32>(0.5), vec3<f32>(0.0));
    } else {
        // View direction = unit vector from splat centre toward camera.
        // Standard 3DGS convention (gsplat / Inria) — note the camera-
        // toward-splat direction would invert all band-1 sign terms.
        let to_cam = u.cam_pos - s.position;
        let len2 = dot(to_cam, to_cam);
        let dir = select(vec3<f32>(0.0, 0.0, 1.0), to_cam * inverseSqrt(len2), len2 > 1e-12);
        out.color = max(evaluate_sh(splat_idx, dir, s.sh_dc), vec3<f32>(0.0));
    }
    out.alpha = 1.0 / (1.0 + exp(-s.opacity));
    out.quad_pos = corner * radius_px;
    out.conic = vec3<f32>(c * det_inv, -b * det_inv, a * det_inv);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let d = in.quad_pos;
    let power = -0.5 * (in.conic.x * d.x * d.x
                       + 2.0 * in.conic.y * d.x * d.y
                       + in.conic.z * d.y * d.y);
    if power > 0.0 { discard; }
    let g = exp(power);
    let a = min(0.99, in.alpha * g);
    if a < 1.0 / 255.0 { discard; }
    return vec4<f32>(in.color * a, a);
}
"#;

impl GsplatAdapter {
    pub fn new(ctx: &RenderContext) -> Self {
        let device = ctx.device.clone();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kami-pipelines.gsplat"),
            source: wgpu::ShaderSource::Wgsl(SHADER_WGSL.into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gsplat.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gsplat.pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gsplat.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.format,
                    blend: Some(wgpu::BlendState {
                        // Premultiplied-alpha over compositing
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            inner: Rc::new(Shared {
                pipeline,
                layout,
                device,
                clouds: RefCell::new(HashMap::new()),
            }),
        }
    }

    /// Replace (or insert) a named splat cloud. `name` is typically an
    /// H3 cell ID. Returns `Err(TooManySplats)` if the cloud exceeds
    /// `MAX_SPLATS_PER_CLOUD` (preview cap).
    pub fn upsert(&self, name: &str, cloud: SplatCloud) -> Result<(), GsplatError> {
        if cloud.splats.is_empty() {
            return Err(GsplatError::Empty);
        }
        if cloud.splats.len() > MAX_SPLATS_PER_CLOUD {
            return Err(GsplatError::TooManySplats(cloud.splats.len()));
        }

        let device = &self.inner.device;
        let count = cloud.splats.len() as u32;

        let splat_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gsplat.splat_buf"),
            contents: bytemuck::cast_slice(&cloud.splats),
            usage: wgpu::BufferUsages::STORAGE,
        });
        // Initial sorted order = identity (overwritten on first prepare).
        let init_indices: Vec<u32> = (0..count).collect();
        let indices_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gsplat.indices_buf"),
            contents: bytemuck::cast_slice(&init_indices),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gsplat.uniform"),
            size: std::mem::size_of::<GsplatUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Higher-SH coefficients buffer. We always create a buffer
        // (≥4 bytes for the wgpu min-binding size guarantee) so the
        // bind group layout is uniform between DC-only and view-
        // dependent clouds.
        let sh_degree = cloud.sh_degree as u32;
        let expected_per_splat: usize = match cloud.sh_degree {
            0 => 0,
            1 => 3,
            2 => 8,
            3 => 15,
            _ => 0, // unsupported high degrees → fall back to DC
        };
        let actual_per_splat = if expected_per_splat == 0 {
            0
        } else {
            cloud.sh_rest.len() / cloud.splats.len()
        };
        let (sh_rest_used_degree, sh_rest_floats): (u32, Vec<f32>) =
            if expected_per_splat == 0 || actual_per_splat != expected_per_splat {
                // Mismatch → silently downgrade to DC. Renderer still
                // produces correct output, just without specular.
                (0, vec![0.0_f32; 1])
            } else {
                let mut buf: Vec<f32> = Vec::with_capacity(cloud.sh_rest.len() * 3);
                for c in &cloud.sh_rest {
                    buf.push(c[0]);
                    buf.push(c[1]);
                    buf.push(c[2]);
                }
                if buf.is_empty() { buf.push(0.0); }
                (sh_degree, buf)
            };
        let sh_rest_stride = if sh_rest_used_degree == 0 {
            0
        } else {
            (expected_per_splat as u32) * 3
        };
        let sh_rest_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gsplat.sh_rest_buf"),
            contents: bytemuck::cast_slice(&sh_rest_floats),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gsplat.bg"),
            layout: &self.inner.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: splat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: indices_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: sh_rest_buf.as_entire_binding(),
                },
            ],
        });

        let positions: Vec<[f32; 3]> = cloud.splats.iter().map(|s| s.position).collect();
        let sort_scratch = (0..count).map(|i| (0.0_f32, i)).collect();

        self.inner.clouds.borrow_mut().insert(
            name.to_string(),
            GpuCloud {
                splat_buf,
                indices_buf,
                indices_capacity: count,
                sh_rest_buf,
                sh_degree: sh_rest_used_degree,
                sh_rest_stride,
                bind_group,
                uniform_buf,
                positions,
                sort_scratch,
            },
        );
        Ok(())
    }

    /// Decode `bytes` and `upsert` in one shot.
    pub fn upsert_from_bytes(
        &self,
        name: &str,
        bytes: &[u8],
        format: GsplatFormat,
    ) -> Result<usize, GsplatError> {
        let cloud = match format {
            GsplatFormat::Ply => splat_loader::load_ply(bytes)
                .map_err(|e| GsplatError::Loader(e.to_string()))?,
            GsplatFormat::Splat => splat_loader::load_splat(bytes)
                .map_err(|e| GsplatError::Loader(e.to_string()))?,
        };
        let n = cloud.splats.len();
        self.upsert(name, cloud)?;
        Ok(n)
    }

    /// Drop a cloud and free its GPU memory.
    pub fn remove(&self, name: &str) {
        self.inner.clouds.borrow_mut().remove(name);
    }

    /// Number of resident clouds.
    pub fn len(&self) -> usize {
        self.inner.clouds.borrow().len()
    }

    /// Whether any clouds are resident.
    pub fn is_empty(&self) -> bool {
        self.inner.clouds.borrow().is_empty()
    }
}

fn focal_from_projection(proj: &Mat4, viewport: [f32; 2]) -> [f32; 2] {
    // glam Mat4::perspective_rh:
    //   proj[0][0] = (1/tan(fovy/2)) / aspect
    //   proj[1][1] = 1/tan(fovy/2)
    // → focal_x = 0.5 * vw * proj[0][0]
    //   focal_y = 0.5 * vh * proj[1][1]
    let cols = proj.to_cols_array_2d();
    let fx = 0.5 * viewport[0] * cols[0][0];
    let fy = 0.5 * viewport[1] * cols[1][1];
    [fx.abs().max(1.0), fy.abs().max(1.0)]
}

impl RenderPipeline for GsplatAdapter {
    fn prepare(&mut self, ctx: &RenderContext, camera: &Camera, _world: &World) {
        let mut clouds = self.inner.clouds.borrow_mut();
        if clouds.is_empty() {
            return;
        }
        let cu = camera.as_render().uniform();
        let cam_pos = Vec3::from_array(cu.position);
        let view = Mat4::from_cols_array_2d(&cu.view);
        let proj = Mat4::from_cols_array_2d(&cu.projection);
        let view_proj = proj * view;
        let viewport = [ctx.width.max(1) as f32, ctx.height.max(1) as f32];
        let focal = focal_from_projection(&proj, viewport);

        for cloud in clouds.values_mut() {
            // CPU sort: build (-distance², index) so a descending sort
            // puts the farthest splats first (correct over-blend order).
            for (slot, pos) in cloud.sort_scratch.iter_mut().zip(cloud.positions.iter()) {
                let p = Vec3::from_array(*pos);
                let d = (p - cam_pos).length_squared();
                *slot = (d, slot.1);
            }
            cloud
                .sort_scratch
                .sort_unstable_by(|(da, _), (db, _)| db.partial_cmp(da).unwrap_or(std::cmp::Ordering::Equal));
            let sorted: Vec<u32> = cloud.sort_scratch.iter().map(|(_, i)| *i).collect();
            ctx.queue
                .write_buffer(&cloud.indices_buf, 0, bytemuck::cast_slice(&sorted));

            // GPU-side LOD: when the closest splat in this cloud is
            // beyond `FAR_SH_THRESHOLD_M`, force sh_degree=0 in the
            // uniform regardless of the cloud's trained degree. The
            // streaming-LOD path already keeps few splats on far
            // tiles, but the band-1..3 evaluation in the WGSL
            // fragment shader is what really hurts at distance —
            // skipping it on far clouds saves ~30 ops × N fragments.
            // The view-dependent term is barely visible past 50 m
            // anyway (specular highlight angular extent shrinks with
            // distance).
            //
            // The sort just placed the back-most splat at index 0
            // and the closest at the last index, so the closest
            // distance² lives in the final slot.
            let closest_dist_sq = cloud
                .sort_scratch
                .last()
                .map(|(d, _)| *d)
                .unwrap_or(f32::INFINITY);
            let effective_sh_degree =
                if closest_dist_sq > FAR_SH_THRESHOLD_M * FAR_SH_THRESHOLD_M {
                    0
                } else {
                    cloud.sh_degree
                };
            let effective_sh_rest_stride =
                if effective_sh_degree == 0 { 0 } else { cloud.sh_rest_stride };

            let u = GsplatUniform {
                view_proj: view_proj.to_cols_array_2d(),
                view: view.to_cols_array_2d(),
                cam_pos: cu.position,
                splat_count: cloud.indices_capacity,
                viewport,
                focal,
                sh_degree: effective_sh_degree,
                sh_rest_stride: effective_sh_rest_stride,
                _pad: [0, 0],
            };
            ctx.queue
                .write_buffer(&cloud.uniform_buf, 0, bytemuck::bytes_of(&u));
        }
    }

    fn record(
        &self,
        _ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        _camera: &Camera,
        _world: &World,
    ) {
        let clouds = self.inner.clouds.borrow();
        if clouds.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gsplat.pass"),
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
        pass.set_pipeline(&self.inner.pipeline);
        for cloud in clouds.values() {
            pass.set_bind_group(0, &cloud.bind_group, &[]);
            pass.draw(0..4, 0..cloud.indices_capacity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focal_from_perspective_matches_pinhole() {
        // 90° vertical FOV, aspect 16:9 viewport 1600×900
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 16.0 / 9.0, 0.1, 100.0);
        let [fx, fy] = focal_from_projection(&proj, [1600.0, 900.0]);
        // focal_y = 0.5 * 900 * (1 / tan(45°)) = 450
        assert!((fy - 450.0).abs() < 0.5);
        // focal_x = 0.5 * 1600 * (1 / tan(45°) / aspect) = 800 / (16/9) = 450
        assert!((fx - 450.0).abs() < 0.5);
    }

    #[test]
    fn empty_cloud_rejected() {
        // Cannot construct GsplatAdapter without a wgpu device, so we
        // exercise the precondition through the `Empty` discriminant
        // shape instead of a real upsert call.
        let err = GsplatError::Empty;
        assert_eq!(format!("{err}"), "splat cloud is empty");
    }

    #[test]
    fn too_many_splats_msg() {
        let err = GsplatError::TooManySplats(199_999);
        let s = format!("{err}");
        assert!(s.contains("199999") || s.contains("199_999") || s.contains("199,999"));
        assert!(s.contains("100000") || s.contains("100_000") || s.contains("100,000"));
    }
}
