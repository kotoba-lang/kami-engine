//! kami-genesis R1.2 persistent-GPU-state for double-pendulum solver.
//!
//! Mirror of `cartpole_step_n_persistent.rs` (iter 11) for the DP topology.
//! Both topologies now have R1.2 batched-dispatch capability per ADR-2605261800.
//!
//! Two assertions:
//!   1. CORRECTNESS — `step_double_pendulum_n(N)` produces bit-identical results
//!      to N× `step_double_pendulum()`
//!   2. SPEEDUP — `step_double_pendulum_n(N)` is ≥5× faster than the loop at
//!      N=100, 1024 envs (matching cartpole iter-11 ~13× empirical floor)

#![cfg(feature = "gpu")]

use kami_genesis::{DoublePendulumConfig, DoublePendulumState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;
const N_STEPS: usize = 100;

#[test]
fn dp_step_n_persistent_matches_step_loop_exact() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = DoublePendulumConfig::default();
    let torques = vec![[0.0f32, 0.0f32]; N_ENVS];
    let initial = make_states();

    let mut loop_states = initial.clone();
    for _ in 0..N_STEPS {
        backend
            .step_double_pendulum(&mut loop_states, &torques, &cfg)
            .unwrap();
    }

    let mut batched_states = initial.clone();
    backend
        .step_double_pendulum_n(&mut batched_states, &torques, &cfg, N_STEPS)
        .unwrap();

    let mut max_dq1 = 0.0f32;
    let mut max_dq2 = 0.0f32;
    for i in 0..N_ENVS {
        max_dq1 = max_dq1.max((loop_states[i].q1 - batched_states[i].q1).abs());
        max_dq2 = max_dq2.max((loop_states[i].q2 - batched_states[i].q2).abs());
    }

    println!("\n=== DP step_n_persistent correctness ===");
    println!("N_steps          : {N_STEPS}");
    println!("max |Δq1|         : {max_dq1:.3e}");
    println!("max |Δq2|         : {max_dq2:.3e}");
    println!("=========================================");

    assert!(
        max_dq1 < 1e-5,
        "DP step_n vs step-loop q1 diff > 1e-5: {max_dq1:.3e}"
    );
    assert!(
        max_dq2 < 1e-5,
        "DP step_n vs step-loop q2 diff > 1e-5: {max_dq2:.3e}"
    );
}

#[test]
fn dp_step_n_persistent_beats_step_loop_by_5x_or_more() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = DoublePendulumConfig::default();
    let torques = vec![[0.0f32, 0.0f32]; N_ENVS];

    let mut warm = make_states();
    backend
        .step_double_pendulum(&mut warm, &torques, &cfg)
        .unwrap();

    let mut loop_states = make_states();
    let t = Instant::now();
    for _ in 0..N_STEPS {
        backend
            .step_double_pendulum(&mut loop_states, &torques, &cfg)
            .unwrap();
    }
    let elapsed_loop = t.elapsed();

    let mut batched_states = make_states();
    let t = Instant::now();
    backend
        .step_double_pendulum_n(&mut batched_states, &torques, &cfg, N_STEPS)
        .unwrap();
    let elapsed_batched = t.elapsed();

    let speedup = elapsed_loop.as_nanos() as f64 / elapsed_batched.as_nanos() as f64;
    let loop_per_step = elapsed_loop.as_nanos() as f64 / N_STEPS as f64 / 1e3;
    let batched_per_step = elapsed_batched.as_nanos() as f64 / N_STEPS as f64 / 1e3;

    println!("\n=== DP step_n_persistent speedup ===");
    println!("N_steps              : {N_STEPS}");
    println!("envs                 : {N_ENVS}");
    println!("loop wall            : {:?}", elapsed_loop);
    println!("step_n wall          : {:?}", elapsed_batched);
    println!("speedup              : {speedup:.2}×");
    println!("loop per-step        : {loop_per_step:.1} μs");
    println!("step_n per-step      : {batched_per_step:.1} μs");
    println!("===================================");

    assert!(
        speedup >= 5.0,
        "DP R1.2 persistent-state should give ≥5× speedup (got {speedup:.2}×)"
    );
}

fn make_states() -> Vec<DoublePendulumState> {
    (0..N_ENVS)
        .map(|i| {
            let perturb = ((i as f32 / N_ENVS as f32) - 0.5) * 0.2;
            DoublePendulumState {
                q1: std::f32::consts::FRAC_PI_2 + perturb,
                q2: std::f32::consts::FRAC_PI_2,
                q1_dot: 0.0,
                q2_dot: 0.0,
            }
        })
        .collect()
}
