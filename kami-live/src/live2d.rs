//! Live2D — a 2D (Cubism-style) avatar driven from the same beat-synced show.
//!
//! VRM (ADR-0043) is a 3D rig; Live2D is a 2D parameter-warp avatar. Both are
//! authored as data and driven by the *same* choreography: the show clock's
//! [`DancePose`] and [`BeatPhase`] map onto the standard Cubism parameters
//! (`ParamAngleX/Y/Z`, `ParamBodyAngle*`, `ParamBreath`, eye blink, mouth
//! lipsync). So one `:dance/setlist` animates a VRM *or* a Live2D performer.
//!
//! This is the **data + driver** layer (parallel to `AvatarBinding`): it parses
//! `:dance/live2d`, resolves per-frame parameter values, and emits a render-IR
//! `:live2d` entry. The actual `.moc3`/`.model3.json` ArtMesh warp + Live2D
//! physics is host-side (the Cubism runtime), exactly as VRM mesh skinning is.
//!
//! ```edn
//! :dance/live2d
//! {:model   "models/haru.model3.json"
//!  :home    [0.0 0.0 0.0] :scale 1.0
//!  :physics true
//!  :lipsync :ParamMouthOpenY
//!  :params  {:ParamAngleX 0.0 :ParamEyeLOpen 1.0}   ;; base/rest values
//!  :motions [{:name "idle" :file "idle.motion3.json"}]}
//! ```

use std::collections::BTreeMap;

use kami_scene::{mget, num, vec3, EdnValue};

use crate::beat::BeatPhase;
use crate::performer::DancePose;

/// A bound Live2D avatar: the model asset plus rest-pose parameters and motions.
#[derive(Debug, Clone, PartialEq)]
pub struct Live2DBinding {
    pub model: String,
    pub home: [f32; 3],
    pub scale: f32,
    /// Enable the Cubism physics sim (hair / accessory swing) host-side.
    pub physics: bool,
    /// Cubism parameter id the mouth lipsync drives (default `ParamMouthOpenY`).
    pub lipsync: String,
    /// Named motions: either a `.motion3.json` file the host plays, or inline
    /// EDN parameter keyframes (the Live2D analogue of `:dance/clips`).
    pub motions: Vec<Live2DMotion>,
    /// The active inline motion (`:motion "name"`), layered over the beat-driven
    /// base params each frame (the Live2D analogue of `:dance/avatar :clip`).
    pub active_motion: Option<String>,
    /// Base/rest parameter values; the per-frame driver layers motion on top.
    pub params: BTreeMap<String, f32>,
}

/// A named Live2D motion. Carries a `.motion3.json` `file` and/or inline
/// parameter keyframes (`keys`) authored in EDN.
#[derive(Debug, Clone, PartialEq)]
pub struct Live2DMotion {
    pub name: String,
    pub file: Option<String>,
    pub looping: bool,
    /// Parameter keyframes, sorted by time.
    pub keys: Vec<MotionKey>,
}

/// One motion keyframe: a set of Cubism parameter values at `time` seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionKey {
    pub time: f32,
    pub params: BTreeMap<String, f32>,
}

impl Default for Live2DBinding {
    fn default() -> Self {
        Self {
            model: String::new(),
            home: [0.0, 0.0, 0.0],
            scale: 1.0,
            physics: true,
            lipsync: "ParamMouthOpenY".into(),
            motions: Vec::new(),
            active_motion: None,
            params: BTreeMap::new(),
        }
    }
}

impl Live2DBinding {
    /// Parse a `:dance/live2d` map.
    pub fn from_edn(m: &BTreeMap<EdnValue, EdnValue>) -> Live2DBinding {
        let params = mget(m, "params")
            .and_then(|v| v.as_map())
            .map(|pm| {
                pm.iter()
                    .filter_map(|(k, v)| ident(Some(k)).map(|name| (name, num(Some(v)))))
                    .collect()
            })
            .unwrap_or_default();
        let motions = mget(m, "motions")
            .and_then(|v| v.as_vector())
            .map(|ms| ms.iter().filter_map(|x| x.as_map()).filter_map(parse_motion).collect())
            .unwrap_or_default();
        Live2DBinding {
            model: mget(m, "model").and_then(|v| v.as_string()).unwrap_or("").to_string(),
            home: opt_vec3(mget(m, "home")).unwrap_or([0.0, 0.0, 0.0]),
            scale: mget(m, "scale").map(|v| num(Some(v))).filter(|s| *s > 0.0).unwrap_or(1.0),
            physics: mget(m, "physics").and_then(|v| v.as_bool()).unwrap_or(true),
            lipsync: ident(mget(m, "lipsync")).unwrap_or_else(|| "ParamMouthOpenY".into()),
            motions,
            active_motion: ident(mget(m, "motion")),
            params,
        }
    }

    /// Resolve the parameter values for this frame: the rest `params` with the
    /// standard Cubism parameters driven by the beat-synced pose. Deterministic.
    /// `voice_mouth` (the `:dance/avatar :voice` vowel weight, when authored)
    /// drives the mouth lipsync instead of the default beat-open — so one
    /// `:voice` timeline syncs the VRM *and* Live2D performer's mouth.
    pub fn drive(&self, pose: &DancePose, phase: &BeatPhase, voice_mouth: Option<f32>) -> BTreeMap<String, f32> {
        let mut p = self.params.clone();
        let set = |p: &mut BTreeMap<String, f32>, k: &str, v: f32| {
            p.insert(k.to_string(), v);
        };
        // head & body follow the dance pose (degrees, clamped to Cubism ranges).
        set(&mut p, "ParamAngleX", (pose.root_yaw.to_degrees()).clamp(-30.0, 30.0));
        set(&mut p, "ParamAngleZ", (pose.spine_sway.to_degrees()).clamp(-30.0, 30.0));
        set(&mut p, "ParamBodyAngleX", (pose.root_yaw * 0.5).to_degrees().clamp(-10.0, 10.0));
        set(&mut p, "ParamBodyAngleZ", (pose.spine_sway).to_degrees().clamp(-10.0, 10.0));
        // vertical bob → a gentle head tilt up/down.
        set(&mut p, "ParamAngleY", (pose.vertical_bob * 200.0).clamp(-30.0, 30.0));
        // breath: slow 0..1 sine, independent of beat.
        set(&mut p, "ParamBreath", 0.5 + 0.5 * (phase.time * 1.5).sin());
        // blink: open=1, quick close ~every 3s.
        let blink = blink_value(phase.time);
        set(&mut p, "ParamEyeLOpen", blink);
        set(&mut p, "ParamEyeROpen", blink);
        // mouth lipsync: the `:voice` vowel weight when authored, else beat-open.
        let mouth = voice_mouth
            .unwrap_or_else(|| (1.0 - (phase.beat_frac * std::f32::consts::TAU).cos()) * 0.5);
        set(&mut p, &self.lipsync, mouth.clamp(0.0, 1.0));
        // overlay the active inline motion (its params override the base ones).
        if let Some(name) = &self.active_motion {
            if let Some(mp) = self.sample_motion(name, phase.time) {
                for (k, v) in mp {
                    p.insert(k, v);
                }
            }
        }
        p
    }

    /// Sample a named inline motion's parameters at `time` seconds (linear
    /// interpolation; loops if the motion is `:loop true`). Returns `None` for an
    /// unknown motion or a file-only motion (no inline `:keys` — the host plays
    /// the `.motion3.json`). Layer the result over [`drive`] for a full pose.
    pub fn sample_motion(&self, name: &str, time: f32) -> Option<BTreeMap<String, f32>> {
        let motion = self.motions.iter().find(|m| m.name == name)?;
        let keys = &motion.keys;
        if keys.is_empty() {
            return None;
        }
        let duration = keys.last().map(|k| k.time).unwrap_or(0.0);
        let t = if motion.looping && duration > 0.0 {
            time - duration * (time / duration).floor()
        } else {
            time
        };
        if t <= keys[0].time {
            return Some(keys[0].params.clone());
        }
        if t >= duration {
            return Some(keys[keys.len() - 1].params.clone());
        }
        let mut i = 0;
        while i < keys.len() - 1 && keys[i + 1].time < t {
            i += 1;
        }
        let a = &keys[i];
        let b = &keys[i + 1];
        let f = if b.time > a.time { (t - a.time) / (b.time - a.time) } else { 0.0 };
        let mut out = BTreeMap::new();
        for k in a.params.keys().chain(b.params.keys()) {
            if out.contains_key(k) {
                continue;
            }
            let va = a.params.get(k).copied();
            let vb = b.params.get(k).copied();
            let v = match (va, vb) {
                (Some(va), Some(vb)) => va + (vb - va) * f,
                (Some(va), None) => va,
                (None, Some(vb)) => vb,
                (None, None) => continue,
            };
            out.insert(k.clone(), v);
        }
        Some(out)
    }

    /// Build the render-IR `:live2d` entry for the given driven parameters.
    pub fn render_entry(&self, driven: &BTreeMap<String, f32>) -> EdnValue {
        let params = EdnValue::map(
            driven
                .iter()
                .map(|(k, v)| (EdnValue::kw_bare(k.clone()), EdnValue::float(*v as f64))),
        );
        EdnValue::map([
            (EdnValue::kw_bare("kind"), EdnValue::kw_bare("live2d")),
            (EdnValue::kw_bare("model"), EdnValue::string(self.model.clone())),
            (
                EdnValue::kw_bare("pos"),
                EdnValue::vector(self.home.iter().map(|x| EdnValue::float(*x as f64))),
            ),
            (EdnValue::kw_bare("scale"), EdnValue::float(self.scale as f64)),
            (EdnValue::kw_bare("physics"), EdnValue::bool(self.physics)),
            (EdnValue::kw_bare("params"), params),
        ])
    }
}

/// Eye-open value in [0,1]: 1 (open) most of the time, a quick close ~every 3s.
fn blink_value(t: f32) -> f32 {
    let m = t - 3.0 * (t / 3.0).floor(); // t mod 3
    if m < 0.12 {
        (m / 0.06 - 1.0).abs().min(1.0) // 1 → 0 → 1 over 0..0.12s
    } else {
        1.0
    }
}

fn parse_motion(mm: &BTreeMap<EdnValue, EdnValue>) -> Option<Live2DMotion> {
    let name = mget(mm, "name").and_then(|v| v.as_string())?.to_string();
    let file = mget(mm, "file")
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let looping = mget(mm, "loop").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut keys: Vec<MotionKey> = mget(mm, "keys")
        .and_then(|v| v.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|k| k.as_map())
        .map(|km| {
            let params = mget(km, "params")
                .and_then(|v| v.as_map())
                .map(|pm| {
                    pm.iter()
                        .filter_map(|(k, v)| ident(Some(k)).map(|n| (n, num(Some(v)))))
                        .collect()
                })
                .unwrap_or_default();
            MotionKey { time: num(mget(km, "t")), params }
        })
        .collect();
    keys.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    Some(Live2DMotion { name, file, looping, keys })
}

fn ident(v: Option<&EdnValue>) -> Option<String> {
    v.and_then(|x| {
        x.as_keyword()
            .map(|k| k.0.name.clone())
            .or_else(|| x.as_string().map(|s| s.to_string()))
    })
}

fn opt_vec3(v: Option<&EdnValue>) -> Option<[f32; 3]> {
    v.and_then(|x| x.as_vector())
        .filter(|s| !s.is_empty())
        .map(|_| vec3(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kami_scene::root_map;

    fn binding(src: &str) -> Live2DBinding {
        let root = root_map(src).unwrap();
        let m = mget(&root, "dance/live2d").and_then(|v| v.as_map()).unwrap();
        Live2DBinding::from_edn(m)
    }

    #[test]
    fn parses_live2d_binding() {
        let b = binding(
            r#"{:dance/live2d
                {:model "models/haru.model3.json" :home [1.0 0.0 0.0] :scale 1.2
                 :physics true :lipsync :ParamMouthOpenY
                 :params {:ParamAngleX 0.0 :ParamEyeLOpen 1.0}
                 :motions [{:name "idle" :file "idle.motion3.json"}
                           {:name "wave" :file "wave.motion3.json"}]}}"#,
        );
        assert_eq!(b.model, "models/haru.model3.json");
        assert_eq!(b.home, [1.0, 0.0, 0.0]);
        assert!((b.scale - 1.2).abs() < 1e-6);
        assert_eq!(b.lipsync, "ParamMouthOpenY");
        assert_eq!(b.motions.len(), 2);
        assert_eq!(b.motions[1].name, "wave");
        assert_eq!(b.motions[1].file.as_deref(), Some("wave.motion3.json"));
        assert_eq!(b.params.get("ParamEyeLOpen"), Some(&1.0));
    }

    #[test]
    fn pose_drives_standard_params() {
        let b = binding(r#"{:dance/live2d {:model "m" :lipsync :ParamMouthOpenY}}"#);
        let pose = DancePose {
            root_yaw: 0.3,
            spine_sway: 0.1,
            vertical_bob: 0.05,
            arms_up: 0.5,
            root_translation: glam::Vec3::ZERO,
        };
        let phase = BeatPhase {
            time: 0.5,
            beat: 1,
            bar: 0,
            phrase: 0,
            beat_frac: 0.5,
            bar_frac: 0.25,
        };
        let p = b.drive(&pose, &phase, None);
        // head angle follows yaw (0.3 rad ≈ 17°).
        assert!((p["ParamAngleX"] - 0.3f32.to_degrees()).abs() < 1e-3);
        // mouth opens at mid-beat (beat_frac 0.5 → 1 - cos(pi) = 2 → *0.5 = 1).
        assert!((p["ParamMouthOpenY"] - 1.0).abs() < 1e-4);
        // breath + blink present and in range.
        assert!(p["ParamBreath"] >= 0.0 && p["ParamBreath"] <= 1.0);
        assert!(p["ParamEyeLOpen"] >= 0.0 && p["ParamEyeLOpen"] <= 1.0);
    }

    #[test]
    fn samples_inline_motion_keyframes() {
        let b = binding(
            r#"{:dance/live2d
                {:model "m"
                 :motions [{:name "wave" :loop true
                            :keys [{:t 0.0 :params {:ParamArmL 0.0 :ParamArmR 1.0}}
                                   {:t 2.0 :params {:ParamArmL 1.0 :ParamArmR 0.0}}]}
                           {:name "bow" :file "bow.motion3.json"}]}}"#,
        );
        // midpoint of the inline motion: linear interp.
        let p = b.sample_motion("wave", 1.0).expect("inline motion");
        assert!((p["ParamArmL"] - 0.5).abs() < 1e-5);
        assert!((p["ParamArmR"] - 0.5).abs() < 1e-5);
        // looping wraps: t=3.0 → t=1.0 within the 2s motion.
        let pl = b.sample_motion("wave", 3.0).unwrap();
        assert!((pl["ParamArmL"] - 0.5).abs() < 1e-5);
        // file-only motion has no inline keys → None (host plays the file).
        assert!(b.sample_motion("bow", 0.5).is_none());
        assert!(b.sample_motion("nope", 0.0).is_none());
    }

    #[test]
    fn active_motion_overlays_driven_params() {
        let b = binding(
            r#"{:dance/live2d
                {:model "m" :motion "wave"
                 :motions [{:name "wave"
                            :keys [{:t 0.0 :params {:ParamArmL 0.0}}
                                   {:t 2.0 :params {:ParamArmL 1.0}}]}]}}"#,
        );
        assert_eq!(b.active_motion.as_deref(), Some("wave"));
        let phase = BeatPhase { time: 1.0, beat: 2, bar: 0, phrase: 0, beat_frac: 0.5, bar_frac: 0.25 };
        let p = b.drive(&DancePose::rest(), &phase, None);
        // motion param present (overlaid) at its midpoint value.
        assert!((p["ParamArmL"] - 0.5).abs() < 1e-4, "motion overlaid: {:?}", p.get("ParamArmL"));
        // beat-driven base params still present alongside.
        assert!(p.contains_key("ParamBreath") && p.contains_key("ParamEyeLOpen"));
    }

    #[test]
    fn drive_is_deterministic() {
        let b = binding(r#"{:dance/live2d {:model "m"}}"#);
        let pose = DancePose::rest();
        let phase = BeatPhase { time: 1.234, beat: 2, bar: 0, phrase: 0, beat_frac: 0.3, bar_frac: 0.1 };
        assert_eq!(b.drive(&pose, &phase, None), b.drive(&pose, &phase, None));
    }

    #[test]
    fn voice_drives_live2d_mouth() {
        // the `:dance/avatar :voice` vowel weight drives the Live2D mouth, not the
        // beat — parity with the VRM mouth. At beat_frac 0 the beat lipsync is 0,
        // so a non-zero result proves the voice override.
        let b = binding(r#"{:dance/live2d {:model "m" :lipsync :ParamMouthOpenY}}"#);
        let phase = BeatPhase { time: 0.0, beat: 0, bar: 0, phrase: 0, beat_frac: 0.0, bar_frac: 0.0 };
        let p = b.drive(&DancePose::rest(), &phase, Some(0.8));
        assert!((p["ParamMouthOpenY"] - 0.8).abs() < 1e-4, "voice vowel weight drives the mouth");
    }

    #[test]
    fn render_entry_is_well_formed() {
        let b = binding(r#"{:dance/live2d {:model "haru.model3.json" :home [0 0 0]}}"#);
        let driven = b.drive(&DancePose::rest(), &BeatPhase {
            time: 0.0, beat: 0, bar: 0, phrase: 0, beat_frac: 0.0, bar_frac: 0.0,
        }, None);
        let entry = b.render_entry(&driven);
        let m = entry.as_map().expect("entry is a map");
        assert_eq!(mget(m, "kind").and_then(|v| v.as_keyword()).map(|k| k.0.name.as_str()), Some("live2d"));
        assert_eq!(mget(m, "model").and_then(|v| v.as_string()), Some("haru.model3.json"));
        assert!(mget(m, "params").and_then(|v| v.as_map()).is_some());
    }

    #[test]
    fn blink_closes_then_opens() {
        assert!((blink_value(0.0) - 1.0).abs() < 1e-6, "open at cycle start");
        assert!(blink_value(0.06) < 0.05, "closed mid-blink");
        assert!((blink_value(1.5) - 1.0).abs() < 1e-6, "open between blinks");
    }
}
