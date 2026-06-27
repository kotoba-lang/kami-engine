//! step_n long-trajectory correctness — extends iter 11 (100 steps) to 1000 steps
//! to verify R1.2 persistent-state correctness holds over longer integrations
//! where f32 round-off + dynamic divergence have more opportunity to surface
//! a divergence between the step_n batched path and the step() loop path.
//!
//! Both paths should produce IDENTICAL results (same kernel, same algorithm,
//! same input, same number of dispatches). Only difference is how dispatches
//! are submitted (N×Submit vs 1×Submit-N-passes). The GPU does identical
//! arithmetic in identical order in both cases.
//!
//! Per ADR-2605261800 §G4 — scalar reference IS the numerical contract.
//! This test extends the contract horizon to 1000 steps × 1024 envs.

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;
const N_STEPS_LONG: usize = 1000;

#[test]
fn step_n_long_trajectory_matches_step_loop_at_1000_steps() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = CartpoleConfig::default();
    let actions = vec![0.0f32; N_ENVS];
    let initial: Vec<CartpoleState> = (0..N_ENVS)
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

    // Loop path baseline.
    let mut loop_states = initial.clone();
    let t = Instant::now();
    for _ in 0..N_STEPS_LONG {
        backend.step(&mut loop_states, &actions, &cfg).unwrap();
    }
    let loop_wall = t.elapsed();

    // Batched path.
    let mut batched_states = initial.clone();
    let t = Instant::now();
    backend
        .step_n(&mut batched_states, &actions, &cfg, N_STEPS_LONG)
        .unwrap();
    let batched_wall = t.elapsed();

    // Compute deltas across full state vector.
    let mut max_dx = 0.0f32;
    let mut max_dxd = 0.0f32;
    let mut max_dt = 0.0f32;
    let mut max_dtd = 0.0f32;
    for i in 0..N_ENVS {
        max_dx = max_dx.max((loop_states[i].x - batched_states[i].x).abs());
        max_dxd = max_dxd.max((loop_states[i].x_dot - batched_states[i].x_dot).abs());
        max_dt = max_dt.max((loop_states[i].theta - batched_states[i].theta).abs());
        max_dtd = max_dtd.max((loop_states[i].theta_dot - batched_states[i].theta_dot).abs());
    }

    let speedup = loop_wall.as_nanos() as f64 / batched_wall.as_nanos() as f64;

    println!("\n=== step_n long-trajectory (1000 steps × 1024 envs) ===");
    println!("loop wall          : {:?}", loop_wall);
    println!("step_n wall        : {:?}", batched_wall);
    println!("speedup            : {speedup:.2}×");
    println!("max |Δx|     loop vs n  : {max_dx:.3e}");
    println!("max |Δx_dot|             : {max_dxd:.3e}");
    println!("max |Δθ|                 : {max_dt:.3e}");
    println!("max |Δθ_dot|             : {max_dtd:.3e}");
    println!("=========================================================");

    // Same kernel + same algorithm + same arithmetic order → bit-identical.
    assert!(max_dx < 1e-5, "x drift over 1000 steps: {max_dx:.3e}");
    assert!(max_dxd < 1e-5, "x_dot drift: {max_dxd:.3e}");
    assert!(max_dt < 1e-5, "theta drift: {max_dt:.3e}");
    assert!(max_dtd < 1e-5, "theta_dot drift: {max_dtd:.3e}");

    // Speedup should remain ≥10× at 1000 steps (vs 13× at 100 steps from iter 11;
    // longer trajectory amortizes the single setup cost better, but readback +
    // single submit per-call overhead remains constant).
    assert!(
        speedup >= 10.0,
        "step_n speedup at 1000 steps should stay ≥10× (got {speedup:.2}×)"
    );
}
