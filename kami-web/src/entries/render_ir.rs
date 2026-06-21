//! Generic EDN render-IR interpreter (ADR-0002, network-isekai).
//!
//! `run_with_render_ir(canvas_id, ir_json)` draws a CLJ-authored render-IR — no game
//! look is hardcoded here. The IR is `{globals:{sky,sun,...}, instances:[{pos,color,
//! size,yaw}]}`; this entry draws the instances as lit boxes under the sky color.
//! One renderer, driven entirely by data, so each game's web look is EDN.
//!
//! MVP: a single framed overview (instanced lit cuboids + sky clear + depth). Camera
//! follow, terrain/water/shadow/UI passes lift in next, sharing the play3d pipelines.

use glam::{Mat4, Vec3};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun_col: [f32; 4],
    sky: [f32; 4],
}

fn cube() -> (Vec<Vertex>, Vec<u16>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
        ([1.0, 0.0, 0.0], [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]]),
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]]),
    ];
    let (mut v, mut idx) = (Vec::new(), Vec::new());
    for (n, quad) in faces {
        let b = v.len() as u16;
        for p in quad { v.push(Vertex { pos: p, normal: n }); }
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    (v, idx)
}

// --- render-IR (serde_json::Value) helpers ----------------------------------

fn arr3(v: &serde_json::Value, k: &str) -> [f32; 3] {
    let a = &v[k];
    [a.get(0).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
     a.get(1).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
     a.get(2).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32]
}
fn f(v: &serde_json::Value, k: &str, d: f32) -> f32 {
    v.get(k).and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(d)
}

/// Run the render-IR interpreter inside a canvas. `ir_json` is the EDN render-IR
/// JSON-encoded by the CLJ brain (isekai.render-ir / isekai.game).
#[wasm_bindgen]
pub async fn run_with_render_ir(canvas_id: &str, ir_json: &str) -> Result<(), JsValue> {
    let document = web_sys::window().ok_or("no window")?.document().ok_or("no document")?;
    let canvas = document.get_element_by_id(canvas_id).ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let (device, queue, surface, config, format, w, h) = crate::init_gpu(&canvas).await?;

    let ir: serde_json::Value = serde_json::from_str(ir_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let g = &ir["globals"];
    let sky_h = arr3(&g["sky"], "horizon");
    let sun_dir = arr3(&g["sky"], "sun-dir");
    let sun_col = arr3(&g["sky"], "sun");

    // build instances (flat colored boxes: {pos, color, size:[w,h], yaw})
    let mut insts: Vec<Instance> = Vec::new();
    let mut centroid = Vec3::ZERO;
    if let Some(arr) = ir["instances"].as_array() {
        for it in arr {
            let p = arr3(it, "pos");
            let c = arr3(it, "color");
            let sz = it.get("size");
            let bw = sz.and_then(|s| s.get(0)).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
            let bh = sz.and_then(|s| s.get(1)).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
            let yaw = f(it, "yaw", 0.0);
            let pos = Vec3::new(p[0], p[1], p[2]);
            centroid += pos;
            let m = Mat4::from_translation(pos + Vec3::new(0.0, bh * 0.5, 0.0))
                * Mat4::from_rotation_y(yaw)
                * Mat4::from_scale(Vec3::new(bw, bh, bw));
            insts.push(Instance { model: m.to_cols_array_2d(), color: [c[0], c[1], c[2], 1.0] });
        }
    }
    if !insts.is_empty() { centroid /= insts.len() as f32; }

    // overview camera framing the instance cloud
    let target = Vec3::new(centroid.x, 0.0, centroid.z);
    let cam = target + Vec3::new(60.0, 80.0, 60.0);
    let aspect = w as f32 / h.max(1) as f32;
    let vp = Mat4::perspective_rh(60f32.to_radians(), aspect.max(0.1), 0.5, 4000.0)
        * Mat4::look_at_rh(cam, target, Vec3::Y);
    let globals = Globals {
        view_proj: vp.to_cols_array_2d(),
        sun_dir: [sun_dir[0], sun_dir[1], sun_dir[2], 0.0],
        sun_col: [sun_col[0], sun_col[1], sun_col[2], 1.0],
        sky: [sky_h[0], sky_h[1], sky_h[2], 1.0],
    };

    // --- gpu resources ---
    let (verts, indices) = cube();
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("v"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("i"), contents: bytemuck::cast_slice(&indices), usage: wgpu::BufferUsages::INDEX });
    let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("inst"), contents: bytemuck::cast_slice(&insts), usage: wgpu::BufferUsages::VERTEX });
    let gbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("g"), contents: bytemuck::bytes_of(&globals),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None, entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None }] });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None, layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() }] });
    let pll = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[] });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render-ir"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });

    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] })
        .create_view(&wgpu::TextureViewDescriptor::default());

    let vbl = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 }] };
    let ibl = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Instance>() as u64, step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 2 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 3 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 4 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 5 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 6 }] };

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render-ir"), layout: Some(&pll),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[vbl, ibl], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), targets: &[Some(format.into())], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual, stencil: Default::default(), bias: Default::default() }),
        multisample: Default::default(), multiview: None, cache: None });

    let _ = config; // configured by init_gpu
    let frame = surface.get_current_texture().map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: sky_h[0] as f64, g: sky_h[1] as f64, b: sky_h[2] as f64, a: 1.0 }), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None, occlusion_query_set: None });
        if !insts.is_empty() {
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &bind, &[]);
            rp.set_vertex_buffer(0, vbuf.slice(..));
            rp.set_vertex_buffer(1, inst_buf.slice(..));
            rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint16);
            rp.draw_indexed(0..indices.len() as u32, 0, 0..insts.len() as u32);
        }
    }
    queue.submit(Some(enc.finish()));
    frame.present();
    Ok(())
}

const SHADER: &str = r#"
struct G { view_proj: mat4x4<f32>, sun_dir: vec4<f32>, sun_col: vec4<f32>, sky: vec4<f32> };
@group(0) @binding(0) var<uniform> g: G;
struct VO { @builtin(position) clip: vec4<f32>, @location(0) n: vec3<f32>, @location(1) col: vec3<f32> };
@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
      @location(6) color: vec4<f32>) -> VO {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  var o: VO;
  o.clip = g.view_proj * model * vec4<f32>(pos, 1.0);
  o.n = normalize((model * vec4<f32>(normal, 0.0)).xyz);
  o.col = color.rgb;
  return o;
}
@fragment
fn fs(i: VO) -> @location(0) vec4<f32> {
  let L = normalize(-g.sun_dir.xyz);
  let lambert = max(dot(normalize(i.n), L), 0.0);
  let amb = mix(vec3<f32>(0.18, 0.2, 0.26), g.sky.rgb * 0.5, normalize(i.n).y * 0.5 + 0.5);
  let col = i.col * (amb + lambert * g.sun_col.rgb * 0.85);
  return vec4<f32>(col, 1.0);
}
"#;
