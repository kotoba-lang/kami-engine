//! Scene pipelines for open-world demos (quarry-walk / terrain-demo / quarry).
//!
//! Each pipeline owns its WGSL module, render pipeline, bind group layout,
//! and uniform buffer. Reusable across demos — shader source is `include_str!`'d.
//!
//! Uniform layouts are authoritative here (not in each demo's JS).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

// ── EDN pipeline specs → wgpu (the single-source cull/depth fields) ──
// The generated `pipeline_specs::PIPELINE_SPECS` (from kami.pipelines EDN) is the authoritative
// source for each pipeline's cull / depth-write / depth-compare. These map a spec to the wgpu
// values the pipelines below build with; `specs_map_to_the_renderer_values` locks every pipeline's
// mapped value to what the hand-written descriptors use, so the EDN and the renderer can't diverge.
use crate::pipeline_specs::{PIPELINE_SPECS, PipelineSpec, Cull};

/// Look up a generated pipeline spec by name.
pub fn spec(name: &str) -> &'static PipelineSpec {
    PIPELINE_SPECS.iter().find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no pipeline spec for {name}"))
}

/// Map a spec `Cull` to wgpu's optional face cull (`None` = no culling).
pub fn wgpu_cull(c: Cull) -> Option<wgpu::Face> {
    match c {
        Cull::Back => Some(wgpu::Face::Back),
        Cull::Front => Some(wgpu::Face::Front),
        Cull::None => None,
    }
}

/// Map a spec depth-compare string to wgpu's `CompareFunction`.
pub fn wgpu_compare(s: &str) -> wgpu::CompareFunction {
    match s {
        "less"          => wgpu::CompareFunction::Less,
        "less-equal"    => wgpu::CompareFunction::LessEqual,
        "greater"       => wgpu::CompareFunction::Greater,
        "greater-equal" => wgpu::CompareFunction::GreaterEqual,
        "equal"         => wgpu::CompareFunction::Equal,
        "always"        => wgpu::CompareFunction::Always,
        "never"         => wgpu::CompareFunction::Never,
        _               => wgpu::CompareFunction::Less,
    }
}

#[cfg(test)]
mod spec_consumption_tests {
    use super::{spec, wgpu_cull, wgpu_compare};

    #[test]
    fn specs_map_to_the_renderer_values() {
        // (name, cull, depth_write, depth_compare) the hand-written pipelines use — the EDN spec,
        // mapped through the helpers, must reproduce each one exactly (a Rust-side oracle complementing
        // the bb parse-rust parity gate). Wiring the descriptors to read these is then provably safe.
        use wgpu::{Face, CompareFunction as CF};
        let expect: &[(&str, Option<Face>, bool, CF)] = &[
            ("terrain",    Some(Face::Back), true,  CF::Less),
            ("sky",        None,             false, CF::LessEqual),
            ("vegetation", None,             true,  CF::Less),
            ("character",  Some(Face::Back), true,  CF::Less),
            ("water",      None,             false, CF::Less),
            ("voxel",      Some(Face::Back), true,  CF::Less),
            ("particle",   None,             false, CF::Less),
            ("atlas",      None,             false, CF::Less),
        ];
        for &(name, cull, dw, cmp) in expect {
            let s = spec(name);
            assert_eq!(wgpu_cull(s.cull), cull, "{name} cull");
            assert_eq!(s.depth_write, dw, "{name} depth_write");
            assert_eq!(wgpu_compare(s.depth_compare), cmp, "{name} depth_compare");
        }
    }
}

// ── Uniform layouts (match WGSL structs byte-for-byte) ──

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct TerrainUniform {
    pub view_proj: [f32; 16],
    pub cam_pos: [f32; 3], pub _p0: f32,
    pub sun_dir: [f32; 3], pub _p1: f32,
    pub sun_color: [f32; 3], pub fog_density: f32,
    pub fog_color: [f32; 3], pub _p2: f32,
    /// 4 materials × vec4 (base RGB + pad)
    pub base_col: [[f32; 4]; 4],
    pub tip_col: [[f32; 4]; 4],
}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SkyUniform {
    pub inv_vp: [f32; 16],
    pub cam_pos: [f32; 3], pub _p0: f32,
    pub sun_dir: [f32; 3], pub _p1: f32,
    pub fog_color: [f32; 3], pub overcast: f32,
    pub scroll_x: f32, pub scroll_z: f32, pub altitude: f32, pub _p2: f32,
}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VegetationUniform {
    pub view_proj: [f32; 16],
    pub cam_pos: [f32; 3], pub time: f32,
    pub sun_dir: [f32; 3], pub wind_speed: f32,
    pub fog_color: [f32; 3], pub fog_density: f32,
    pub wind_dir: [f32; 2], pub gust_mul: f32, pub biome_dry: f32,
}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CharacterUniform {
    pub view_proj: [f32; 16],
    pub model: [f32; 16],
    pub cam_pos: [f32; 3], pub _p0: f32,
    pub sun_dir: [f32; 3], pub _p1: f32,
    pub sun_color: [f32; 3], pub fog_density: f32,
    pub fog_color: [f32; 3], pub _p2: f32,
}

// ── Helper: create uniform buffer + bind group + layout ──

fn make_uniform_bind(
    device: &wgpu::Device,
    size: u64,
    label: &str,
) -> (wgpu::Buffer, wgpu::BindGroupLayout, wgpu::BindGroup) {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, layout, bind)
}

// ══════════════════════════════════════════════════════════
// Terrain pipeline
// ══════════════════════════════════════════════════════════

pub struct TerrainPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl TerrainPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<TerrainUniform>() as u64, "terrain_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_terrain.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 48,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 1, offset: 12, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 2, offset: 24, format: wgpu::VertexFormat::Float32x2 },
                        wgpu::VertexAttribute { shader_location: 3, offset: 32, format: wgpu::VertexFormat::Float32x4 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: color_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group }
    }
}

// ══════════════════════════════════════════════════════════
// Sky pipeline (fullscreen triangle)
// ══════════════════════════════════════════════════════════

pub struct SkyPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl SkyPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<SkyUniform>() as u64, "sky_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_sky.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: color_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group }
    }
}

// ══════════════════════════════════════════════════════════
// Vegetation pipeline (instanced)
// ══════════════════════════════════════════════════════════

pub struct VegetationPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    /// Legacy cross-quad (used when no per-species meshes uploaded).
    pub quad_vb: wgpu::Buffer,
    pub instance_vb: wgpu::Buffer,
    pub instance_capacity: u32,
    /// Per-species vertex + index buffers (5 entries, indexed by SpeciesId u32).
    /// Populated via `upload_species_meshes()`.
    pub species_meshes: Vec<SpeciesMeshGpu>,
}

pub struct SpeciesMeshGpu {
    pub vb: wgpu::Buffer,
    pub ib: wgpu::Buffer,
    pub index_count: u32,
}

impl VegetationPipeline {
    /// Upload the 5 species meshes to GPU. Call once after `new`.
    pub fn upload_species_meshes(
        &mut self,
        device: &wgpu::Device,
        meshes: &[(u32, Vec<f32>, Vec<u32>)],  // (species_id, vertices flat, indices)
    ) {
        self.species_meshes = meshes.iter().map(|(_, verts, idxs)| {
            let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("veg_species_vb"),
                contents: bytemuck::cast_slice(verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("veg_species_ib"),
                contents: bytemuck::cast_slice(idxs),
                usage: wgpu::BufferUsages::INDEX,
            });
            SpeciesMeshGpu { vb, ib, index_count: idxs.len() as u32 }
        }).collect();
    }
}

impl VegetationPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat, instance_capacity: u32) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<VegetationUniform>() as u64, "veg_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("veg_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_vegetation.wgsl").into()),
        });
        // Cross-quad vertex data (2 quads perpendicular for billboard-like volume)
        let quad: [f32; 60] = [
            -0.5,0.0,0.0, 0.0,1.0,  0.5,0.0,0.0, 1.0,1.0,  0.5,1.0,0.0, 1.0,0.0,
            -0.5,0.0,0.0, 0.0,1.0,  0.5,1.0,0.0, 1.0,0.0, -0.5,1.0,0.0, 0.0,0.0,
             0.0,0.0,-0.5, 0.0,1.0,  0.0,0.0,0.5, 1.0,1.0,  0.0,1.0,0.5, 1.0,0.0,
             0.0,0.0,-0.5, 0.0,1.0,  0.0,1.0,0.5, 1.0,0.0,  0.0,1.0,-0.5, 0.0,0.0,
        ];
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("veg_quad"),
            contents: bytemuck::cast_slice(&quad),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("veg_instances"),
            size: (instance_capacity * 32) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("veg_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("veg_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 20, step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { shader_location: 1, offset: 12, format: wgpu::VertexFormat::Float32x2 },
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: 32, step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 2, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { shader_location: 3, offset: 12, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 4, offset: 16, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 5, offset: 20, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 6, offset: 24, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 7, offset: 28, format: wgpu::VertexFormat::Float32 },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
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
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group, quad_vb, instance_vb, instance_capacity, species_meshes: Vec::new() }
    }
}

// ══════════════════════════════════════════════════════════
// Character pipeline (procedural humanoid)
// ══════════════════════════════════════════════════════════

pub struct CharacterPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl CharacterPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<CharacterUniform>() as u64, "char_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("char_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_character.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("char_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("char_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 36, step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 1, offset: 12, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 2, offset: 24, format: wgpu::VertexFormat::Float32x3 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: color_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group }
    }
}

// ══════════════════════════════════════════════════════════
// Water pipeline (flat plane, alpha-blended, time-driven ripple)
// ══════════════════════════════════════════════════════════

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct WaterUniform {
    pub view_proj: [f32; 16],
    pub cam_pos: [f32; 3],
    pub time: f32,
    pub sun_dir: [f32; 3],
    pub water_y: f32,
    pub fog_color: [f32; 3],
    pub _p0: f32,
    pub base_col: [f32; 3],
    pub _p1: f32,
}

pub struct WaterPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl WaterPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<WaterUniform>() as u64, "water_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_water.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 1, offset: 12, format: wgpu::VertexFormat::Float32x2 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,  // alpha blend — let terrain show through
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group }
    }
}

// ══════════════════════════════════════════════════════════
// Voxel chunk pipeline (blocky, flat-shaded per-face color)
// ══════════════════════════════════════════════════════════

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VoxelUniform {
    pub view_proj: [f32; 16],
    pub cam_pos: [f32; 3],
    pub _p0: f32,
    pub sun_dir: [f32; 3],
    pub _p1: f32,
    pub sun_color: [f32; 3],
    pub fog_density: f32,
    pub fog_color: [f32; 3],
    pub _p2: f32,
}

pub struct VoxelPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl VoxelPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<VoxelUniform>() as u64, "voxel_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_voxel.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                // pos3 + norm3 + col3 = 36 B per vertex
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 36,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 1, offset: 12, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { shader_location: 2, offset: 24, format: wgpu::VertexFormat::Float32x3 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group }
    }
}

// ══════════════════════════════════════════════════════════
// Particle pipeline (billboard quads, alpha-blended)
// ══════════════════════════════════════════════════════════

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ParticleUniform {
    pub view_proj: [f32; 16],
    pub cam_right: [f32; 3], pub _p0: f32,
    pub cam_up:    [f32; 3], pub _p1: f32,
}

pub struct ParticlePipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    /// Unit quad shared across all draws.
    pub quad_vb: wgpu::Buffer,
    pub quad_ib: wgpu::Buffer,
}

impl ParticlePipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<ParticleUniform>() as u64, "particle_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_particle.wgsl").into()),
        });
        // Unit quad: pos2 (4 vertices), indexed as 2 triangles.
        let quad: [f32; 8] = [-0.5, -0.5,  0.5, -0.5,  0.5, 0.5,  -0.5, 0.5];
        let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle_quad_vb"),
            contents: bytemuck::cast_slice(&quad),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle_quad_ib"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x2 },
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        // pos3 + col3 + size1 + age1 + life1 = 9 f32 = 36 B
                        array_stride: 36, step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 1, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { shader_location: 2, offset: 12, format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { shader_location: 3, offset: 24, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 4, offset: 28, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 5, offset: 32, format: wgpu::VertexFormat::Float32 },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,  // alpha blend — let scene show through
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group, quad_vb, quad_ib }
    }
}

// ══════════════════════════════════════════════════════════
// Atlas pipeline — Nintendo-style procedural sprite shapes
// (flame / water / sparkle / shock_wave / wind_swirl / …)
// 1 pipeline renders 16 shape kinds selected per instance.
// ══════════════════════════════════════════════════════════

pub use ParticleUniform as AtlasUniform;

/// Instance layout mirrors `scene_atlas.wgsl`. 32 bytes per instance.
/// pos (12) + tint (12) + size (4) + slot (4 u32) + rot (4) + alpha (4) = 40 B.
#[repr(C, align(4))]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct AtlasInstance {
    pub pos:   [f32; 3],
    pub tint:  [f32; 3],
    pub size:  f32,
    pub slot:  u32,
    pub rot:   f32,
    pub alpha: f32,
}

pub struct AtlasPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub quad_vb: wgpu::Buffer,
    pub quad_ib: wgpu::Buffer,
}

impl AtlasPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let (uniform, layout, bind_group) = make_uniform_bind(
            device, std::mem::size_of::<AtlasUniform>() as u64, "atlas_uniform");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("atlas_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene_atlas.wgsl").into()),
        });
        let quad: [f32; 8] = [-0.5, -0.5,  0.5, -0.5,  0.5, 0.5,  -0.5, 0.5];
        let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atlas_quad_vb"),
            contents: bytemuck::cast_slice(&quad),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atlas_quad_ib"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("atlas_pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("atlas_pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs"), compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x2 },
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        // pos3(12) + tint3(12) + size(4) + slot_u32(4) + rot(4) + alpha(4) = 40
                        array_stride: 40, step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 1, offset: 0,  format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { shader_location: 2, offset: 12, format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { shader_location: 3, offset: 24, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 4, offset: 28, format: wgpu::VertexFormat::Uint32 },
                            wgpu::VertexAttribute { shader_location: 5, offset: 32, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { shader_location: 6, offset: 36, format: wgpu::VertexFormat::Float32 },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        Self { pipeline, uniform, bind_group, quad_vb, quad_ib }
    }
}

/// Canonical sprite-atlas slot IDs. Keep in sync with
/// `shaders/scene_atlas.wgsl` `sample_shape`.
pub mod atlas_slot {
    pub const FLAME_SMALL: u32 = 0;
    pub const FLAME_MEDIUM: u32 = 1;
    pub const FLAME_LARGE: u32 = 2;
    pub const EMBER: u32 = 3;
    pub const SMOKE_THIN: u32 = 4;
    pub const SMOKE_THICK: u32 = 5;
    pub const ASH: u32 = 6;
    pub const ASH_FINE: u32 = 7;
    pub const WATER_DROP: u32 = 8;
    pub const WATER_SPLASH: u32 = 9;
    pub const STEAM_PUFF: u32 = 10;
    pub const BUBBLE: u32 = 11;
    pub const SPARKLE_STAR: u32 = 12;
    pub const SHOCK_WAVE: u32 = 13;
    pub const WIND_SWIRL: u32 = 14;
    pub const ARROW_TRAIL: u32 = 15;
}
