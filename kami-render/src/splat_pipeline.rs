//! Gaussian Splatting GPU pipeline: compute sort + alpha-blend render.
//!
//! Behind `gaussian-splat` + `wgpu-backend` features.

#[cfg(all(feature = "gaussian-splat", feature = "wgpu-backend"))]
use bytemuck::{Pod, Zeroable};

/// Sort parameters uniform (passed to compute shader).
#[cfg(all(feature = "gaussian-splat", feature = "wgpu-backend"))]
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SortParams {
    pub camera_pos: [f32; 3],
    pub splat_count: u32,
}

/// Render parameters uniform (passed to vertex shader).
#[cfg(all(feature = "gaussian-splat", feature = "wgpu-backend"))]
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct RenderParams {
    pub viewport_size: [f32; 2],
    pub focal_x: f32,
    pub focal_y: f32,
}

/// Complete Gaussian Splatting pipeline (compute sort + render).
#[cfg(all(feature = "gaussian-splat", feature = "wgpu-backend"))]
pub struct SplatPipeline {
    pub sort_pipeline: wgpu::ComputePipeline,
    pub sort_bind_group_layout: wgpu::BindGroupLayout,
    pub render_pipeline: wgpu::RenderPipeline,
    pub render_bind_group_layout: wgpu::BindGroupLayout,
}

#[cfg(all(feature = "gaussian-splat", feature = "wgpu-backend"))]
impl SplatPipeline {
    /// Create the splatting pipeline.
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        camera_light_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kami-gaussian-splat"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/gaussian_splat.wgsl").into()),
        });

        // ── Compute sort pipeline ──

        let sort_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("splat-sort-layout"),
                entries: &[
                    // binding 0: splat data (read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: sort entries (read-write)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 2: sort params (uniform)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let sort_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("splat-sort-pipe-layout"),
            bind_group_layouts: &[&sort_bind_group_layout],
            push_constant_ranges: &[],
        });

        let sort_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("splat-sort-pipeline"),
            layout: Some(&sort_layout),
            module: &shader,
            entry_point: Some("cs_compute_distances"),
            compilation_options: Default::default(),
            cache: None,
        });

        // ── Render pipeline ──

        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("splat-render-layout"),
                entries: &[
                    // binding 0: splat data (read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: sorted indices (read)
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
                    // binding 2: render params (uniform)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let render_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("splat-render-pipe-layout"),
            bind_group_layouts: &[camera_light_layout, &render_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("splat-render-pipeline"),
            layout: Some(&render_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_splat"),
                compilation_options: Default::default(),
                buffers: &[], // No vertex buffers — data from storage buffers
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_splat"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    // Premultiplied alpha blending: src + (1 - src_alpha) * dst
                    blend: Some(wgpu::BlendState {
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
                cull_mode: None, // Billboards always face camera
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Read only — don't occlude other splats
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        Self {
            sort_pipeline,
            sort_bind_group_layout,
            render_pipeline,
            render_bind_group_layout,
        }
    }

    /// Dispatch compute shader to calculate distances for sorting.
    pub fn dispatch_distances(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        sort_bind_group: &wgpu::BindGroup,
        splat_count: u32,
    ) {
        let workgroups = (splat_count + 255) / 256;
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("splat-sort"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.sort_pipeline);
        pass.set_bind_group(0, sort_bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
}
