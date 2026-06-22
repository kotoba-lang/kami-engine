//! kami-webgpu-rs — the native twin of the CLJS `kami.webgpu` executor.
//!
//! It interprets the **same EDN render-IR** (globals + instances) the web renders, but
//! drives wgpu directly instead of the browser WebGPU API (ADR-0001/0040: one EDN, two
//! executors — web = CLJS→WebGPU, native = Rust→wgpu). Rendering is headless (offscreen
//! texture + pixel readback), so it verifies by golden frame in `cargo test` — no window.
//!
//! v1 is the forward lit pass (instanced cuboids, hemisphere ambient + sun + spec + rim,
//! Reinhard tonemap), matching the web instance layout (model + colour + material = 96 B).
//! The shadow pass ports next.

use glam::{Mat4, Vec3};
use kami_scene::{mget, num, root_map, vec3};
use kotoba_edn::EdnValue;

#[derive(Clone, Debug)]
pub struct Instance {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub size: [f32; 2],
    pub yaw: f32,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: f32,
}

#[derive(Clone, Debug)]
pub struct Globals {
    pub horizon: [f32; 3],
    pub sun_dir: [f32; 3],
    pub sun: [f32; 3],
    pub eye: Option<[f32; 3]>,
    pub target: Option<[f32; 3]>,
}

impl Default for Globals {
    fn default() -> Self {
        Globals {
            horizon: [0.7, 0.8, 0.9],
            sun_dir: [-0.4, -0.85, -0.35],
            sun: [1.0, 0.96, 0.85],
            eye: None,
            target: None,
        }
    }
}

fn vec2(v: Option<&EdnValue>) -> [f32; 2] {
    let s = v.and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(1.0);
    [g(0), g(1)]
}
fn opt_vec3(v: Option<&EdnValue>) -> Option<[f32; 3]> {
    v.and_then(|x| x.as_vector()).map(|_| vec3(v))
}

/// Parse the EDN render-IR — the same data the CLJS executor consumes.
pub fn parse_ir(edn: &str) -> (Globals, Vec<Instance>) {
    let root = match root_map(edn) {
        Some(m) => m,
        None => return (Globals::default(), vec![]),
    };
    let g = mget(&root, "globals").and_then(|x| x.as_map().cloned());
    let mut globals = Globals::default();
    if let Some(g) = &g {
        if let Some(sky) = mget(g, "sky").and_then(|x| x.as_map().cloned()) {
            globals.horizon = vec3(mget(&sky, "horizon"));
            globals.sun_dir = vec3(mget(&sky, "sun-dir"));
            globals.sun = vec3(mget(&sky, "sun"));
        }
        globals.eye = opt_vec3(mget(g, "eye"));
        globals.target = opt_vec3(mget(g, "target"));
    }
    let insts = mget(&root, "instances")
        .and_then(|x| x.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|iv| iv.as_map().cloned())
        .map(|m| Instance {
            pos: vec3(mget(&m, "pos")),
            color: vec3(mget(&m, "color")),
            size: vec2(mget(&m, "size")),
            yaw: num(mget(&m, "yaw")),
            metallic: num(mget(&m, "metallic")),
            roughness: mget(&m, "roughness").map(|v| num(Some(v))).unwrap_or(0.65),
            emissive: num(mget(&m, "emissive")),
        })
        .collect();
    (globals, insts)
}

// --- cube (pos+normal), 24 verts / 36 indices — same mesh as the web ---------

fn cube() -> (Vec<f32>, Vec<u16>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
        ([1.0, 0.0, 0.0], [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]]),
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]]),
    ];
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for (n, quad) in faces.iter() {
        let base = (v.len() / 6) as u16;
        for p in quad.iter() {
            v.extend_from_slice(p);
            v.extend_from_slice(n);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (v, idx)
}

fn model_mat(i: &Instance) -> Mat4 {
    let [w, h] = i.size;
    Mat4::from_translation(Vec3::new(i.pos[0], i.pos[1] + h * 0.5, i.pos[2]))
        * Mat4::from_rotation_y(i.yaw)
        * Mat4::from_scale(Vec3::new(w, h, w))
}

// Main shader — identical WGSL to the web kami.webgpu (shadow-map PCF included).
const SHADER: &str = r#"
struct G { vp: mat4x4<f32>, sun_dir: vec4<f32>, sun_col: vec4<f32>, sky: vec4<f32>, light_vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> g: G;
@group(0) @binding(1) var shadowMap: texture_depth_2d;
@group(0) @binding(2) var shadowSamp: sampler_comparison;
fn shadow(wpos: vec3<f32>, ndl: f32) -> f32 {
  let lc = g.light_vp * vec4<f32>(wpos, 1.0);
  let ndc = lc.xyz / lc.w;
  let uv = vec2<f32>(ndc.x*0.5+0.5, 0.5-ndc.y*0.5);
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) { return 1.0; }
  let bias = max(0.0025*(1.0-ndl), 0.0006);
  let texel = 1.0/2048.0;
  var lit = 0.0;
  for (var dx = -1; dx <= 1; dx++) {
    for (var dy = -1; dy <= 1; dy++) {
      lit += textureSampleCompareLevel(shadowMap, shadowSamp, uv + vec2<f32>(f32(dx),f32(dy))*texel, ndc.z - bias);
    }
  }
  return lit/9.0;
}
struct VO { @builtin(position) clip: vec4<f32>, @location(0) n: vec3<f32>, @location(1) col: vec3<f32>, @location(2) wpos: vec3<f32>, @location(3) mat: vec3<f32> };
@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
      @location(6) color: vec4<f32>, @location(7) material: vec4<f32>) -> VO {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = model * vec4<f32>(pos, 1.0);
  var o: VO; o.clip = g.vp * world;
  o.n = normalize((model * vec4<f32>(normal, 0.0)).xyz); o.col = color.rgb; o.wpos = world.xyz;
  o.mat = material.xyz; return o;
}
@fragment
fn fs(i: VO) -> @location(0) vec4<f32> {
  let N = normalize(i.n);
  let L = normalize(-g.sun_dir.xyz);
  let eye = vec3<f32>(g.sun_dir.w, g.sun_col.w, g.sky.w);
  let V = normalize(eye - i.wpos);
  let H = normalize(L + V);
  let ndl = max(dot(N, L), 0.0);
  let metallic = clamp(i.mat.x, 0.0, 1.0);
  let rough = clamp(i.mat.y, 0.04, 1.0);
  let emissive = i.mat.z;
  let amb = mix(vec3<f32>(0.20,0.22,0.26), g.sky.rgb*0.65, N.y*0.5+0.5);
  let shininess = mix(4.0, 256.0, 1.0 - rough);
  let spec = pow(max(dot(N, H), 0.0), shininess) * mix(0.25, 0.9, metallic);
  let specTint = mix(vec3<f32>(1.0), i.col, metallic);
  let rim = pow(1.0 - max(dot(N, V), 0.0), 3.0) * 0.25;
  let sh = shadow(i.wpos, ndl);
  var c = i.col * (amb + ndl * g.sun_col.rgb * 0.9 * (1.0 - metallic*0.7) * sh)
        + specTint * g.sun_col.rgb * spec * sh + g.sky.rgb * rim + i.col * emissive;
  c = c / (c + vec3<f32>(1.0));
  c = pow(c, vec3<f32>(1.0/2.2));
  return vec4<f32>(c, 1.0);
}
"#;

// Depth-only shadow pass — renders instances from the sun's POV into the shadow map.
const SHADOW_WGSL: &str = r#"
struct G { vp: mat4x4<f32>, sun_dir: vec4<f32>, sun_col: vec4<f32>, sky: vec4<f32>, light_vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> g: G;
@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
      @location(6) color: vec4<f32>, @location(7) material: vec4<f32>) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  return g.light_vp * model * vec4<f32>(pos, 1.0);
}
"#;

fn align256(n: u32) -> u32 {
    (n + 255) & !255
}

/// Render the EDN render-IR headless and return RGBA8 pixels (w*h*4), top row first.
/// This is the native execution of the same data the web renders.
pub fn render_to_pixels(ir_edn: &str, w: u32, h: u32) -> Vec<u8> {
    let (g, insts) = parse_ir(ir_edn);
    pollster::block_on(render_async(&g, &insts, w, h))
}

async fn render_async(g: &Globals, insts: &[Instance], w: u32, h: u32) -> Vec<u8> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("no GPU adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .expect("no device");

    let fmt = wgpu::TextureFormat::Rgba8Unorm;
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: fmt,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    // geometry + instances
    let (verts, idx) = cube();
    let vbuf = make_buf(&device, &queue, bytemuck::cast_slice(&verts), wgpu::BufferUsages::VERTEX);
    let ibuf = make_buf(&device, &queue, bytemuck::cast_slice(&idx), wgpu::BufferUsages::INDEX);

    // camera (matches the web overview/follow)
    let centroid = insts.iter().fold([0.0f32, 0.0], |a, i| [a[0] + i.pos[0], a[1] + i.pos[2]]);
    let n = insts.len().max(1) as f32;
    let (cx, cz) = (centroid[0] / n, centroid[1] / n);
    let eye = g.eye.unwrap_or([cx + 60.0, 80.0, cz + 60.0]);
    let target = g.target.unwrap_or([cx, 0.0, cz]);
    let vp = Mat4::perspective_rh(60f32.to_radians(), w as f32 / h.max(1) as f32, 0.5, 4000.0)
        * Mat4::look_at_rh(Vec3::from(eye), Vec3::from(target), Vec3::Y);

    // sun light view-proj: orthographic, centred on the camera target (the shadow camera)
    let sd = Vec3::from(g.sun_dir).normalize_or_zero();
    let ltgt = Vec3::new(cx, 0.0, cz);
    let leye = ltgt - sd * 200.0;
    let light_vp = Mat4::orthographic_rh(-130.0, 130.0, -130.0, 130.0, 1.0, 420.0)
        * Mat4::look_at_rh(leye, ltgt, Vec3::Y);

    // globals uniform: vp(16) + sun_dir(4,w=eye.x) + sun_col(4,w=eye.y) + sky(4,w=eye.z) + light_vp(16)
    let mut gf = [0f32; 44];
    gf[0..16].copy_from_slice(&vp.to_cols_array());
    gf[16..20].copy_from_slice(&[g.sun_dir[0], g.sun_dir[1], g.sun_dir[2], eye[0]]);
    gf[20..24].copy_from_slice(&[g.sun[0], g.sun[1], g.sun[2], eye[1]]);
    gf[24..28].copy_from_slice(&[g.horizon[0], g.horizon[1], g.horizon[2], eye[2]]);
    gf[28..44].copy_from_slice(&light_vp.to_cols_array());
    let gbuf = make_buf(&device, &queue, bytemuck::cast_slice(&gf), wgpu::BufferUsages::UNIFORM);

    // instance buffer: model(16) + color(4) + material(4) = 24 floats
    let mut idata: Vec<f32> = Vec::with_capacity(insts.len() * 24);
    for i in insts {
        idata.extend_from_slice(&model_mat(i).to_cols_array());
        idata.extend_from_slice(&[i.color[0], i.color[1], i.color[2], 1.0]);
        idata.extend_from_slice(&[i.metallic, i.roughness, i.emissive, 0.0]);
    }
    let inst = make_buf(&device, &queue, bytemuck::cast_slice(&idata), wgpu::BufferUsages::VERTEX);

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()),
    });
    let va = |fmt, off, loc| wgpu::VertexAttribute { format: fmt, offset: off, shader_location: loc };
    // vertex layout shared by the shadow + main pipelines
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
    // shadow map (depth) + comparison sampler
    let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: 2048, height: 2048, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_tex.create_view(&Default::default());
    let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
        compare: Some(wgpu::CompareFunction::LessEqual),
        mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // PASS pipelines: shadow (depth-only) + main (samples the shadow map) — the web :passes, in Rust
    let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: None,
        vertex: wgpu::VertexState { module: &shadow_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
        fragment: None,
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default(),
        }),
        multisample: Default::default(), multiview: None, cache: None,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: None,
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
        fragment: Some(wgpu::FragmentState {
            module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: fmt, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual, stencil: Default::default(), bias: Default::default(),
        }),
        multisample: Default::default(), multiview: None, cache: None,
    });
    let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None, layout: &shadow_pipe.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() }],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&shadow_samp) },
        ],
    });

    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    // PASS 1 — shadow map: depth from the sun's POV
    {
        let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None, occlusion_query_set: None,
        });
        if !insts.is_empty() {
            sp.set_pipeline(&shadow_pipe);
            sp.set_bind_group(0, &shadow_bind, &[]);
            sp.set_vertex_buffer(0, vbuf.slice(..));
            sp.set_vertex_buffer(1, inst.slice(..));
            sp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint16);
            sp.draw_indexed(0..idx.len() as u32, 0, 0..insts.len() as u32);
        }
    }
    // PASS 2 — main: lit, sampling the shadow map
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: g.horizon[0] as f64, g: g.horizon[1] as f64, b: g.horizon[2] as f64, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if !insts.is_empty() {
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &bind, &[]);
            rp.set_vertex_buffer(0, vbuf.slice(..));
            rp.set_vertex_buffer(1, inst.slice(..));
            rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint16);
            rp.draw_indexed(0..idx.len() as u32, 0, 0..insts.len() as u32);
        }
    }
    // copy color → readback buffer (bytes_per_row 256-aligned)
    let bpr = align256(w * 4);
    let rb = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (bpr * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &rb,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);

    let slice = rb.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    // un-pad rows
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * bpr) as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    out
}

fn make_buf(device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let b = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: data.len() as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST, // COPY_DST or writes silently no-op
        mapped_at_creation: false,
    });
    queue.write_buffer(&b, 0, data);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_same_edn_render_ir() {
        let edn = "{:globals {:sky {:horizon [0.74 0.84 0.95] :sun-dir [-0.4 -0.85 -0.35] :sun [1.0 0.96 0.85]}}
                    :instances [{:pos [0 0 0] :color [0.6 0.6 0.66] :size [2 5] :metallic 0.8 :roughness 0.25}]}";
        let (g, insts) = parse_ir(edn);
        assert_eq!(g.horizon, [0.74, 0.84, 0.95]);
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].size, [2.0, 5.0]);
        assert_eq!(insts[0].metallic, 0.8);
    }

    #[test]
    fn renders_geometry_headless() {
        // a single building filling the view; centre must differ from the sky clear.
        let edn = "{:globals {:sky {:horizon [0.74 0.84 0.95] :sun-dir [-0.4 -0.85 -0.35] :sun [1.0 0.96 0.85]}
                              :eye [6 5 6] :target [0 1 0]}
                    :instances [{:pos [0 0 0] :color [0.85 0.3 0.3] :size [3 4] :roughness 0.6}]}";
        let px = render_to_pixels(edn, 64, 64);
        assert_eq!(px.len(), 64 * 64 * 4);
        let c = ((32 * 64 + 32) * 4) as usize; // centre pixel
        let (r, gc, b) = (px[c], px[c + 1], px[c + 2]);
        let sky = (189u8, 214, 242); // ~horizon in 8-bit
        let is_sky = (r as i32 - sky.0 as i32).abs() < 12
            && (gc as i32 - sky.1 as i32).abs() < 12
            && (b as i32 - sky.2 as i32).abs() < 12;
        assert!(!is_sky, "centre should be the lit building, not sky: got {r},{gc},{b}");
        assert!(r > gc && r > b, "building is reddish: got {r},{gc},{b}");
    }

    #[test]
    fn caster_casts_a_shadow() {
        // a ground plane filling the view; a tall caster should darken the ground (shadow map).
        let cam = ":eye [0 50 22] :target [0 0 0]";
        let sky = ":horizon [0.1 0.1 0.12] :sun-dir [-0.45 -0.8 -0.4] :sun [1 0.96 0.85]";
        let ground = "{:pos [0 -0.5 0] :color [0.7 0.7 0.7] :size [200 1] :roughness 0.95}";
        let caster = "{:pos [0 0 0] :color [0.5 0.5 0.5] :size [5 16] :roughness 0.95}";
        let lit_only = format!("{{:globals {{:sky {{{sky}}} {cam}}} :instances [{ground}]}}");
        let shadowed = format!("{{:globals {{:sky {{{sky}}} {cam}}} :instances [{ground} {caster}]}}");
        // darkest luminance anywhere in the frame
        let darkest = |px: &[u8]| px.chunks(4)
            .map(|c| (c[0] as i32 * 30 + c[1] as i32 * 59 + c[2] as i32 * 11) / 100)
            .min().unwrap_or(0);
        let la = darkest(&render_to_pixels(&lit_only, 96, 96));
        let lb = darkest(&render_to_pixels(&shadowed, 96, 96));
        assert!(lb + 12 < la, "the caster should darken the ground via shadow: lit min={la}, shadowed min={lb}");
    }
}
