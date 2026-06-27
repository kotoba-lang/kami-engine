//! VRM node constraint solver (VRMC_node_constraint spec).
//!
//! Supports three constraint types:
//! * `Rotation` — destination inherits source's local rotation delta (slerped by weight).
//! * `Aim` — destination rotates so its local aim axis points toward the source position (world).
//! * `Roll` — destination inherits the component of source's rotation around a specified roll axis.
//!
//! Input: current pose overrides (per-bone local quat) + ability to look up world matrices.
//! Output: patched pose overrides for constrained nodes.
//!
//! Reference: <https://github.com/vrm-c/vrm-specification/blob/master/specifications/VRMC_node_constraint-1.0/README.md>

use glam::{Mat4, Quat, Vec3};

use crate::vrm_types::{ConstraintType, VrmDocument, VrmNodeConstraint};

/// Static constraint config derived once from the VRM document.
struct ConstraintEntry {
    /// Destination glTF node index.
    dest_node: usize,
    /// Source glTF node index.
    source_node: usize,
    /// Constraint kind-specific state.
    kind: Kind,
    /// Destination's rest local rotation (from glTF node).
    dest_initial_local_rot: Quat,
    /// Source's rest local rotation (from glTF node). Used by Rotation/Roll to
    /// compute the delta between source's current and rest pose.
    source_initial_local_rot: Quat,
}

enum Kind {
    Rotation { weight: f32 },
    Aim { aim_axis: Vec3, weight: f32 },
    Roll { roll_axis: Vec3, weight: f32 },
}

/// Node constraint solver. Stateless per frame.
pub struct ConstraintSolver {
    entries: Vec<ConstraintEntry>,
}

impl ConstraintSolver {
    pub fn new(doc: &VrmDocument) -> Self {
        let mut entries = Vec::with_capacity(doc.node_constraints.len());
        for c in &doc.node_constraints {
            let dest_initial_local_rot = doc
                .gltf
                .nodes
                .get(c.node)
                .and_then(|n| n.rotation)
                .map(Quat::from_array)
                .unwrap_or(Quat::IDENTITY);
            let (source_node, kind) = match c.constraint {
                ConstraintType::Rotation { source, weight } => (source, Kind::Rotation { weight }),
                ConstraintType::Aim {
                    source,
                    aim_axis,
                    weight,
                } => (
                    source,
                    Kind::Aim {
                        aim_axis: Vec3::from(aim_axis),
                        weight,
                    },
                ),
                ConstraintType::Roll {
                    source,
                    roll_axis,
                    weight,
                } => (
                    source,
                    Kind::Roll {
                        roll_axis: Vec3::from(roll_axis),
                        weight,
                    },
                ),
            };
            let source_initial_local_rot = doc
                .gltf
                .nodes
                .get(source_node)
                .and_then(|n| n.rotation)
                .map(Quat::from_array)
                .unwrap_or(Quat::IDENTITY);
            entries.push(ConstraintEntry {
                dest_node: c.node,
                source_node,
                kind,
                dest_initial_local_rot,
                source_initial_local_rot,
            });
        }
        ConstraintSolver { entries }
    }

    /// Apply all constraints.
    ///
    /// `source_local_rot(node)` returns the current local rotation of the source
    /// node as a quaternion (includes any upstream pose overrides and spring).
    /// `source_world(node)` returns the current world matrix of the source node
    /// (used by Aim).
    /// `dest_head_world(node)` returns the current world matrix of the destination
    /// node's head (used by Aim).
    ///
    /// Output: per-destination new local rotation quaternion (xyzw), appended to `out`.
    pub fn apply<L, W, H>(
        &self,
        mut source_local_rot: L,
        mut source_world: W,
        mut dest_head_world: H,
        out: &mut Vec<(usize, [f32; 4])>,
    ) where
        L: FnMut(usize) -> Option<Quat>,
        W: FnMut(usize) -> Option<Mat4>,
        H: FnMut(usize) -> Option<Mat4>,
    {
        for e in &self.entries {
            let new_rot = match &e.kind {
                Kind::Rotation { weight } => {
                    let cur = match source_local_rot(e.source_node) {
                        Some(q) => q,
                        None => continue,
                    };
                    // Delta = source_cur * source_rest^-1
                    let delta = cur * e.source_initial_local_rot.conjugate();
                    let blended = Quat::IDENTITY.slerp(delta, *weight);
                    blended * e.dest_initial_local_rot
                }
                Kind::Roll { roll_axis, weight } => {
                    let cur = match source_local_rot(e.source_node) {
                        Some(q) => q,
                        None => continue,
                    };
                    let delta = cur * e.source_initial_local_rot.conjugate();
                    // Decompose delta into swing-twist around roll_axis.
                    let axis = roll_axis.normalize_or_zero();
                    if axis.length_squared() < 1e-8 {
                        continue;
                    }
                    let twist_angle = twist_angle_about(delta, axis);
                    let twist_rot = Quat::from_axis_angle(axis, twist_angle * *weight);
                    twist_rot * e.dest_initial_local_rot
                }
                Kind::Aim { aim_axis, weight } => {
                    let src_w = match source_world(e.source_node) {
                        Some(m) => m,
                        None => continue,
                    };
                    let dst_head = match dest_head_world(e.dest_node) {
                        Some(m) => m,
                        None => continue,
                    };
                    let (_, dst_head_rot, dst_head_pos) = dst_head.to_scale_rotation_translation();
                    let (_, _src_rot, src_pos) = src_w.to_scale_rotation_translation();
                    let to_target = (src_pos - dst_head_pos).normalize_or_zero();
                    if to_target.length_squared() < 1e-8 {
                        continue;
                    }
                    let current_aim_world = (dst_head_rot * *aim_axis).normalize_or_zero();
                    if current_aim_world.length_squared() < 1e-8 {
                        continue;
                    }
                    let delta_world = Quat::from_rotation_arc(current_aim_world, to_target);
                    let blended = Quat::IDENTITY.slerp(delta_world, *weight);
                    // Convert world delta to local-space rotation applied to initial.
                    // Approx: local_rot = world_rot^-1 * delta * world_rot * initial
                    (dst_head_rot.conjugate() * blended * dst_head_rot) * e.dest_initial_local_rot
                }
            };
            out.push((e.dest_node, [new_rot.x, new_rot.y, new_rot.z, new_rot.w]));
        }
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

/// Extract the twist angle of a quaternion around an axis (swing-twist decomposition).
fn twist_angle_about(q: Quat, axis: Vec3) -> f32 {
    let dot = q.x * axis.x + q.y * axis.y + q.z * axis.z;
    let twist = Quat::from_xyzw(axis.x * dot, axis.y * dot, axis.z * dot, q.w).normalize();
    2.0 * twist.w.clamp(-1.0, 1.0).acos() * twist.xyz().dot(axis).signum()
}
