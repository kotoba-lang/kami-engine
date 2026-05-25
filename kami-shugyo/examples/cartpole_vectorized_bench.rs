//! Benchmark: VectorizedCartpoleEnv throughput at N=1024, N=4096, N=16384.
//!
//! Demonstrates scaling of the kami-genesis step_vectorized backend that
//! powers Isaac Lab-style RL training across many parallel envs.

use kami_shugyo::{VectorizedCartpoleEnv, load_scene_yaml};

const URDF: &str = include_str!(
    "../../../../70-tools/e7m-sim/scenes/cartpole/cartpole.urdf"
);
const SCENE: &str = include_str!(
    "../../../../70-tools/e7m-sim/scenes/cartpole/scene.yaml"
);

fn bench(num_envs: usize, steps: usize) -> (f64, f64) {
    let cfg = load_scene_yaml(SCENE).expect("scene.yaml");
    let mut env = VectorizedCartpoleEnv::new(num_envs, cfg, URDF).expect("env");
    env.reset_all(Some(42));

    // Random-ish per-env actions; same per step (deterministic for bench).
    let actions: Vec<f32> = (0..num_envs).map(|i| (i as f32 % 11.0) - 5.0).collect();

    let start = std::time::Instant::now();
    for _ in 0..steps {
        let _ = env.step_all(&actions);
    }
    let elapsed_s = start.elapsed().as_secs_f64();
    let env_steps_per_s = (num_envs * steps) as f64 / elapsed_s;
    (elapsed_s, env_steps_per_s)
}

fn main() {
    println!("VectorizedCartpoleEnv benchmark (kami-shugyo)");
    println!("=============================================");
    println!("Backend: kami_genesis::step_vectorized (CPU; WGSL fallback to wgpu in iter 1)");
    println!();
    for &(n, steps) in &[(32usize, 1000), (256, 500), (1024, 200), (4096, 100), (16_384, 50)] {
        let (elapsed, eps) = bench(n, steps);
        println!(
            "  N={:6}  steps={:5}  elapsed={:.3}s  throughput={:>12.0} env-steps/s",
            n, steps, elapsed, eps
        );
    }
}
