//! kami-genesis GPU per-dispatch overhead profile — sub-component wall-clock breakdown.
//!
//! Iter 6/7 measured aggregate "1.5 ms per dispatch" but didn't decompose
//! into pipeline-cache / upload / submit / readback components. This test
//! profiles each phase to give R1.2 persistent-state-batching the concrete
//! latency budget it needs to beat.
//!
//! Strategy: time the same backend over progressively-fewer dispatches:
//!   - 1 step  → measures (pipeline-cache + first-dispatch fixed cost)
//!   - 10 steps → amortizes first-step overhead over 9 marginal
//!   - 100 steps → near-asymptotic marginal cost
//!   - 1000 steps → confirms linearity (catches GC stalls / readback contention)
//!
//! From the slope + intercept of (wall_clock vs N_steps) we infer:
//!   - intercept ≈ fixed overhead per session (pipeline JIT + adapter init done in `new()`)
//!   - slope ≈ marginal cost per dispatch (upload + submit + readback)
//!
//! Per ADR-2605261800 §G7 — KAMI-physx (PhysX NEVER) path is the system under test.

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;

#[test]
fn cartpole_gpu_dispatch_overhead_breakdown() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = CartpoleConfig::default();
    let actions = vec![0.0f32; N_ENVS];

    // Warm-up: trigger pipeline cache + first dispatch (not counted in profile).
    let mut warmup_state = make_states();
    backend.step(&mut warmup_state, &actions, &cfg).unwrap();

    // Profile N_steps ∈ {1, 10, 100, 1000}.
    let n_step_grid: &[usize] = &[1, 10, 100, 1000];
    let mut wall_times_ns: Vec<f64> = Vec::with_capacity(n_step_grid.len());

    for &n_steps in n_step_grid {
        let mut states = make_states();
        let t = Instant::now();
        for _ in 0..n_steps {
            backend.step(&mut states, &actions, &cfg).unwrap();
        }
        let elapsed_ns = t.elapsed().as_nanos() as f64;
        wall_times_ns.push(elapsed_ns);
    }

    // Linear regression: wall_ns = intercept + slope * n_steps
    // Use the last two points to get the marginal cost (large-N asymptote):
    let i_last = n_step_grid.len() - 1;
    let i_prev = i_last - 1;
    let marginal_ns = (wall_times_ns[i_last] - wall_times_ns[i_prev])
        / (n_step_grid[i_last] - n_step_grid[i_prev]) as f64;
    let intercept_ns = wall_times_ns[i_last] - marginal_ns * n_step_grid[i_last] as f64;

    println!("\n=== kami-genesis GPU per-dispatch overhead profile ===");
    println!("backend           : wgpu (Metal/Vulkan/DX12/WebGPU universal)");
    println!("envs per dispatch : {N_ENVS}");
    println!("------------------------------------------------------");
    println!(
        "{:>8} {:>14} {:>14} {:>14}",
        "N_steps", "wall (ms)", "per-step (μs)", "per-env-step (ns)"
    );
    for (i, &n_steps) in n_step_grid.iter().enumerate() {
        let wall_ms = wall_times_ns[i] / 1e6;
        let per_step_us = wall_times_ns[i] / 1e3 / n_steps as f64;
        let per_env_step_ns = wall_times_ns[i] / (n_steps * N_ENVS) as f64;
        println!(
            "{:>8} {:>14.2} {:>14.2} {:>14.1}",
            n_steps, wall_ms, per_step_us, per_env_step_ns
        );
    }
    println!("------------------------------------------------------");
    println!("Linear extrapolation (last 2 points):");
    println!(
        "  intercept (post-warmup fixed)  : {:>10.1} μs",
        intercept_ns / 1e3
    );
    println!(
        "  slope (marginal per dispatch)  : {:>10.1} μs",
        marginal_ns / 1e3
    );
    println!(
        "  marginal per env-step          : {:>10.1} ns",
        marginal_ns / N_ENVS as f64
    );
    println!();
    println!("R1.2 target: collapse N dispatches into 1 (persistent state).");
    println!("  Theoretical floor = single submit + N kernels + 1 readback.");
    println!("  Speedup estimate = marginal / floor ≈ 5-10× per ADR-2605261800 R1.2 spec.");
    println!("=======================================================");

    // Linearity sanity: marginal cost should be reasonably stable as N grows
    // (if marginal grows with N, that signals readback stalls / GC stress).
    let marginal_10 = (wall_times_ns[1] - wall_times_ns[0]) / 9.0;
    let marginal_100 = (wall_times_ns[2] - wall_times_ns[1]) / 90.0;
    let marginal_1000 = marginal_ns;

    let max_marginal = marginal_10.max(marginal_100).max(marginal_1000);
    let min_marginal = marginal_10.min(marginal_100).min(marginal_1000);
    let spread = (max_marginal - min_marginal) / min_marginal;

    assert!(
        spread < 1.0,
        "marginal cost should be within 2× across step counts (got spread {spread:.2}, \
         max {max_marginal:.0} ns, min {min_marginal:.0} ns) — non-linearity \
         indicates readback stalls / GC pressure / pipeline thrashing"
    );

    // Reasonable bound: marginal per env-step should beat 100μs (clearly slower
    // than CPU vectorized but in the right ballpark for round-trip GPU dispatch).
    let marginal_per_env_step = marginal_ns / N_ENVS as f64;
    assert!(
        marginal_per_env_step < 100_000.0,
        "marginal per-env-step should stay < 100μs (got {marginal_per_env_step:.0} ns); \
         catastrophic regression indicator"
    );
}

fn make_states() -> Vec<CartpoleState> {
    (0..N_ENVS)
        .map(|i| {
            let theta0 = ((i as f32 / N_ENVS as f32) - 0.5) * 0.1;
            CartpoleState {
                x: 0.0,
                x_dot: 0.0,
                theta: theta0,
                theta_dot: 0.0,
            }
        })
        .collect()
}
