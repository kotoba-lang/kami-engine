//! VRM spring bone simulator (VRMC_springBone spec).
//!
//! Per-joint verlet chain: gravity + stiffness toward rest pose + drag. Output
//! is per-spring-joint local rotation overrides, merged into the caller's pose
//! state before palette computation. Colliders not yet implemented.
//!
//! Reference: <https://github.com/vrm-c/vrm-specification/blob/master/specifications/VRMC_springBone-1.0/README.md>

use glam::{Mat4, Quat, Vec3};

use crate::vrm_types::{ColliderShape, VrmDocument};

/// Resolved collider for spring simulation.
#[derive(Clone, Copy)]
enum Shape {
    Sphere {
        offset: Vec3,
        radius: f32,
    },
    Capsule {
        offset: Vec3,
        tail: Vec3,
        radius: f32,
    },
}

struct ColliderEntry {
    /// glTF node index the collider is attached to.
    node: usize,
    shape: Shape,
}

/// Static joint config derived once from the VRM document.
struct JointStatic {
    /// glTF node index.
    node: usize,
    /// Rest local rotation (quaternion xyzw) from the glTF node.
    initial_local_rot: Quat,
    /// Rest bone axis in this joint's local frame: direction from this joint
    /// to the next joint's local position, normalized.
    bone_axis_local: Vec3,
    /// Distance from this joint to the next joint's local position.
    bone_length: f32,
    /// Spring stiffness (0..=1).
    stiffness: f32,
    /// Drag force (0..=1). Higher = more damping.
    drag: f32,
    /// Pre-multiplied gravity vector (direction * power).
    gravity: Vec3,
    /// Radius of the joint's hit sphere (for collider push-out).
    hit_radius: f32,
    /// If false, this joint is a chain tip with no next bone and is skipped.
    has_tail: bool,
}

struct ChainStatic {
    joints: Vec<JointStatic>,
    /// Indices into `SpringSimulator::colliders` (flattened from collider groups).
    colliders: Vec<usize>,
}

struct JointRuntime {
    prev_tail_world: Vec3,
    current_tail_world: Vec3,
    initialized: bool,
}

/// Spring bone simulator. One instance per VRM document.
pub struct SpringSimulator {
    chains: Vec<ChainStatic>,
    runtime: Vec<Vec<JointRuntime>>,
    colliders: Vec<ColliderEntry>,
}

impl SpringSimulator {
    /// Build a simulator from the parsed VRM document.
    pub fn new(doc: &VrmDocument) -> Self {
        // Flatten collider list — spring chains reference by group index,
        // groups reference by collider index within `doc.spring_bone_colliders`.
        let colliders: Vec<ColliderEntry> = doc
            .spring_bone_colliders
            .iter()
            .map(|c| {
                let shape = match c.shape {
                    ColliderShape::Sphere { offset, radius } => Shape::Sphere {
                        offset: Vec3::from(offset),
                        radius,
                    },
                    ColliderShape::Capsule {
                        offset,
                        tail,
                        radius,
                    } => Shape::Capsule {
                        offset: Vec3::from(offset),
                        tail: Vec3::from(tail),
                        radius,
                    },
                };
                ColliderEntry {
                    node: c.node,
                    shape,
                }
            })
            .collect();

        let mut chains = Vec::new();
        let mut runtime = Vec::new();

        for chain in &doc.spring_bones {
            let mut statics = Vec::with_capacity(chain.joints.len());
            let mut rts = Vec::with_capacity(chain.joints.len());

            for (i, joint) in chain.joints.iter().enumerate() {
                let node = doc.gltf.nodes.get(joint.node);
                let initial_local_rot = node
                    .and_then(|n| n.rotation)
                    .map(|r| Quat::from_array(r))
                    .unwrap_or(Quat::IDENTITY);

                // Determine bone axis & length from the next joint's local position.
                let (bone_axis_local, bone_length, has_tail) =
                    if let Some(next) = chain.joints.get(i + 1) {
                        let next_pos = doc
                            .gltf
                            .nodes
                            .get(next.node)
                            .and_then(|n| n.translation)
                            .map(Vec3::from)
                            .unwrap_or(Vec3::Y * 0.07);
                        let len = next_pos.length();
                        if len > 1e-6 {
                            (next_pos / len, len, true)
                        } else {
                            (Vec3::Y, 0.07, false)
                        }
                    } else {
                        // Chain tip — no next joint; simulate with fallback axis.
                        (Vec3::Y, 0.07, false)
                    };

                statics.push(JointStatic {
                    node: joint.node,
                    initial_local_rot,
                    bone_axis_local,
                    bone_length,
                    stiffness: joint.stiffness.clamp(0.0, 1.0),
                    drag: joint.drag_force.clamp(0.0, 1.0),
                    gravity: Vec3::from(joint.gravity_dir) * joint.gravity_power,
                    hit_radius: joint.hit_radius.max(0.0),
                    has_tail,
                });
                rts.push(JointRuntime {
                    prev_tail_world: Vec3::ZERO,
                    current_tail_world: Vec3::ZERO,
                    initialized: false,
                });
            }

            // Flatten collider-group indices to collider indices.
            let mut chain_colliders: Vec<usize> = Vec::new();
            for &group_idx in &chain.collider_groups {
                if let Some(group) = doc.spring_bone_collider_groups.get(group_idx) {
                    chain_colliders.extend(group.colliders.iter().copied());
                }
            }

            chains.push(ChainStatic {
                joints: statics,
                colliders: chain_colliders,
            });
            runtime.push(rts);
        }

        SpringSimulator {
            chains,
            runtime,
            colliders,
        }
    }

    /// Advance simulation by `dt` seconds and write per-joint quaternion
    /// overrides into `out`, keyed by glTF node index.
    ///
    /// `node_world` is a closure that returns the **current** pose-applied
    /// world matrix of the given glTF node, BEFORE spring overrides are
    /// applied this frame. The previous frame's spring output is implicitly
    /// cascaded through this world matrix (simple, robust, one frame of
    /// latency on intra-chain cascades — acceptable at 60 Hz).
    pub fn step<F>(&mut self, dt: f32, mut node_world: F, out: &mut Vec<(usize, [f32; 4])>)
    where
        F: FnMut(usize) -> Option<Mat4>,
    {
        let dt = dt.clamp(0.0, 1.0 / 30.0); // clamp large steps to avoid blowing up
        for (chain_idx, chain_st) in self.chains.iter().enumerate() {
            // Pre-resolve world-space colliders for this chain.
            let resolved_colliders: Vec<(Shape, Mat4)> = chain_st
                .colliders
                .iter()
                .filter_map(|&ci| {
                    let entry = self.colliders.get(ci)?;
                    let world = node_world(entry.node)?;
                    Some((entry.shape, world))
                })
                .collect();

            let statics = &chain_st.joints;
            let rts = &mut self.runtime[chain_idx];
            for (i, js) in statics.iter().enumerate() {
                if !js.has_tail {
                    continue;
                }
                let world_m = match node_world(js.node) {
                    Some(m) => m,
                    None => continue,
                };
                let head_world = world_m.to_scale_rotation_translation().2;
                let world_rot = world_m.to_scale_rotation_translation().1;

                // Rest tail direction in world space = world_rot applied to local bone axis.
                let rest_dir_world = (world_rot * js.bone_axis_local).normalize_or_zero();
                let rest_tail_world = head_world + rest_dir_world * js.bone_length;

                let rt = &mut rts[i];
                if !rt.initialized {
                    rt.prev_tail_world = rest_tail_world;
                    rt.current_tail_world = rest_tail_world;
                    rt.initialized = true;
                    continue;
                }

                // Verlet: inertia = (current - prev) * (1 - drag)
                let inertia = (rt.current_tail_world - rt.prev_tail_world) * (1.0 - js.drag);
                // Stiffness pulls toward the rest tail.
                let stiffness_force = rest_dir_world * js.stiffness * dt;
                let ext = js.gravity * dt;
                let mut next_tail = rt.current_tail_world + inertia + stiffness_force + ext;
                // Constrain to sphere of radius bone_length around head.
                let delta = next_tail - head_world;
                let len = delta.length();
                if len > 1e-6 {
                    next_tail = head_world + delta * (js.bone_length / len);
                } else {
                    next_tail = head_world + rest_dir_world * js.bone_length;
                }

                // Collider resolution: push the tail outside any penetrating
                // sphere/capsule. Multiple passes not needed for typical VRM hair/skirt.
                for (shape, world) in &resolved_colliders {
                    match shape {
                        Shape::Sphere { offset, radius } => {
                            let c_world = world.transform_point3(*offset);
                            let min_dist = js.hit_radius + *radius;
                            let d = next_tail - c_world;
                            let dl = d.length();
                            if dl > 1e-6 && dl < min_dist {
                                next_tail = c_world + d * (min_dist / dl);
                            }
                        }
                        Shape::Capsule {
                            offset,
                            tail,
                            radius,
                        } => {
                            let a = world.transform_point3(*offset);
                            let b = world.transform_point3(*tail);
                            let ab = b - a;
                            let ab_len_sq = ab.length_squared();
                            let t = if ab_len_sq > 1e-8 {
                                ((next_tail - a).dot(ab) / ab_len_sq).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            let closest = a + ab * t;
                            let min_dist = js.hit_radius + *radius;
                            let d = next_tail - closest;
                            let dl = d.length();
                            if dl > 1e-6 && dl < min_dist {
                                next_tail = closest + d * (min_dist / dl);
                            }
                        }
                    }
                }

                // Re-constrain to bone length after collision push-out.
                let delta = next_tail - head_world;
                let len = delta.length();
                if len > 1e-6 {
                    next_tail = head_world + delta * (js.bone_length / len);
                }

                rt.prev_tail_world = rt.current_tail_world;
                rt.current_tail_world = next_tail;

                // Compute new local rotation: rotate bone_axis_local so that in world
                // space it points along (next_tail - head_world).
                let desired_dir_world = (next_tail - head_world).normalize_or_zero();
                if rest_dir_world.length_squared() < 1e-8
                    || desired_dir_world.length_squared() < 1e-8
                {
                    continue;
                }
                let delta_q_world = Quat::from_rotation_arc(rest_dir_world, desired_dir_world);
                // new_world_rot = delta_q_world * world_rot
                // new_local_rot = parent_world_rot.inverse() * new_world_rot
                //               = parent_world_rot.inverse() * delta_q_world * world_rot
                // But world_rot = parent_world_rot * initial_local_rot, so:
                //   new_local_rot = initial_local_rot.conjugated by (parent_world_rot^-1 * delta_q_world * parent_world_rot)
                // Simpler: compute new_world_rot then factor out parent.
                // We don't have parent_world_rot directly. Approximation: apply delta_q_world on the local rotation.
                // In most VRM chains the parent rotation is identity-ish near the rest pose, so:
                let new_local_rot =
                    (world_rot.conjugate() * delta_q_world * world_rot) * js.initial_local_rot;
                out.push((
                    js.node,
                    [
                        new_local_rot.x,
                        new_local_rot.y,
                        new_local_rot.z,
                        new_local_rot.w,
                    ],
                ));
            }
        }
    }

    /// Number of chains (for debug / telemetry).
    pub fn chain_count(&self) -> usize {
        self.chains.len()
    }

    /// Total number of simulated joints across all chains.
    pub fn joint_count(&self) -> usize {
        self.chains
            .iter()
            .map(|c| c.joints.iter().filter(|j| j.has_tail).count())
            .sum()
    }

    /// Total number of colliders available.
    pub fn collider_count(&self) -> usize {
        self.colliders.len()
    }
}
