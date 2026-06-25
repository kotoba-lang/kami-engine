//! MMD `.vmd` (Vocaloid Motion Data) motion import → [`kami_skeleton::AnimationClip`].
//!
//! The binary counterpart of [`crate::clip_from_edn`]: load a MikuMikuDance bone
//! animation and retarget it onto any skeleton via a caller-supplied bone map.
//! MMD bone names are **Shift-JIS**; [`mmd_bone_to_humanoid`] maps the standard
//! ones (センター/頭/左腕/…) to VRM humanoid names so one `.vmd` drives a VRM rig.
//!
//! `.vmd` v2 layout: 30-byte signature + 20-byte model name, then a `u32` bone
//! keyframe count, then that many **111-byte** records:
//!   `[15: name (SJIS)] [4: frame u32] [12: pos f32×3] [16: quat f32×4] [64: interp]`.
//! Morph / camera / light keyframes after the bone block are ignored (motion only).

use glam::{Quat, Vec3};
use kami_skeleton::{AnimationClip, BoneTrack, Interpolation, Keyframe};
use std::collections::BTreeMap;

fn u32_le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn f32_le(b: &[u8], o: usize) -> Option<f32> {
    Some(f32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Decode a null-padded Shift-JIS bone name field.
fn decode_sjis(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    encoding_rs::SHIFT_JIS.decode(&b[..end]).0.into_owned()
}

/// Map a standard MMD bone name (Shift-JIS-decoded) to a VRM humanoid bone name.
/// Covers the common locomotion/upper-body bones; unknown → `None`.
pub fn mmd_bone_to_humanoid(name: &str) -> Option<&'static str> {
    Some(match name {
        "センター" | "center" => "hips",
        "上半身" => "spine",
        "上半身2" => "chest",
        "首" => "neck",
        "頭" => "head",
        "左腕" => "leftUpperArm",
        "右腕" => "rightUpperArm",
        "左ひじ" => "leftLowerArm",
        "右ひじ" => "rightLowerArm",
        "左足" => "leftUpperLeg",
        "右足" => "rightUpperLeg",
        "左ひざ" => "leftLowerLeg",
        "右ひざ" => "rightLowerLeg",
        _ => return None,
    })
}

/// Parse a `.vmd` motion into an [`AnimationClip`]. `fps` is the playback rate
/// (MMD authors at 30); `bone_index` resolves a bone name → skeleton index —
/// it is tried on the raw name first, then on its [`mmd_bone_to_humanoid`]
/// equivalent, so a caller can map either MMD names or VRM humanoid names.
/// Returns `None` if the data is too short or no track resolves.
pub fn vmd_to_clip<F>(bytes: &[u8], fps: f32, bone_index: F) -> Option<AnimationClip>
where
    F: Fn(&str) -> Option<usize>,
{
    let fps = if fps > 0.0 { fps } else { 30.0 };
    // header: 30 (signature) + 20 (model name) = 50, then the u32 keyframe count.
    let mut off = 50usize;
    let count = u32_le(bytes, off)? as usize;
    off += 4;

    let mut by_bone: BTreeMap<usize, Vec<Keyframe>> = BTreeMap::new();
    let mut max_frame = 0u32;
    for _ in 0..count {
        if off + 111 > bytes.len() {
            break;
        }
        let name = decode_sjis(&bytes[off..off + 15]);
        let frame = u32_le(bytes, off + 15)?;
        let pos = Vec3::new(f32_le(bytes, off + 19)?, f32_le(bytes, off + 23)?, f32_le(bytes, off + 27)?);
        let rot = Quat::from_xyzw(
            f32_le(bytes, off + 31)?,
            f32_le(bytes, off + 35)?,
            f32_le(bytes, off + 39)?,
            f32_le(bytes, off + 43)?,
        );
        off += 111;

        let idx = bone_index(&name)
            .or_else(|| mmd_bone_to_humanoid(&name).and_then(|h| bone_index(h)));
        let Some(idx) = idx else { continue };
        max_frame = max_frame.max(frame);
        by_bone.entry(idx).or_default().push(Keyframe {
            time: frame as f32 / fps,
            position: Some(pos),
            rotation: Some(rot),
            scale: None,
        });
    }
    if by_bone.is_empty() {
        return None;
    }
    let tracks = by_bone
        .into_iter()
        .map(|(bone_index, mut keys)| {
            keys.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
            BoneTrack { bone_index, keyframes: keys, interpolation: Interpolation::Linear }
        })
        .collect();
    Some(AnimationClip { name: "vmd".into(), duration: max_frame as f32 / fps, tracks, looping: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal 2-keyframe `.vmd` for `bone_name` (frames 0 and 30).
    fn synthetic_vmd(bone_name: &str) -> Vec<u8> {
        let mut v = vec![0u8; 50]; // 30 sig + 20 model name (zeroed)
        v.extend_from_slice(&2u32.to_le_bytes()); // 2 bone keyframes
        let name_sjis = encoding_rs::SHIFT_JIS.encode(bone_name).0.into_owned();
        for (i, frame) in [0u32, 30].iter().enumerate() {
            let mut name = [0u8; 15];
            let n = name_sjis.len().min(15);
            name[..n].copy_from_slice(&name_sjis[..n]);
            v.extend_from_slice(&name);
            v.extend_from_slice(&frame.to_le_bytes());
            // pos (rise on the 2nd key), quat identity.
            v.extend_from_slice(&0.0f32.to_le_bytes());
            v.extend_from_slice(&(i as f32 * 0.5).to_le_bytes());
            v.extend_from_slice(&0.0f32.to_le_bytes());
            for q in [0.0f32, 0.0, 0.0, 1.0] {
                v.extend_from_slice(&q.to_le_bytes());
            }
            v.extend_from_slice(&[0u8; 64]); // interpolation
        }
        v
    }

    #[test]
    fn parses_vmd_bone_track() {
        let vmd = synthetic_vmd("センター"); // → hips
        let clip = vmd_to_clip(&vmd, 30.0, |n| (n == "hips").then_some(0usize)).expect("clip");
        assert_eq!(clip.tracks.len(), 1, "one bone track");
        assert_eq!(clip.tracks[0].bone_index, 0);
        assert_eq!(clip.tracks[0].keyframes.len(), 2);
        assert!((clip.duration - 1.0).abs() < 1e-6, "30 frames @ 30fps = 1.0s");
        assert!((clip.tracks[0].keyframes[1].time - 1.0).abs() < 1e-6);
    }

    #[test]
    fn humanoid_name_map() {
        assert_eq!(mmd_bone_to_humanoid("頭"), Some("head"));
        assert_eq!(mmd_bone_to_humanoid("左腕"), Some("leftUpperArm"));
        assert_eq!(mmd_bone_to_humanoid("unknown"), None);
    }
}
