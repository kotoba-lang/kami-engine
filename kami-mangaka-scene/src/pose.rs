// Character pose + facial expression.
// VRM standard humanoid bone names + ARKit-style blendshape weights.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::scene::Transform;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoseSpec {
    pub root_xform: Transform,
    pub bones: Vec<BoneRotation>,
    pub ik_targets: Vec<IkTarget>,
    pub label: Option<String>, // semantic preset name (e.g. "action.dash")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneRotation {
    pub bone: String, // VRM humanoid bone name (hips, spine, leftUpperArm, ...)
    pub rotation: Quat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IkTarget {
    pub end_bone: String,
    pub target: Vec3,
    pub weight: f32, // 0.0..1.0 blend
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Expression {
    Neutral,
    Happy,
    Angry,
    Sad,
    Surprised,
    Determined,
    Pained,
    Smirk,
}

impl PoseSpec {
    pub fn rest() -> Self {
        Self {
            root_xform: Transform::default(),
            bones: Vec::new(),
            ik_targets: Vec::new(),
            label: Some("rest".into()),
        }
    }
}
