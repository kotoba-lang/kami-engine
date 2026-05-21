//! Control Rig: procedural rigging system for MetaHuman face/body animation.
//!
//! Maps high-level controls (FACS AUs, look-at, head pose) to bone transforms
//! via a directed acyclic graph of rig nodes. Each node applies a transform
//! operation (blend, constraint, math) to produce final bone poses.
//!
//! Architecture:
//!   Control inputs (FACS, gaze, pose) → RigGraph evaluation → Bone transforms
//!   → Skeleton joint matrices → GPU skinning

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Control rig: DAG of nodes that transform controls into bone poses.
#[derive(Debug, Clone)]
pub struct ControlRig {
    /// Rig node definitions (evaluated in topological order).
    pub nodes: Vec<RigNode>,
    /// Evaluation order (topologically sorted node indices).
    pub eval_order: Vec<usize>,
    /// Named control inputs (e.g. "AU12_L" → 0.8).
    pub controls: HashMap<String, f32>,
    /// Output bone transform overrides (bone_index → local transform).
    pub bone_outputs: HashMap<usize, BoneTransform>,
}

/// A rig node in the control rig DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigNode {
    pub name: String,
    pub node_type: RigNodeType,
    /// Input connections: (source_node_index, output_channel).
    pub inputs: Vec<(usize, u32)>,
    /// Target bone index (if this node drives a bone).
    pub target_bone: Option<usize>,
}

/// Types of rig node operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RigNodeType {
    /// Direct control input (reads from controls map).
    ControlInput { control_name: String },
    /// Multiply input by constant.
    Multiply { factor: f32 },
    /// Clamp input to range.
    Clamp { min: f32, max: f32 },
    /// Remap input range [in_min, in_max] → [out_min, out_max].
    Remap {
        in_min: f32,
        in_max: f32,
        out_min: f32,
        out_max: f32,
    },
    /// Blend N inputs with weights.
    Blend { weights: Vec<f32> },
    /// Apply as rotation around axis (input = angle in radians).
    RotationAxis { axis: [f32; 3] },
    /// Apply as translation along axis (input = distance).
    TranslationAxis { axis: [f32; 3] },
    /// Aim constraint: point bone at target position.
    AimConstraint {
        aim_axis: [f32; 3],
        up_axis: [f32; 3],
    },
    /// Corrective blendshape: activate when multiple inputs exceed thresholds.
    Corrective { thresholds: Vec<f32> },
    /// Two-bone IK solver.
    TwoBoneIk {
        root_bone: usize,
        mid_bone: usize,
        end_bone: usize,
        pole_vector: [f32; 3],
    },
}

/// Bone transform output from rig evaluation.
#[derive(Debug, Clone, Copy)]
pub struct BoneTransform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for BoneTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl ControlRig {
    /// Create a MetaHuman face control rig with FACS AU → bone mappings.
    pub fn metahuman_face_rig() -> Self {
        let mut nodes = Vec::new();
        let mut idx = 0;

        // Helper to add a control → rotation chain for a bone
        let mut add_au_bone = |au_name: &str, bone_idx: usize, axis: [f32; 3], max_angle: f32| {
            let control_idx = idx;
            nodes.push(RigNode {
                name: format!("ctrl_{au_name}"),
                node_type: RigNodeType::ControlInput {
                    control_name: au_name.to_string(),
                },
                inputs: vec![],
                target_bone: None,
            });
            idx += 1;

            let mul_idx = idx;
            nodes.push(RigNode {
                name: format!("mul_{au_name}"),
                node_type: RigNodeType::Multiply { factor: max_angle },
                inputs: vec![(control_idx, 0)],
                target_bone: None,
            });
            idx += 1;

            nodes.push(RigNode {
                name: format!("rot_{au_name}"),
                node_type: RigNodeType::RotationAxis { axis },
                inputs: vec![(mul_idx, 0)],
                target_bone: Some(bone_idx),
            });
            idx += 1;
        };

        // Eyelids (bones 13-16 from metahuman skeleton)
        add_au_bone("AU43_L", 13, [1.0, 0.0, 0.0], -0.5); // left upper eyelid close
        add_au_bone("AU43_R", 15, [1.0, 0.0, 0.0], -0.5); // right upper eyelid close
        add_au_bone("AU7_L", 14, [1.0, 0.0, 0.0], 0.3); // left lower eyelid tighten
        add_au_bone("AU7_R", 16, [1.0, 0.0, 0.0], 0.3); // right lower eyelid tighten

        // Brows (bones 17-22)
        add_au_bone("AU1", 17, [1.0, 0.0, 0.0], 0.3); // inner brow raise L
        add_au_bone("AU2_L", 19, [1.0, 0.0, 0.0], 0.25); // outer brow raise L
        add_au_bone("AU2_R", 22, [1.0, 0.0, 0.0], 0.25); // outer brow raise R
        add_au_bone("AU4", 18, [1.0, 0.0, 0.0], -0.2); // brow lower (mid)

        // Jaw (bone 8)
        add_au_bone("AU26", 8, [1.0, 0.0, 0.0], 0.4); // jaw drop
        add_au_bone("AU30", 8, [0.0, 1.0, 0.0], 0.15); // jaw sideways

        // Lip corners (bones 34-35)
        add_au_bone("AU12_L", 34, [0.0, 0.0, 1.0], 0.3); // smile L
        add_au_bone("AU12_R", 35, [0.0, 0.0, 1.0], 0.3); // smile R
        add_au_bone("AU15_L", 34, [0.0, 0.0, 1.0], -0.2); // frown L
        add_au_bone("AU15_R", 35, [0.0, 0.0, 1.0], -0.2); // frown R

        // Nostrils (bones 25-26)
        add_au_bone("AU38_L", 25, [1.0, 0.0, 0.0], 0.15); // nostril dilate L
        add_au_bone("AU38_R", 26, [1.0, 0.0, 0.0], 0.15); // nostril dilate R

        // Cheeks (bones 36-37)
        add_au_bone("AU6_L", 36, [1.0, 0.0, 0.0], 0.2); // cheek raise L
        add_au_bone("AU6_R", 37, [1.0, 0.0, 0.0], 0.2); // cheek raise R

        // Tongue (bones 42-44)
        add_au_bone("AU19", 44, [1.0, 0.0, 0.0], 0.5); // tongue out

        let eval_order: Vec<usize> = (0..nodes.len()).collect();

        Self {
            nodes,
            eval_order,
            controls: HashMap::new(),
            bone_outputs: HashMap::new(),
        }
    }

    /// Set a control input value (0.0–1.0).
    pub fn set_control(&mut self, name: &str, value: f32) {
        self.controls.insert(name.to_string(), value);
    }

    /// Evaluate the rig graph and compute bone transforms.
    pub fn evaluate(&mut self) {
        self.bone_outputs.clear();
        let mut node_values: Vec<f32> = vec![0.0; self.nodes.len()];

        for &ni in &self.eval_order {
            let node = &self.nodes[ni];
            let input_val = if !node.inputs.is_empty() {
                node.inputs
                    .iter()
                    .map(|&(src, _)| node_values[src])
                    .sum::<f32>()
                    / node.inputs.len() as f32
            } else {
                0.0
            };

            let output = match &node.node_type {
                RigNodeType::ControlInput { control_name } => {
                    self.controls.get(control_name).copied().unwrap_or(0.0)
                }
                RigNodeType::Multiply { factor } => input_val * factor,
                RigNodeType::Clamp { min, max } => input_val.clamp(*min, *max),
                RigNodeType::Remap {
                    in_min,
                    in_max,
                    out_min,
                    out_max,
                } => {
                    let t = ((input_val - in_min) / (in_max - in_min)).clamp(0.0, 1.0);
                    out_min + t * (out_max - out_min)
                }
                RigNodeType::Blend { weights } => {
                    node.inputs
                        .iter()
                        .zip(weights.iter())
                        .map(|(&(src, _), &w)| node_values[src] * w)
                        .sum()
                }
                RigNodeType::RotationAxis { axis } => {
                    if let Some(bone_idx) = node.target_bone {
                        let axis_vec = Vec3::from_array(*axis).normalize_or_zero();
                        let rot = Quat::from_axis_angle(axis_vec, input_val);
                        let entry = self
                            .bone_outputs
                            .entry(bone_idx)
                            .or_insert_with(BoneTransform::default);
                        entry.rotation = entry.rotation * rot;
                    }
                    input_val
                }
                RigNodeType::TranslationAxis { axis } => {
                    if let Some(bone_idx) = node.target_bone {
                        let axis_vec = Vec3::from_array(*axis);
                        let entry = self
                            .bone_outputs
                            .entry(bone_idx)
                            .or_insert_with(BoneTransform::default);
                        entry.position += axis_vec * input_val;
                    }
                    input_val
                }
                RigNodeType::Corrective { thresholds } => {
                    let all_above = node
                        .inputs
                        .iter()
                        .zip(thresholds.iter())
                        .all(|(&(src, _), &th)| node_values[src] >= th);
                    if all_above {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => input_val,
            };

            node_values[ni] = output;
        }
    }

    /// Apply rig outputs to a skeleton, returning modified joint matrices.
    pub fn apply_to_skeleton(
        &self,
        skeleton: &kami_skeleton::Skeleton,
        clip: &kami_skeleton::AnimationClip,
        time: f32,
    ) -> Vec<kami_skeleton::JointMatrix> {
        let mut world = skeleton.evaluate(clip, time);

        // Apply rig bone overrides
        for (&bone_idx, transform) in &self.bone_outputs {
            if bone_idx < world.len() {
                let rig_mat = Mat4::from_scale_rotation_translation(
                    transform.scale,
                    transform.rotation,
                    transform.position,
                );
                world[bone_idx] = world[bone_idx] * rig_mat;
            }
        }

        // Compute joint matrices (world × inverse_bind)
        skeleton
            .bones
            .iter()
            .enumerate()
            .map(|(i, bone)| {
                let inv_bind = Mat4::from_cols_array_2d(&bone.inverse_bind);
                let joint = world[i] * inv_bind;
                kami_skeleton::JointMatrix {
                    mat: joint.to_cols_array_2d(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_face_rig_creation() {
        let rig = ControlRig::metahuman_face_rig();
        assert!(!rig.nodes.is_empty());
        // 19 AU mappings × 3 nodes each (control + multiply + rotation) = 57
        assert!(rig.nodes.len() >= 19 * 3, "Expected 57+ nodes, got {}", rig.nodes.len());
    }

    #[test]
    fn test_rig_evaluation() {
        let mut rig = ControlRig::metahuman_face_rig();
        rig.set_control("AU12_L", 0.8);
        rig.set_control("AU12_R", 0.8);
        rig.set_control("AU26", 0.3);
        rig.evaluate();
        // Smile should affect lip corner bones
        assert!(rig.bone_outputs.contains_key(&34)); // left lip corner
        assert!(rig.bone_outputs.contains_key(&35)); // right lip corner
        assert!(rig.bone_outputs.contains_key(&8)); // jaw
    }

    #[test]
    fn test_control_set_get() {
        let mut rig = ControlRig::metahuman_face_rig();
        rig.set_control("AU1", 0.5);
        assert!((rig.controls["AU1"] - 0.5).abs() < f32::EPSILON);
    }
}
