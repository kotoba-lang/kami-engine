//! Per-env domain randomisation for VectorizedCartpoleEnv.
//!
//! Mirrors the sim2real workflow that Isaac Lab / Replicator users follow:
//! each parallel env gets a slightly different physics config so the trained
//! policy generalises across the real-world parameter distribution.
//!
//! Each randomisable field has a `(low, high)` range; the apply helper draws
//! per-env values from a uniform LCG sampler (same constants as
//! kami_genesis::cartpole_env::Lcg → bit-reproducible across the Rust ↔
//! Python boundary).

use kami_genesis::CartpoleConfig;
use serde::{Deserialize, Serialize};

/// LCG sampler matching kami_genesis / kami_shugyo / nv_compat conventions.
#[derive(Debug, Clone, Copy)]
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407))
    }
    fn next_u01(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }
    fn next_uniform(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next_u01()
    }
}

/// Inclusive (low, high) pair. `apply(...)` samples `low + (high-low)·u01()`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub low: f32,
    pub high: f32,
}

impl Range {
    pub fn new(low: f32, high: f32) -> Self {
        Range { low, high }
    }
    /// Constant value (low == high): no randomisation.
    pub fn fixed(v: f32) -> Self {
        Range { low: v, high: v }
    }
}

/// Per-field DR ranges. Any field set to `Range::fixed(...)` keeps that
/// parameter constant across envs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainRandomizationCfg {
    pub cart_mass: Range,
    pub pole_mass: Range,
    pub pole_half_length: Range,
    pub gravity: Range,
    pub force_mag: Range,
    /// dt is per-env constant by convention; allow randomising for
    /// numerical robustness studies.
    pub dt: Range,
}

impl DomainRandomizationCfg {
    /// Default sim2real ranges around `base`: ±20% mass, ±5% length, ±5%
    /// gravity, fixed force_mag and dt.
    pub fn around(base: &CartpoleConfig) -> Self {
        DomainRandomizationCfg {
            cart_mass: Range::new(base.cart_mass * 0.8, base.cart_mass * 1.2),
            pole_mass: Range::new(base.pole_mass * 0.8, base.pole_mass * 1.2),
            pole_half_length: Range::new(base.pole_half_length * 0.95, base.pole_half_length * 1.05),
            gravity: Range::new(base.gravity * 0.95, base.gravity * 1.05),
            force_mag: Range::fixed(base.force_mag),
            dt: Range::fixed(base.dt),
        }
    }

    /// No randomisation at all — every env gets `base` exactly.
    pub fn identity(base: &CartpoleConfig) -> Self {
        DomainRandomizationCfg {
            cart_mass: Range::fixed(base.cart_mass),
            pole_mass: Range::fixed(base.pole_mass),
            pole_half_length: Range::fixed(base.pole_half_length),
            gravity: Range::fixed(base.gravity),
            force_mag: Range::fixed(base.force_mag),
            dt: Range::fixed(base.dt),
        }
    }

    /// Sample one CartpoleConfig from the DR distribution using a seed.
    pub fn sample(&self, base: &CartpoleConfig, seed: u64) -> CartpoleConfig {
        let mut rng = Lcg::new(seed);
        CartpoleConfig {
            cart_mass: rng.next_uniform(self.cart_mass.low, self.cart_mass.high),
            pole_mass: rng.next_uniform(self.pole_mass.low, self.pole_mass.high),
            pole_half_length: rng.next_uniform(self.pole_half_length.low, self.pole_half_length.high),
            gravity: rng.next_uniform(self.gravity.low, self.gravity.high),
            force_mag: rng.next_uniform(self.force_mag.low, self.force_mag.high),
            dt: rng.next_uniform(self.dt.low, self.dt.high),
            // `..base` would help but `CartpoleConfig` has no spread support.
            // All fields explicit above.
            ..*base
        }
    }

    /// Produce N per-env configs from this DR distribution, seeded reproducibly.
    pub fn sample_n(&self, base: &CartpoleConfig, n: usize, base_seed: u64) -> Vec<CartpoleConfig> {
        (0..n)
            .map(|i| self.sample(base, base_seed.wrapping_add(i as u64)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> CartpoleConfig {
        CartpoleConfig {
            cart_mass: 1.0,
            pole_mass: 0.1,
            pole_half_length: 0.25,
            gravity: 9.81,
            force_mag: 100.0,
            dt: 1.0 / 60.0,
        }
    }

    #[test]
    fn identity_dr_produces_base_exactly() {
        let b = base();
        let cfg = DomainRandomizationCfg::identity(&b);
        for s in 0..5u64 {
            let sampled = cfg.sample(&b, s);
            assert_eq!(sampled.cart_mass, b.cart_mass);
            assert_eq!(sampled.pole_mass, b.pole_mass);
            assert_eq!(sampled.gravity, b.gravity);
        }
    }

    #[test]
    fn around_default_keeps_values_within_bounds() {
        let b = base();
        let cfg = DomainRandomizationCfg::around(&b);
        for s in 0..100u64 {
            let sampled = cfg.sample(&b, s);
            assert!(sampled.cart_mass >= 0.8 && sampled.cart_mass <= 1.2);
            assert!(sampled.pole_mass >= 0.08 && sampled.pole_mass <= 0.12);
            assert!(sampled.pole_half_length >= 0.2375 && sampled.pole_half_length <= 0.2625);
            assert!(sampled.gravity >= 9.3195 && sampled.gravity <= 10.3005);
        }
    }

    #[test]
    fn same_seed_produces_same_cfg() {
        let b = base();
        let cfg = DomainRandomizationCfg::around(&b);
        let a = cfg.sample(&b, 42);
        let bb = cfg.sample(&b, 42);
        assert_eq!(a, bb);
    }

    #[test]
    fn different_seeds_produce_different_cfgs() {
        let b = base();
        let cfg = DomainRandomizationCfg::around(&b);
        let a = cfg.sample(&b, 42);
        let bb = cfg.sample(&b, 43);
        assert_ne!(a, bb);
    }

    #[test]
    fn sample_n_produces_n_distinct_cfgs() {
        let b = base();
        let cfg = DomainRandomizationCfg::around(&b);
        let cfgs = cfg.sample_n(&b, 8, 100);
        assert_eq!(cfgs.len(), 8);
        // All should differ pairwise (extremely high probability with continuous DR).
        for i in 0..cfgs.len() {
            for j in (i + 1)..cfgs.len() {
                assert_ne!(cfgs[i], cfgs[j], "env {i} and {j} got identical cfgs");
            }
        }
    }
}
