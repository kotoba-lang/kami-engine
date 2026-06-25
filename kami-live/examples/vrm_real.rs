//! Load a REAL VRM (Seed-san, VRM Public License 1.0) and render its actual
//! geometry offscreen — proving kami-vrm geometry → GPU. Static (no skinning yet).
//! `cargo run -p kami-live --example vrm_real --target aarch64-apple-darwin`

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct V { pos: [f32; 3], normal: [f32; 3] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct G { vp: [[f32; 4]; 4], light: [f32; 4] }

fn main() { pollster::block_on(run()); }

async fn run() {
    let bytes = std::fs::read("assets/Seed-san.vrm").expect("assets/Seed-san.vrm — download first");
    let doc = kami_vrm::parse_vrm(&bytes).expect("parse VRM");
    println!("VRM: {} meshes, {} nodes, {} materials", doc.gltf.meshes.len(), doc.gltf.nodes.len(), doc.gltf.materials.len());

    // concatenate every primitive's geometry into one mesh.
    let mut verts: Vec<V> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for (mi, mesh) in doc.gltf.meshes.iter().enumerate() {
        for pi in 0..mesh.primitives.len() {
            if let Ok((interleaved, idx)) = kami_vrm::convert::extract_primitive_mesh(&doc, mi, pi) {
                let base = verts.len() as u32;
                for v in interleaved.chunks_exact(8) {
                    verts.push(V { pos: [v[0], v[1], v[2]], normal: [v[3], v[4], v[5]] });
                }
                indices.extend(idx.iter().map(|i| i + base));
            }
        }
    }
    println!("geometry: {} verts, {} tris", verts.len(), indices.len() / 3);

    // bounds → frame the model.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &verts { for k in 0..3 { lo[k] = lo[k].min(v.pos[k]); hi[k] = hi[k].max(v.pos[k]); } }
    let center = Vec3::new((lo[0]+hi[0])/2.0, (lo[1]+hi[1])/2.0, (lo[2]+hi[2])/2.0);
    let height = hi[1] - lo[1];

    let (w, h) = (420u32, 620u32);
    let inst = wgpu::Instance::default();
    let adapter = inst.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.expect("device");

    let vbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of_val(&verts[..]) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&verts));
    let ibuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of_val(&indices[..]) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    queue.write_buffer(&ibuf, 0, bytemuck::cast_slice(&indices));
    let gbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<G>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }] });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() }] });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[] });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(r#"
        struct G { vp: mat4x4<f32>, light: vec4<f32> };
        @group(0) @binding(0) var<uniform> g: G;
        struct VO { @builtin(position) clip: vec4<f32>, @location(0) n: vec3<f32> };
        @vertex fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>) -> VO {
          var o: VO; o.clip = g.vp * vec4<f32>(p, 1.0); o.n = n; return o;
        }
        @fragment fn fs(i: VO) -> @location(0) vec4<f32> {
          let d = max(dot(normalize(i.n), -normalize(g.light.xyz)), 0.0);
          let c = vec3<f32>(0.78, 0.68, 0.64) * (0.28 + 0.62 * d);
          return vec4<f32>(pow(c, vec3<f32>(1.0/2.2)), 1.0);
        }
    "#.into()) });
    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let vbl = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<V>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x3] };
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

    // front view (VRM faces -Z): camera on -Z looking at the model centre.
    let dist = height * 1.6;
    let eye = center + Vec3::new(0.0, height * 0.05, dist);
    let vp = Mat4::perspective_rh(0.7, w as f32 / h as f32, 0.05, 100.0) * Mat4::look_at_rh(eye, center, Vec3::Y);
    queue.write_buffer(&gbuf, 0, bytemuck::bytes_of(&G { vp: vp.to_cols_array_2d(), light: [-0.3, -0.5, -0.75, 0.0] }));

    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor { label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &cview, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.55, g: 0.6, b: 0.7, a: 1.0 }), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dview, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None, occlusion_query_set: None });
        rp.set_pipeline(&pipeline); rp.set_bind_group(0, &bg, &[]);
        rp.set_vertex_buffer(0, vbuf.slice(..)); rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
        rp.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
    let bpr = (w * 4).div_ceil(256) * 256;
    let rbuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(wgpu::ImageCopyTexture { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::ImageCopyBuffer { buffer: &rbuf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    queue.submit([enc.finish()]);
    let slice = rbuf.slice(..); slice.map_async(wgpu::MapMode::Read, |_| {}); device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    let mut px = vec![0u8; (w*h*4) as usize];
    for y in 0..h { let s=(y*bpr) as usize; let d=(y*w*4) as usize; px[d..d+(w*4) as usize].copy_from_slice(&data[s..s+(w*4) as usize]); }
    image::save_buffer("vrm_real.png", &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
    println!("wrote vrm_real.png — the real Seed-san VRM geometry rendered");
}
