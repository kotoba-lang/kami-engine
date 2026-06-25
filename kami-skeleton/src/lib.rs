//! kami-skeleton: Skeletal animation (bone hierarchy, skinning, blend).
//!
//! glTF-compatible bone system. GPU skinning via joint matrices.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// A single bone in the skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub local_position: [f32; 3],
    pub local_rotation: [f32; 4], // quaternion xyzw
    pub local_scale: [f32; 3],
    pub inverse_bind: [[f32; 4]; 4], // inverse bind matrix (column-major)
}

/// Skeleton: bone hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

/// Animation keyframe.
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub time: f32,
    pub position: Option<Vec3>,
    pub rotation: Option<Quat>,
    pub scale: Option<Vec3>,
}

/// Keyframe interpolation mode (matches glTF `LINEAR` / `STEP` / `CUBICSPLINE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    #[default]
    Linear,
    /// Hold the previous keyframe's value (no blend) — glTF `STEP`.
    Step,
    /// Smooth spline through the keyframes — Catmull-Rom for translation/scale,
    /// smoothstep-eased slerp for rotation (glTF `CUBICSPLINE`, auto-tangent).
    CubicSpline,
}

impl Interpolation {
    pub fn by_name(name: &str) -> Interpolation {
        match name {
            "step" => Interpolation::Step,
            "cubic" | "cubicspline" | "cubic-spline" => Interpolation::CubicSpline,
            _ => Interpolation::Linear,
        }
    }
}

/// Animation clip for one bone.
#[derive(Debug, Clone)]
pub struct BoneTrack {
    pub bone_index: usize,
    pub keyframes: Vec<Keyframe>,
    /// How to interpolate between keyframes.
    pub interpolation: Interpolation,
}

impl BoneTrack {
    /// A linear-interpolated track (the common case).
    pub fn new(bone_index: usize, keyframes: Vec<Keyframe>) -> Self {
        Self { bone_index, keyframes, interpolation: Interpolation::Linear }
    }
    /// Set the interpolation mode (builder style).
    pub fn with_interpolation(mut self, i: Interpolation) -> Self {
        self.interpolation = i;
        self
    }
}

/// Animation clip.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub tracks: Vec<BoneTrack>,
    pub looping: bool,
}

impl AnimationClip {
    /// Retarget this clip from `source` to `target` by **bone name** — the
    /// `SkeletonUtils.retargetClip` analogue. Each track's source bone index is
    /// resolved to its name in `source`, then matched to the same-named bone in
    /// `target`; the track is re-emitted with the target index (keyframes +
    /// interpolation preserved). Tracks with no name match in `target` are
    /// dropped. Assumes compatible rest orientation (e.g. both VRM 1.0 T-pose),
    /// so local rotations transfer directly.
    pub fn retarget(&self, source: &Skeleton, target: &Skeleton) -> AnimationClip {
        let target_index = |name: &str| target.bones.iter().position(|b| b.name == name);
        let tracks = self
            .tracks
            .iter()
            .filter_map(|t| {
                let name = source.bones.get(t.bone_index).map(|b| b.name.as_str())?;
                let ti = target_index(name)?;
                Some(BoneTrack {
                    bone_index: ti,
                    keyframes: t.keyframes.clone(),
                    interpolation: t.interpolation,
                })
            })
            .collect();
        AnimationClip {
            name: self.name.clone(),
            duration: self.duration,
            tracks,
            looping: self.looping,
        }
    }
}

/// Joint matrix for GPU skinning (4x4, column-major).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct JointMatrix {
    pub mat: [[f32; 4]; 4],
}

impl Skeleton {
    /// Compute world transforms for all bones at a given animation time.
    pub fn evaluate(&self, clip: &AnimationClip, time: f32) -> Vec<Mat4> {
        let n = self.bones.len();
        let mut local_transforms = Vec::with_capacity(n);

        for (i, bone) in self.bones.iter().enumerate() {
            let pos = Vec3::from(bone.local_position);
            let rot = Quat::from_array(bone.local_rotation);
            let scl = Vec3::from(bone.local_scale);

            // Find track for this bone and interpolate
            let (p, r, s) = if let Some(track) = clip.tracks.iter().find(|t| t.bone_index == i) {
                interpolate_track(track, time)
            } else {
                (pos, rot, scl)
            };

            local_transforms.push(Mat4::from_scale_rotation_translation(s, r, p));
        }

        // Compute world transforms (parent-first order assumed)
        let mut world = vec![Mat4::IDENTITY; n];
        for i in 0..n {
            world[i] = match self.bones[i].parent {
                Some(p) => world[p] * local_transforms[i],
                None => local_transforms[i],
            };
        }
        world
    }

    /// Per-bone local TRS for one clip at `time` (rest pose where no track).
    fn local_trs(&self, clip: &AnimationClip, time: f32) -> Vec<(Vec3, Quat, Vec3)> {
        self.bones
            .iter()
            .enumerate()
            .map(|(i, bone)| {
                if let Some(track) = clip.tracks.iter().find(|t| t.bone_index == i) {
                    interpolate_track(track, time)
                } else {
                    (
                        Vec3::from(bone.local_position),
                        Quat::from_array(bone.local_rotation),
                        Vec3::from(bone.local_scale),
                    )
                }
            })
            .collect()
    }

    /// Blend several clips into one pose and return world matrices —
    /// `AnimationMixer`-style weighted blending / cross-fade. Each layer is
    /// `(clip, time, weight)`; weights are normalised. Translation/scale blend
    /// by weighted average, rotation by weighted nlerp (hemisphere-aligned).
    /// Empty / zero-weight input → the skeleton's rest pose.
    pub fn evaluate_blend(&self, layers: &[(&AnimationClip, f32, f32)]) -> Vec<Mat4> {
        let n = self.bones.len();
        let total: f32 = layers.iter().map(|(_, _, w)| w.max(0.0)).sum();
        if n == 0 || total <= 0.0 {
            return self.rest_world();
        }
        // Evaluate each layer's local TRS once.
        let per_layer: Vec<(Vec<(Vec3, Quat, Vec3)>, f32)> = layers
            .iter()
            .filter(|(_, _, w)| *w > 0.0)
            .map(|(clip, time, w)| (self.local_trs(clip, *time), w / total))
            .collect();

        let mut local = Vec::with_capacity(n);
        for i in 0..n {
            let mut pos = Vec3::ZERO;
            let mut scl = Vec3::ZERO;
            let mut acc = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
            let mut reference: Option<Quat> = None;
            for (trs, w) in &per_layer {
                let (p, mut r, s) = trs[i];
                pos += p * *w;
                scl += s * *w;
                // Align to a hemisphere reference so the nlerp sum is stable.
                match reference {
                    None => reference = Some(r),
                    Some(rf) => {
                        if rf.dot(r) < 0.0 {
                            r = Quat::from_xyzw(-r.x, -r.y, -r.z, -r.w);
                        }
                    }
                }
                acc = Quat::from_xyzw(
                    acc.x + r.x * *w,
                    acc.y + r.y * *w,
                    acc.z + r.z * *w,
                    acc.w + r.w * *w,
                );
            }
            let rot = if acc.length_squared() > 1e-12 {
                acc.normalize()
            } else {
                Quat::IDENTITY
            };
            local.push(Mat4::from_scale_rotation_translation(scl, rot, pos));
        }

        let mut world = vec![Mat4::IDENTITY; n];
        for i in 0..n {
            world[i] = match self.bones[i].parent {
                Some(p) => world[p] * local[i],
                None => local[i],
            };
        }
        world
    }

    /// Cross-fade from `from` to `to` by `alpha` in [0,1] (0 = all `from`).
    pub fn evaluate_crossfade(
        &self,
        from: &AnimationClip,
        from_time: f32,
        to: &AnimationClip,
        to_time: f32,
        alpha: f32,
    ) -> Vec<Mat4> {
        let a = alpha.clamp(0.0, 1.0);
        self.evaluate_blend(&[(from, from_time, 1.0 - a), (to, to_time, a)])
    }

    /// Solve a bone chain with **CCD inverse kinematics** so the chain tip
    /// (effector) reaches `target` (world space) — the `CCDIKSolver` analogue.
    ///
    /// `chain` is bone indices ordered **root → effector**. Returns the updated
    /// **local** rotations for the chain's joints (every bone except the
    /// effector). Starts from the rest pose; `iterations` cycles of Cyclic
    /// Coordinate Descent, stopping early once within `threshold` of the target.
    pub fn solve_ik_ccd(
        &self,
        chain: &[usize],
        target: Vec3,
        iterations: usize,
        threshold: f32,
    ) -> Vec<(usize, Quat)> {
        let n = self.bones.len();
        if chain.len() < 2 || chain.iter().any(|&b| b >= n) {
            return Vec::new();
        }
        let pos: Vec<Vec3> = self.bones.iter().map(|b| Vec3::from(b.local_position)).collect();
        let scl: Vec<Vec3> = self.bones.iter().map(|b| Vec3::from(b.local_scale)).collect();
        let mut local_rot: Vec<Quat> =
            self.bones.iter().map(|b| Quat::from_array(b.local_rotation)).collect();

        let world_of = |local_rot: &[Quat]| -> Vec<Mat4> {
            let mut w = vec![Mat4::IDENTITY; n];
            for i in 0..n {
                let l = Mat4::from_scale_rotation_translation(scl[i], local_rot[i], pos[i]);
                w[i] = match self.bones[i].parent {
                    Some(p) => w[p] * l,
                    None => l,
                };
            }
            w
        };
        let trans = |m: &Mat4| m.to_scale_rotation_translation().2;
        let rot = |m: &Mat4| m.to_scale_rotation_translation().1;

        let effector = *chain.last().unwrap();
        for _ in 0..iterations.max(1) {
            if trans(&world_of(&local_rot)[effector]).distance(target) < threshold {
                break;
            }
            // adjust each joint from near-effector up to the chain root.
            for &bone in chain[..chain.len() - 1].iter().rev() {
                let world = world_of(&local_rot);
                let bpos = trans(&world[bone]);
                let to_eff = trans(&world[effector]) - bpos;
                let to_tgt = target - bpos;
                if to_eff.length() < 1e-6 || to_tgt.length() < 1e-6 {
                    continue;
                }
                let delta = Quat::from_rotation_arc(to_eff.normalize(), to_tgt.normalize());
                let new_world_rot = (delta * rot(&world[bone])).normalize();
                let parent_rot = self.bones[bone]
                    .parent
                    .map(|p| rot(&world[p]))
                    .unwrap_or(Quat::IDENTITY);
                local_rot[bone] = (parent_rot.inverse() * new_world_rot).normalize();
            }
        }
        chain[..chain.len() - 1].iter().map(|&b| (b, local_rot[b])).collect()
    }

    /// World matrices of the unanimated rest pose.
    fn rest_world(&self) -> Vec<Mat4> {
        let n = self.bones.len();
        let local: Vec<Mat4> = self
            .bones
            .iter()
            .map(|b| {
                Mat4::from_scale_rotation_translation(
                    Vec3::from(b.local_scale),
                    Quat::from_array(b.local_rotation),
                    Vec3::from(b.local_position),
                )
            })
            .collect();
        let mut world = vec![Mat4::IDENTITY; n];
        for i in 0..n {
            world[i] = match self.bones[i].parent {
                Some(p) => world[p] * local[i],
                None => local[i],
            };
        }
        world
    }

    /// Compute world transforms with anatomical joint constraints applied.
    ///
    /// Each entry in `constraints` maps a bone index to its constraint.
    /// Bones without constraints are unclamped.
    pub fn evaluate_constrained(
        &self,
        clip: &AnimationClip,
        time: f32,
        constraints: &[(usize, JointConstraint)],
    ) -> Vec<Mat4> {
        let n = self.bones.len();
        let mut local_transforms = Vec::with_capacity(n);

        for (i, bone) in self.bones.iter().enumerate() {
            let pos = Vec3::from(bone.local_position);
            let rot = Quat::from_array(bone.local_rotation);
            let scl = Vec3::from(bone.local_scale);

            let (p, mut r, s) =
                if let Some(track) = clip.tracks.iter().find(|t| t.bone_index == i) {
                    interpolate_track(track, time)
                } else {
                    (pos, rot, scl)
                };

            // Apply joint constraint if present for this bone
            if let Some((_, constraint)) = constraints.iter().find(|(idx, _)| *idx == i) {
                r = constraint.clamp(r);
            }

            local_transforms.push(Mat4::from_scale_rotation_translation(s, r, p));
        }

        let mut world = vec![Mat4::IDENTITY; n];
        for i in 0..n {
            world[i] = match self.bones[i].parent {
                Some(p) => world[p] * local_transforms[i],
                None => local_transforms[i],
            };
        }
        world
    }

    /// Compute joint matrices with anatomical constraints for GPU upload.
    pub fn joint_matrices_constrained(
        &self,
        clip: &AnimationClip,
        time: f32,
        constraints: &[(usize, JointConstraint)],
    ) -> Vec<JointMatrix> {
        let world = self.evaluate_constrained(clip, time, constraints);
        self.bones
            .iter()
            .enumerate()
            .map(|(i, bone)| {
                let inv_bind = Mat4::from_cols_array_2d(&bone.inverse_bind);
                let joint = world[i] * inv_bind;
                JointMatrix {
                    mat: joint.to_cols_array_2d(),
                }
            })
            .collect()
    }

    /// Build constraint index from bone names using `default_humanoid_constraints`.
    ///
    /// Returns pairs of `(bone_index, JointConstraint)` for bones found in this
    /// skeleton by name.
    pub fn build_humanoid_constraints(&self) -> Vec<(usize, JointConstraint)> {
        let defaults = default_humanoid_constraints();
        let mut result = Vec::new();
        for (name, constraint) in defaults {
            if let Some(idx) = self.bones.iter().position(|b| b.name == name) {
                result.push((idx, constraint));
            }
        }
        result
    }

    /// Compute joint matrices for GPU upload (world * inverse_bind).
    pub fn joint_matrices(&self, clip: &AnimationClip, time: f32) -> Vec<JointMatrix> {
        let world = self.evaluate(clip, time);
        self.bones
            .iter()
            .enumerate()
            .map(|(i, bone)| {
                let inv_bind = Mat4::from_cols_array_2d(&bone.inverse_bind);
                let joint = world[i] * inv_bind;
                JointMatrix {
                    mat: joint.to_cols_array_2d(),
                }
            })
            .collect()
    }
}

/// Anatomical joint rotation constraint (Euler angles in radians).
///
/// Constrains bone rotation to prevent humanly impossible poses.
/// Each axis specifies `[min, max]` in radians. Applied after interpolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointConstraint {
    /// Minimum Euler angle per axis `[x, y, z]` in radians.
    pub min: [f32; 3],
    /// Maximum Euler angle per axis `[x, y, z]` in radians.
    pub max: [f32; 3],
}

impl JointConstraint {
    /// Clamp a quaternion rotation to the Euler angle limits.
    ///
    /// Decomposes the quaternion to Euler XYZ, clamps each axis, then
    /// recomposes. Suitable for humanoid bones where gimbal lock is
    /// unlikely within normal anatomical ranges.
    pub fn clamp(&self, rotation: Quat) -> Quat {
        let (x, y, z) = quat_to_euler_xyz(rotation);
        let cx = x.clamp(self.min[0], self.max[0]);
        let cy = y.clamp(self.min[1], self.max[1]);
        let cz = z.clamp(self.min[2], self.max[2]);
        euler_xyz_to_quat(cx, cy, cz)
    }
}

/// Default anatomical constraints for VRM humanoid bones.
///
/// Returns `(bone_name, JointConstraint)` pairs covering standard humanoid
/// skeleton bones. Values derived from orthopedic range-of-motion references.
pub fn default_humanoid_constraints() -> Vec<(&'static str, JointConstraint)> {
    let d = std::f32::consts::PI / 180.0;
    vec![
        ("head", JointConstraint { min: [-60.0 * d, -80.0 * d, -40.0 * d], max: [60.0 * d, 80.0 * d, 40.0 * d] }),
        ("neck", JointConstraint { min: [-30.0 * d, -45.0 * d, -30.0 * d], max: [30.0 * d, 45.0 * d, 30.0 * d] }),
        ("spine", JointConstraint { min: [-30.0 * d, -30.0 * d, -20.0 * d], max: [30.0 * d, 30.0 * d, 20.0 * d] }),
        ("chest", JointConstraint { min: [-15.0 * d, -15.0 * d, -10.0 * d], max: [15.0 * d, 15.0 * d, 10.0 * d] }),
        ("hips", JointConstraint { min: [-30.0 * d, -30.0 * d, -15.0 * d], max: [30.0 * d, 30.0 * d, 15.0 * d] }),
        ("leftUpperArm", JointConstraint { min: [-60.0 * d, -45.0 * d, -30.0 * d], max: [90.0 * d, 90.0 * d, 180.0 * d] }),
        ("rightUpperArm", JointConstraint { min: [-60.0 * d, -90.0 * d, -180.0 * d], max: [90.0 * d, 45.0 * d, 30.0 * d] }),
        ("leftLowerArm", JointConstraint { min: [-5.0 * d, 0.0, -5.0 * d], max: [5.0 * d, 145.0 * d, 5.0 * d] }),
        ("rightLowerArm", JointConstraint { min: [-5.0 * d, -145.0 * d, -5.0 * d], max: [5.0 * d, 0.0, 5.0 * d] }),
        ("leftUpperLeg", JointConstraint { min: [-30.0 * d, -45.0 * d, -20.0 * d], max: [120.0 * d, 30.0 * d, 45.0 * d] }),
        ("rightUpperLeg", JointConstraint { min: [-30.0 * d, -30.0 * d, -45.0 * d], max: [120.0 * d, 45.0 * d, 20.0 * d] }),
        ("leftLowerLeg", JointConstraint { min: [-140.0 * d, -5.0 * d, -5.0 * d], max: [0.0, 5.0 * d, 5.0 * d] }),
        ("rightLowerLeg", JointConstraint { min: [-140.0 * d, -5.0 * d, -5.0 * d], max: [0.0, 5.0 * d, 5.0 * d] }),
    ]
}

/// Decompose quaternion to intrinsic Euler XYZ angles.
fn quat_to_euler_xyz(q: Quat) -> (f32, f32, f32) {
    let (x, y, z, w) = (q.x, q.y, q.z, q.w);
    let sinr_cosp = 2.0 * (w * x + y * z);
    let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
    let roll = sinr_cosp.atan2(cosr_cosp);
    let sinp = 2.0 * (w * y - z * x);
    let pitch = if sinp.abs() >= 1.0 {
        std::f32::consts::FRAC_PI_2.copysign(sinp)
    } else {
        sinp.asin()
    };
    let siny_cosp = 2.0 * (w * z + x * y);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    let yaw = siny_cosp.atan2(cosy_cosp);
    (roll, pitch, yaw)
}

/// Compose intrinsic Euler XYZ angles to quaternion.
fn euler_xyz_to_quat(x: f32, y: f32, z: f32) -> Quat {
    Quat::from_rotation_z(z) * Quat::from_rotation_y(y) * Quat::from_rotation_x(x)
}

fn interpolate_track(track: &BoneTrack, time: f32) -> (Vec3, Quat, Vec3) {
    let kfs = &track.keyframes;
    if kfs.is_empty() {
        return (Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    }
    if kfs.len() == 1 || time <= kfs[0].time {
        let k = &kfs[0];
        return (
            k.position.unwrap_or(Vec3::ZERO),
            k.rotation.unwrap_or(Quat::IDENTITY),
            k.scale.unwrap_or(Vec3::ONE),
        );
    }
    let last = &kfs[kfs.len() - 1];
    if time >= last.time {
        return (
            last.position.unwrap_or(Vec3::ZERO),
            last.rotation.unwrap_or(Quat::IDENTITY),
            last.scale.unwrap_or(Vec3::ONE),
        );
    }

    // Find bracket
    let mut i = 0;
    while i < kfs.len() - 1 && kfs[i + 1].time < time {
        i += 1;
    }
    let a = &kfs[i];
    let b = &kfs[i + 1];
    let t = (time - a.time) / (b.time - a.time);

    match track.interpolation {
        Interpolation::Step => (
            a.position.unwrap_or(Vec3::ZERO),
            a.rotation.unwrap_or(Quat::IDENTITY),
            a.scale.unwrap_or(Vec3::ONE),
        ),
        Interpolation::Linear => {
            let pos = a.position.unwrap_or(Vec3::ZERO).lerp(b.position.unwrap_or(Vec3::ZERO), t);
            let rot = a.rotation.unwrap_or(Quat::IDENTITY).slerp(b.rotation.unwrap_or(Quat::IDENTITY), t);
            let scl = a.scale.unwrap_or(Vec3::ONE).lerp(b.scale.unwrap_or(Vec3::ONE), t);
            (pos, rot, scl)
        }
        Interpolation::CubicSpline => {
            // Catmull-Rom with clamped neighbours for translation/scale.
            let p0p = kfs[i.saturating_sub(1)].position.unwrap_or_else(|| a.position.unwrap_or(Vec3::ZERO));
            let p3p = kfs[(i + 2).min(kfs.len() - 1)].position.unwrap_or_else(|| b.position.unwrap_or(Vec3::ZERO));
            let pos = catmull_rom(p0p, a.position.unwrap_or(Vec3::ZERO), b.position.unwrap_or(Vec3::ZERO), p3p, t);
            let p0s = kfs[i.saturating_sub(1)].scale.unwrap_or_else(|| a.scale.unwrap_or(Vec3::ONE));
            let p3s = kfs[(i + 2).min(kfs.len() - 1)].scale.unwrap_or_else(|| b.scale.unwrap_or(Vec3::ONE));
            let scl = catmull_rom(p0s, a.scale.unwrap_or(Vec3::ONE), b.scale.unwrap_or(Vec3::ONE), p3s, t);
            // smoothstep-eased slerp for rotation (C1 at the keyframes).
            let te = t * t * (3.0 - 2.0 * t);
            let rot = a.rotation.unwrap_or(Quat::IDENTITY).slerp(b.rotation.unwrap_or(Quat::IDENTITY), te);
            (pos, rot, scl)
        }
    }
}

/// Centripetal-uniform Catmull-Rom spline point at `t` in [0,1] between p1,p2.
fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skeleton_eval() {
        let skeleton = Skeleton {
            bones: vec![
                Bone {
                    name: "root".into(),
                    parent: None,
                    local_position: [0.0; 3],
                    local_rotation: [0.0, 0.0, 0.0, 1.0],
                    local_scale: [1.0; 3],
                    inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
                },
                Bone {
                    name: "arm".into(),
                    parent: Some(0),
                    local_position: [1.0, 0.0, 0.0],
                    local_rotation: [0.0, 0.0, 0.0, 1.0],
                    local_scale: [1.0; 3],
                    inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
                },
            ],
        };
        let clip = AnimationClip {
            name: "idle".into(),
            duration: 1.0,
            tracks: vec![],
            looping: true,
        };
        let joints = skeleton.joint_matrices(&clip, 0.0);
        assert_eq!(joints.len(), 2);
    }

    #[test]
    fn test_joint_constraint_clamp() {
        let d = std::f32::consts::PI / 180.0;
        let constraint = JointConstraint {
            min: [-30.0 * d, -30.0 * d, -30.0 * d],
            max: [30.0 * d, 30.0 * d, 30.0 * d],
        };
        // Rotation within limits should pass through unchanged (approximately)
        let small = Quat::from_rotation_x(10.0 * d);
        let clamped = constraint.clamp(small);
        let (cx, _, _) = quat_to_euler_xyz(clamped);
        assert!((cx - 10.0 * d).abs() < 0.01);

        // Rotation exceeding limits should be clamped
        let big = Quat::from_rotation_x(90.0 * d);
        let clamped = constraint.clamp(big);
        let (cx, _, _) = quat_to_euler_xyz(clamped);
        assert!((cx - 30.0 * d).abs() < 0.01);
    }

    #[test]
    fn test_default_humanoid_constraints() {
        let constraints = default_humanoid_constraints();
        assert!(constraints.len() >= 13);
        // Verify head constraint exists with expected range
        let (name, c) = &constraints[0];
        assert_eq!(*name, "head");
        let d = std::f32::consts::PI / 180.0;
        assert!((c.max[0] - 60.0 * d).abs() < 0.001);
    }

    #[test]
    fn test_evaluate_constrained() {
        let d = std::f32::consts::PI / 180.0;
        let skeleton = Skeleton {
            bones: vec![
                Bone {
                    name: "root".into(),
                    parent: None,
                    local_position: [0.0; 3],
                    local_rotation: [0.0, 0.0, 0.0, 1.0],
                    local_scale: [1.0; 3],
                    inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
                },
                Bone {
                    name: "head".into(),
                    parent: Some(0),
                    local_position: [0.0, 1.0, 0.0],
                    local_rotation: [0.0, 0.0, 0.0, 1.0],
                    local_scale: [1.0; 3],
                    inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
                },
            ],
        };
        // Animate head with extreme rotation (90° X)
        let clip = AnimationClip {
            name: "extreme".into(),
            duration: 1.0,
            tracks: vec![BoneTrack::new(
                1,
                vec![Keyframe {
                    time: 0.0,
                    position: Some(Vec3::new(0.0, 1.0, 0.0)),
                    rotation: Some(Quat::from_rotation_x(90.0 * d)),
                    scale: Some(Vec3::ONE),
                }],
            )],
            looping: false,
        };
        let constraints = skeleton.build_humanoid_constraints();
        let world = skeleton.evaluate_constrained(&clip, 0.0, &constraints);
        assert_eq!(world.len(), 2);
        // Head should be clamped — verify it differs from unconstrained
        let unconstrained = skeleton.evaluate(&clip, 0.0);
        assert_ne!(
            world[1].to_cols_array_2d(),
            unconstrained[1].to_cols_array_2d()
        );
    }

    fn one_bone() -> Skeleton {
        Skeleton {
            bones: vec![Bone {
                name: "root".into(),
                parent: None,
                local_position: [0.0, 0.0, 0.0],
                local_rotation: [0.0, 0.0, 0.0, 1.0],
                local_scale: [1.0, 1.0, 1.0],
                inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
            }],
        }
    }

    fn pos_clip(interp: Interpolation, pts: &[(f32, f32)]) -> AnimationClip {
        let kfs = pts
            .iter()
            .map(|(t, x)| Keyframe {
                time: *t,
                position: Some(Vec3::new(*x, 0.0, 0.0)),
                rotation: None,
                scale: None,
            })
            .collect();
        AnimationClip {
            name: "c".into(),
            duration: pts.last().map(|(t, _)| *t).unwrap_or(0.0),
            tracks: vec![BoneTrack::new(0, kfs).with_interpolation(interp)],
            looping: false,
        }
    }

    fn x_of(world: &[Mat4]) -> f32 {
        world[0].to_cols_array()[12]
    }

    #[test]
    fn step_holds_previous_keyframe() {
        let sk = one_bone();
        let clip = pos_clip(Interpolation::Step, &[(0.0, 0.0), (1.0, 10.0)]);
        // at t=0.5, STEP holds the t=0 value (0), not the linear midpoint (5).
        assert!((x_of(&sk.evaluate(&clip, 0.5)) - 0.0).abs() < 1e-5);
        assert!((x_of(&sk.evaluate(&clip, 1.0)) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn cubic_passes_through_keyframes_and_smooths() {
        let sk = one_bone();
        // asymmetric points so the smooth spline departs from the linear chord.
        let pts = [(0.0, 0.0), (1.0, 0.0), (2.0, 10.0), (3.0, 30.0)];
        let cubic = pos_clip(Interpolation::CubicSpline, &pts);
        // passes through the keyframes exactly.
        assert!((x_of(&sk.evaluate(&cubic, 1.0)) - 0.0).abs() < 1e-4);
        assert!((x_of(&sk.evaluate(&cubic, 2.0)) - 10.0).abs() < 1e-4);
        // between kf1 and kf2 the smooth spline differs from the linear midpoint.
        let lin = pos_clip(Interpolation::Linear, &pts);
        let cubic_mid = x_of(&sk.evaluate(&cubic, 1.5));
        let lin_mid = x_of(&sk.evaluate(&lin, 1.5));
        assert!((cubic_mid - lin_mid).abs() > 1e-3, "cubic {cubic_mid} vs linear {lin_mid}");
    }

    #[test]
    fn blend_two_clips_averages_by_weight() {
        let sk = one_bone();
        let a = pos_clip(Interpolation::Step, &[(0.0, 0.0)]); // x=0
        let b = pos_clip(Interpolation::Step, &[(0.0, 10.0)]); // x=10
        // equal weights → midpoint 5; 75/25 → 2.5.
        let mid = x_of(&sk.evaluate_blend(&[(&a, 0.0, 0.5), (&b, 0.0, 0.5)]));
        assert!((mid - 5.0).abs() < 1e-4, "got {mid}");
        let q = x_of(&sk.evaluate_blend(&[(&a, 0.0, 0.75), (&b, 0.0, 0.25)]));
        assert!((q - 2.5).abs() < 1e-4, "got {q}");
    }

    #[test]
    fn crossfade_endpoints_match_source_clips() {
        let sk = one_bone();
        let a = pos_clip(Interpolation::Step, &[(0.0, 0.0)]);
        let b = pos_clip(Interpolation::Step, &[(0.0, 10.0)]);
        assert!((x_of(&sk.evaluate_crossfade(&a, 0.0, &b, 0.0, 0.0)) - 0.0).abs() < 1e-4);
        assert!((x_of(&sk.evaluate_crossfade(&a, 0.0, &b, 0.0, 1.0)) - 10.0).abs() < 1e-4);
        assert!((x_of(&sk.evaluate_crossfade(&a, 0.0, &b, 0.0, 0.5)) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn zero_weight_blend_is_rest_pose() {
        let sk = one_bone();
        let a = pos_clip(Interpolation::Step, &[(0.0, 99.0)]);
        let rest = sk.evaluate_blend(&[(&a, 0.0, 0.0)]);
        assert!((x_of(&rest) - 0.0).abs() < 1e-5, "rest pose, no animation applied");
    }

    fn chain3() -> Skeleton {
        // root at origin, two unit-length child bones along +X (effector reach 2).
        let bone = |name: &str, parent: Option<usize>, x: f32| Bone {
            name: name.into(),
            parent,
            local_position: [x, 0.0, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0, 1.0, 1.0],
            inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
        };
        Skeleton { bones: vec![bone("root", None, 0.0), bone("mid", Some(0), 1.0), bone("tip", Some(1), 1.0)] }
    }

    fn effector_pos(sk: &Skeleton, overrides: &[(usize, Quat)]) -> Vec3 {
        // apply local-rotation overrides, then FK to the tip.
        let mut local_rot: Vec<Quat> = sk.bones.iter().map(|b| Quat::from_array(b.local_rotation)).collect();
        for (i, q) in overrides {
            local_rot[*i] = *q;
        }
        let mut world = vec![Mat4::IDENTITY; sk.bones.len()];
        for i in 0..sk.bones.len() {
            let b = &sk.bones[i];
            let l = Mat4::from_scale_rotation_translation(Vec3::from(b.local_scale), local_rot[i], Vec3::from(b.local_position));
            world[i] = match b.parent { Some(p) => world[p] * l, None => l };
        }
        world[2].to_scale_rotation_translation().2
    }

    #[test]
    fn ccd_ik_reaches_a_reachable_target() {
        let sk = chain3();
        // straight up, distance 2 from origin — within the chain's reach.
        let target = Vec3::new(0.0, 2.0, 0.0);
        let solved = sk.solve_ik_ccd(&[0, 1, 2], target, 32, 1e-3);
        assert_eq!(solved.len(), 2, "two joints adjusted (root + mid, not the tip)");
        let eff = effector_pos(&sk, &solved);
        assert!(eff.distance(target) < 0.05, "effector at {eff:?}, target {target:?}");
    }

    #[test]
    fn ccd_ik_clamps_to_reach_for_far_target() {
        let sk = chain3();
        // far away on +Y: unreachable (reach 2), effector should extend toward it.
        let target = Vec3::new(0.0, 100.0, 0.0);
        let solved = sk.solve_ik_ccd(&[0, 1, 2], target, 32, 1e-3);
        let eff = effector_pos(&sk, &solved);
        // effector points up near max reach (≈ (0,2,0)).
        assert!(eff.y > 1.9, "extended toward target, y={}", eff.y);
        assert!(eff.x.abs() < 0.2 && eff.z.abs() < 0.2);
    }

    #[test]
    fn retarget_remaps_tracks_by_bone_name() {
        // source: hips(0), spine(1). target has a different layout/order.
        let mk = |names: &[&str], parents: &[Option<usize>]| Skeleton {
            bones: names
                .iter()
                .zip(parents)
                .map(|(n, p)| Bone {
                    name: (*n).into(),
                    parent: *p,
                    local_position: [0.0, 0.0, 0.0],
                    local_rotation: [0.0, 0.0, 0.0, 1.0],
                    local_scale: [1.0, 1.0, 1.0],
                    inverse_bind: Mat4::IDENTITY.to_cols_array_2d(),
                })
                .collect(),
        };
        let source = mk(&["hips", "spine"], &[None, Some(0)]);
        let target = mk(&["root", "spine", "hips"], &[None, Some(0), Some(1)]);

        let clip = AnimationClip {
            name: "dance".into(),
            duration: 1.0,
            tracks: vec![
                BoneTrack::new(0, vec![Keyframe { time: 0.0, position: Some(Vec3::new(1.0, 2.0, 3.0)), rotation: None, scale: None }])
                    .with_interpolation(Interpolation::CubicSpline),
                BoneTrack::new(1, vec![Keyframe { time: 0.0, position: Some(Vec3::Y), rotation: None, scale: None }]),
                // a track for a bone the target lacks → dropped.
                BoneTrack::new(99, vec![Keyframe { time: 0.0, position: Some(Vec3::ZERO), rotation: None, scale: None }]),
            ],
            looping: true,
        };
        let rt = clip.retarget(&source, &target);
        assert_eq!(rt.name, "dance");
        assert!(rt.looping);
        assert_eq!(rt.tracks.len(), 2, "two matched bones, one dropped");
        // hips: source idx 0 → target idx 2; keyframes + interp preserved.
        let hips = rt.tracks.iter().find(|t| t.bone_index == 2).expect("hips remapped");
        assert_eq!(hips.interpolation, Interpolation::CubicSpline);
        assert_eq!(hips.keyframes[0].position, Some(Vec3::new(1.0, 2.0, 3.0)));
        // spine: source idx 1 → target idx 1 (same name, coincidentally same index).
        assert!(rt.tracks.iter().any(|t| t.bone_index == 1));
    }

    #[test]
    fn ccd_ik_rejects_degenerate_chains() {
        let sk = chain3();
        assert!(sk.solve_ik_ccd(&[0], Vec3::ZERO, 8, 1e-3).is_empty(), "chain too short");
        assert!(sk.solve_ik_ccd(&[0, 99], Vec3::ZERO, 8, 1e-3).is_empty(), "out-of-range bone");
    }

    #[test]
    fn interpolation_by_name() {
        assert_eq!(Interpolation::by_name("step"), Interpolation::Step);
        assert_eq!(Interpolation::by_name("cubic"), Interpolation::CubicSpline);
        assert_eq!(Interpolation::by_name("cubic-spline"), Interpolation::CubicSpline);
        assert_eq!(Interpolation::by_name("whatever"), Interpolation::Linear);
    }
}
