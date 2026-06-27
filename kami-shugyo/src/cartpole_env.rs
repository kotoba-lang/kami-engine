//! CartpoleEnv — gym-style wrapper around `kami-genesis` Cartpole World.
//!
//! Mirrors `isaaclab_tasks.manager_based.classic.cartpole.CartpoleEnvCfg`.
//! Observation = [x, x_dot, theta, theta_dot]; action = [force_on_cart].

use crate::scene_cfg::SceneCfg;
use crate::traits::{RLEnv, StepResult};
use kami_articulated::parse_urdf;
use kami_genesis::{ArticulationHandle, CartpoleState, World};

/// Deterministic LCG for seedable reset (no rand-crate dep — keeps WASM build slim).
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }
    fn next_f32_centered(&mut self, half_range: f32) -> f32 {
        // LCG step
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u = ((self.0 >> 33) as f32) / (1u64 << 31) as f32; // [0,1)
        (u * 2.0 - 1.0) * half_range
    }
}

pub struct CartpoleEnv {
    cfg: SceneCfg,
    world: World,
    handle: ArticulationHandle,
    steps_in_episode: u32,
    max_episode_steps: u32,
    rng: Lcg,
    last_seed: u64,
}

impl CartpoleEnv {
    /// Build a Cartpole env from an Isaac Lab-style scene.yaml + URDF text.
    pub fn new(scene: SceneCfg, urdf_text: &str) -> Result<Self, String> {
        let sys = parse_urdf(urdf_text).map_err(|e| format!("urdf parse: {e}"))?;
        let world_dt = scene.scene.dt; // 60 Hz physics
        let world_gravity = -scene.scene.gravity[2]; // gravity_vec[2] negative; engine expects positive g magnitude
        let mut world = World::new(world_gravity, world_dt);
        let handle = world
            .add_articulation(sys)
            .map_err(|e| format!("articulation add: {e}"))?;
        let max_episode_steps =
            (scene.termination.time_out.max_episode_length_s / world_dt).round() as u32;
        Ok(CartpoleEnv {
            cfg: scene,
            world,
            handle,
            steps_in_episode: 0,
            max_episode_steps,
            rng: Lcg::new(0),
            last_seed: 0,
        })
    }

    fn observation_inner(&self) -> Vec<f32> {
        let s = self
            .world
            .get(self.handle)
            .unwrap()
            .cartpole_state()
            .unwrap();
        vec![s.x, s.x_dot, s.theta, s.theta_dot]
    }

    fn pole_out_of_bounds(&self, theta: f32) -> bool {
        let b = &self.cfg.termination.pole_out_of_bounds.bounds;
        theta < b[0] || theta > b[1]
    }
    fn cart_out_of_bounds(&self, x: f32) -> bool {
        let b = &self.cfg.termination.cart_out_of_bounds.bounds;
        x < b[0] || x > b[1]
    }
}

impl RLEnv for CartpoleEnv {
    fn reset(&mut self, seed: Option<u64>) -> Vec<f32> {
        if let Some(s) = seed {
            self.rng = Lcg::new(s);
            self.last_seed = s;
        }
        // Small random init within ±0.05 m / ±0.05 rad (Isaac Lab baseline).
        let init = CartpoleState {
            x: self.rng.next_f32_centered(0.05),
            x_dot: self.rng.next_f32_centered(0.05),
            theta: self.rng.next_f32_centered(0.05),
            theta_dot: self.rng.next_f32_centered(0.05),
        };
        self.world
            .get_mut(self.handle)
            .unwrap()
            .set_cartpole_state(init);
        self.steps_in_episode = 0;
        self.observation_inner()
    }

    fn step(&mut self, action: &[f32]) -> StepResult {
        assert_eq!(action.len(), 1, "Cartpole action is 1-dim (force on cart)");
        let force = action[0];
        let decimation = self.cfg.scene.decimation.max(1);
        for _ in 0..decimation {
            self.world
                .get_mut(self.handle)
                .unwrap()
                .set_cart_force(force);
            self.world.step();
        }
        self.steps_in_episode += decimation;

        let obs = self.observation_inner();
        let (x, x_dot, theta, theta_dot) = (obs[0], obs[1], obs[2], obs[3]);

        let terminated = self.pole_out_of_bounds(theta) || self.cart_out_of_bounds(x);
        let truncated = self.steps_in_episode >= self.max_episode_steps;

        let r = &self.cfg.reward;
        let reward = r.alive
            + (if terminated { r.terminating } else { 0.0 })
            + r.pole_pos_penalty * theta * theta
            + r.cart_vel_penalty * x_dot * x_dot
            + r.pole_vel_penalty * theta_dot * theta_dot;

        StepResult {
            observation: obs,
            reward,
            terminated,
            truncated,
        }
    }

    fn observation_dim(&self) -> usize {
        4
    }
    fn action_dim(&self) -> usize {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_cfg::load_scene_yaml;

    const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");
    const CARTPOLE_SCENE: &str = include_str!("../../fixtures/cartpole/scene.yaml");

    fn make_env() -> CartpoleEnv {
        let cfg = load_scene_yaml(CARTPOLE_SCENE).unwrap();
        CartpoleEnv::new(cfg, CARTPOLE_URDF).unwrap()
    }

    #[test]
    fn reset_produces_4dim_observation() {
        let mut env = make_env();
        let obs = env.reset(Some(42));
        assert_eq!(obs.len(), 4);
        assert_eq!(env.observation_dim(), 4);
        assert_eq!(env.action_dim(), 1);
    }

    #[test]
    fn reset_is_seed_deterministic() {
        let mut a = make_env();
        let mut b = make_env();
        let oa = a.reset(Some(1337));
        let ob = b.reset(Some(1337));
        assert_eq!(oa, ob);
    }

    #[test]
    fn null_policy_terminates_eventually() {
        let mut env = make_env();
        env.reset(Some(7));
        let mut terminated = false;
        for _ in 0..1_000 {
            let r = env.step(&[0.0]);
            if r.terminated || r.truncated {
                terminated = r.terminated;
                break;
            }
        }
        assert!(
            terminated,
            "passive cartpole should tip over from small init"
        );
    }

    #[test]
    fn alive_reward_accumulates_while_balanced() {
        let mut env = make_env();
        env.reset(Some(123));
        // tiny restoring controller: push opposite to pole tilt
        let mut total_r = 0.0;
        for _ in 0..30 {
            let obs = env.observation_inner();
            let theta = obs[2];
            let action = -10.0 * theta; // crude balancing
            let r = env.step(&[action]);
            total_r += r.reward;
            if r.terminated || r.truncated {
                break;
            }
        }
        assert!(
            total_r > 0.0,
            "good balancing should yield positive cumulative reward"
        );
    }
}
