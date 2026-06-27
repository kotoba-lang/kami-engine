//! RT compute dispatch — the GPU executor for `kami.rt` (software-BVH path).
//!
//! Wires the CPU-built LBVH from `kami-rt` to a wgpu compute pipeline running
//! `shaders/rt_bvh_compute.wgsl`: it uploads the node + triangle storage buffers,
//! the camera globals, and an output hit buffer, then dispatches one thread per
//! pixel. This is the portable path (stable WebGPU, no ray-query extension); the
//! hardware ray-query variant comes from `kami_rt::wgsl_ray_query`.
//!
//! `RtGlobals` mirrors the WGSL uniform; `GpuNode`/`GpuTri` come straight from
//! `kami_rt::gpu`. Only compiled with the `wgpu-backend` feature.

use bytemuck::{Pod, Zeroable};
use kami_rt::bvh::Bvh;
use wgpu::util::DeviceExt;

/// Camera + framebuffer dimensions uniform (matches WGSL `RtGlobals`, 96 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct RtGlobals {
    pub inv_view_proj: [f32; 16],
    pub cam_pos: [f32; 4],
    pub dims: [u32; 4], // x=width, y=height
}

impl RtGlobals {
    pub fn new(inv_view_proj: [f32; 16], cam_pos: [f32; 3], width: u32, height: u32) -> Self {
        Self {
            inv_view_proj,
            cam_pos: [cam_pos[0], cam_pos[1], cam_pos[2], 1.0],
            dims: [width, height, 0, 0],
        }
    }
}

/// Bytes per output hit record: vec4<f32> = (t, tri_id, bary_u, bary_v).
pub const HIT_STRIDE: u64 = 16;

/// The RT compute pipeline + its bind-group layout (built once per device).
pub struct RayTracePipeline {
    pub pipeline: wgpu::ComputePipeline,
    pub layout: wgpu::BindGroupLayout,
}

impl RayTracePipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rt_bvh_compute"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rt_bvh_compute.wgsl").into()),
        });

        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rt_bind_layout"),
            entries: &[
                storage(0, true), // nodes
                storage(1, true), // tris
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
                storage(3, false), // out_hits
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rt_pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rt_compute"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("trace"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    /// Upload `bvh` + `globals`, dispatch the trace, and return the output hit
    /// buffer (size `width*height*HIT_STRIDE`, usage STORAGE|COPY_SRC — map or
    /// copy it to read results back). One thread per pixel, 8×8 workgroups.
    pub fn trace(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bvh: &Bvh,
        globals: RtGlobals,
        width: u32,
        height: u32,
    ) -> wgpu::Buffer {
        let (mut nodes, mut tris) = bvh.to_gpu();
        // Storage buffers must be non-empty; pad degenerate scenes with a zero row.
        if nodes.is_empty() {
            nodes.push(Zeroable::zeroed());
        }
        if tris.is_empty() {
            tris.push(Zeroable::zeroed());
        }

        let nodes_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt_nodes"),
            contents: bytemuck::cast_slice(&nodes),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let tris_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt_tris"),
            contents: bytemuck::cast_slice(&tris),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt_globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt_out_hits"),
            size: (width as u64) * (height as u64) * HIT_STRIDE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rt_bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: nodes_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tris_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: out_buf.as_entire_binding(),
                },
            ],
        });

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rt_encoder"),
        });
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rt_pass"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.pipeline);
            cp.set_bind_group(0, &bind, &[]);
            cp.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        queue.submit([enc.finish()]);
        out_buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_layout_is_96_bytes() {
        assert_eq!(std::mem::size_of::<RtGlobals>(), 96);
        let g = RtGlobals::new([0.0; 16], [1.0, 2.0, 3.0], 1280, 720);
        assert_eq!(g.dims[0], 1280);
        assert_eq!(g.cam_pos, [1.0, 2.0, 3.0, 1.0]);
    }
}
