// Camera + lighting specs with manga shot grammar.

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ShotGrammar {
    FullShot,
    MediumShot,
    Closeup,
    OverShoulder,
    Dutch,
    BirdsEye,
    WormsEye,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Dof {
    pub focus_distance_m: f32,
    pub aperture: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CameraSpec {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_deg: f32,
    pub roll_deg: f32,
    pub dof: Option<Dof>,
    pub shot: ShotGrammar,
}

impl Default for CameraSpec {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 1.6, 3.5),
            target: Vec3::new(0.0, 1.4, 0.0),
            up: Vec3::Y,
            fov_deg: 35.0,
            roll_deg: 0.0,
            dof: None,
            shot: ShotGrammar::MediumShot,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LightRole {
    Key,
    Fill,
    Rim,
    Ambient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LightSpec {
    pub role: LightRole,
    pub direction: Vec3,
    pub colour: [f32; 3],
    pub intensity: f32,
}

impl LightSpec {
    pub fn three_point_key() -> Self {
        Self {
            role: LightRole::Key,
            direction: Vec3::new(-0.6, -0.8, -0.4).normalize(),
            colour: [1.0, 0.96, 0.92],
            intensity: 4.0,
        }
    }
    pub fn three_point_fill() -> Self {
        Self {
            role: LightRole::Fill,
            direction: Vec3::new(0.7, -0.4, -0.2).normalize(),
            colour: [0.86, 0.92, 1.0],
            intensity: 1.4,
        }
    }
    pub fn three_point_rim() -> Self {
        Self {
            role: LightRole::Rim,
            direction: Vec3::new(0.1, -0.2, 0.95).normalize(),
            colour: [1.0, 1.0, 1.0],
            intensity: 2.0,
        }
    }
}
