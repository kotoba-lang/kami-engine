//! CPU-side vectorized Cartpole simulator.
//!
//! Bit-for-bit reference implementation of `wgsl/cartpole_step.wgsl`. The WGSL
//! kernel runs one env per global invocation; this Rust function steps N envs
//! in a tight loop using the same formulas. A unit test verifies the scalar
//! `CartpoleState::step` and this vectorized version produce identical state
//! evolution.
//!
//! At R1.x, `kami-genesis-wgpu` (deferred crate) will wire the same WGSL into
//! `wgpu::ComputePipeline` and dispatch over storage buffers — the boundary
//! is the WGSL file, not this Rust wrapper.

use crate::cartpole::{CartpoleConfig, CartpoleState};

/// The exact WGSL source that the wgpu backend will compile at R1.x.
pub const WGSL_SOURCE: &str = include_str!("wgsl/cartpole_step.wgsl");

/// Run one `step` per env in `states` with per-env `actions`, mirroring WGSL.
/// All envs share the same physics config — use `step_vectorized_per_env` for
/// per-env domain randomisation (sim2real).
pub fn step_vectorized(
    states: &mut [CartpoleState],
    actions: &[f32],
    cfg: &CartpoleConfig,
) {
    assert_eq!(
        states.len(),
        actions.len(),
        "states.len() must equal actions.len() (1 action per env)"
    );
    for i in 0..states.len() {
        states[i].step(actions[i], cfg);
    }
}

/// Per-env-config variant for domain randomisation: each env gets its own
/// CartpoleConfig (cart_mass / pole_mass / pole_half_length / gravity /
/// force_mag / dt). `states.len() == actions.len() == cfgs.len()` is required.
pub fn step_vectorized_per_env(
    states: &mut [CartpoleState],
    actions: &[f32],
    cfgs: &[CartpoleConfig],
) {
    assert_eq!(states.len(), actions.len(), "states.len() must equal actions.len()");
    assert_eq!(states.len(), cfgs.len(), "states.len() must equal cfgs.len()");
    for i in 0..states.len() {
        states[i].step(actions[i], &cfgs[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectorized_matches_scalar_for_identical_envs() {
        // Drive 1024 envs with the same action and confirm all states agree
        // with the scalar reference after 100 steps.
        let cfg = CartpoleConfig::default();
        let n = 1024;
        let mut vstates = vec![
            CartpoleState { theta: 0.05, ..Default::default() };
            n
        ];
        let actions = vec![5.0_f32; n];
        for _ in 0..100 {
            step_vectorized(&mut vstates, &actions, &cfg);
        }

        let mut scalar = CartpoleState { theta: 0.05, ..Default::default() };
        for _ in 0..100 {
            scalar.step(5.0, &cfg);
        }

        for s in vstates {
            assert!((s.x - scalar.x).abs() < 1e-6);
            assert!((s.theta - scalar.theta).abs() < 1e-6);
        }
    }

    #[test]
    fn vectorized_handles_distinct_per_env_actions() {
        // Two envs, two different forces: they must diverge.
        let cfg = CartpoleConfig::default();
        let mut states = vec![CartpoleState::default(), CartpoleState::default()];
        let actions = vec![10.0_f32, -10.0_f32];
        for _ in 0..60 {
            step_vectorized(&mut states, &actions, &cfg);
        }
        assert!(states[0].x > 0.0, "+10 N → cart moves +x");
        assert!(states[1].x < 0.0, "-10 N → cart moves -x");
    }

    #[test]
    fn wgsl_source_embeds_state_struct() {
        // Sanity-check the WGSL string is the canonical kernel; we don't need
        // to compile it here (that lands when kami-genesis-wgpu plugs wgpu).
        assert!(WGSL_SOURCE.contains("struct State"));
        assert!(WGSL_SOURCE.contains("@workgroup_size(64)"));
        assert!(WGSL_SOURCE.contains("@compute"));
    }
}
