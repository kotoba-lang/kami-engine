//! LQR-balancing baseline: closes the gap between random policy (iter A,
//! avg 9.3 steps) and Isaac Sim PPO baseline (~500 steps).
//!
//! Runs N parallel Cartpole envs with the LQR upright-balance controller and
//! reports average episode length + cumulative return.

use kami_genesis::{CartpoleConfig, CartpoleState, LqrController, LqrWeights};
use kami_shugyo::{VectorizedCartpoleEnv, load_scene_yaml};

const URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");
const SCENE: &str = include_str!("../../fixtures/cartpole/scene.yaml");

fn main() {
    let cfg = load_scene_yaml(SCENE).expect("scene.yaml");
    let lqr = LqrController::build(&CartpoleConfig::default(), LqrWeights::default());
    println!("LQR controller built: K={:?}", lqr.gain);
    println!(
        "DARE: {} iters, residual={:.3e}",
        lqr.dare_iters, lqr.dare_residual
    );
    println!();

    let n_envs = 128;
    let n_episodes = 256;
    let mut env = VectorizedCartpoleEnv::new(n_envs, cfg, URDF).unwrap();
    let mut total_steps = 0_u64;
    let mut total_return = 0.0_f64;
    let mut ep_lengths = Vec::with_capacity(n_episodes);
    let mut ep_returns = Vec::with_capacity(n_episodes);

    for ep in 0..n_episodes {
        env.reset_all(Some(ep as u64));
        let mut ep_len = vec![0_u32; n_envs];
        let mut ep_ret = vec![0.0_f64; n_envs];
        let mut alive = vec![true; n_envs];

        // Roll out up to a generous cap (max episode steps from scene.yaml).
        for _ in 0..300 {
            // Compute LQR action for each alive env from current state.
            let actions: Vec<f32> = (0..n_envs)
                .map(|i| {
                    if !alive[i] {
                        return 0.0;
                    }
                    let obs = env.observations_flat();
                    let s = CartpoleState {
                        x: obs[i * 4],
                        x_dot: obs[i * 4 + 1],
                        theta: obs[i * 4 + 2],
                        theta_dot: obs[i * 4 + 3],
                    };
                    lqr.control(&s)
                })
                .collect();
            let results = env.step_all(&actions);
            for i in 0..n_envs {
                if alive[i] {
                    ep_len[i] += 1;
                    ep_ret[i] += results[i].reward as f64;
                    if results[i].terminated || results[i].truncated {
                        alive[i] = false;
                    }
                }
            }
            if alive.iter().all(|&a| !a) {
                break;
            }
        }
        let avg_len: f64 = ep_len.iter().map(|&x| x as f64).sum::<f64>() / n_envs as f64;
        let avg_ret: f64 = ep_ret.iter().sum::<f64>() / n_envs as f64;
        total_steps += ep_len.iter().map(|&x| x as u64).sum::<u64>();
        total_return += ep_ret.iter().sum::<f64>();
        ep_lengths.push(avg_len);
        ep_returns.push(avg_ret);
    }

    let overall_avg_len = total_steps as f64 / (n_envs * n_episodes) as f64;
    let overall_avg_ret = total_return / (n_envs * n_episodes) as f64;
    println!("LQR baseline over {} eps × {} envs:", n_episodes, n_envs);
    println!("  Avg episode length: {:.2} steps", overall_avg_len);
    println!("  Avg episode return: {:.3}", overall_avg_ret);
    println!();
    println!("Random baseline (iter A, /loop run 1): 9.3 steps / return 7.10");
    println!("Improvement factor (steps): {:.1}×", overall_avg_len / 9.3);
}
