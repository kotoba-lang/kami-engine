//! Isaac Lab-compat scene.yaml loader.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SceneCfgError {
    #[error("yaml parse: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneCfg {
    pub adr: Option<String>,
    pub phase: Option<String>,
    pub nv_compat_target: Option<String>,
    pub scene: SceneSection,
    pub robot: RobotSection,
    pub observation: ObservationSection,
    pub action: ActionSection,
    pub reward: RewardSection,
    pub termination: TerminationSection,
    pub quality_gate: Option<QualityGate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneSection {
    pub num_envs: usize,
    pub env_spacing: f32,
    pub gravity: [f32; 3],
    pub dt: f32,
    pub decimation: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotSection {
    pub urdf: String,
    pub base_link: String,
    pub spawn: SpawnPose,
    #[serde(default)]
    pub actuators: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPose {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationSection {
    #[serde(default)]
    pub joint_pos: Vec<String>,
    #[serde(default)]
    pub joint_vel: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSection {
    #[serde(default)]
    pub joint_efforts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardSection {
    pub alive: f32,
    pub terminating: f32,
    pub pole_pos_penalty: f32,
    pub cart_vel_penalty: f32,
    pub pole_vel_penalty: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminationSection {
    pub time_out: TimeOut,
    pub pole_out_of_bounds: Bounds,
    pub cart_out_of_bounds: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeOut {
    pub max_episode_length_s: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub asset: String,
    pub bounds: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    pub reward_curve_tolerance_pct: f32,
    pub reference_baseline: String,
    pub reference_basis: String,
    #[serde(default)]
    pub reference_seed: Vec<u64>,
}

pub fn load_scene_yaml(yaml_text: &str) -> Result<SceneCfg, SceneCfgError> {
    Ok(serde_yaml::from_str(yaml_text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARTPOLE_SCENE: &str = include_str!("../../fixtures/cartpole/scene.yaml");

    #[test]
    fn parses_cartpole_scene_yaml() {
        let cfg = load_scene_yaml(CARTPOLE_SCENE).expect("scene.yaml must parse");
        assert_eq!(cfg.scene.num_envs, 1024);
        assert!((cfg.scene.dt - 1.0 / 60.0).abs() < 1e-6);
        assert_eq!(cfg.scene.decimation, 2);
        assert!((cfg.scene.gravity[2] + 9.81).abs() < 1e-6);
        assert_eq!(cfg.action.joint_efforts, vec!["slider_to_cart"]);
        assert!(cfg.quality_gate.is_some());
    }

    #[test]
    fn cartpole_reward_weights_match_isaaclab_baseline() {
        let cfg = load_scene_yaml(CARTPOLE_SCENE).unwrap();
        assert!((cfg.reward.alive - 1.0).abs() < 1e-6);
        assert!((cfg.reward.terminating + 2.0).abs() < 1e-6);
    }
}
