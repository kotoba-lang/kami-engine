//! kami-genesis extreme-scale rollout — 1024 envs × 500 steps = 512K env-steps
//! via existing `WgpuBackend::step()` loop on Apple M4 Metal / Vulkan / DX12 /
//! WebGPU universal backend.
//!
//! Demonstrates the practical RL training scale that the kami-native KAMI-physx
//! path (Apache-2.0 Genesis-5-solver via WGSL) sustains via existing public API.
//! Per ADR-2605261800 §G7 — PhysX NEVER constitutional invariant operational
//! at non-trivial batch sizes.
//!
//! Note: an earlier R1.2 persistent-GPU-state experiment (step_n + step_double_pendulum_n)
//! was rolled back by the maintainer team. This test uses ONLY the canonical
//! public API (step / step_double_pendulum) and characterizes baseline GPU
//! throughput at that API surface.

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend};
use std::time::Instant;

const N_ENVS: usize = 1024;
const N_STEPS: usize = 500;

#[test]
fn cartpole_512k_env_steps_canonical_api_throughput() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let cfg = CartpoleConfig::default();
    let actions = vec![0.0f32; N_ENVS];
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

    // Warm-up to exclude pipeline-cache cost from the throughput measurement.
    let mut warm = states.clone();
    backend.step(&mut warm, &actions, &cfg).unwrap();

    let total_env_steps = N_ENVS * N_STEPS;

    let t = Instant::now();
    for _ in 0..N_STEPS {
        backend.step(&mut states, &actions, &cfg).unwrap();
    }
    let wall = t.elapsed();

    let throughput_m_per_sec = total_env_steps as f64 / wall.as_secs_f64() / 1e6;
    let ns_per_env_step = wall.as_nanos() as f64 / total_env_steps as f64;
    let us_per_dispatch = wall.as_nanos() as f64 / N_STEPS as f64 / 1e3;

    // Final-state diagnostics.
    let theta_min = states.iter().map(|s| s.theta).fold(f32::INFINITY, f32::min);
    let theta_max = states
        .iter()
        .map(|s| s.theta)
        .fold(f32::NEG_INFINITY, f32::max);
    let mut env_count_past_pi = 0;
    for s in &states {
        if s.theta.abs() > std::f32::consts::PI {
            env_count_past_pi += 1;
        }
    }
    let frac_past_pi = env_count_past_pi as f32 / N_ENVS as f32;

    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║   kami-genesis EXTREME SCALE — 1024 envs × 500 steps              ║");
    println!("║   = 512 K env-steps via canonical step() API on Metal GPU         ║");
    println!("║   ADR-2605261800 §G7 PhysX NEVER constitutional invariant         ║");
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ total env-steps     : {:>12}                                ║",
        total_env_steps
    );
    println!(
        "║ wall clock          : {:>10}                              ║",
        format!("{:?}", wall)
    );
    println!(
        "║ μs per dispatch     : {:>10.1}                                ║",
        us_per_dispatch
    );
    println!(
        "║ ns per env-step     : {:>10.1}                                ║",
        ns_per_env_step
    );
    println!(
        "║ throughput          : {:>10.2} M env-steps/sec                ║",
        throughput_m_per_sec
    );
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ θ range             : [{:>7.2}, {:>7.2}] rad                  ║",
        theta_min, theta_max
    );
    println!(
        "║ envs past ±π        : {:>5} / {} ({:.1}%)                       ║",
        env_count_past_pi,
        N_ENVS,
        frac_past_pi * 100.0
    );
    println!("╚═══════════════════════════════════════════════════════════════════╝");

    // Acceptance gates:
    // 1. Baseline throughput characterization (canonical step() loop on GPU).
    //    Per iter-10 overhead profile: ~1.7 μs/dispatch × 1024 envs = ~0.6 M env-steps/sec
    //    Realistic floor: 0.3 M env-steps/sec.
    assert!(
        throughput_m_per_sec >= 0.3,
        "canonical step() loop throughput should be ≥0.3 M env-steps/sec at 1024 envs (got {throughput_m_per_sec:.2})"
    );

    // 2. Physics: zero-control divergence reaches full rotation range.
    assert!(
        theta_max.abs() > std::f32::consts::PI || theta_min.abs() > std::f32::consts::PI,
        "at extreme scale, some envs should span past ±π; got [{theta_min:.2}, {theta_max:.2}]"
    );

    // 3. No NaN/inf at extreme scale.
    for s in &states {
        assert!(s.x.is_finite(), "x went non-finite at extreme scale");
        assert!(
            s.theta.is_finite(),
            "theta went non-finite at extreme scale"
        );
    }
}
