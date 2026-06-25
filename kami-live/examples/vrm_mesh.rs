//! A GPU-skinned humanoid MESH danced by the clj/edn show — the three.js
//! `SkinnedMesh` core (one mesh deformed by a bone palette), the foundation a
//! real VRM swaps its loaded geometry into. Procedural humanoid (smooth tapered
//! limbs, not box-assembly), rigid-bound per bone, posed each frame from the
//! beat-synced `DancePose`, rendered offscreen (Lambert + depth) to PNG/GIF.
//! `cargo run -p kami-live --example vrm_mesh --target aarch64-apple-darwin`

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use kami_live::scene::DanceScene;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
const MAX_JOINTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex { pos: [f32; 3], normal: [f32; 3], joints: [u32; 4], weights: [f32; 4] }

const MAX_LIGHTS: usize = 8;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLight { dir: [f32; 4], color: [f32; 4] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    base: [f32; 4],
    shade: [f32; 4],
    ambient: [f32; 4],
    n_lights: u32,
    _pad: [u32; 3],
    lights: [GpuLight; MAX_LIGHTS],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Palette { joints: [[[f32; 4]; 4]; MAX_JOINTS] }

/// Append a tapered cylinder (p0→p1, radii r0/r1) bound rigidly to `bone`.
fn cylinder(v: &mut Vec<Vertex>, idx: &mut Vec<u16>, p0: Vec3, p1: Vec3, r0: f32, r1: f32, bone: u32) {
    let sides = 10u32;
    let axis = (p1 - p0).normalize_or_zero();
    let up = if axis.dot(Vec3::Y).abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let t = axis.cross(up).normalize_or_zero();
    let b = axis.cross(t);
    let base = v.len() as u16;
    for s in 0..=sides {
        let a = s as f32 / sides as f32 * std::f32::consts::TAU;
        let dir = t * a.cos() + b * a.sin();
        v.push(Vertex { pos: (p0 + dir * r0).into(), normal: dir.into(), joints: [bone, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] });
        v.push(Vertex { pos: (p1 + dir * r1).into(), normal: dir.into(), joints: [bone, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] });
    }
    for s in 0..sides {
        let i = base + (s * 2) as u16;
        idx.extend_from_slice(&[i, i + 1, i + 2, i + 2, i + 1, i + 3]);
    }
}

/// Humanoid mesh: torso(spine), head, 2 arms, 2 legs — rigid per bone.
fn humanoid_mesh() -> (Vec<Vertex>, Vec<u16>) {
    let (mut v, mut idx) = (Vec::new(), Vec::new());
    cylinder(&mut v, &mut idx, Vec3::new(0.0, 0.85, 0.0), Vec3::new(0.0, 1.4, 0.0), 0.17, 0.13, 1); // torso
    cylinder(&mut v, &mut idx, Vec3::new(0.0, 1.4, 0.0), Vec3::new(0.0, 1.68, 0.0), 0.12, 0.13, 2); // head
    cylinder(&mut v, &mut idx, Vec3::new(-0.17, 1.4, 0.0), Vec3::new(-0.52, 1.05, 0.0), 0.06, 0.05, 3); // L arm
    cylinder(&mut v, &mut idx, Vec3::new(0.17, 1.4, 0.0), Vec3::new(0.52, 1.05, 0.0), 0.06, 0.05, 4); // R arm
    cylinder(&mut v, &mut idx, Vec3::new(-0.09, 0.86, 0.0), Vec3::new(-0.12, 0.02, 0.0), 0.09, 0.06, 5); // L leg
    cylinder(&mut v, &mut idx, Vec3::new(0.09, 0.86, 0.0), Vec3::new(0.12, 0.02, 0.0), 0.09, 0.06, 6); // R leg
    (v, idx)
}

/// Rest-world bone positions (translation only). idx: 0 hips,1 spine,2 head,3 Larm,4 Rarm,5 Lleg,6 Rleg.
fn rest_worlds() -> [Mat4; MAX_JOINTS] {
    let p = [
        Vec3::new(0.0, 0.9, 0.0), Vec3::new(0.0, 1.15, 0.0), Vec3::new(0.0, 1.45, 0.0),
        Vec3::new(-0.17, 1.4, 0.0), Vec3::new(0.17, 1.4, 0.0), Vec3::new(-0.09, 0.86, 0.0),
        Vec3::new(0.09, 0.86, 0.0), Vec3::ZERO,
    ];
    p.map(Mat4::from_translation)
}
const PARENT: [i32; MAX_JOINTS] = [-1, 0, 1, 1, 1, 0, 0, -1];

/// Per-frame joint palette (jointMatrix = boneWorld * inverse(restWorld)) from the pose.
fn palette(pose: &kami_live::DancePose) -> Palette {
    let rest = rest_worlds();
    let mut anim = [Quat::IDENTITY; MAX_JOINTS];
    anim[1] = Quat::from_rotation_z(pose.spine_sway);
    anim[3] = Quat::from_rotation_z(pose.arms_up * 1.3);   // raise L arm
    anim[4] = Quat::from_rotation_z(-pose.arms_up * 1.3);  // raise R arm
    let root_motion = Mat4::from_translation(Vec3::new(pose.root_translation.x, pose.vertical_bob, pose.root_translation.z));
    let mut world = [Mat4::IDENTITY; MAX_JOINTS];
    for i in 0..7 {
        let local = if PARENT[i] < 0 {
            root_motion * rest[i] * Mat4::from_quat(Quat::from_rotation_y(pose.root_yaw))
        } else {
            let par = PARENT[i] as usize;
            rest[par].inverse() * rest[i] * Mat4::from_quat(anim[i])
        };
        world[i] = if PARENT[i] < 0 { local } else { world[PARENT[i] as usize] * local };
    }
    let mut joints = [[[0.0f32; 4]; 4]; MAX_JOINTS];
    for i in 0..MAX_JOINTS {
        let jm = if i < 7 { world[i] * rest[i].inverse() } else { Mat4::IDENTITY };
        joints[i] = jm.to_cols_array_2d();
    }
    Palette { joints }
}

fn main() { pollster::block_on(run()); }

async fn run() {
    let (w, h) = (480u32, 420u32);
    let inst = wgpu::Instance::default();
    let adapter = inst.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.expect("device");

    let (verts, indices) = humanoid_mesh();
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of_val(&verts[..]) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&verts));
    let ibuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of_val(&indices[..]) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&ibuf, 0, bytemuck::cast_slice(&indices));

    let gbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<Globals>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let pbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<Palette>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
        wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
    ]});
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 1, resource: pbuf.as_entire_binding() },
    ]});
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[] });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(format!(r#"
        struct L {{ dir: vec4<f32>, color: vec4<f32> }};
        struct G {{ vp: mat4x4<f32>, base: vec4<f32>, shade: vec4<f32>, ambient: vec4<f32>, n: vec4<u32>, lights: array<L, {MAX_LIGHTS}u> }};
        struct P {{ joints: array<mat4x4<f32>, {MAX_JOINTS}u> }};
        @group(0) @binding(0) var<uniform> g: G;
        @group(0) @binding(1) var<uniform> p: P;
        struct VO {{ @builtin(position) clip: vec4<f32>, @location(0) n: vec3<f32> }};
        @vertex fn vs(@location(0) pos: vec3<f32>, @location(1) nor: vec3<f32>, @location(2) j: vec4<u32>, @location(3) wts: vec4<f32>) -> VO {{
          let skin = p.joints[j.x] * wts.x + p.joints[j.y] * wts.y + p.joints[j.z] * wts.z + p.joints[j.w] * wts.w;
          let wp = skin * vec4<f32>(pos, 1.0);
          var o: VO; o.clip = g.vp * wp; o.n = normalize((skin * vec4<f32>(nor, 0.0)).xyz); return o;
        }}
        @fragment fn fs(i: VO) -> @location(0) vec4<f32> {{
          let nn = normalize(i.n);
          var lit = g.ambient.xyz * g.base.xyz;
          let count = min(g.n.x, {MAX_LIGHTS}u);
          for (var k: u32 = 0u; k < count; k = k + 1u) {{
            let ndl = dot(nn, -normalize(g.lights[k].dir.xyz));
            // MToon two-tone: hard-ish step between shade and base.
            let t = smoothstep(-0.05, 0.15, ndl);
            let toon = mix(g.shade.xyz, g.base.xyz, t);
            lit = lit + toon * g.lights[k].color.xyz * g.lights[k].color.w;
          }}
          let exposure = g.ambient.w;
          let mapped = lit * exposure / (lit * exposure + vec3<f32>(1.0));
          return vec4<f32>(pow(mapped, vec3<f32>(1.0/2.2)), 1.0);
        }}
    "#).into()) });

    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let vbl = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x3, 2=>Uint32x4, 3=>Float32x4] };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: Some(&pl),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[vbl], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), targets: &[Some(fmt.into())], compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
        multisample: Default::default(), multiview: None, cache: None,
    });

    let color = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
    let cview = color.create_view(&Default::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] });
    let dview = depth.create_view(&Default::default());

    // camera framing the figure
    let eye = Vec3::new(0.0, 1.1, 3.0);
    let target = Vec3::new(0.0, 1.0, 0.0);
    let vp = Mat4::perspective_rh(0.9, w as f32 / h as f32, 0.1, 100.0) * Mat4::look_at_rh(eye, target, Vec3::Y);

    let mut scene = DanceScene::from_edn(SCENE).expect("scene");
    scene.show.start();
    for _ in 0..(61.0 * 60.0) as i32 { scene.frame(1.0 / 60.0); } // advance to the Chorus (wota: arms up)

    let bpr = (w * 4).div_ceil(256) * 256;
    let rbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut gif = Vec::new();
    for frame in 0..32 {
        for _ in 0..2 { scene.frame(1.0 / 60.0); }
        let frame_out = scene.frame(1.0 / 60.0);
        let ir = kami_webgpu_rs::parse_render_ir(&frame_out.render_ir_edn());
        // material + env + beat-synced lights come from the clj/edn render-IR.
        let mat = ir.material("performer");
        let base = mat.map(|m| m.base).unwrap_or([1.0, 0.82, 0.72]);
        let shade = mat.map(|m| m.shade).unwrap_or([0.7, 0.55, 0.5]);
        let amb = ir.env.ambient;
        let mut lights = [GpuLight { dir: [0.0; 4], color: [0.0; 4] }; MAX_LIGHTS];
        let nl = ir.lights.len().min(MAX_LIGHTS);
        for (k, l) in ir.lights.iter().take(MAX_LIGHTS).enumerate() {
            lights[k] = GpuLight { dir: [l.dir[0], l.dir[1], l.dir[2], 0.0], color: [l.color[0], l.color[1], l.color[2], l.intensity] };
        }
        // a fallback key light if the rig is empty.
        let n_lights = if nl == 0 { lights[0] = GpuLight { dir: [-0.4, -0.8, -0.4, 0.0], color: [1.0, 0.96, 0.85, 1.0] }; 1 } else { nl };
        queue.write_buffer(&gbuf, 0, bytemuck::bytes_of(&Globals {
            view_proj: vp.to_cols_array_2d(),
            base: [base[0], base[1], base[2], 1.0],
            shade: [shade[0], shade[1], shade[2], 1.0],
            ambient: [amb[0] * 0.5, amb[1] * 0.5, amb[2] * 0.5, ir.env.exposure.max(0.5)],
            n_lights: n_lights as u32,
            _pad: [0; 3],
            lights,
        }));
        let pose = scene.show.snapshot().performer_pose;
        queue.write_buffer(&pbuf, 0, bytemuck::bytes_of(&palette(&pose)));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor { label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &cview, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.16, g: 0.18, b: 0.28, a: 1.0 }), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dview, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                timestamp_writes: None, occlusion_query_set: None });
            rp.set_pipeline(&pipeline); rp.set_bind_group(0, &bg, &[]);
            rp.set_vertex_buffer(0, vbuf.slice(..)); rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint16);
            rp.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }
        enc.copy_texture_to_buffer(wgpu::ImageCopyTexture { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::ImageCopyBuffer { buffer: &rbuf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        queue.submit([enc.finish()]);
        let slice = rbuf.slice(..); slice.map_async(wgpu::MapMode::Read, |_| {}); device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h { let s = (y * bpr) as usize; let d = (y * w * 4) as usize; px[d..d + (w * 4) as usize].copy_from_slice(&data[s..s + (w * 4) as usize]); }
        drop(data); rbuf.unmap();
        if frame % 8 == 0 { image::save_buffer(format!("vrm_{frame:02}.png"), &px, w, h, image::ExtendedColorType::Rgba8).unwrap(); }
        gif.push(image::Frame::from_parts(image::RgbaImage::from_raw(w, h, px).unwrap(), 0, 0, image::Delay::from_numer_denom_ms(70, 1)));
    }
    let f = std::fs::File::create("vrm.gif").unwrap();
    let mut e = image::codecs::gif::GifEncoder::new(f);
    e.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
    e.encode_frames(gif.into_iter()).unwrap();
    println!("wrote vrm.gif + vrm_*.png — GPU-skinned humanoid mesh dancing the wota");
}
