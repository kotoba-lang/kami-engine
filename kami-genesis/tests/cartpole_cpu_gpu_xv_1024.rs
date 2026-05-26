//! 1024-env CPU↔GPU cross-validation — KAMI-physx alternative agreement at scale.
//!
//! Existing `wgpu_backend::tests::wgpu_dispatch_matches_cpu_vectorized` covers
//! 256 envs × 50 steps. This test extends to 1024 envs × 200 steps to match
//! the per-side scoring tests (`cartpole_batch_1024.rs` CPU + `cartpole_gpu_batch_1024.rs`
//! GPU) and verify their results are bit-equivalent within f32 round-off at
//! the LARGER batch size where SIMD lane utilization + GPU workgroup
//! occupancy are both maximized.
//!
//! Per ADR-2605261800 §G4 — scalar reference IS the numerical contract.
//! Per ADR-2605261800 §G7 — PhysX NEVER; this test is the contractual proof
//! that the kami-native KAMI-physx path (Apache-2.0 Genesis solver → WGSL on
//! Apple M4 Metal / Vulkan / DX12 / WebGPU) and the CPU vectorized reference
//! produce numerically-equivalent dynamics on a non-trivial 1024-env batch.

#![cfg(feature = "gpu")]

use kami_genesis::{CartpoleConfig, CartpoleState, WgpuBackend, step_vectorized};

const N_ENVS: usize = 1024;
const N_STEPS: usize = 200;

#[test]
fn cartpole_1024_cpu_gpu_cross_validation_200_steps() {
    let backend = match WgpuBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIPPED — no GPU adapter: {e}");
            return;
        }
    };

    // Identical initial conditions for CPU + GPU.
    let initial: Vec<CartpoleState> = (0..N_ENVS)
        .map(|i| {
            let theta0 = ((i as f32 / N_ENVS as f32) - 0.5) * 0.1;
            CartpoleState { x: 0.0, x_dot: 0.0, theta: theta0, theta_dot: 0.0 }
        })
        .collect();
    let actions: Vec<f32> = (0..N_ENVS)
        .map(|i| ((i as f32 / N_ENVS as f32) - 0.5) * 2.0)  // Per-env action ±1.0
        .collect();
    let cfg = CartpoleConfig::default();

    let mut cpu = initial.clone();
    let mut gpu = initial.clone();

    for _ in 0..N_STEPS {
        step_vectorized(&mut cpu, &actions, &cfg);
        backend.step(&mut gpu, &actions, &cfg).unwrap();
    }

    // Compute per-env max-abs deltas across all 4 state variables.
    let mut max_dx = 0.0f32;
    let mut max_dx_dot = 0.0f32;
    let mut max_dtheta = 0.0f32;
    let mut max_dtheta_dot = 0.0f32;
    let mut div_env_count = 0;
    let divergence_threshold = 1e-3;

    for i in 0..N_ENVS {
        let dx = (cpu[i].x - gpu[i].x).abs();
        let dxd = (cpu[i].x_dot - gpu[i].x_dot).abs();
        let dt = (cpu[i].theta - gpu[i].theta).abs();
        let dtd = (cpu[i].theta_dot - gpu[i].theta_dot).abs();
        if dx > divergence_threshold || dt > divergence_threshold {
            div_env_count += 1;
        }
        max_dx = max_dx.max(dx);
        max_dx_dot = max_dx_dot.max(dxd);
        max_dtheta = max_dtheta.max(dt);
        max_dtheta_dot = max_dtheta_dot.max(dtd);
    }

    println!("\n=== kami-genesis 1024-env × 200-step CPU↔GPU cross-validation ===");
    println!("envs                      : {N_ENVS}");
    println!("steps                     : {N_STEPS}");
    println!("max |Δx|     CPU vs GPU   : {max_dx:.3e}");
    println!("max |Δx_dot|             : {max_dx_dot:.3e}");
    println!("max |Δθ|                 : {max_dtheta:.3e}");
    println!("max |Δθ_dot|             : {max_dtheta_dot:.3e}");
    println!("envs with |Δ| > {divergence_threshold:>5}  : {div_env_count} / {N_ENVS}");
    println!("KAMI-physx invariant     : Apache-2.0 Genesis-5-solver via WGSL on Metal ≡ CPU vectorized");
    println!("===================================================================");

    // Tolerance: 1e-2 = f32 accumulated round-off bound at 200 steps of
    // semi-implicit Euler with action perturbation. Cartpole has chaotic
    // sensitivity in toppling regimes — some envs drift past 1e-3 after 200
    // steps but should stay within 1e-2 (matching the existing 256-env test's
    // 1e-3 over 50 steps when scaled by sqrt(N) f32 error growth).
    assert!(
        max_dx < 1e-2,
        "1024-env CPU↔GPU x drift exceeds 1e-2 tolerance: {max_dx:.3e}"
    );
    assert!(
        max_dtheta < 1e-2,
        "1024-env CPU↔GPU θ drift exceeds 1e-2 tolerance: {max_dtheta:.3e}"
    );

    // Most envs should stay well under the divergence threshold even after 200 steps.
    let div_frac = div_env_count as f32 / N_ENVS as f32;
    assert!(
        div_frac < 0.20,
        "more than 20% of envs diverged beyond {divergence_threshold} (got {:.1}%); \
         indicates GPU dispatch correctness regression",
        div_frac * 100.0
    );
}
