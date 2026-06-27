//! Choreography as **data**: a dance authored purely as EDN `:dance/clips`
//! keyframes (no Rust `DanceMove` preset) drives a rigged figure.
//!
//! `EDN clip → clip_from_edn → AnimationClip → Skeleton::evaluate → world·
//! inverse-bind CPU skinning → per-frame offscreen GPU render → looping GIF`.
//!
//! This is the ADR-0046 "clj frontier" made concrete: the *motion itself* is
//! data an author/Datomic can write and fork, not a compiled-in pose function.
//! The figure is a small four-bone puppet (hips → spine → L/R upper-arm) built
//! procedurally; the wave + sway come entirely from the EDN below.
//!
//! `cargo run -p kami-live --example clip_dance --target aarch64-apple-darwin`

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use kami_skeleton_scene::{pmx_to_skeleton, PmxBone, PmxModel, PmxVertex};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct V { pos: [f32; 3], normal: [f32; 3] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct G { vp: [[f32; 4]; 4], light: [f32; 4] }

/// The dance — pure EDN keyframes (the only authoring surface). Arms wave in
/// anti-phase, the spine sways; looped over 2 s. No Rust choreography.
const DANCE_EDN: &str = r#"
{:name "wave" :duration 2.0 :loop true
 :tracks
 [{:bone "leftUpperArm" :interp :cubic
   :keys [{:t 0.0 :rot [0 0 0.20 0.98]} {:t 1.0 :rot [0 0 0.62 0.78]} {:t 2.0 :rot [0 0 0.20 0.98]}]}
  {:bone "rightUpperArm" :interp :cubic
   :keys [{:t 0.0 :rot [0 0 -0.62 0.78]} {:t 1.0 :rot [0 0 -0.20 0.98]} {:t 2.0 :rot [0 0 -0.62 0.78]}]}
  {:bone "spine" :interp :cubic
   :keys [{:t 0.0 :rot [0 0 0.06 0.998]} {:t 1.0 :rot [0 0 -0.06 0.998]} {:t 2.0 :rot [0 0 0.06 0.998]}]}]}
"#;

/// Add an axis-aligned box (per-face verts → clean normals); each vertex is
/// weighted by `wfn(pos) -> ([bone0,bone1],[w0,w1])`.
fn push_box(verts: &mut Vec<PmxVertex>, idx: &mut Vec<u32>, lo: Vec3, hi: Vec3, wfn: &dyn Fn(Vec3) -> ([i32; 2], [f32; 2])) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[lo.x, lo.y, hi.z], [hi.x, lo.y, hi.z], [hi.x, hi.y, hi.z], [lo.x, hi.y, hi.z]]),
        ([0.0, 0.0, -1.0], [[hi.x, lo.y, lo.z], [lo.x, lo.y, lo.z], [lo.x, hi.y, lo.z], [hi.x, hi.y, lo.z]]),
        ([1.0, 0.0, 0.0], [[hi.x, lo.y, hi.z], [hi.x, lo.y, lo.z], [hi.x, hi.y, lo.z], [hi.x, hi.y, hi.z]]),
        ([-1.0, 0.0, 0.0], [[lo.x, lo.y, lo.z], [lo.x, lo.y, hi.z], [lo.x, hi.y, hi.z], [lo.x, hi.y, lo.z]]),
        ([0.0, 1.0, 0.0], [[lo.x, hi.y, hi.z], [hi.x, hi.y, hi.z], [hi.x, hi.y, lo.z], [lo.x, hi.y, lo.z]]),
        ([0.0, -1.0, 0.0], [[lo.x, lo.y, lo.z], [hi.x, lo.y, lo.z], [hi.x, lo.y, hi.z], [lo.x, lo.y, hi.z]]),
    ];
    for (n, quad) in faces {
        let b = verts.len() as u32;
        for p in quad {
            let pos = Vec3::from(p);
            let (bones, w) = wfn(pos);
            verts.push(PmxVertex { pos, normal: n.into(), uv: Vec2::ZERO, bones: [bones[0], bones[1], -1, -1], weights: [w[0], w[1], 0.0, 0.0] });
        }
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
}

/// A four-bone puppet: hips(0) → spine(1) → leftUpperArm(2) / rightUpperArm(3).
fn puppet() -> PmxModel {
    let (mut verts, mut idx) = (Vec::new(), Vec::new());
    // torso: y 0..1.1, weighted hips↔spine by height.
    push_box(&mut verts, &mut idx, Vec3::new(-0.20, 0.0, -0.12), Vec3::new(0.20, 1.10, 0.12), &|p| {
        let t = ((p.y - 0.4) / 0.6).clamp(0.0, 1.0);
        let w1 = t * t * (3.0 - 2.0 * t);
        ([0, 1], [1.0 - w1, w1])
    });
    // left arm: extends out-down from the L shoulder (-0.2, 1.0); 100% leftUpperArm(2).
    push_box(&mut verts, &mut idx, Vec3::new(-0.62, 0.50, -0.08), Vec3::new(-0.20, 0.66, 0.08), &|_| ([2, -1], [1.0, 0.0]));
    // right arm: 100% rightUpperArm(3).
    push_box(&mut verts, &mut idx, Vec3::new(0.20, 0.50, -0.08), Vec3::new(0.62, 0.66, 0.08), &|_| ([3, -1], [1.0, 0.0]));
    PmxModel {
        name: "puppet".into(),
        vertices: verts,
        indices: idx,
        bones: vec![
            PmxBone { name: "hips".into(), pos: Vec3::new(0.0, 0.0, 0.0), parent: -1 },
            PmxBone { name: "spine".into(), pos: Vec3::new(0.0, 0.5, 0.0), parent: 0 },
            PmxBone { name: "leftUpperArm".into(), pos: Vec3::new(-0.20, 1.0, 0.0), parent: 1 },
            PmxBone { name: "rightUpperArm".into(), pos: Vec3::new(0.20, 1.0, 0.0), parent: 1 },
        ],
        morphs: vec![], materials: vec![], textures: vec![],
    }
}

fn main() { pollster::block_on(run()); }

async fn run() {
    let model = puppet();
    let skel = pmx_to_skeleton(&model);
    let clip = kami_skeleton_scene::clip_from_edn(DANCE_EDN, |name| skel.bones.iter().position(|b| b.name == name))
        .expect("clip_from_edn parsed the EDN dance");
    println!("EDN choreography → clip '{}' ({} tracks, {:.1}s loop)", clip.name, clip.tracks.len(), clip.duration);

    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &model.vertices { lo = lo.min(v.pos); hi = hi.max(v.pos); }
    let center = (lo + hi) * 0.5;
    let height = (hi.y - lo.y).max(0.5);

    let (w, h) = (420u32, 480u32);
    let inst = wgpu::Instance::default();
    let adapter = inst.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.unwrap();
    let nverts = model.vertices.len();
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (nverts * std::mem::size_of::<V>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let ibuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of_val(&model.indices[..]) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&ibuf, 0, bytemuck::cast_slice(&model.indices));
    let gbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<G>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }] });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() }] });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[] });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(r#"
        struct G { vp: mat4x4<f32>, light: vec4<f32> };
        @group(0) @binding(0) var<uniform> g: G;
        struct VO { @builtin(position) clip: vec4<f32>, @location(0) n: vec3<f32> };
        @vertex fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> VO { var o: VO; o.clip = g.vp*vec4<f32>(p,1.0); o.n = n; return o; }
        @fragment fn fs(i: VO) -> @location(0) vec4<f32> { let d = max(dot(normalize(i.n), -normalize(g.light.xyz)), 0.0); let c = vec3<f32>(0.95,0.55,0.45)*(0.32+0.62*d); return vec4<f32>(pow(c, vec3<f32>(1.0/2.2)), 1.0); }
    "#.into()) });
    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let vbl = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<V>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3] };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { label: None, layout: Some(&pl),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[vbl], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), targets: &[Some(fmt.into())], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
        multisample: Default::default(), multiview: None, cache: None });
    let color = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
    let cview = color.create_view(&Default::default());
    let dtex = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
    let dview = dtex.create_view(&Default::default());
    let dist = height * 3.6;
    let eye = center + Vec3::new(dist * 0.28, height * 0.1, dist);
    let vp = (Mat4::perspective_rh(0.7, w as f32 / h as f32, 0.05, 100.0) * Mat4::look_at_rh(eye, center, Vec3::Y)).to_cols_array_2d();
    queue.write_buffer(&gbuf, 0, bytemuck::bytes_of(&G { vp, light: [-0.3, -0.5, -0.75, 0.0] }));

    let bpr = (w * 4).div_ceil(256) * 256;
    let frames_n = 48u32;
    let mut frames = Vec::new();
    for f in 0..frames_n {
        let t = f as f32 / frames_n as f32 * clip.duration;
        let world = skel.evaluate(&clip, t);
        let palette: Vec<Mat4> = world.iter().enumerate().map(|(i, wm)| *wm * Mat4::from_cols_array_2d(&skel.bones[i].inverse_bind)).collect();
        let skinned: Vec<V> = model.vertices.iter().map(|v| {
            let (mut p, mut n) = (Vec3::ZERO, Vec3::ZERO);
            for k in 0..4 {
                let (b, wt) = (v.bones[k], v.weights[k]);
                if b < 0 || wt == 0.0 { continue; }
                let m = palette[b as usize];
                p += wt * m.transform_point3(v.pos);
                n += wt * m.transform_vector3(v.normal);
            }
            V { pos: p.into(), normal: n.normalize_or_zero().into() }
        }).collect();
        queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&skinned));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor { label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &cview, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.94, g: 0.92, b: 0.84, a: 1.0 }), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dview, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }), timestamp_writes: None, occlusion_query_set: None });
            rp.set_pipeline(&pipeline); rp.set_bind_group(0, &bg, &[]); rp.set_vertex_buffer(0, vbuf.slice(..)); rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32); rp.draw_indexed(0..model.indices.len() as u32, 0, 0..1);
        }
        let rbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        enc.copy_texture_to_buffer(wgpu::ImageCopyTexture { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::ImageCopyBuffer { buffer: &rbuf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        queue.submit([enc.finish()]);
        let sl = rbuf.slice(..); sl.map_async(wgpu::MapMode::Read, |_| {}); device.poll(wgpu::Maintain::Wait);
        let data = sl.get_mapped_range();
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h { let s = (y * bpr) as usize; let d = (y * w * 4) as usize; px[d..d + (w * 4) as usize].copy_from_slice(&data[s..s + (w * 4) as usize]); }
        frames.push(image::Frame::from_parts(image::RgbaImage::from_raw(w, h, px).unwrap(), 0, 0, image::Delay::from_numer_denom_ms(1000, 30)));
    }
    let fl = std::fs::File::create("clip_dance.gif").unwrap();
    let mut e = image::codecs::gif::GifEncoder::new(fl);
    e.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
    e.encode_frames(frames.into_iter()).unwrap();
    println!("wrote clip_dance.gif — EDN-authored choreography skinned a rigged figure ({frames_n} frames)");
}
