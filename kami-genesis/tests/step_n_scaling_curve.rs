//! step_n speedup-vs-N scaling curve — sweeps N ∈ {1, 10, 50, 100, 500, 1000}
//! at 1024 envs to show how R1.2 batched-dispatch advantage grows with longer
//! trajectories.
//!
//! Per iter-10 overhead profile: per-step round-trip cost is ~1.7 ms (constant
//! across N). Per iter-11 step_n implementation: collapsing N dispatches into 1
//! submission with N back-to-back compute passes drops per-step cost ~10-20×.
//!
//! Expected scaling shape:
//!   - At N=1, step_n ≈ step() (single dispatch each way)
//!   - At N→∞, step_n speedup approaches theoretical N× ceiling
//!   - Practical asymptote ~15-25× (limited by per-submit constant cost +
//!     workgroup scheduling between passes)
//!
//! This test produces a scaling-curve table useful for benchmarking R1.2
//! optimization headroom + scoring R1.x progression in future iterations.

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;
const N_STEPS_GRID: &[usize] = &[1, 10, 50, 100, 500, 1000];

#[test]
fn step_n_speedup_scales_favorably_with_n() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = CartpoleConfig::default();
    let actions = vec![0.0f32; N_ENVS];

    // Warm-up pipeline cache.
    let mut warm = make_states();
    backend.step(&mut warm, &actions, &cfg).unwrap();
    backend.step_n(&mut warm, &actions, &cfg, 10).unwrap();

    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║         step_n speedup scaling — 1024 envs × variable N           ║");
    println!("║         R1.2 batched-dispatch headroom characterization           ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ {:>5} {:>12} {:>14} {:>14} {:>10} ║",
        "N", "loop (ms)", "step_n (ms)", "loop μs/step", "speedup"
    );
    println!("╠══════════════════════════════════════════════════════════════════╣");

    let mut speedups = Vec::with_capacity(N_STEPS_GRID.len());
    for &n_steps in N_STEPS_GRID {
        let mut loop_states = make_states();
        let t = Instant::now();
        for _ in 0..n_steps {
            backend.step(&mut loop_states, &actions, &cfg).unwrap();
        }
        let loop_wall = t.elapsed();

        let mut batched_states = make_states();
        let t = Instant::now();
        backend
            .step_n(&mut batched_states, &actions, &cfg, n_steps)
            .unwrap();
        let batched_wall = t.elapsed();

        let speedup = loop_wall.as_nanos() as f64 / batched_wall.as_nanos() as f64;
        let loop_per_step_us = loop_wall.as_nanos() as f64 / 1e3 / n_steps as f64;

        println!(
            "║ {:>5} {:>12.2} {:>14.2} {:>14.1} {:>9.2}× ║",
            n_steps,
            loop_wall.as_micros() as f64 / 1000.0,
            batched_wall.as_micros() as f64 / 1000.0,
            loop_per_step_us,
            speedup,
        );

        speedups.push((n_steps, speedup));
    }
    println!("╚══════════════════════════════════════════════════════════════════╝");

    // Find the speedup at the asymptotic end (largest N).
    let (n_max, max_speedup) = speedups.last().copied().unwrap();
    let (n_min, min_speedup) = speedups.first().copied().unwrap();

    println!("\n  Asymptotic (N={n_max}) speedup: {max_speedup:.2}×");
    println!("  Threshold (N={n_min}) speedup:   {min_speedup:.2}×");
    println!(
        "  Ratio (large/small N):       {:.1}×",
        max_speedup / min_speedup.max(0.1)
    );

    // Acceptance gates:
    //   1. At N=1, no expected speedup (≤2× is reasonable noise margin).
    //   2. At N≥100, speedup should be ≥10× (matches iter 11/12 baseline).
    //   3. At N=1000, speedup should be ≥15× (long-trajectory amortization).

    let small_n_speedup = speedups
        .iter()
        .find(|(n, _)| *n == 1)
        .map(|(_, s)| *s)
        .unwrap();
    assert!(
        small_n_speedup < 5.0,
        "at N=1, step_n shouldn't show >5× speedup (got {small_n_speedup:.2}×) — \
         either step() got slower or step_n bypasses single-step overhead artificially"
    );

    let medium_n_speedup = speedups
        .iter()
        .find(|(n, _)| *n == 100)
        .map(|(_, s)| *s)
        .unwrap();
    assert!(
        medium_n_speedup >= 10.0,
        "at N=100, step_n speedup should be ≥10× per iter-11 baseline (got {medium_n_speedup:.2}×)"
    );

    let large_n_speedup = speedups
        .iter()
        .find(|(n, _)| *n == 1000)
        .map(|(_, s)| *s)
        .unwrap();
    assert!(
        large_n_speedup >= 15.0,
        "at N=1000, step_n speedup should be ≥15× per iter-13 baseline (got {large_n_speedup:.2}×)"
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
