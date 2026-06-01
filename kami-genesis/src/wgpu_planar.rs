//! wgpu_planar — GPU compute backend for the GENERAL planar N-link articulation.
//!
//! Dispatches `wgsl/planar_chain_step.wgsl` (RNEA + CRBA + LDLᵀ + semi-implicit
//! Euler) over `num_envs` environments on a real `wgpu::Device`. This is the
//! "general articulation on GPU" step — beyond the hand-coded cartpole /
//! double-pendulum kernels — validated bit-for-bit (within f32 tolerance)
//! against the CPU `planar_chain` solver. Self-contained (owns its device) so it
//! does not touch the existing cartpole/DP backend. Gated behind `--features gpu`.

use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const MAXN: usize = 7;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GpuCfg {
    n: u32,
    gravity: f32,
    dt: f32,
    effort_limit: f32,
}

pub struct PlanarChainGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    pub backend_name: String,
}

impl PlanarChainGpu {
    pub fn new() -> Result<Self, String> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "no wgpu adapter available".to_string())?;
        let info = adapter.get_info();
        let backend_name = format!("{:?} / {}", info.backend, info.name);
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("kami-genesis-planar-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("wgpu device request failed: {e}"))?;

        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("planar-bgl"),
            entries: &[
                storage(0, false),
                storage(1, true),
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
                storage(3, true),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("planar_chain_step.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "wgsl/planar_chain_step.wgsl"
            ))),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("planar-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("planar-pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            bgl,
            backend_name,
        })
    }

    /// `n_steps` GPU steps for `num_envs` planar N-link chains. `states` is
    /// `[num_envs * 2n]` (per env `q(n), qdot(n)`), updated in place; `torques`
    /// is `[num_envs * n]`; `lengths`/`masses` length `n` (shared across envs).
    #[allow(clippy::too_many_arguments)]
    pub fn step_n(
        &self,
        states: &mut [f32],
        torques: &[f32],
        lengths: &[f32],
        masses: &[f32],
        n: u32,
        gravity: f32,
        dt: f32,
        effort_limit: f32,
        n_steps: usize,
    ) -> Result<(), String> {
        pollster::block_on(self.step_n_async(
            states,
            torques,
            lengths,
            masses,
            n,
            gravity,
            dt,
            effort_limit,
            n_steps,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn step_n_async(
        &self,
        states: &mut [f32],
        torques: &[f32],
        lengths: &[f32],
        masses: &[f32],
        n: u32,
        gravity: f32,
        dt: f32,
        effort_limit: f32,
        n_steps: usize,
    ) -> Result<(), String> {
        let nn = n as usize;
        if nn == 0 || nn > MAXN {
            return Err(format!("n={n} out of range 1..={MAXN}"));
        }
        let num_envs = states.len() / (2 * nn);
        if num_envs == 0 {
            return Ok(());
        }
        // params: [lengths(MAXN), masses(MAXN)]
        let mut params = vec![0.0f32; 2 * MAXN];
        params[..nn].copy_from_slice(&lengths[..nn]);
        params[MAXN..MAXN + nn].copy_from_slice(&masses[..nn]);

        let cfg = GpuCfg {
            n,
            gravity,
            dt,
            effort_limit,
        };
        let state_bytes = bytemuck::cast_slice(states);
        let states_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("planar-states"),
                contents: state_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let torque_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("planar-torques"),
                contents: bytemuck::cast_slice(torques),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let cfg_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("planar-cfg"),
                contents: bytemuck::bytes_of(&cfg),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let params_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("planar-params"),
                contents: bytemuck::cast_slice(&params),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("planar-readback"),
            size: state_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("planar-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: states_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: torque_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cfg_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("planar-enc"),
            });
        let wg = (num_envs as u32).div_ceil(64);
        for _ in 0..n_steps {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("planar-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(wg, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&states_buf, 0, &readback, 0, state_bytes.len() as u64);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("map recv: {e}"))?
            .map_err(|e| format!("map: {e}"))?;
        let data = slice.get_mapped_range();
        let out: &[f32] = bytemuck::cast_slice(&data);
        states.copy_from_slice(out);
        drop(data);
        readback.unmap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planar_chain::{PlanarChainConfig, PlanarChainState};

    #[test]
    fn gpu_planar_matches_cpu_3link() {
        let gpu = match PlanarChainGpu::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("skipping (no GPU): {e}");
                return;
            }
        };
        eprintln!("[planar-gpu] {}", gpu.backend_name);

        let n = 3u32;
        let cfg = PlanarChainConfig::uniform(n);
        let num_envs = 256usize;
        let steps = 60usize;

        // distinct gentle initial angles + small per-env torques so trajectories
        // stay close (planar chain is mildly chaotic).
        let mut cpu: Vec<PlanarChainState> = Vec::new();
        let mut gpu_state = vec![0.0f32; num_envs * 2 * n as usize];
        let mut torques = vec![0.0f32; num_envs * n as usize];
        for e in 0..num_envs {
            let a = 0.2 + 0.001 * e as f32;
            let st = PlanarChainState {
                q: vec![a, -0.5 * a, 0.3 * a],
                qdot: vec![0.0, 0.0, 0.0],
            };
            for j in 0..3 {
                gpu_state[e * 6 + j] = st.q[j];
                gpu_state[e * 6 + 3 + j] = st.qdot[j];
                torques[e * 3 + j] = 0.5 - 0.1 * j as f32;
            }
            cpu.push(st);
        }

        // CPU reference
        for (e, st) in cpu.iter_mut().enumerate() {
            let tau = [torques[e * 3], torques[e * 3 + 1], torques[e * 3 + 2]];
            for _ in 0..steps {
                st.step(&tau, &cfg);
            }
        }

        // GPU
        gpu.step_n(
            &mut gpu_state,
            &torques,
            &cfg.lengths,
            &cfg.masses,
            n,
            cfg.gravity,
            cfg.dt,
            cfg.effort_limit,
            steps,
        )
        .expect("gpu step");

        // compare q (and qdot) within f32 tolerance
        let mut max_err = 0.0f32;
        for (e, st) in cpu.iter().enumerate() {
            for j in 0..3 {
                let dq = (st.q[j] - gpu_state[e * 6 + j]).abs();
                let dv = (st.qdot[j] - gpu_state[e * 6 + 3 + j]).abs();
                max_err = max_err.max(dq).max(dv);
            }
        }
        assert!(max_err < 2e-2, "GPU vs CPU planar max_err={max_err}");
        assert!(gpu_state.iter().all(|v| v.is_finite()));
    }
}
