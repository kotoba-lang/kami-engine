//! Seed-san (real VRM): TEXTURED + MToon toon-shading + render-IR multi-light +
//! GPU-skinned + danced by the clj/edn show. `--example vrm_full`
//! VRM baseColor textures decoded from the GLB, sampled per material; skinning
//! and humanoid posing as in vrm_dance. The real character with face + colours.
//! `cargo run -p kami-live --example vrm_textured --target aarch64-apple-darwin`

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use kami_live::scene::DanceScene;
use kami_vrm::vrm_types::HumanBoneName;
use std::collections::HashMap;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct V { pos: [f32; 3], normal: [f32; 3], uv: [f32; 2], joints: [u32; 4], weights: [f32; 4] }
const MAX_LIGHTS: usize = 8;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLight { dir: [f32; 4], color: [f32; 4] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct G { vp: [[f32; 4]; 4], ambient: [f32; 4], n_lights: [u32; 4], lights: [GpuLight; MAX_LIGHTS] }

struct Item { img: Option<usize>, first: u32, count: u32 }

fn node_local(n: &kami_vrm::gltf_types::Node) -> Mat4 {
    if let Some(m) = n.matrix { return Mat4::from_cols_array(&m); }
    Mat4::from_scale_rotation_translation(
        n.scale.map(Vec3::from).unwrap_or(Vec3::ONE),
        n.rotation.map(Quat::from_array).unwrap_or(Quat::IDENTITY),
        n.translation.map(Vec3::from).unwrap_or(Vec3::ZERO))
}

fn prim_image(doc: &kami_vrm::vrm_types::VrmDocument, prim: &kami_vrm::gltf_types::Primitive) -> Option<usize> {
    let m = doc.gltf.materials.get(prim.material?)?;
    let ti = m.pbr_metallic_roughness.as_ref()?.base_color_texture.as_ref()?.index;
    doc.gltf.textures.get(ti)?.source
}

fn image_rgba(doc: &kami_vrm::vrm_types::VrmDocument, img: usize) -> Option<(u32, u32, Vec<u8>)> {
    let im = doc.gltf.images.get(img)?;
    let bv = doc.gltf.buffer_views.get(im.buffer_view?)?;
    let bytes = doc.bin.get(bv.byte_offset..bv.byte_offset + bv.byte_length)?;
    let d = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some((d.width(), d.height(), d.into_raw()))
}

fn main() { pollster::block_on(run()); }

async fn run() {
    let bytes = std::fs::read("assets/Seed-san.vrm").expect("assets/Seed-san.vrm");
    let doc = kami_vrm::parse_vrm(&bytes).expect("parse VRM");
    let nodes = &doc.gltf.nodes;
    let nn = nodes.len();

    let mut parent = vec![-1i32; nn];
    for (i, n) in nodes.iter().enumerate() { for &c in &n.children { parent[c] = i as i32; } }
    let mut order = Vec::new(); let mut seen = vec![false; nn];
    fn visit(i: usize, nd: &[kami_vrm::gltf_types::Node], s: &mut [bool], o: &mut Vec<usize>) {
        if s[i] { return; } s[i] = true; o.push(i);
        for &c in &nd[i].children { visit(c, nd, s, o); }
    }
    for i in 0..nn { if parent[i] < 0 { visit(i, nodes, &mut seen, &mut order); } }

    let mut inv_bind = vec![Mat4::IDENTITY; nn];
    for skin in &doc.gltf.skins {
        if let Some(ibm) = skin.inverse_bind_matrices {
            if let Ok(flat) = kami_vrm::convert::read_accessor_f32(&doc, ibm) {
                for (j, &node) in skin.joints.iter().enumerate() {
                    if (j + 1) * 16 <= flat.len() {
                        let mut m = [0.0f32; 16]; m.copy_from_slice(&flat[j*16..j*16+16]);
                        inv_bind[node] = Mat4::from_cols_array(&m);
                    }
                }
            }
        }
    }
    let hb: HashMap<HumanBoneName, usize> = doc.humanoid.human_bones.iter().map(|b| (b.bone, b.node)).collect();

    let mut verts: Vec<V> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    for node in nodes.iter() {
        let (Some(mi), Some(si)) = (node.mesh, node.skin) else { continue };
        let skin = &doc.gltf.skins[si];
        let mesh = &doc.gltf.meshes[mi];
        for pi in 0..mesh.primitives.len() {
            let Ok((inter, idx)) = kami_vrm::convert::extract_primitive_mesh(&doc, mi, pi) else { continue };
            let prim = &mesh.primitives[pi];
            let img = prim_image(&doc, prim);
            let jd = prim.attributes.get("JOINTS_0").and_then(|v| v.as_u64()).and_then(|a| kami_vrm::convert::read_accessor_f32(&doc, a as usize).ok());
            let wd = prim.attributes.get("WEIGHTS_0").and_then(|v| v.as_u64()).and_then(|a| kami_vrm::convert::read_accessor_f32(&doc, a as usize).ok());
            let base = verts.len() as u32;
            let vc = inter.len() / 8;
            for v in 0..vc {
                let mut j = [0u32; 4]; let mut w = [0.0f32; 4];
                if let (Some(jj), Some(ww)) = (&jd, &wd) {
                    for k in 0..4 { j[k] = *skin.joints.get(jj[v*4+k] as usize).unwrap_or(&0) as u32; w[k] = ww[v*4+k]; }
                    let s: f32 = w.iter().sum(); if s > 0.0 { for x in &mut w { *x /= s; } } else { w[0] = 1.0; }
                } else { w[0] = 1.0; }
                verts.push(V { pos: [inter[v*8], inter[v*8+1], inter[v*8+2]], normal: [inter[v*8+3], inter[v*8+4], inter[v*8+5]], uv: [inter[v*8+6], inter[v*8+7]], joints: j, weights: w });
            }
            let first = indices.len() as u32;
            indices.extend(idx.iter().map(|i| i + base));
            items.push(Item { img, first, count: idx.len() as u32 });
        }
    }
    println!("Seed-san: {} verts, {} tris, {} draw-items, {} images", verts.len(), indices.len()/3, items.len(), doc.gltf.images.len());

    let (mut lo, mut hi) = ([f32::MAX;3],[f32::MIN;3]);
    for v in &verts { for k in 0..3 { lo[k]=lo[k].min(v.pos[k]); hi[k]=hi[k].max(v.pos[k]); } }
    let center = Vec3::new((lo[0]+hi[0])/2.0,(lo[1]+hi[1])/2.0,(lo[2]+hi[2])/2.0);
    let height = hi[1]-lo[1];

    let (w,h)=(420u32,620u32);
    let inst = wgpu::Instance::default();
    let adapter = inst.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
    let (device,queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.unwrap();
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:std::mem::size_of_val(&verts[..]) as u64,usage:wgpu::BufferUsages::VERTEX|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
    queue.write_buffer(&vbuf,0,bytemuck::cast_slice(&verts));
    let ibuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:std::mem::size_of_val(&indices[..]) as u64,usage:wgpu::BufferUsages::INDEX|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
    queue.write_buffer(&ibuf,0,bytemuck::cast_slice(&indices));
    let gbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:std::mem::size_of::<G>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
    let pbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:(nn*64) as u64,usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor{address_mode_u:wgpu::AddressMode::Repeat,address_mode_v:wgpu::AddressMode::Repeat,mag_filter:wgpu::FilterMode::Linear,min_filter:wgpu::FilterMode::Linear,..Default::default()});

    let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:&[
        wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::VERTEX_FRAGMENT,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
        wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::VERTEX,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None},
    ]});
    let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:&[
        wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Texture{sample_type:wgpu::TextureSampleType::Float{filterable:true},view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
        wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),count:None},
    ]});
    let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&bgl0,entries:&[
        wgpu::BindGroupEntry{binding:0,resource:gbuf.as_entire_binding()},
        wgpu::BindGroupEntry{binding:1,resource:pbuf.as_entire_binding()},
    ]});

    // decode each image used → texture → bind group; white fallback.
    let make_bg = |dev: &wgpu::Device, q: &wgpu::Queue, rgba: &[u8], iw: u32, ih: u32| {
        let t = dev.create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width:iw,height:ih,depth_or_array_layers:1},mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::Rgba8UnormSrgb,usage:wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST,view_formats:&[]});
        q.write_texture(wgpu::ImageCopyTexture{texture:&t,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All}, rgba, wgpu::ImageDataLayout{offset:0,bytes_per_row:Some(iw*4),rows_per_image:Some(ih)}, wgpu::Extent3d{width:iw,height:ih,depth_or_array_layers:1});
        let view = t.create_view(&Default::default());
        dev.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&bgl1,entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(&view)},wgpu::BindGroupEntry{binding:1,resource:wgpu::BindingResource::Sampler(&sampler)}]})
    };
    let white = make_bg(&device,&queue,&[255,255,255,255],1,1);
    let mut tex_bg: HashMap<usize, wgpu::BindGroup> = HashMap::new();
    for it in &items { if let Some(im) = it.img { if !tex_bg.contains_key(&im) { if let Some((iw,ih,px)) = image_rgba(&doc, im) { tex_bg.insert(im, make_bg(&device,&queue,&px,iw,ih)); } } } }
    println!("decoded {} textures", tex_bg.len());

    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:None,bind_group_layouts:&[&bgl0,&bgl1],push_constant_ranges:&[]});
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor{label:None,source:wgpu::ShaderSource::Wgsl(r#"
        struct L { dir: vec4<f32>, color: vec4<f32> };
        struct G { vp: mat4x4<f32>, ambient: vec4<f32>, n: vec4<u32>, lights: array<L, 8u> };
        @group(0) @binding(0) var<uniform> g: G;
        @group(0) @binding(1) var<storage, read> palette: array<mat4x4<f32>>;
        @group(1) @binding(0) var tex: texture_2d<f32>;
        @group(1) @binding(1) var samp: sampler;
        struct VO { @builtin(position) clip: vec4<f32>, @location(0) n: vec3<f32>, @location(1) uv: vec2<f32> };
        @vertex fn vs(@location(0) p: vec3<f32>, @location(1) nor: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) j: vec4<u32>, @location(4) wt: vec4<f32>) -> VO {
          let skin = palette[j.x]*wt.x + palette[j.y]*wt.y + palette[j.z]*wt.z + palette[j.w]*wt.w;
          var o: VO; o.clip = g.vp*(skin*vec4<f32>(p,1.0)); o.n = normalize((skin*vec4<f32>(nor,0.0)).xyz); o.uv = uv; return o;
        }
        @fragment fn fs(i: VO) -> @location(0) vec4<f32> {
          let base = textureSample(tex, samp, i.uv);
          if (base.a < 0.5) { discard; }
          let nn = normalize(i.n);
          var lit = g.ambient.xyz * base.rgb;
          let count = min(g.n.x, 8u);
          for (var k: u32 = 0u; k < count; k = k + 1u) {
            let ndl = dot(nn, -normalize(g.lights[k].dir.xyz));
            // MToon two-tone: a narrow toon ramp shade->lit.
            let t = smoothstep(0.0, 0.08, ndl);
            let shade = base.rgb * 0.55;
            let toon = mix(shade, base.rgb, t);
            lit = lit + toon * g.lights[k].color.xyz * g.lights[k].color.w;
          }
          // rim light (fresnel) for the anime edge glow.
          let rim = pow(1.0 - max(nn.z, 0.0), 3.0) * 0.25;
          lit = lit + vec3<f32>(rim);
          let mapped = lit / (lit + vec3<f32>(1.0));
          return vec4<f32>(pow(mapped, vec3<f32>(1.0/2.2)), 1.0);
        }
    "#.into())});
    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let vbl = wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<V>() as u64,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Uint32x4,4=>Float32x4]};
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:None,layout:Some(&pl),
        vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),buffers:&[vbl],compilation_options:Default::default()},
        fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),targets:&[Some(fmt.into())],compilation_options:Default::default()}),
        primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},
        depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),
        multisample:Default::default(),multiview:None,cache:None});
    let color = device.create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width:w,height:h,depth_or_array_layers:1},mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:fmt,usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::COPY_SRC,view_formats:&[]});
    let cview = color.create_view(&Default::default());
    let dtex = device.create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width:w,height:h,depth_or_array_layers:1},mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::Depth32Float,usage:wgpu::TextureUsages::RENDER_ATTACHMENT,view_formats:&[]});
    let dview = dtex.create_view(&Default::default());
    let dist = height*1.5;
    let eye = center + Vec3::new(0.0, height*0.02, dist);
    let vp = (Mat4::perspective_rh(0.7, w as f32/h as f32, 0.05, 100.0) * Mat4::look_at_rh(eye, center, Vec3::Y)).to_cols_array_2d();

    let mut scene = DanceScene::from_edn(SCENE).unwrap();
    scene.show.start();
    for _ in 0..(61.0*60.0) as i32 { scene.frame(1.0/60.0); }
    let base_local: Vec<Mat4> = nodes.iter().map(node_local).collect();
    let bpr=(w*4).div_ceil(256)*256;
    let rbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:(bpr*h) as u64,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    let mut gif=Vec::new();
    for frame in 0..32 {
        for _ in 0..2 { scene.frame(1.0/60.0); }
        let fr = scene.frame(1.0/60.0);
        let ir = kami_webgpu_rs::parse_render_ir(&fr.render_ir_edn());
        let mut lights = [GpuLight { dir: [0.0;4], color: [0.0;4] }; MAX_LIGHTS];
        let nl = ir.lights.len().min(MAX_LIGHTS);
        for (k, l) in ir.lights.iter().take(MAX_LIGHTS).enumerate() {
            lights[k] = GpuLight { dir: [l.dir[0], l.dir[1], l.dir[2], 0.0], color: [l.color[0], l.color[1], l.color[2], l.intensity.max(0.3)] };
        }
        let n_used = if nl == 0 { lights[0] = GpuLight { dir: [-0.3,-0.5,-0.75,0.0], color: [1.0,0.96,0.85,1.0] }; 1 } else { nl };
        let amb = ir.env.ambient;
        queue.write_buffer(&gbuf, 0, bytemuck::bytes_of(&G { vp, ambient: [amb[0]*0.45, amb[1]*0.45, amb[2]*0.5, 1.0], n_lights: [n_used as u32,0,0,0], lights }));
        let pose = scene.show.snapshot().performer_pose;
        let mut local = base_local.clone();
        let mut apply = |b: HumanBoneName, q: Quat| { if let Some(&n) = hb.get(&b) { local[n] = local[n]*Mat4::from_quat(q); } };
        apply(HumanBoneName::Spine, Quat::from_rotation_z(pose.spine_sway*0.5));
        apply(HumanBoneName::LeftUpperArm, Quat::from_rotation_z(-pose.arms_up*1.1));
        apply(HumanBoneName::RightUpperArm, Quat::from_rotation_z(pose.arms_up*1.1));
        apply(HumanBoneName::LeftUpperLeg, Quat::from_rotation_x(pose.vertical_bob*2.0));
        apply(HumanBoneName::RightUpperLeg, Quat::from_rotation_x(-pose.vertical_bob*2.0));
        if let Some(&hips)=hb.get(&HumanBoneName::Hips) { local[hips]=Mat4::from_translation(Vec3::new(pose.root_translation.x,pose.vertical_bob,0.0))*Mat4::from_rotation_y(pose.root_yaw)*local[hips]; }
        let mut world = vec![Mat4::IDENTITY; nn];
        for &i in &order { world[i] = if parent[i]<0 { local[i] } else { world[parent[i] as usize]*local[i] }; }
        let palette: Vec<[[f32;4];4]> = (0..nn).map(|i| (world[i]*inv_bind[i]).to_cols_array_2d()).collect();
        queue.write_buffer(&pbuf,0,bytemuck::cast_slice(&palette));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor{label:None,
                color_attachments:&[Some(wgpu::RenderPassColorAttachment{view:&cview,resolve_target:None,ops:wgpu::Operations{load:wgpu::LoadOp::Clear(wgpu::Color{r:0.55,g:0.6,b:0.7,a:1.0}),store:wgpu::StoreOp::Store}})],
                depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment{view:&dview,depth_ops:Some(wgpu::Operations{load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
                timestamp_writes:None,occlusion_query_set:None});
            rp.set_pipeline(&pipeline); rp.set_bind_group(0,&bg0,&[]);
            rp.set_vertex_buffer(0,vbuf.slice(..)); rp.set_index_buffer(ibuf.slice(..),wgpu::IndexFormat::Uint32);
            for it in &items {
                let bg = it.img.and_then(|im| tex_bg.get(&im)).unwrap_or(&white);
                rp.set_bind_group(1, bg, &[]);
                rp.draw_indexed(it.first..it.first+it.count, 0, 0..1);
            }
        }
        enc.copy_texture_to_buffer(wgpu::ImageCopyTexture{texture:&color,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},wgpu::ImageCopyBuffer{buffer:&rbuf,layout:wgpu::ImageDataLayout{offset:0,bytes_per_row:Some(bpr),rows_per_image:Some(h)}},wgpu::Extent3d{width:w,height:h,depth_or_array_layers:1});
        queue.submit([enc.finish()]);
        let sl=rbuf.slice(..); sl.map_async(wgpu::MapMode::Read,|_|{}); device.poll(wgpu::Maintain::Wait);
        let data=sl.get_mapped_range();
        let mut px=vec![0u8;(w*h*4) as usize];
        for y in 0..h { let s=(y*bpr) as usize; let d=(y*w*4) as usize; px[d..d+(w*4) as usize].copy_from_slice(&data[s..s+(w*4) as usize]); }
        drop(data); rbuf.unmap();
        if frame%8==0 { image::save_buffer(format!("seedfull_{frame:02}.png"),&px,w,h,image::ExtendedColorType::Rgba8).unwrap(); }
        gif.push(image::Frame::from_parts(image::RgbaImage::from_raw(w,h,px).unwrap(),0,0,image::Delay::from_numer_denom_ms(70,1)));
    }
    let fl=std::fs::File::create("seed_full.gif").unwrap();
    let mut e=image::codecs::gif::GifEncoder::new(fl); e.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
    e.encode_frames(gif.into_iter()).unwrap();
    println!("wrote seed_full.gif + seedtex_*.png — Seed-san: MToon + multi-light + textured + skinned + dancing");
}
