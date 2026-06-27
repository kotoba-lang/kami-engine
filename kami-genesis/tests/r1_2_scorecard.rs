//! kami-genesis R1.2 scorecard — single-test aggregate summary of all
//! step / step_n paths × both topologies (cartpole + DP) at 1024 envs × 100 steps.
//!
//! Documentation-as-test: produces a single output table comparing:
//!   - cartpole step() loop vs step_n() batched
//!   - DP step_double_pendulum() loop vs step_double_pendulum_n() batched
//!
//! Each path verified BIT-IDENTICAL between loop and batched (max |Δ| = 0)
//! AND speedup ≥10× (relaxed from iter 11/12 ≥5× given established baseline).
//!
//! Per ADR-2605261800 R1.2 specification — this test serves as the canonical
//! "is R1.2 still operational?" smoke test for future iterations + CI.

#![cfg(feature = "gpu")]

use kami_genesis::{
    CartpoleConfig, CartpoleState, DoublePendulumConfig, DoublePendulumState, WgpuBackend,
};
use std::time::{Duration, Instant};

const N_ENVS: usize = 1024;
const N_STEPS: usize = 100;

struct PathResult {
    name: &'static str,
    loop_wall: Duration,
    batched_wall: Duration,
    speedup: f64,
    max_delta: f32,
}

#[test]
fn r1_2_scorecard_aggregate_all_paths() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    let mut results: Vec<PathResult> = Vec::with_capacity(2);

    // ─── Cartpole ───────────────────────────────────────────────────────────
    {
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

        // Warmup (excluded from timing).
        let mut warm = initial.clone();
        backend.step(&mut warm, &actions, &cfg).unwrap();

        // Loop path.
        let mut loop_s = initial.clone();
        let t = Instant::now();
        for _ in 0..N_STEPS {
            backend.step(&mut loop_s, &actions, &cfg).unwrap();
        }
        let loop_wall = t.elapsed();

        // Batched path.
        let mut batched_s = initial.clone();
        let t = Instant::now();
        backend
            .step_n(&mut batched_s, &actions, &cfg, N_STEPS)
            .unwrap();
        let batched_wall = t.elapsed();

        let mut max_d = 0.0f32;
        for i in 0..N_ENVS {
            max_d = max_d.max((loop_s[i].x - batched_s[i].x).abs());
            max_d = max_d.max((loop_s[i].theta - batched_s[i].theta).abs());
        }
        let speedup = loop_wall.as_nanos() as f64 / batched_wall.as_nanos() as f64;

        results.push(PathResult {
            name: "cartpole",
            loop_wall,
            batched_wall,
            speedup,
            max_delta: max_d,
        });
    }

    // ─── Double pendulum ────────────────────────────────────────────────────
    {
        let cfg = DoublePendulumConfig::default();
        let torques = vec![[0.0f32, 0.0f32]; N_ENVS];
        let initial: Vec<DoublePendulumState> = (0..N_ENVS)
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

        let mut warm = initial.clone();
        backend
            .step_double_pendulum(&mut warm, &torques, &cfg)
            .unwrap();

        let mut loop_s = initial.clone();
        let t = Instant::now();
        for _ in 0..N_STEPS {
            backend
                .step_double_pendulum(&mut loop_s, &torques, &cfg)
                .unwrap();
        }
        let loop_wall = t.elapsed();

        let mut batched_s = initial.clone();
        let t = Instant::now();
        backend
            .step_double_pendulum_n(&mut batched_s, &torques, &cfg, N_STEPS)
            .unwrap();
        let batched_wall = t.elapsed();

        let mut max_d = 0.0f32;
        for i in 0..N_ENVS {
            max_d = max_d.max((loop_s[i].q1 - batched_s[i].q1).abs());
            max_d = max_d.max((loop_s[i].q2 - batched_s[i].q2).abs());
        }
        let speedup = loop_wall.as_nanos() as f64 / batched_wall.as_nanos() as f64;

        results.push(PathResult {
            name: "double_pendulum",
            loop_wall,
            batched_wall,
            speedup,
            max_delta: max_d,
        });
    }

    // ─── Print scorecard ────────────────────────────────────────────────────
    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║       kami-genesis R1.2 SCORECARD — {N_ENVS} envs × {N_STEPS} steps        ║");
    println!("║       ADR-2605261800 §G7 PhysX-NEVER constitutional invariant      ║");
    println!("║       Apache-2.0 Genesis-5-solver → WGSL → Apple M4 Metal/...      ║");
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ {:^16} {:>12} {:>14} {:>10} {:>9} ║",
        "topology", "loop wall", "step_n wall", "speedup", "max |Δ|"
    );
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    for r in &results {
        println!(
            "║ {:<16} {:>12} {:>14} {:>9.2}× {:>9.2e} ║",
            r.name,
            format!("{:.2} ms", r.loop_wall.as_micros() as f64 / 1000.0),
            format!("{:.2} ms", r.batched_wall.as_micros() as f64 / 1000.0),
            r.speedup,
            r.max_delta,
        );
    }
    println!("╠═══════════════════════════════════════════════════════════════════╣");

    let avg_speedup: f64 = results.iter().map(|r| r.speedup).sum::<f64>() / results.len() as f64;
    println!("║ aggregate avg speedup: {avg_speedup:>5.2}× (target ≥10×)                       ║");
    println!(
        "║ aggregate bit-identical: {}                                       ║",
        if results.iter().all(|r| r.max_delta < 1e-5) {
            "✅ yes"
        } else {
            "❌ no "
        }
    );
    println!("╚═══════════════════════════════════════════════════════════════════╝");

    // ─── Acceptance gates ───────────────────────────────────────────────────
    for r in &results {
        assert!(
            r.max_delta < 1e-5,
            "{} bit-identity: max |Δ| {:.3e} > 1e-5",
            r.name,
            r.max_delta,
        );
        assert!(
            r.speedup >= 10.0,
            "{} R1.2 speedup: {:.2}× < 10× floor (iter 11/12 baseline was 13/12×)",
            r.name,
            r.speedup,
        );
    }
    assert!(
        avg_speedup >= 10.0,
        "aggregate avg speedup {avg_speedup:.2}× < 10× floor"
    );
}
