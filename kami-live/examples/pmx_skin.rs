//! Animate an MMD-style rigged mesh: `.pmx` skeleton + a bending clip, GPU-drawn
//! per frame via CPU skinning → an animated GIF. The moving counterpart of
//! `pmx_render` (static) — it carries the MMD path all the way to *animated*
//! pixels: `PmxModel → pmx_to_skeleton → Skeleton::evaluate → world·inverse-bind
//! skinning`, the same math proved headless in `pmx::tests`.
//!
//! `cargo run -p kami-live --example pmx_skin --target aarch64-apple-darwin`
//!
//! No asset ships (MMD models aren't redistributable), so it builds a small
//! two-bone rigged prism procedurally as a `PmxModel`; drop a rigged
//! `assets/model.pmx` to skin a real model with the same code path.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec2, Vec3};
use kami_skeleton::{AnimationClip, BoneTrack, Interpolation, Keyframe};
use kami_skeleton_scene::{pmx_to_model, pmx_to_skeleton, PmxBone, PmxModel, PmxVertex};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct V { pos: [f32; 3], normal: [f32; 3] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct G { vp: [[f32; 4]; 4], light: [f32; 4] }

/// A two-bone rigged prism (square cross-section, y = 0..2). The upper half is
/// weighted to a child bone pivoting at y = 1, so rotating it bends the mesh —
/// a minimal stand-in for a rigged `.pmx` limb.
fn rigged_prism() -> PmxModel {
    let rings = [0.0f32, 0.5, 1.0, 1.5, 2.0];
    let corners = [Vec2::new(0.22, 0.22), Vec2::new(-0.22, 0.22), Vec2::new(-0.22, -0.22), Vec2::new(0.22, -0.22)];
    let mut vertices = Vec::new();
    for &y in &rings {
        for c in corners {
            // weight: lower half → bone 0 (root), upper half → bone 1 (smoothstep at y=1).
            let t = ((y - 0.5) / 1.0).clamp(0.0, 1.0);
            let w1 = t * t * (3.0 - 2.0 * t);
            vertices.push(PmxVertex {
                pos: Vec3::new(c.x, y, c.y),
                normal: Vec3::new(c.x, 0.0, c.y).normalize(), // radial
                uv: Vec2::ZERO,
                bones: [0, 1, -1, -1],
                weights: [1.0 - w1, w1, 0.0, 0.0],
            });
        }
    }
    let mut indices = Vec::new();
    for r in 0..rings.len() as u32 - 1 {
        for e in 0..4u32 {
            let (a, b) = (r * 4 + e, r * 4 + (e + 1) % 4);
            let (c, d) = (a + 4, b + 4);
            indices.extend_from_slice(&[a, b, d, a, d, c]);
        }
    }
    PmxModel {
        name: "rigged-prism".into(),
        vertices,
        indices,
        bones: vec![
            PmxBone { name: "root".into(), pos: Vec3::new(0.0, 0.0, 0.0), parent: -1 },
            PmxBone { name: "upper".into(), pos: Vec3::new(0.0, 1.0, 0.0), parent: 0 },
        ],
        morphs: vec![],
        materials: vec![],
        textures: vec![],
    }
}

/// A looping bend clip on bone 1 (`upper`): sway ±40° around Z over 2 s.
fn bend_clip(upper: usize) -> AnimationClip {
    let key = |t: f32, deg: f32| Keyframe {
        time: t,
        position: None,
        rotation: Some(Quat::from_rotation_z(deg.to_radians())),
        scale: None,
    };
    AnimationClip {
        name: "bend".into(),
        duration: 2.0,
        looping: true,
        tracks: vec![BoneTrack {
            bone_index: upper,
            keyframes: vec![key(0.0, 0.0), key(0.5, 40.0), key(1.0, 0.0), key(1.5, -40.0), key(2.0, 0.0)],
            interpolation: Interpolation::Linear,
        }],
    }
}

fn main() { pollster::block_on(run()); }

async fn run() {
    let model = match std::fs::read("assets/model.pmx").ok().and_then(|b| pmx_to_model(&b)) {
        Some(m) => { println!("loaded PMX '{}': {} verts, {} bones", m.name, m.vertices.len(), m.bones.len()); m }
        None => { println!("no assets/model.pmx — skinning the built-in rigged prism"); rigged_prism() }
    };
    let skel = pmx_to_skeleton(&model);
    let upper = skel.bones.iter().position(|b| b.name == "upper").unwrap_or(skel.bones.len().saturating_sub(1));
    let clip = bend_clip(upper);

    // bounds (rest pose) for camera framing.
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &model.vertices { lo = lo.min(v.pos); hi = hi.max(v.pos); }
    let center = (lo + hi) * 0.5;
    let height = (hi.y - lo.y).max(0.5);

    let (w, h) = (360u32, 480u32);
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
        @fragment fn fs(i: VO) -> @location(0) vec4<f32> { let d = max(dot(normalize(i.n), -normalize(g.light.xyz)), 0.0); let c = vec3<f32>(0.45,0.7,0.9)*(0.3+0.65*d); return vec4<f32>(pow(c, vec3<f32>(1.0/2.2)), 1.0); }
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

    let dist = height * 3.0;
    let eye = center + Vec3::new(dist * 0.35, height * 0.2, dist);
    let vp = (Mat4::perspective_rh(0.7, w as f32 / h as f32, 0.05, 100.0) * Mat4::look_at_rh(eye, center, Vec3::Y)).to_cols_array_2d();
    queue.write_buffer(&gbuf, 0, bytemuck::bytes_of(&G { vp, light: [-0.3, -0.5, -0.75, 0.0] }));

    let bpr = (w * 4).div_ceil(256) * 256;
    let frames_n = 48u32;
    let mut frames = Vec::new();
    for f in 0..frames_n {
        let t = f as f32 / frames_n as f32 * clip.duration;
        // MMD pipeline: evaluate the clip → world matrices → skinning palette.
        let world = skel.evaluate(&clip, t);
        let palette: Vec<Mat4> = world.iter().enumerate()
            .map(|(i, wm)| *wm * Mat4::from_cols_array_2d(&skel.bones[i].inverse_bind))
            .collect();
        // CPU skin each vertex with its bone weights.
        let skinned: Vec<V> = model.vertices.iter().map(|v| {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::ZERO;
            for k in 0..4 {
                let b = v.bones[k];
                let wt = v.weights[k];
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

    let fl = std::fs::File::create("pmx_skin.gif").unwrap();
    let mut e = image::codecs::gif::GifEncoder::new(fl);
    e.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
    e.encode_frames(frames.into_iter()).unwrap();
    println!("wrote pmx_skin.gif — MMD .pmx mesh skinned by a clip ({frames_n} frames)");
}
