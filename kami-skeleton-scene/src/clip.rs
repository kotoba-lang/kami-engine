//! EDN-authored animation clips — `clj/edn` → [`kami_skeleton::AnimationClip`].
//!
//! The data-tier counterpart for skeletal *motion* (alongside this crate's
//! humanoid constraint table): a dance/idle/gesture clip authored as plain EDN,
//! loaded into the clip the hot `kami-skeleton` evaluates + blends. Bone names
//! resolve to indices via a caller-supplied map (VRM humanoid name → skeleton
//! index), so one clip retargets onto any skeleton. `kami-skeleton` stays
//! untouched — the EDN dependency lives only here (ADR-0038).
//!
//! ```edn
//! {:name "wave" :duration 2.0 :loop true
//!  :tracks [{:bone "rightUpperArm" :interp :cubic
//!            :keys [{:t 0.0 :rot [0 0 0 1]} {:t 1.0 :rot [0 0 0.38 0.92]}]}]}
//! ```

use glam::{Quat, Vec3};
use kami_scene::{mget, num, root_map, vec3, EdnValue};
use kami_skeleton::{AnimationClip, BoneTrack, Interpolation, Keyframe};

fn opt_vec3(v: Option<&EdnValue>) -> Option<Vec3> {
    v.and_then(|x| x.as_vector())
        .filter(|s| !s.is_empty())
        .map(|_| Vec3::from(vec3(v)))
}

/// Read a quaternion `[x y z w]`; `None` when absent.
fn opt_quat(v: Option<&EdnValue>) -> Option<Quat> {
    let s = v.and_then(|x| x.as_vector())?;
    if s.is_empty() {
        return None;
    }
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(if i == 3 { 1.0 } else { 0.0 });
    Some(Quat::from_xyzw(g(0), g(1), g(2), g(3)))
}

fn ident(v: Option<&EdnValue>) -> Option<String> {
    v.and_then(|x| {
        x.as_keyword()
            .map(|k| k.0.name.clone())
            .or_else(|| x.as_string().map(|s| s.to_string()))
    })
}

/// Parse an EDN animation clip. `bone_index` maps a track's `:bone` name to a
/// skeleton bone index; tracks whose bone doesn't resolve are dropped. Returns
/// `None` only if the top form isn't a map.
pub fn clip_from_edn<F>(src: &str, bone_index: F) -> Option<AnimationClip>
where
    F: Fn(&str) -> Option<usize>,
{
    let root = root_map(src)?;
    let name = mget(&root, "name").and_then(|v| v.as_string()).unwrap_or("clip").to_string();
    let looping = mget(&root, "loop").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut tracks = Vec::new();
    let mut max_t = 0.0f32;
    for t in mget(&root, "tracks").and_then(|v| v.as_vector()).unwrap_or(&[]) {
        let Some(tm) = t.as_map() else { continue };
        let Some(bone) = ident(mget(tm, "bone")).and_then(|n| bone_index(&n)) else { continue };
        let interp = ident(mget(tm, "interp"))
            .map(|n| Interpolation::by_name(&n))
            .unwrap_or(Interpolation::Linear);
        let keyframes: Vec<Keyframe> = mget(tm, "keys")
            .and_then(|v| v.as_vector())
            .unwrap_or(&[])
            .iter()
            .filter_map(|k| k.as_map())
            .map(|km| {
                let time = num(mget(km, "t"));
                max_t = max_t.max(time);
                Keyframe {
                    time,
                    position: opt_vec3(mget(km, "pos")),
                    rotation: opt_quat(mget(km, "rot")),
                    scale: opt_vec3(mget(km, "scale")),
                }
            })
            .collect();
        if keyframes.is_empty() {
            continue;
        }
        tracks.push(BoneTrack { bone_index: bone, keyframes, interpolation: interp });
    }

    let duration = mget(&root, "duration")
        .map(|v| num(Some(v)))
        .filter(|d| *d > 0.0)
        .unwrap_or(max_t);

    Some(AnimationClip { name, duration, tracks, looping })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(name: &str) -> Option<usize> {
        match name {
            "hips" => Some(0),
            "rightUpperArm" => Some(1),
            _ => None,
        }
    }

    const CLIP: &str = r#"
    {:name "wave" :duration 2.0 :loop true
     :tracks [{:bone "rightUpperArm" :interp :cubic
               :keys [{:t 0.0 :rot [0 0 0 1]}
                      {:t 1.0 :rot [0 0 0.38 0.92]}
                      {:t 2.0 :rot [0 0 0 1]}]}
              {:bone "hips" :interp :linear
               :keys [{:t 0.0 :pos [0 0 0]} {:t 2.0 :pos [0 0.05 0]}]}
              {:bone "unknownBone" :keys [{:t 0.0 :pos [9 9 9]}]}]}
    "#;

    #[test]
    fn parses_clip_with_named_bones() {
        let clip = clip_from_edn(CLIP, idx).expect("clip");
        assert_eq!(clip.name, "wave");
        assert!((clip.duration - 2.0).abs() < 1e-6);
        assert!(clip.looping);
        assert_eq!(clip.tracks.len(), 2, "unknown bone track dropped");
        let arm = clip.tracks.iter().find(|t| t.bone_index == 1).unwrap();
        assert_eq!(arm.interpolation, Interpolation::CubicSpline);
        assert_eq!(arm.keyframes.len(), 3);
        let hips = clip.tracks.iter().find(|t| t.bone_index == 0).unwrap();
        assert_eq!(hips.keyframes[1].position, Some(Vec3::new(0.0, 0.05, 0.0)));
    }

    #[test]
    fn duration_falls_back_to_last_key() {
        let clip = clip_from_edn(
            r#"{:name "g" :tracks [{:bone "hips" :keys [{:t 0.0 :pos [0 0 0]} {:t 1.5 :pos [0 1 0]}]}]}"#,
            idx,
        )
        .unwrap();
        assert!((clip.duration - 1.5).abs() < 1e-6);
        assert!(!clip.looping);
    }

    #[test]
    fn non_map_returns_none() {
        assert!(clip_from_edn("[1 2 3]", idx).is_none());
    }
}
