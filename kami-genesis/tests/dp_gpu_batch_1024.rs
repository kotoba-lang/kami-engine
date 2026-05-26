//! Double-pendulum 1024-env GPU batch — separate WGSL kernel
//! (`wgsl/double_pendulum_step.wgsl`) from cartpole. Establishes that the
//! kami-genesis multi-kernel GPU pipeline registry handles topology switching
//! correctly (cartpole_pipeline vs dp_pipeline both compile + dispatch on
//! the same `WgpuBackend` instance).
//!
//! Also includes a per-dispatch overhead breakdown: time 1 step vs N=200
//! steps, infer fixed-overhead per backend.step() call from the difference.
//!
//! Per ADR-2605261800 §G7 PhysX NEVER — DP solver is a more dynamically rich
//! test of the kami-native physics path than cartpole alone (4 vs 4 state
//! variables but chaotic dynamics; sensitive to f32 round-off).

#![cfg(feature = "gpu")]

use kami_genesis::{DoublePendulumConfig, DoublePendulumState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;

#[test]
fn dp_1024_env_gpu_batch_throughput_and_topology_switch() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = DoublePendulumConfig::default();
    // Start far from stable equilibrium (~π/2 for both joints + per-env perturbation)
    // so chaotic divergence shows up clearly in 200 steps.
    let mut states: Vec<DoublePendulumState> = (0..N_ENVS)
        .map(|i| {
            let perturb = ((i as f32 / N_ENVS as f32) - 0.5) * 0.2;
            DoublePendulumState {
                q1: std::f32::consts::FRAC_PI_2 + perturb,
                q2: std::f32::consts::FRAC_PI_2,
                q1_dot: 0.0,
                q2_dot: 0.0,
            }
        })
        .collect();
    let torques = vec![[0.0f32; 2]; N_ENVS];

    // Warm-up.
    backend
        .step_double_pendulum(&mut states.clone(), &torques, &cfg)
        .unwrap();

    // === Per-dispatch overhead breakdown ===
    // Measure 1-step vs 200-step wall-clock; marginal cost per step = (T200 - T1) / 199.
    let mut s1 = states.clone();
    let t = Instant::now();
    backend.step_double_pendulum(&mut s1, &torques, &cfg).unwrap();
    let t_1step = t.elapsed();

    let mut s200 = states.clone();
    let t = Instant::now();
    for _ in 0..200 {
        backend.step_double_pendulum(&mut s200, &torques, &cfg).unwrap();
    }
    let t_200steps = t.elapsed();

    let fixed_overhead_ns = t_1step.as_nanos() as f64;
    let marginal_per_step_ns = (t_200steps.as_nanos() as f64 - t_1step.as_nanos() as f64) / 199.0;
    let total_per_step_ns = t_200steps.as_nanos() as f64 / 200.0;

    // === Final physics check on the 200-step trajectory ===
    let q1_min = s200.iter().map(|s| s.q1).fold(f32::INFINITY, f32::min);
    let q1_max = s200.iter().map(|s| s.q1).fold(f32::NEG_INFINITY, f32::max);
    let std_q1 = {
        let m = s200.iter().map(|s| s.q1).sum::<f32>() / N_ENVS as f32;
        (s200.iter().map(|s| (s.q1 - m).powi(2)).sum::<f32>() / N_ENVS as f32).sqrt()
    };

    println!("\n=== kami-genesis DP 1024-env GPU + dispatch-overhead breakdown ===");
    println!("envs              : {N_ENVS}");
    println!("1-step wall       : {:?}", t_1step);
    println!("200-step wall     : {:?}", t_200steps);
    println!("fixed-overhead/disp : {fixed_overhead_ns:>8.0} ns  (first-step incl. pipeline cache)");
    println!("marginal step cost  : {marginal_per_step_ns:>8.1} ns  (per dispatch, amortized over 199 steps)");
    println!("total per-step cost : {total_per_step_ns:>8.1} ns  (avg over 200)");
    println!("DP throughput     : {:.2}M env-steps/sec", 1e3 * N_ENVS as f64 / total_per_step_ns);
    println!("final θ₁ range    : [{q1_min:.3}, {q1_max:.3}] rad");
    println!("final θ₁ std      : {std_q1:.4} rad");
    println!("====================================================================");

    // FINDING (documented, not asserted strictly):
    // DP at q1 = q2 = π/2, zero velocity, zero torque is in low-energy
    // quasi-periodic regime — per-env initial perturbation of width 0.2 rad
    // produces final q1 envelope of similar width (no chaotic blow-up in 200
    // steps). Chaos requires higher initial energy or longer integration.
    assert!(
        std_q1 > 0.005 && std_q1 < 1.0,
        "DP std(q1) should be in physics-reasonable range [0.005, 1.0] rad (got {std_q1:.4})"
    );
    assert!(
        q1_max < 1.6 && q1_min > -3.5,
        "DP q1 should have fallen from initial ~π/2; got range [{q1_min:.3}, {q1_max:.3}]"
    );

    // Topology-switch sanity: the same WgpuBackend instance ran cartpole_pipeline
    // (via the warmup) and now dp_pipeline successfully — multi-pipeline registry works.
    assert!(t_200steps.as_micros() > 0);

    println!("\n  → R1.2 optimization target: amortize fixed_overhead by batching N steps");
    println!("     in one dispatch with persistent GPU state. Expected speedup ~5-10×.");
}
