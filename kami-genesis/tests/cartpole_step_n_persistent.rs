//! kami-genesis R1.2 persistent-GPU-state batched-dispatch — speedup verification.
//!
//! Per ADR-2605261800 §R1.2 spec, derived empirically from
//! `cartpole_gpu_overhead_profile.rs` (iter 10): per-dispatch wall-clock is
//! ~1.7 ms ≈ 100% marginal cost (no fixed-overhead intercept). Theoretical
//! speedup of collapsing N dispatches into one submission ≈ N×.
//!
//! This test exercises the new `WgpuBackend::step_n()` method which uploads
//! state ONCE, dispatches N kernels in the same encoder with implicit
//! storage-buffer barriers between passes, and reads back state ONCE.
//!
//! Two assertions:
//!   1. CORRECTNESS — `step_n(N)` produces results bit-equivalent to N×`step()`
//!      within f32 round-off (max |Δ| < 1e-4 over 100 steps, 1024 envs).
//!   2. SPEEDUP — `step_n(N)` wall-clock is at least 5× faster than N×`step()`
//!      at N=100, 1024 envs (conservative; theoretical headroom is ~N×).

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;
const N_STEPS: usize = 100;

#[test]
fn step_n_persistent_matches_step_loop_within_round_off() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = CartpoleConfig::default();
    let actions = vec![0.0f32; N_ENVS]; // Zero control — must give identical results in both paths.
    let initial = make_states();

    // Path A: N separate dispatches via existing `step()`.
    let mut states_loop = initial.clone();
    for _ in 0..N_STEPS {
        backend.step(&mut states_loop, &actions, &cfg).unwrap();
    }

    // Path B: single batched dispatch via new `step_n()`.
    let mut states_batched = initial.clone();
    backend
        .step_n(&mut states_batched, &actions, &cfg, N_STEPS)
        .unwrap();

    // Compare.
    let mut max_dx = 0.0f32;
    let mut max_dtheta = 0.0f32;
    for i in 0..N_ENVS {
        max_dx = max_dx.max((states_loop[i].x - states_batched[i].x).abs());
        max_dtheta = max_dtheta.max((states_loop[i].theta - states_batched[i].theta).abs());
    }
    println!("\n=== step_n_persistent correctness ===");
    println!("N_steps                : {N_STEPS}");
    println!("max |Δx|     loop vs n  : {max_dx:.3e}");
    println!("max |Δθ|                : {max_dtheta:.3e}");
    println!("======================================");

    // Same kernel, same input, same algorithm → must be bit-identical.
    assert!(
        max_dx < 1e-5,
        "step_n vs step-loop x divergence > 1e-5: {max_dx:.3e}"
    );
    assert!(
        max_dtheta < 1e-5,
        "step_n vs step-loop θ divergence > 1e-5: {max_dtheta:.3e}"
    );
}

#[test]
fn step_n_persistent_beats_step_loop_by_5x_or_more() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = CartpoleConfig::default();
    let actions = vec![0.0f32; N_ENVS];

    // Warm up pipeline cache so neither path pays cold-start cost.
    let mut warm = make_states();
    backend.step(&mut warm, &actions, &cfg).unwrap();

    // Path A: N separate dispatches.
    let mut states_loop = make_states();
    let t = Instant::now();
    for _ in 0..N_STEPS {
        backend.step(&mut states_loop, &actions, &cfg).unwrap();
    }
    let elapsed_loop = t.elapsed();

    // Path B: single batched dispatch.
    let mut states_batched = make_states();
    let t = Instant::now();
    backend
        .step_n(&mut states_batched, &actions, &cfg, N_STEPS)
        .unwrap();
    let elapsed_batched = t.elapsed();

    let speedup = elapsed_loop.as_nanos() as f64 / elapsed_batched.as_nanos() as f64;

    println!("\n=== step_n_persistent speedup ===");
    println!("N_steps              : {N_STEPS}");
    println!("envs                 : {N_ENVS}");
    println!("loop wall            : {:?}", elapsed_loop);
    println!("step_n wall          : {:?}", elapsed_batched);
    println!("speedup              : {speedup:.2}×");
    let loop_per_step = elapsed_loop.as_nanos() as f64 / N_STEPS as f64 / 1e3;
    let batched_per_step = elapsed_batched.as_nanos() as f64 / N_STEPS as f64 / 1e3;
    println!("loop per-step        : {loop_per_step:.1} μs");
    println!("step_n per-step      : {batched_per_step:.1} μs");
    println!("===================================");

    assert!(
        speedup >= 5.0,
        "R1.2 persistent-state should give ≥5× speedup (got {speedup:.2}×). \
         Theoretical headroom per iter-10 overhead profile is ~N× = ~100×, \
         so anything less than 5× indicates implementation regression \
         (e.g., barriers per dispatch instead of per pass, or accidental \
         multi-submit)."
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
