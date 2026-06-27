//! kami-genesis GPU dispatch — 1024-env Cartpole batch (Apache-2.0 Genesis-5-solver
//! via wgpu / Metal / Vulkan / DX12 / WebGPU per universal backend strategy).
//!
//! Per ADR-2605261800 §G7 PhysX NEVER — this test runs the SAME WGSL kernel
//! that the CPU vectorized backend mirrors (cartpole_step.wgsl, workgroup_size=64),
//! exercising the kami-native PhysX-replacement physics path end-to-end on real
//! GPU hardware.
//!
//! Pair with `tests/cartpole_batch_1024.rs` (CPU scalar baseline) — together
//! they establish the CPU-vs-GPU performance + correctness contract.
//!
//! Skips cleanly if no GPU adapter is available (CI without a GPU).

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;
const N_STEPS: usize = 200;

#[test]
fn cartpole_1024_env_gpu_batch_throughput_and_correctness() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter available: {e}");
            return;
        }
    };

    // Match CPU scoring (tests/cartpole_batch_1024.rs) initial conditions.
    let mut states: Vec<CartpoleState> = (0..N_ENVS)
        .map(|i| {
            let theta0 = ((i as f32 / N_ENVS as f32) - 0.5) * 0.1;
            CartpoleState {
                x: 0.0,
                x_dot: 0.0,
                theta: theta0,
                theta_dot: 0.0,
            }
        })
        .collect();
    let actions = vec![0.0f32; N_ENVS];
    let cfg = CartpoleConfig::default();

    // Warm-up step (first dispatch incurs pipeline compile + buffer alloc).
    backend.step(&mut states.clone(), &actions, &cfg).unwrap();

    // Timed dispatch.
    let t0 = Instant::now();
    for _ in 0..N_STEPS {
        backend.step(&mut states, &actions, &cfg).unwrap();
    }
    let elapsed = t0.elapsed();
    let env_steps = N_ENVS * N_STEPS;
    let ns_per_env_step = elapsed.as_nanos() as f64 / env_steps as f64;

    // Physics observables (must match CPU run shape: divergence + range > π).
    let theta_min = states.iter().map(|s| s.theta).fold(f32::INFINITY, f32::min);
    let theta_max = states
        .iter()
        .map(|s| s.theta)
        .fold(f32::NEG_INFINITY, f32::max);
    let mean_abs_theta = states.iter().map(|s| s.theta.abs()).sum::<f32>() / N_ENVS as f32;
    let std_theta = {
        let mean = states.iter().map(|s| s.theta).sum::<f32>() / N_ENVS as f32;
        let var = states.iter().map(|s| (s.theta - mean).powi(2)).sum::<f32>() / N_ENVS as f32;
        var.sqrt()
    };

    println!("\n=== kami-genesis cartpole 1024-env GPU scoring ===");
    println!("backend           : wgpu — Metal/Vulkan/DX12/WebGPU universal");
    println!("envs              : {N_ENVS}");
    println!("steps             : {N_STEPS}");
    println!("env-steps total   : {env_steps}");
    println!("wall time         : {:?}", elapsed);
    println!("ns per env-step   : {ns_per_env_step:.1}");
    println!(
        "throughput        : {:.2}M env-steps/sec",
        1e3 / ns_per_env_step
    );
    println!("final θ range     : [{theta_min:.3}, {theta_max:.3}] rad");
    println!("final mean |θ|    : {mean_abs_theta:.4} rad");
    println!("final θ std       : {std_theta:.4} rad");
    println!("===================================================");

    // Correctness gates — same shape as CPU test.
    assert!(
        theta_max.abs() > std::f32::consts::PI || theta_min.abs() > std::f32::consts::PI,
        "envs should span > ±π on GPU same as CPU; got [{theta_min:.2}, {theta_max:.2}]"
    );
    assert!(
        std_theta > 0.1,
        "envs should diverge on GPU; got std {std_theta:.4}"
    );

    // Throughput observation — NOT a tight gate.
    //
    // Empirical Apple M4 baseline: ~1.6 μs/env-step over 200 per-step round-trips
    // (sync upload + dispatch + readback per step via pollster::block_on).
    // This is ~33× SLOWER than the CPU vectorized path (48 ns/env-step on the
    // same M4 P-cores) because per-step CPU↔GPU buffer marshalling dominates
    // wall-clock for cartpole's tiny 4-f32-per-env state.
    //
    // The pattern that wins on GPU is persistent state + async multi-step kernels
    // (deferred to R1.2+ per ADR-2605261800 — the test that fires when that
    // path lands will use < 50 ns/env-step including amortized upload).
    //
    // Current gate just confirms the GPU completes in reasonable wall-clock so CI
    // catches catastrophic regressions (e.g., a 1 s/step timeout).
    assert!(
        ns_per_env_step < 50_000.0,
        "GPU per-step round-trip should stay < 50μs/env-step (got {ns_per_env_step:.0} ns) — \
         regression indicates pipeline cold-start, sync mismatch, or readback stall"
    );
}
