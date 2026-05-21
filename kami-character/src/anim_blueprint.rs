//! Animation Blueprint: state machine for MetaHuman character animation.
//!
//! Provides a hierarchical state machine that blends animation clips,
//! driven by parameters (speed, direction, emotion) and FACS controls.
//!
//! Architecture:
//!   Parameters → State Machine → Active States → Blend Tree → Final Pose
//!   + Control Rig overlay → GPU Skinning

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Animation blueprint: state machine + blend trees + parameter bindings.
#[derive(Debug, Clone)]
pub struct AnimBlueprint {
    /// Named parameters that drive transitions and blends.
    pub parameters: HashMap<String, AnimParam>,
    /// State machine layers (evaluated in order, blended additively).
    pub layers: Vec<AnimLayer>,
    /// Blend profiles for smooth transitions.
    pub blend_profiles: Vec<BlendProfile>,
}

/// Animation parameter (drives state transitions and blend weights).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimParam {
    pub name: String,
    pub param_type: AnimParamType,
    pub value: f32,
    pub default_value: f32,
}

/// Parameter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimParamType {
    /// Continuous float (e.g. speed, blend weight).
    Float,
    /// Integer (e.g. state index).
    Int,
    /// Boolean trigger (e.g. jump, attack).
    Bool,
}

/// Animation layer: independent state machine with blend mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimLayer {
    pub name: String,
    pub blend_mode: LayerBlendMode,
    pub weight: f32,
    /// State machine for this layer.
    pub states: Vec<AnimState>,
    /// Transitions between states.
    pub transitions: Vec<AnimTransition>,
    /// Currently active state index.
    pub active_state: usize,
    /// Transition progress (0.0 = source state, 1.0 = target state).
    pub transition_progress: f32,
    /// Target state during transition (None if not transitioning).
    pub transition_target: Option<usize>,
}

/// Layer blend mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerBlendMode {
    /// Override: replace lower layers.
    Override,
    /// Additive: add on top of lower layers.
    Additive,
}

/// Animation state: a node in the state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimState {
    pub name: String,
    pub state_type: AnimStateType,
    /// Playback speed multiplier.
    pub play_rate: f32,
    /// Whether this state loops.
    pub looping: bool,
}

/// State content type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnimStateType {
    /// Single animation clip.
    Clip { clip_name: String },
    /// 1D blend space (e.g. walk/run by speed).
    BlendSpace1D {
        axis_param: String,
        entries: Vec<BlendSpaceEntry>,
    },
    /// 2D blend space (e.g. locomotion by speed + direction).
    BlendSpace2D {
        x_param: String,
        y_param: String,
        entries: Vec<BlendSpace2DEntry>,
    },
    /// Layered blend per bone (e.g. upper body override).
    LayeredBlendPerBone {
        base_clip: String,
        overlay_clip: String,
        bone_filter: Vec<String>,
        blend_param: String,
    },
    /// Pose snapshot (freeze current pose).
    PoseSnapshot,
}

/// 1D blend space entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendSpaceEntry {
    pub clip_name: String,
    pub position: f32,
}

/// 2D blend space entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendSpace2DEntry {
    pub clip_name: String,
    pub x: f32,
    pub y: f32,
}

/// Transition between states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimTransition {
    pub source: usize,
    pub target: usize,
    /// Transition duration in seconds.
    pub duration: f32,
    /// Blend curve type.
    pub blend_curve: BlendCurve,
    /// Conditions that must be met to trigger this transition.
    pub conditions: Vec<TransitionCondition>,
    /// Priority (higher = checked first when multiple transitions match).
    pub priority: u32,
}

/// Blend curve for transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendCurve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Cubic,
}

/// Condition for triggering a transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionCondition {
    pub param_name: String,
    pub comparison: Comparison,
    pub threshold: f32,
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Comparison {
    Greater,
    Less,
    Equal,
    NotEqual,
    GreaterEqual,
    LessEqual,
}

/// Blend profile: per-bone blend weights for masked transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendProfile {
    pub name: String,
    /// Bone name → blend weight override (0.0 = no blend, 1.0 = full blend).
    pub bone_weights: HashMap<String, f32>,
}

/// Evaluated pose output from the animation blueprint.
#[derive(Debug, Clone)]
pub struct EvaluatedPose {
    /// Per-bone local transforms (indexed by bone index).
    pub bone_transforms: Vec<BoneLocalTransform>,
    /// Active curves (e.g. morph target weights).
    pub curves: HashMap<String, f32>,
}

/// Local bone transform.
#[derive(Debug, Clone, Copy)]
pub struct BoneLocalTransform {
    pub position: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
}

impl Default for BoneLocalTransform {
    fn default() -> Self {
        Self {
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
        }
    }
}

impl AnimBlueprint {
    /// Create a MetaHuman default animation blueprint.
    ///
    /// Layers: body (locomotion), face (FACS-driven), additive (breathing/idle).
    pub fn metahuman_default() -> Self {
        let mut parameters = HashMap::new();
        for (name, default) in [
            ("speed", 0.0),
            ("direction", 0.0),
            ("is_moving", 0.0),
            ("emotion_happy", 0.0),
            ("emotion_sad", 0.0),
            ("emotion_angry", 0.0),
            ("blink", 0.0),
            ("look_x", 0.0),
            ("look_y", 0.0),
            ("jaw_open", 0.0),
            ("breath_cycle", 0.0),
        ] {
            parameters.insert(
                name.to_string(),
                AnimParam {
                    name: name.to_string(),
                    param_type: if name.starts_with("is_") {
                        AnimParamType::Bool
                    } else {
                        AnimParamType::Float
                    },
                    value: default,
                    default_value: default,
                },
            );
        }

        let body_layer = AnimLayer {
            name: "body".into(),
            blend_mode: LayerBlendMode::Override,
            weight: 1.0,
            states: vec![
                AnimState {
                    name: "idle".into(),
                    state_type: AnimStateType::Clip {
                        clip_name: "idle_breathe".into(),
                    },
                    play_rate: 1.0,
                    looping: true,
                },
                AnimState {
                    name: "locomotion".into(),
                    state_type: AnimStateType::BlendSpace1D {
                        axis_param: "speed".into(),
                        entries: vec![
                            BlendSpaceEntry { clip_name: "walk".into(), position: 0.3 },
                            BlendSpaceEntry { clip_name: "jog".into(), position: 0.6 },
                            BlendSpaceEntry { clip_name: "run".into(), position: 1.0 },
                        ],
                    },
                    play_rate: 1.0,
                    looping: true,
                },
            ],
            transitions: vec![
                AnimTransition {
                    source: 0,
                    target: 1,
                    duration: 0.3,
                    blend_curve: BlendCurve::EaseInOut,
                    conditions: vec![TransitionCondition {
                        param_name: "is_moving".into(),
                        comparison: Comparison::Greater,
                        threshold: 0.5,
                    }],
                    priority: 1,
                },
                AnimTransition {
                    source: 1,
                    target: 0,
                    duration: 0.4,
                    blend_curve: BlendCurve::EaseOut,
                    conditions: vec![TransitionCondition {
                        param_name: "is_moving".into(),
                        comparison: Comparison::Less,
                        threshold: 0.5,
                    }],
                    priority: 1,
                },
            ],
            active_state: 0,
            transition_progress: 0.0,
            transition_target: None,
        };

        let face_layer = AnimLayer {
            name: "face".into(),
            blend_mode: LayerBlendMode::Additive,
            weight: 1.0,
            states: vec![
                AnimState {
                    name: "face_idle".into(),
                    state_type: AnimStateType::Clip {
                        clip_name: "face_idle_micro".into(),
                    },
                    play_rate: 1.0,
                    looping: true,
                },
                AnimState {
                    name: "face_talking".into(),
                    state_type: AnimStateType::BlendSpace1D {
                        axis_param: "jaw_open".into(),
                        entries: vec![
                            BlendSpaceEntry { clip_name: "viseme_rest".into(), position: 0.0 },
                            BlendSpaceEntry { clip_name: "viseme_open".into(), position: 1.0 },
                        ],
                    },
                    play_rate: 1.0,
                    looping: true,
                },
            ],
            transitions: vec![
                AnimTransition {
                    source: 0,
                    target: 1,
                    duration: 0.15,
                    blend_curve: BlendCurve::Linear,
                    conditions: vec![TransitionCondition {
                        param_name: "jaw_open".into(),
                        comparison: Comparison::Greater,
                        threshold: 0.1,
                    }],
                    priority: 1,
                },
                AnimTransition {
                    source: 1,
                    target: 0,
                    duration: 0.2,
                    blend_curve: BlendCurve::EaseOut,
                    conditions: vec![TransitionCondition {
                        param_name: "jaw_open".into(),
                        comparison: Comparison::Less,
                        threshold: 0.1,
                    }],
                    priority: 1,
                },
            ],
            active_state: 0,
            transition_progress: 0.0,
            transition_target: None,
        };

        Self {
            parameters,
            layers: vec![body_layer, face_layer],
            blend_profiles: vec![BlendProfile {
                name: "upper_body".into(),
                bone_weights: [
                    ("head", 1.0),
                    ("neck", 1.0),
                    ("upperChest", 0.8),
                    ("chest", 0.5),
                    ("leftShoulder", 0.9),
                    ("rightShoulder", 0.9),
                    ("leftUpperArm", 1.0),
                    ("rightUpperArm", 1.0),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            }],
        }
    }

    /// Set a parameter value.
    pub fn set_param(&mut self, name: &str, value: f32) {
        if let Some(p) = self.parameters.get_mut(name) {
            p.value = value;
        }
    }

    /// Advance the state machine by dt seconds.
    pub fn update(&mut self, dt: f32) {
        for layer in &mut self.layers {
            // Check transitions from active state
            if layer.transition_target.is_none() {
                // Find highest priority matching transition
                let mut best: Option<(u32, usize)> = None;
                for t in &layer.transitions {
                    if t.source != layer.active_state {
                        continue;
                    }
                    let all_met = t.conditions.iter().all(|c| {
                        let val = self
                            .parameters
                            .get(&c.param_name)
                            .map(|p| p.value)
                            .unwrap_or(0.0);
                        match c.comparison {
                            Comparison::Greater => val > c.threshold,
                            Comparison::Less => val < c.threshold,
                            Comparison::Equal => (val - c.threshold).abs() < 0.001,
                            Comparison::NotEqual => (val - c.threshold).abs() >= 0.001,
                            Comparison::GreaterEqual => val >= c.threshold,
                            Comparison::LessEqual => val <= c.threshold,
                        }
                    });
                    if all_met {
                        if best.is_none() || t.priority > best.unwrap().0 {
                            best = Some((t.priority, t.target));
                        }
                    }
                }
                if let Some((_, target)) = best {
                    layer.transition_target = Some(target);
                    layer.transition_progress = 0.0;
                }
            }

            // Advance transition
            if let Some(target) = layer.transition_target {
                let duration = layer
                    .transitions
                    .iter()
                    .find(|t| t.source == layer.active_state && t.target == target)
                    .map(|t| t.duration)
                    .unwrap_or(0.3);
                layer.transition_progress += dt / duration;
                if layer.transition_progress >= 1.0 {
                    layer.active_state = target;
                    layer.transition_target = None;
                    layer.transition_progress = 0.0;
                }
            }
        }
    }

    /// Get the current blend weight between source and target state for a layer.
    pub fn layer_blend(&self, layer_index: usize) -> (usize, Option<usize>, f32) {
        let layer = &self.layers[layer_index];
        (
            layer.active_state,
            layer.transition_target,
            layer.transition_progress,
        )
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        // Serialize layers and parameters
        serde_json::to_string_pretty(&self.layers).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_blueprint() {
        let bp = AnimBlueprint::metahuman_default();
        assert_eq!(bp.layers.len(), 2);
        assert_eq!(bp.layers[0].name, "body");
        assert_eq!(bp.layers[1].name, "face");
        assert!(bp.parameters.contains_key("speed"));
        assert!(bp.parameters.contains_key("jaw_open"));
    }

    #[test]
    fn test_state_transition() {
        let mut bp = AnimBlueprint::metahuman_default();
        assert_eq!(bp.layers[0].active_state, 0); // idle
        bp.set_param("is_moving", 1.0);
        bp.update(0.016); // one frame
        assert!(bp.layers[0].transition_target.is_some());
        // Complete transition
        for _ in 0..30 {
            bp.update(0.016);
        }
        assert_eq!(bp.layers[0].active_state, 1); // locomotion
    }

    #[test]
    fn test_param_set() {
        let mut bp = AnimBlueprint::metahuman_default();
        bp.set_param("speed", 0.8);
        assert!((bp.parameters["speed"].value - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_blend_profile() {
        let bp = AnimBlueprint::metahuman_default();
        assert_eq!(bp.blend_profiles.len(), 1);
        assert_eq!(bp.blend_profiles[0].name, "upper_body");
        assert!(bp.blend_profiles[0].bone_weights.contains_key("head"));
    }
}
