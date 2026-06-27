//! WebGPU compute backend for `cartpole_step.wgsl`.
//!
//! Phase D real-device verification per ADR-2605261800: dispatches
//! `kami-genesis/src/wgsl/cartpole_step.wgsl` on a real `wgpu::Device`
//! (Metal on macOS, Vulkan on Linux, DX12 on Windows, WebGPU on browser).
//! Validates that the WGSL kernel produces results matching the scalar
//! Rust formula bit-for-bit (within f32 epsilon).
//!
//! Gated behind feature `gpu` so default builds don't pull in wgpu.

use crate::cartpole::{CartpoleConfig, CartpoleState};
use crate::double_pendulum::{DoublePendulumConfig, DoublePendulumState};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuState {
    x: f32,
    x_dot: f32,
    theta: f32,
    theta_dot: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuCfg {
    cart_mass: f32,
    pole_mass: f32,
    pole_half_length: f32,
    gravity: f32,
    force_mag: f32,
    dt: f32,
    num_envs: u32,
    _pad: u32,
}

impl From<&CartpoleState> for GpuState {
    fn from(s: &CartpoleState) -> Self {
        GpuState {
            x: s.x,
            x_dot: s.x_dot,
            theta: s.theta,
            theta_dot: s.theta_dot,
        }
    }
}

impl From<GpuState> for CartpoleState {
    fn from(s: GpuState) -> Self {
        CartpoleState {
            x: s.x,
            x_dot: s.x_dot,
            theta: s.theta,
            theta_dot: s.theta_dot,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuDpState {
    q1: f32,
    q2: f32,
    q1_dot: f32,
    q2_dot: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuDpTorque {
    t1: f32,
    t2: f32,
    _pad0: f32,
    _pad1: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GpuDpCfg {
    m1: f32,
    m2: f32,
    l1: f32,
    l2: f32,
    gravity: f32,
    effort_limit: f32,
    dt: f32,
    num_envs: u32,
}

impl From<&DoublePendulumState> for GpuDpState {
    fn from(s: &DoublePendulumState) -> Self {
        GpuDpState {
            q1: s.q1,
            q2: s.q2,
            q1_dot: s.q1_dot,
            q2_dot: s.q2_dot,
        }
    }
}

impl From<GpuDpState> for DoublePendulumState {
    fn from(s: GpuDpState) -> Self {
        DoublePendulumState {
            q1: s.q1,
            q2: s.q2,
            q1_dot: s.q1_dot,
            q2_dot: s.q2_dot,
        }
    }
}

/// Wraps `wgpu::Device + Queue` plus per-topology compute pipelines.
/// Holds the cartpole and double-pendulum pipelines side-by-side; future
/// topologies (R1.5 Featherstone) plug in as additional pipelines.
pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    // Cartpole
    cartpole_pipeline: wgpu::ComputePipeline,
    cartpole_bgl: wgpu::BindGroupLayout,
    // Double pendulum
    dp_pipeline: wgpu::ComputePipeline,
    dp_bgl: wgpu::BindGroupLayout,
    pub backend_name: String,
}

impl WgpuBackend {
    /// Initialise wgpu and compile the Cartpole compute pipeline.
    /// Blocking; uses `pollster` to drive the async adapter/device requests.
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
                    label: Some("kami-genesis-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("wgpu device request failed: {e}"))?;

        // Standard 3-binding layout used by both cartpole and dp pipelines.
        let triple_storage_layout = |label: &'static str| wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
        };

        // --- Cartpole pipeline ---
        let cp_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cartpole_step.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(super::WGSL_SOURCE)),
        });
        let cartpole_bgl =
            device.create_bind_group_layout(&triple_storage_layout("kami-genesis-cartpole-bgl"));
        let cp_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kami-genesis-cartpole-pl"),
            bind_group_layouts: &[&cartpole_bgl],
            push_constant_ranges: &[],
        });
        let cartpole_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("kami-genesis-cartpole-pipeline"),
            layout: Some(&cp_pl_layout),
            module: &cp_shader,
            entry_point: Some("step_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // --- Double pendulum pipeline ---
        let dp_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("double_pendulum_step.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(DP_WGSL_SOURCE)),
        });
        let dp_bgl = device.create_bind_group_layout(&triple_storage_layout("kami-genesis-dp-bgl"));
        let dp_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kami-genesis-dp-pl"),
            bind_group_layouts: &[&dp_bgl],
            push_constant_ranges: &[],
        });
        let dp_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("kami-genesis-dp-pipeline"),
            layout: Some(&dp_pl_layout),
            module: &dp_shader,
            entry_point: Some("step_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(WgpuBackend {
            device,
            queue,
            cartpole_pipeline,
            cartpole_bgl,
            dp_pipeline,
            dp_bgl,
            backend_name,
        })
    }

    /// One step on the GPU for `num_envs` envs.
    /// Mirrors `step_vectorized` semantics; returns the new state.
    pub fn step(
        &self,
        states: &mut [CartpoleState],
        actions: &[f32],
        cfg: &CartpoleConfig,
    ) -> Result<(), String> {
        pollster::block_on(self.step_async(states, actions, cfg))
    }

    async fn step_async(
        &self,
        states: &mut [CartpoleState],
        actions: &[f32],
        cfg: &CartpoleConfig,
    ) -> Result<(), String> {
        if states.len() != actions.len() {
            return Err(format!(
                "states.len() ({}) != actions.len() ({})",
                states.len(),
                actions.len()
            ));
        }
        let n = states.len() as u32;
        if n == 0 {
            return Ok(());
        }

        let gpu_states: Vec<GpuState> = states.iter().map(Into::into).collect();
        let states_bytes = bytemuck::cast_slice(&gpu_states);
        let actions_bytes = bytemuck::cast_slice(actions);
        let gpu_cfg = GpuCfg {
            cart_mass: cfg.cart_mass,
            pole_mass: cfg.pole_mass,
            pole_half_length: cfg.pole_half_length,
            gravity: cfg.gravity,
            force_mag: cfg.force_mag,
            dt: cfg.dt,
            num_envs: n,
            _pad: 0,
        };

        let states_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("states"),
                contents: states_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let actions_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("actions"),
                contents: actions_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let cfg_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cfg"),
                contents: bytemuck::bytes_of(&gpu_cfg),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: states_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kami-genesis-cartpole-bg"),
            layout: &self.cartpole_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: states_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: actions_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cfg_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kami-genesis-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cartpole_step"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.cartpole_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let workgroup_count = n.div_ceil(64);
            cpass.dispatch_workgroups(workgroup_count, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&states_buf, 0, &readback_buf, 0, states_bytes.len() as u64);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback_buf.slice(..);
        let (tx, rx) = futures_channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("map_async receive failed: {e}"))?
            .map_err(|e| format!("map_async failed: {e}"))?;

        let mapped = slice.get_mapped_range();
        let result: &[GpuState] = bytemuck::cast_slice(&mapped);
        for (i, s) in result.iter().enumerate() {
            states[i] = (*s).into();
        }
        drop(mapped);
        readback_buf.unmap();
        Ok(())
    }

    /// N steps on the GPU with persistent state — uploads + readback once,
    /// dispatches N kernels in the same submission with implicit barriers
    /// between passes. Mirrors `step_vectorized` semantics applied N times
    /// with the SAME action vector across all steps.
    ///
    /// Per ADR-2605261800 R1.2 spec: collapses per-step round-trip overhead
    /// (1.7 ms/dispatch baseline measured in `cartpole_gpu_overhead_profile.rs`)
    /// into single-submit / single-readback. Theoretical speedup ≈ N×.
    pub fn step_n(
        &self,
        states: &mut [CartpoleState],
        actions: &[f32],
        cfg: &CartpoleConfig,
        n_steps: usize,
    ) -> Result<(), String> {
        pollster::block_on(self.step_n_async(states, actions, cfg, n_steps))
    }

    async fn step_n_async(
        &self,
        states: &mut [CartpoleState],
        actions: &[f32],
        cfg: &CartpoleConfig,
        n_steps: usize,
    ) -> Result<(), String> {
        if states.len() != actions.len() {
            return Err(format!(
                "states.len() ({}) != actions.len() ({})",
                states.len(),
                actions.len()
            ));
        }
        let n = states.len() as u32;
        if n == 0 || n_steps == 0 {
            return Ok(());
        }

        let gpu_states: Vec<GpuState> = states.iter().map(Into::into).collect();
        let states_bytes = bytemuck::cast_slice(&gpu_states);
        let actions_bytes = bytemuck::cast_slice(actions);
        let gpu_cfg = GpuCfg {
            cart_mass: cfg.cart_mass,
            pole_mass: cfg.pole_mass,
            pole_half_length: cfg.pole_half_length,
            gravity: cfg.gravity,
            force_mag: cfg.force_mag,
            dt: cfg.dt,
            num_envs: n,
            _pad: 0,
        };

        let states_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-genesis-step-n-states"),
                contents: states_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let actions_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-genesis-step-n-actions"),
                contents: actions_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let cfg_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-genesis-step-n-cfg"),
                contents: bytemuck::bytes_of(&gpu_cfg),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kami-genesis-step-n-readback"),
            size: states_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kami-genesis-step-n-bg"),
            layout: &self.cartpole_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: states_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: actions_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cfg_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kami-genesis-step-n-encoder"),
            });
        let workgroup_count = n.div_ceil(64);

        // N back-to-back compute passes in the same encoder. Each pass's
        // `cpass` Drop ends the pass which inserts a storage-buffer barrier,
        // so the next pass sees the previous step's writes (read-modify-write
        // on `states` storage buffer is the read-after-write dep).
        for step_idx in 0..n_steps {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("kami-genesis-step-n-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.cartpole_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(workgroup_count, 1, 1);
            drop(cpass);
            let _ = step_idx; // step_idx is just for label/clarity; loop body is uniform
        }
        encoder.copy_buffer_to_buffer(&states_buf, 0, &readback_buf, 0, states_bytes.len() as u64);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback_buf.slice(..);
        let (tx, rx) = futures_channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("map_async receive failed: {e}"))?
            .map_err(|e| format!("map_async failed: {e}"))?;

        let mapped = slice.get_mapped_range();
        let result: &[GpuState] = bytemuck::cast_slice(&mapped);
        for (i, s) in result.iter().enumerate() {
            states[i] = (*s).into();
        }
        drop(mapped);
        readback_buf.unmap();
        Ok(())
    }
}

/// Tiny single-shot channel for `map_async` callback. Avoids pulling in
/// `futures` / `tokio` just for one signal.
fn futures_channel<T>() -> (std::sync::mpsc::Sender<T>, std::sync::mpsc::Receiver<T>) {
    std::sync::mpsc::channel()
}

/// WGSL source for the double pendulum step kernel.
pub const DP_WGSL_SOURCE: &str = include_str!("wgsl/double_pendulum_step.wgsl");

impl WgpuBackend {
    /// One step on the GPU for `num_envs` double pendulum envs.
    /// `torques[i] = [tau1_i, tau2_i]` packed contiguously (length = 2 * num_envs).
    pub fn step_double_pendulum(
        &self,
        states: &mut [DoublePendulumState],
        torques: &[[f32; 2]],
        cfg: &DoublePendulumConfig,
    ) -> Result<(), String> {
        pollster::block_on(self.step_double_pendulum_async(states, torques, cfg))
    }

    async fn step_double_pendulum_async(
        &self,
        states: &mut [DoublePendulumState],
        torques: &[[f32; 2]],
        cfg: &DoublePendulumConfig,
    ) -> Result<(), String> {
        if states.len() != torques.len() {
            return Err(format!(
                "states.len() ({}) != torques.len() ({})",
                states.len(),
                torques.len()
            ));
        }
        let n = states.len() as u32;
        if n == 0 {
            return Ok(());
        }

        let gpu_states: Vec<GpuDpState> = states.iter().map(Into::into).collect();
        let gpu_torques: Vec<GpuDpTorque> = torques
            .iter()
            .map(|t| GpuDpTorque {
                t1: t[0],
                t2: t[1],
                _pad0: 0.0,
                _pad1: 0.0,
            })
            .collect();
        let states_bytes = bytemuck::cast_slice(&gpu_states);
        let torques_bytes = bytemuck::cast_slice(&gpu_torques);
        let gpu_cfg = GpuDpCfg {
            m1: cfg.m1,
            m2: cfg.m2,
            l1: cfg.l1,
            l2: cfg.l2,
            gravity: cfg.gravity,
            effort_limit: cfg.effort_limit,
            dt: cfg.dt,
            num_envs: n,
        };

        let states_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dp-states"),
                contents: states_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let torques_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dp-torques"),
                contents: torques_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let cfg_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dp-cfg"),
                contents: bytemuck::bytes_of(&gpu_cfg),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dp-readback"),
            size: states_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dp-bg"),
            layout: &self.dp_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: states_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: torques_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cfg_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dp-encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("double_pendulum_step"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.dp_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(n.div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&states_buf, 0, &readback_buf, 0, states_bytes.len() as u64);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback_buf.slice(..);
        let (tx, rx) = futures_channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("map_async receive failed: {e}"))?
            .map_err(|e| format!("map_async failed: {e}"))?;

        let mapped = slice.get_mapped_range();
        let result: &[GpuDpState] = bytemuck::cast_slice(&mapped);
        for (i, s) in result.iter().enumerate() {
            states[i] = (*s).into();
        }
        drop(mapped);
        readback_buf.unmap();
        Ok(())
    }

    /// N steps on the GPU for `num_envs` double-pendulum envs with persistent
    /// state — uploads + readback once, dispatches N kernels in one submission.
    /// Mirror of `WgpuBackend::step_n` for the DP topology per ADR-2605261800 R1.2.
    /// Same fixed torque vector applied across all N steps.
    pub fn step_double_pendulum_n(
        &self,
        states: &mut [DoublePendulumState],
        torques: &[[f32; 2]],
        cfg: &DoublePendulumConfig,
        n_steps: usize,
    ) -> Result<(), String> {
        pollster::block_on(self.step_double_pendulum_n_async(states, torques, cfg, n_steps))
    }

    async fn step_double_pendulum_n_async(
        &self,
        states: &mut [DoublePendulumState],
        torques: &[[f32; 2]],
        cfg: &DoublePendulumConfig,
        n_steps: usize,
    ) -> Result<(), String> {
        if states.len() != torques.len() {
            return Err(format!(
                "states.len() ({}) != torques.len() ({})",
                states.len(),
                torques.len()
            ));
        }
        let n = states.len() as u32;
        if n == 0 || n_steps == 0 {
            return Ok(());
        }

        let gpu_states: Vec<GpuDpState> = states.iter().map(Into::into).collect();
        let gpu_torques: Vec<GpuDpTorque> = torques
            .iter()
            .map(|t| GpuDpTorque {
                t1: t[0],
                t2: t[1],
                _pad0: 0.0,
                _pad1: 0.0,
            })
            .collect();
        let states_bytes = bytemuck::cast_slice(&gpu_states);
        let torques_bytes = bytemuck::cast_slice(&gpu_torques);
        let gpu_cfg = GpuDpCfg {
            m1: cfg.m1,
            m2: cfg.m2,
            l1: cfg.l1,
            l2: cfg.l2,
            gravity: cfg.gravity,
            effort_limit: cfg.effort_limit,
            dt: cfg.dt,
            num_envs: n,
        };

        let states_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dp-step-n-states"),
                contents: states_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let torques_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dp-step-n-torques"),
                contents: torques_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let cfg_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dp-step-n-cfg"),
                contents: bytemuck::bytes_of(&gpu_cfg),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dp-step-n-readback"),
            size: states_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dp-step-n-bg"),
            layout: &self.dp_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: states_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: torques_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cfg_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dp-step-n-encoder"),
            });
        let workgroup_count = n.div_ceil(64);
        for _ in 0..n_steps {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dp-step-n-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.dp_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(workgroup_count, 1, 1);
            drop(cpass);
        }
        encoder.copy_buffer_to_buffer(&states_buf, 0, &readback_buf, 0, states_bytes.len() as u64);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback_buf.slice(..);
        let (tx, rx) = futures_channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("map_async receive failed: {e}"))?
            .map_err(|e| format!("map_async failed: {e}"))?;

        let mapped = slice.get_mapped_range();
        let result: &[GpuDpState] = bytemuck::cast_slice(&mapped);
        for (i, s) in result.iter().enumerate() {
            states[i] = (*s).into();
        }
        drop(mapped);
        readback_buf.unmap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vectorized::step_vectorized;

    #[test]
    fn wgpu_dispatch_matches_cpu_vectorized() {
        let backend = match WgpuBackend::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping GPU test, no adapter available: {e}");
                return;
            }
        };
        println!("wgpu backend: {}", backend.backend_name);

        let cfg = CartpoleConfig::default();
        let n = 256;
        let mut gpu_states: Vec<CartpoleState> = (0..n)
            .map(|i| CartpoleState {
                theta: 0.05 + (i as f32) * 0.0001,
                ..Default::default()
            })
            .collect();
        let mut cpu_states = gpu_states.clone();
        let actions: Vec<f32> = (0..n).map(|i| (i as f32) * 0.01).collect();

        // 50 step iterations to compound numerical work and catch any drift.
        for _ in 0..50 {
            backend.step(&mut gpu_states, &actions, &cfg).unwrap();
            step_vectorized(&mut cpu_states, &actions, &cfg);
        }

        let mut max_dx = 0.0_f32;
        let mut max_dtheta = 0.0_f32;
        for i in 0..n {
            max_dx = max_dx.max((gpu_states[i].x - cpu_states[i].x).abs());
            max_dtheta = max_dtheta.max((gpu_states[i].theta - cpu_states[i].theta).abs());
        }
        println!(
            "max |Δx| = {:.3e}, max |Δθ| = {:.3e} over {} envs × 50 steps",
            max_dx, max_dtheta, n
        );
        // f32 rounding may produce small drift over 50 steps; require ≤ 1e-3.
        assert!(max_dx < 1e-3, "x drift too large: {max_dx}");
        assert!(max_dtheta < 1e-3, "theta drift too large: {max_dtheta}");
    }

    #[test]
    fn single_step_matches_scalar() {
        let backend = match WgpuBackend::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping GPU test, no adapter available: {e}");
                return;
            }
        };
        let cfg = CartpoleConfig::default();
        let mut gpu = vec![
            CartpoleState {
                theta: 0.1,
                ..Default::default()
            };
            1
        ];
        let mut scalar = CartpoleState {
            theta: 0.1,
            ..Default::default()
        };
        let actions = vec![5.0_f32];
        backend.step(&mut gpu, &actions, &cfg).unwrap();
        scalar.step(5.0, &cfg);
        assert!((gpu[0].x - scalar.x).abs() < 1e-6);
        assert!((gpu[0].theta - scalar.theta).abs() < 1e-6);
    }

    #[test]
    fn wgpu_dp_dispatch_matches_cpu_scalar() {
        let backend = match WgpuBackend::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping GPU test, no adapter available: {e}");
                return;
            }
        };
        println!("wgpu backend (dp): {}", backend.backend_name);

        let cfg = DoublePendulumConfig::default();
        let n = 256;
        let mut gpu_states: Vec<DoublePendulumState> = (0..n)
            .map(|i| DoublePendulumState {
                q1: 0.5 + (i as f32) * 0.001,
                q2: -0.3 + (i as f32) * 0.0005,
                ..Default::default()
            })
            .collect();
        let mut cpu_states = gpu_states.clone();
        let torques: Vec<[f32; 2]> = (0..n)
            .map(|i| [(i as f32) * 0.01, (i as f32) * -0.005])
            .collect();

        for _ in 0..50 {
            backend
                .step_double_pendulum(&mut gpu_states, &torques, &cfg)
                .unwrap();
            for (st, tau) in cpu_states.iter_mut().zip(torques.iter()) {
                st.step(*tau, &cfg);
            }
        }

        let mut max_d = 0.0_f32;
        for i in 0..n {
            max_d = max_d
                .max((gpu_states[i].q1 - cpu_states[i].q1).abs())
                .max((gpu_states[i].q2 - cpu_states[i].q2).abs())
                .max((gpu_states[i].q1_dot - cpu_states[i].q1_dot).abs())
                .max((gpu_states[i].q2_dot - cpu_states[i].q2_dot).abs());
        }
        println!(
            "dp max state drift = {:.3e} over {} envs × 50 steps",
            max_d, n
        );
        assert!(max_d < 1e-3, "dp gpu-vs-cpu drift too large: {max_d}");
    }

    #[test]
    fn dp_single_step_matches_scalar() {
        let backend = match WgpuBackend::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping GPU test, no adapter available: {e}");
                return;
            }
        };
        let cfg = DoublePendulumConfig::default();
        let mut gpu = vec![DoublePendulumState {
            q1: 0.5,
            q2: -0.2,
            ..Default::default()
        }];
        let mut scalar = DoublePendulumState {
            q1: 0.5,
            q2: -0.2,
            ..Default::default()
        };
        backend
            .step_double_pendulum(&mut gpu, &[[2.0, -1.0]], &cfg)
            .unwrap();
        scalar.step([2.0, -1.0], &cfg);
        assert!((gpu[0].q1 - scalar.q1).abs() < 1e-6);
        assert!((gpu[0].q2 - scalar.q2).abs() < 1e-6);
    }
}
