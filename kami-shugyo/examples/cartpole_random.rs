//! Cartpole random-policy baseline runner — R1.1 reward-curve evidence.
//!
//! Usage:
//!   cargo run --example cartpole_random --release \
//!     -p kami-shugyo > 70-tools/e7m-sim/benches/cartpole-r1.1-random-baseline.jsonl
//!
//! Produces one NDJSON line per episode:
//!   {"seed":N,"ep":I,"steps":S,"return":R,"terminated":bool,"truncated":bool}

use kami_shugyo::{CartpoleEnv, RLEnv, load_scene_yaml};

const URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");
const SCENE: &str = include_str!("../../fixtures/cartpole/scene.yaml");

fn run_episode(env: &mut CartpoleEnv, seed: u64, rng_state: &mut u64) -> EpisodeResult {
    env.reset(Some(seed));
    let mut steps = 0u32;
    let mut total_r = 0.0f32;
    loop {
        // U(-1,1) action — pure random policy (no learning).
        *rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u = ((*rng_state >> 33) as f32) / (1u64 << 31) as f32; // [0,1)
        let force = (u * 2.0 - 1.0) * 10.0; // ±10 N random force
        let r = env.step(&[force]);
        steps += 1;
        total_r += r.reward;
        if r.terminated || r.truncated {
            return EpisodeResult {
                steps,
                return_: total_r,
                terminated: r.terminated,
                truncated: r.truncated,
            };
        }
    }
}

struct EpisodeResult {
    steps: u32,
    return_: f32,
    terminated: bool,
    truncated: bool,
}

fn main() {
    let cfg = load_scene_yaml(SCENE).expect("scene.yaml");
    // R1.1 quality-gate baseline: 1000 episodes × 5 seeds (per scene.yaml).
    let seeds = [42u64, 1337, 8675309, 271828, 314159];
    let episodes_per_seed = 1000;
    let mut env = CartpoleEnv::new(cfg, URDF).expect("env build");

    for &seed in &seeds {
        let mut rng_state = seed;
        for ep in 0..episodes_per_seed {
            let r = run_episode(&mut env, seed.wrapping_add(ep as u64), &mut rng_state);
            println!(
                "{{\"seed\":{},\"ep\":{},\"steps\":{},\"return\":{:.4},\"terminated\":{},\"truncated\":{}}}",
                seed, ep, r.steps, r.return_, r.terminated, r.truncated
            );
        }
    }
}
