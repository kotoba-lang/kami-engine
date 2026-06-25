//! Reusable VRM-dance renderer shared by the kami-live examples (included via
//! `#[path = "common/vrm.rs"] mod vrm;`). Consolidates what the per-feature
//! examples each re-implemented: load a real VRM (geometry + skin + textures +
//! morph targets + spring), pose it from a `DancePose` (FK + expression morph +
//! spring bones), and render it offscreen (GPU skinning + MToon + multi-light +
//! textures). The real-VRM half of ADR-0044, runnable headless.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use kami_vrm::vrm_types::{HumanBoneName, VrmDocument};
use std::collections::HashMap;

pub const MAX_LIGHTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct V { pub pos: [f32; 3], pub normal: [f32; 3], pub uv: [f32; 2], pub joints: [u32; 4], pub weights: [f32; 4] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuLight { pub dir: [f32; 4], pub color: [f32; 4] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals { pub vp: [[f32; 4]; 4], pub ambient: [f32; 4], pub n_lights: [u32; 4], pub lights: [GpuLight; MAX_LIGHTS] }

pub struct Item { pub img: Option<usize>, pub first: u32, pub count: u32 }

fn node_local(n: &kami_vrm::gltf_types::Node) -> Mat4 {
    if let Some(m) = n.matrix { return Mat4::from_cols_array(&m); }
    Mat4::from_scale_rotation_translation(
        n.scale.map(Vec3::from).unwrap_or(Vec3::ONE),
        n.rotation.map(Quat::from_array).unwrap_or(Quat::IDENTITY),
        n.translation.map(Vec3::from).unwrap_or(Vec3::ZERO))
}

/// A loaded VRM ready to be posed: rest geometry + skeleton + morph + spring.
pub struct VrmDance {
    pub doc: VrmDocument,
    pub verts: Vec<V>,
    pub indices: Vec<u32>,
    pub items: Vec<Item>,
    pub morph_prims: Vec<(usize, usize, usize, Vec<Vec<[f32; 3]>>)>,
    pub nn: usize,
    parent: Vec<i32>,
    order: Vec<usize>,
    inv_bind: Vec<Mat4>,
    base_local: Vec<Mat4>,
    pub hb: HashMap<HumanBoneName, usize>,
    pub spring: kami_vrm::spring::SpringSimulator,
    pub center: Vec3,
    pub height: f32,
}

impl VrmDance {
    pub fn load(bytes: &[u8]) -> Self {
        let doc = kami_vrm::parse_vrm(bytes).expect("parse VRM");
        let nn = doc.gltf.nodes.len();
        let mut parent = vec![-1i32; nn];
        for (i, n) in doc.gltf.nodes.iter().enumerate() { for &c in &n.children { parent[c] = i as i32; } }
        let mut order = Vec::new(); let mut seen = vec![false; nn];
        fn visit(i: usize, nd: &[kami_vrm::gltf_types::Node], s: &mut [bool], o: &mut Vec<usize>) {
            if s[i] { return; } s[i] = true; o.push(i);
            for &c in &nd[i].children { visit(c, nd, s, o); }
        }
        for i in 0..nn { if parent[i] < 0 { visit(i, &doc.gltf.nodes, &mut seen, &mut order); } }

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
        let hb = doc.humanoid.human_bones.iter().map(|b| (b.bone, b.node)).collect();

        let mut verts = Vec::new(); let mut indices = Vec::new();
        let mut items = Vec::new(); let mut morph_prims = Vec::new();
        for node in doc.gltf.nodes.iter() {
            let (Some(mi), Some(si)) = (node.mesh, node.skin) else { continue };
            let skin = &doc.gltf.skins[si];
            let mesh = &doc.gltf.meshes[mi];
            for pi in 0..mesh.primitives.len() {
                let Ok((inter, idx)) = kami_vrm::convert::extract_primitive_mesh(&doc, mi, pi) else { continue };
                let prim = &mesh.primitives[pi];
                let img = prim.material
                    .and_then(|m| doc.gltf.materials.get(m))
                    .and_then(|m| m.pbr_metallic_roughness.as_ref())
                    .and_then(|p| p.base_color_texture.as_ref())
                    .and_then(|t| doc.gltf.textures.get(t.index))
                    .and_then(|t| t.source);
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
                if !prim.targets.is_empty() {
                    let deltas = prim.targets.iter().map(|tgt| {
                        tgt.get("POSITION").and_then(|v| v.as_u64())
                            .and_then(|a| kami_vrm::convert::read_accessor_f32(&doc, a as usize).ok())
                            .map(|raw| raw.chunks_exact(3).map(|c| [c[0],c[1],c[2]]).collect::<Vec<_>>())
                            .unwrap_or_else(|| vec![[0.0;3]; vc])
                    }).collect();
                    morph_prims.push((base as usize, vc, mi, deltas));
                }
                let first = indices.len() as u32;
                indices.extend(idx.iter().map(|i| i + base));
                items.push(Item { img, first, count: idx.len() as u32 });
            }
        }
        let (mut lo, mut hi) = ([f32::MAX;3],[f32::MIN;3]);
        for v in &verts { for k in 0..3 { lo[k]=lo[k].min(v.pos[k]); hi[k]=hi[k].max(v.pos[k]); } }
        let center = Vec3::new((lo[0]+hi[0])/2.0,(lo[1]+hi[1])/2.0,(lo[2]+hi[2])/2.0);
        let height = hi[1]-lo[1];
        let base_local = doc.gltf.nodes.iter().map(node_local).collect();
        let spring = kami_vrm::spring::SpringSimulator::new(&doc);
        VrmDance { doc, verts, indices, items, morph_prims, nn, parent, order, inv_bind, base_local, hb, spring, center, height }
    }

    /// Pose for one frame: returns (morphed rest verts, joint palette). Drives
    /// humanoid bones from `pose`, expression morph from `expr_weights`
    /// (VRM-expression-name → intensity, e.g. from `AvatarBinding::expression_weights`),
    /// and (when `spring_enabled`) VRM spring bones.
    pub fn frame(&mut self, pose: &kami_live::DancePose, expr_weights: &std::collections::BTreeMap<String, f32>, spring_enabled: bool) -> (Vec<V>, Vec<[[f32; 4]; 4]>) {
        let nn = self.nn;
        // expression morph — resolve the EDN-driven weights against the VRM's own
        // expression names (case-insensitive, so "Happy"/"happy"/"A"/"aa" match).
        let mgr = kami_vrm::ExpressionManager::new(&self.doc.expressions);
        let mut ew = std::collections::BTreeMap::new();
        for e in &self.doc.expressions {
            let ln = e.name.to_lowercase();
            if let Some(&w) = expr_weights.get(&ln).or_else(|| expr_weights.iter().find(|(k, _)| k.to_lowercase() == ln).map(|(_, v)| v)) {
                if w > 0.0 { ew.insert(e.name.clone(), w); }
            }
        }
        let resolved = mgr.resolve(&ew);
        let mut mv = self.verts.clone();
        for (gstart, count, mi, deltas) in &self.morph_prims {
            for (t, dt) in deltas.iter().enumerate() {
                let w = resolved.morphs.get(&(*mi, t)).copied().unwrap_or(0.0);
                if w.abs() < 1e-4 { continue; }
                for v in 0..*count {
                    let d = dt[v];
                    mv[gstart+v].pos[0]+=w*d[0]; mv[gstart+v].pos[1]+=w*d[1]; mv[gstart+v].pos[2]+=w*d[2];
                }
            }
        }
        // humanoid posing
        let mut local = self.base_local.clone();
        let mut apply = |b: HumanBoneName, q: Quat| { if let Some(&n) = self.hb.get(&b) { local[n] = local[n]*Mat4::from_quat(q); } };
        apply(HumanBoneName::Spine, Quat::from_rotation_z(pose.spine_sway*0.5));
        apply(HumanBoneName::LeftUpperArm, Quat::from_rotation_z(-pose.arms_up*1.1));
        apply(HumanBoneName::RightUpperArm, Quat::from_rotation_z(pose.arms_up*1.1));
        apply(HumanBoneName::LeftUpperLeg, Quat::from_rotation_x(pose.vertical_bob*2.0));
        apply(HumanBoneName::RightUpperLeg, Quat::from_rotation_x(-pose.vertical_bob*2.0));
        if let Some(&hips) = self.hb.get(&HumanBoneName::Hips) {
            local[hips] = Mat4::from_translation(Vec3::new(pose.root_translation.x, pose.vertical_bob, 0.0)) * Mat4::from_rotation_y(pose.root_yaw) * local[hips];
        }
        let mut world = vec![Mat4::IDENTITY; nn];
        for &i in &self.order { world[i] = if self.parent[i]<0 { local[i] } else { world[self.parent[i] as usize]*local[i] }; }
        if spring_enabled {
            let mut sout: Vec<(usize, [f32; 4])> = Vec::new();
            self.spring.step(1.0/60.0, |n| world.get(n).copied(), &mut sout);
            for (node, q) in &sout {
                let t = self.doc.gltf.nodes[*node].translation.map(Vec3::from).unwrap_or(Vec3::ZERO);
                let sc = self.doc.gltf.nodes[*node].scale.map(Vec3::from).unwrap_or(Vec3::ONE);
                local[*node] = Mat4::from_scale_rotation_translation(sc, Quat::from_array(*q), t);
            }
            if !sout.is_empty() { for &i in &self.order { world[i] = if self.parent[i]<0 { local[i] } else { world[self.parent[i] as usize]*local[i] }; } }
        }
        let palette = (0..nn).map(|i| (world[i]*self.inv_bind[i]).to_cols_array_2d()).collect();
        (mv, palette)
    }
}

/// Offscreen wgpu renderer: GPU skinning + MToon + multi-light + textures.
pub struct GpuRenderer {
    pub device: wgpu::Device, pub queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline, bg0: wgpu::BindGroup, bgl1: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler, gbuf: wgpu::Buffer, pbuf: wgpu::Buffer, vbuf: wgpu::Buffer, ibuf: wgpu::Buffer,
    white: wgpu::BindGroup, tex_bg: HashMap<usize, wgpu::BindGroup>,
    color: wgpu::Texture, cview: wgpu::TextureView, dview: wgpu::TextureView, rbuf: wgpu::Buffer,
    pub w: u32, pub h: u32, index_count: u32, items: Vec<Item>,
}

impl GpuRenderer {
    pub async fn new(model: &VrmDance, w: u32, h: u32) -> Self {
        let inst = wgpu::Instance::default();
        let adapter = inst.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.unwrap();
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:std::mem::size_of_val(&model.verts[..]) as u64,usage:wgpu::BufferUsages::VERTEX|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        queue.write_buffer(&vbuf,0,bytemuck::cast_slice(&model.verts));
        let ibuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:std::mem::size_of_val(&model.indices[..]) as u64,usage:wgpu::BufferUsages::INDEX|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        queue.write_buffer(&ibuf,0,bytemuck::cast_slice(&model.indices));
        let gbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:std::mem::size_of::<Globals>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        let pbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:(model.nn*64) as u64,usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor{address_mode_u:wgpu::AddressMode::Repeat,address_mode_v:wgpu::AddressMode::Repeat,mag_filter:wgpu::FilterMode::Linear,min_filter:wgpu::FilterMode::Linear,..Default::default()});
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:&[
            wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::VERTEX_FRAGMENT,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
            wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::VERTEX,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None}]});
        let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:&[
            wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Texture{sample_type:wgpu::TextureSampleType::Float{filterable:true},view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
            wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),count:None}]});
        let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&bgl0,entries:&[
            wgpu::BindGroupEntry{binding:0,resource:gbuf.as_entire_binding()},
            wgpu::BindGroupEntry{binding:1,resource:pbuf.as_entire_binding()}]});
        let mk = |dev:&wgpu::Device,q:&wgpu::Queue,rgba:&[u8],iw:u32,ih:u32,bgl1:&wgpu::BindGroupLayout,sampler:&wgpu::Sampler| {
            let t = dev.create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width:iw,height:ih,depth_or_array_layers:1},mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::Rgba8UnormSrgb,usage:wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST,view_formats:&[]});
            q.write_texture(wgpu::ImageCopyTexture{texture:&t,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},rgba,wgpu::ImageDataLayout{offset:0,bytes_per_row:Some(iw*4),rows_per_image:Some(ih)},wgpu::Extent3d{width:iw,height:ih,depth_or_array_layers:1});
            let view=t.create_view(&Default::default());
            dev.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:bgl1,entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(&view)},wgpu::BindGroupEntry{binding:1,resource:wgpu::BindingResource::Sampler(sampler)}]})
        };
        let white = mk(&device,&queue,&[255,255,255,255],1,1,&bgl1,&sampler);
        let mut tex_bg = HashMap::new();
        for it in &model.items { if let Some(im)=it.img { if !tex_bg.contains_key(&im) {
            if let Some(img)=model.doc.gltf.images.get(im) { if let Some(bvi)=img.buffer_view {
                if let Some(bv)=model.doc.gltf.buffer_views.get(bvi) {
                    if let Some(bytes)=model.doc.bin.get(bv.byte_offset..bv.byte_offset+bv.byte_length) {
                        if let Ok(d)=image::load_from_memory(bytes) { let d=d.to_rgba8(); tex_bg.insert(im, mk(&device,&queue,&d,d.width(),d.height(),&bgl1,&sampler)); }
                    }
                }
            }}
        }}}
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
                let t = smoothstep(0.0, 0.08, ndl);
                let toon = mix(base.rgb*0.55, base.rgb, t);
                lit = lit + toon * g.lights[k].color.xyz * g.lights[k].color.w;
              }
              lit = lit + vec3<f32>(pow(1.0 - max(nn.z, 0.0), 3.0) * 0.25);
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
        let bpr=(w*4).div_ceil(256)*256;
        let rbuf = device.create_buffer(&wgpu::BufferDescriptor{label:None,size:(bpr*h) as u64,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
        let items = model.items.iter().map(|i| Item{img:i.img,first:i.first,count:i.count}).collect();
        let _ = dtex; // depth texture kept alive by dview's internal Arc
        Self{device,queue,pipeline,bg0,bgl1,sampler,gbuf,pbuf,vbuf,ibuf,white,tex_bg,color,cview,dview,rbuf,w,h,index_count:model.indices.len() as u32,items}
    }

    pub fn render(&self, morphed: &[V], palette: &[[[f32;4];4]], g: Globals) -> Vec<u8> {
        let _ = (&self.bgl1, &self.sampler);
        self.queue.write_buffer(&self.vbuf, 0, bytemuck::cast_slice(morphed));
        self.queue.write_buffer(&self.pbuf, 0, bytemuck::cast_slice(palette));
        self.queue.write_buffer(&self.gbuf, 0, bytemuck::bytes_of(&g));
        let (w,h)=(self.w,self.h);
        let bpr=(w*4).div_ceil(256)*256;
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor{label:None,
                color_attachments:&[Some(wgpu::RenderPassColorAttachment{view:&self.cview,resolve_target:None,ops:wgpu::Operations{load:wgpu::LoadOp::Clear(wgpu::Color{r:0.55,g:0.6,b:0.7,a:1.0}),store:wgpu::StoreOp::Store}})],
                depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment{view:&self.dview,depth_ops:Some(wgpu::Operations{load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
                timestamp_writes:None,occlusion_query_set:None});
            rp.set_pipeline(&self.pipeline); rp.set_bind_group(0,&self.bg0,&[]);
            rp.set_vertex_buffer(0,self.vbuf.slice(..)); rp.set_index_buffer(self.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            let _ = self.index_count;
            for it in &self.items {
                let bg = it.img.and_then(|im| self.tex_bg.get(&im)).unwrap_or(&self.white);
                rp.set_bind_group(1, bg, &[]);
                rp.draw_indexed(it.first..it.first+it.count, 0, 0..1);
            }
        }
        enc.copy_texture_to_buffer(wgpu::ImageCopyTexture{texture:&self.color, mip_level:0, origin:wgpu::Origin3d::ZERO, aspect:wgpu::TextureAspect::All}, wgpu::ImageCopyBuffer{buffer:&self.rbuf,layout:wgpu::ImageDataLayout{offset:0,bytes_per_row:Some(bpr),rows_per_image:Some(h)}}, wgpu::Extent3d{width:w,height:h,depth_or_array_layers:1});
        self.queue.submit([enc.finish()]);
        let sl=self.rbuf.slice(..); sl.map_async(wgpu::MapMode::Read,|_|{}); self.device.poll(wgpu::Maintain::Wait);
        let data=sl.get_mapped_range();
        let mut px=vec![0u8;(w*h*4) as usize];
        for y in 0..h { let s=(y*bpr) as usize; let d=(y*w*4) as usize; px[d..d+(w*4) as usize].copy_from_slice(&data[s..s+(w*4) as usize]); }
        drop(data); self.rbuf.unmap();
        px
    }
}
