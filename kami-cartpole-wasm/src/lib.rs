//! kami-cartpole-wasm — JS-callable Cartpole simulator built on kami-shugyo.
//!
//! Demonstrates Phase C of ADR-2605261800: KAMI canonical Rust impl compiles
//! to wasm32-unknown-unknown and exposes the Isaac Sim / PhysX API surface
//! to JavaScript via wasm-bindgen.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use kami_shugyo::{CartpoleEnv, RLEnv, load_scene_yaml};

const URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");
const SCENE: &str = include_str!("../../fixtures/cartpole/scene.yaml");

/// JS-callable handle around a single Cartpole env.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct CartpoleHandle {
    env: CartpoleEnv,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl CartpoleHandle {
    /// `new CartpoleHandle()` — loads the embedded Cartpole URDF + scene.yaml.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> CartpoleHandle {
        let cfg = load_scene_yaml(SCENE).expect("scene.yaml");
        let env = CartpoleEnv::new(cfg, URDF).expect("cartpole env");
        CartpoleHandle { env }
    }

    /// `cartpole.reset(seed)` → 4-tuple flattened: [x, x_dot, theta, theta_dot]
    pub fn reset(&mut self, seed: u32) -> Vec<f32> {
        self.env.reset(Some(seed as u64))
    }

    /// `cartpole.step(force)` → 7-tuple flattened:
    ///   [x, x_dot, theta, theta_dot, reward, terminated_as_f32, truncated_as_f32]
    pub fn step(&mut self, force: f32) -> Vec<f32> {
        let r = self.env.step(&[force]);
        let mut v = r.observation;
        v.push(r.reward);
        v.push(if r.terminated { 1.0 } else { 0.0 });
        v.push(if r.truncated { 1.0 } else { 0.0 });
        v
    }

    /// `cartpole.observation_dim()` → 4
    pub fn observation_dim(&self) -> u32 {
        self.env.observation_dim() as u32
    }

    /// `cartpole.action_dim()` → 1
    pub fn action_dim(&self) -> u32 {
        self.env.action_dim() as u32
    }
}

impl Default for CartpoleHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// `kamiCartpoleVersion()` — returns "ADR-2605261800@R1.1".
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = kamiCartpoleVersion))]
pub fn kami_cartpole_version() -> String {
    format!("{}@{}", kami_shugyo::ADR, kami_shugyo::PHASE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_lifecycle() {
        let mut h = CartpoleHandle::new();
        let obs = h.reset(42);
        assert_eq!(obs.len(), 4);
        let r = h.step(0.0);
        assert_eq!(r.len(), 7);
    }
}
