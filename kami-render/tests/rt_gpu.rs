//! Headless GPU integration test for the RT compute dispatch (ADR-0045).
//!
//! Builds a one-triangle LBVH with `kami-rt`, runs the real `rt_bvh_compute.wgsl`
//! pass on a wgpu device (no surface), reads the hit buffer back, and asserts the
//! centre pixel hit the triangle at the expected distance. This actually executes
//! the shader on whatever adapter is present (Metal/Vulkan/DX12/GL); if no
//! adapter is available (pure CI), it skips rather than failing.
//!
//! Requires the `wgpu-backend` feature (default).

#![cfg(feature = "wgpu-backend")]

use kami_rt::bvh::{Bvh, Tri};
use kami_render::raytrace::{RayTracePipeline, RtGlobals, HIT_STRIDE};
use glam::Vec3;

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("rt-headless"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .ok()
}

#[test]
fn rt_compute_hits_a_triangle_on_real_gpu() {
    let Some((device, queue)) = headless_device() else {
        eprintln!("rt_gpu: no GPU adapter available — skipping");
        return;
    };

    // Triangle straight ahead at z = +5 (identity inv_view_proj → centre ray +z).
    let bvh = Bvh::build(vec![Tri {
        v0: Vec3::new(-1.0, -1.0, 5.0),
        v1: Vec3::new(1.0, -1.0, 5.0),
        v2: Vec3::new(0.0, 1.0, 5.0),
        id: 7,
    }]);

    let (w, h) = (16u32, 16u32);
    let identity = [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, //
    ];
    let globals = RtGlobals::new(identity, [0.0, 0.0, 0.0], w, h);

    let pipeline = RayTracePipeline::new(&device);
    let out = pipeline.trace(&device, &queue, &bvh, globals, w, h);

    // Copy the storage output into a mappable staging buffer.
    let size = (w as u64) * (h as u64) * HIT_STRIDE;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rt_readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(&out, 0, &staging, 0, size);
    queue.submit([enc.finish()]);

    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);

    let data = slice.get_mapped_range();
    let hits: &[f32] = bytemuck::cast_slice(&data);

    // Centre pixel (8,8): index = y*w + x; each hit is 4 floats (t, id, u, v).
    let idx = (8 * w + 8) as usize * 4;
    let t = hits[idx];
    let id = hits[idx + 1];
    assert!(t > 4.9 && t < 5.2, "centre pixel t = {t}, expected ~5.0");
    assert_eq!(id as u32, 7, "centre pixel should hit triangle id 7");

    // A far-corner pixel's ray tilts away and must miss (t < 0 sentinel).
    let corner = 0usize * 4;
    assert!(hits[corner] < 0.0, "corner pixel should miss (t={})", hits[corner]);
}
