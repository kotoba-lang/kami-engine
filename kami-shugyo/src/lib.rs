//! kami-shugyo (修行) — Isaac Lab-equivalent RL training framework.
//!
//! R1.1 PoC scope (ADR-2605261800):
//!   - `RLEnv` trait (reset / step / observation / action / reward / done)
//!   - `CartpoleEnv` loading scene.yaml + URDF
//!   - random-policy baseline runner
//!
//! API surface mirrors `isaaclab.envs.ManagerBasedRLEnv` (Isaac Lab 1.x).
//! See `nv-compat/isaaclab` for facade.

pub const ADR: &str = "ADR-2605261800";
pub const PHASE: &str = "R1.1-cartpole-poc";
pub const KAMI_NAME: &str = "e7m-shugyo";
pub const NV_COMPAT_TARGET: &str = "isaaclab.envs.ManagerBasedRLEnv";

mod cartpole_env;
mod dr;
mod scene_cfg;
pub mod traits;
mod vectorized_env;

pub use cartpole_env::CartpoleEnv;
pub use dr::{DomainRandomizationCfg, Range};
pub use scene_cfg::{SceneCfg, load_scene_yaml};
pub use traits::{RLEnv, StepResult};
pub use vectorized_env::VectorizedCartpoleEnv;
