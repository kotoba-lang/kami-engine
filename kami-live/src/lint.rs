//! Lint — validate a `:dance/*` scene before it silently falls back.
//!
//! [`DanceScene::from_edn`](crate::scene::DanceScene::from_edn) is deliberately
//! tolerant: an unknown `:stage`, a mistyped `:dance` preset, or an out-of-range
//! intensity all resolve to a default rather than panic. That is right for
//! runtime, but it hides authoring mistakes — `:stage :halll` just becomes
//! `:hall`. This module re-reads the raw EDN and reports those silent
//! corrections so a `clojure -M author.clj` (or an editor) can catch them.
//!
//! ```ignore
//! for l in kami_live::lint::lint_scene(&edn) {
//!     eprintln!("{}: {} — {}", l.severity, l.path, l.message);
//! }
//! ```

use kami_scene::{mget, root_map, EdnValue};
use std::collections::BTreeSet;

/// How serious a finding is. `Error` = the scene won't load as authored;
/// `Warn` = it loads but a value was silently corrected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Severity::Warn => "warn",
            Severity::Error => "error",
        })
    }
}

/// One lint finding: where in the scene, and what's wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub severity: Severity,
    /// Dotted path into the scene, e.g. `dance/setlist[2].dance`.
    pub path: String,
    pub message: String,
}

const STAGES: &[&str] = &["club", "hall", "festival"];
const DANCES: &[&str] = &["idle", "four-on-floor", "wota", "kpop-point", "shuffle", "hold"];
const CUE_KINDS: &[&str] = &["drop", "breakdown", "callout", "custom"];
const TRIGGER_ON: &[&str] = &[
    "drop", "breakdown", "callout", "custom", "beat", "bar", "phrase", "track",
];
const FIXTURES: &[&str] = &["front-par", "back-par", "spot", "blinder", "laser", "strobe"];
const ENVELOPES: &[&str] = &["hold", "breathe", "ramp", "pulse", "strobe"];
const VJ_PATTERNS: &[&str] = &["solid", "stripes", "pulse", "rings", "scope", "noise"];
const PALETTES: &[&str] = &["neon-pink", "cool-wave", "sunset", "monochrome"];
// Canonical effect ids (kami-postfx-scene) + tolerated short aliases + fxaa.
const POST_FX: &[&str] = &[
    "bloom", "outline", "vignette", "crt", "color-grade", "pixelate", "ssao",
    "depth-of-field", "dof", "ssr", "aces-tonemap", "aces", "film-grain",
    "chromatic-aberration", "chromatic", "god-rays", "fxaa",
];

/// Local keyword/string name (namespace dropped), if `v` is one.
fn name(v: Option<&EdnValue>) -> Option<String> {
    let v = v?;
    v.as_keyword()
        .map(|k| k.0.name.clone())
        .or_else(|| v.as_string().map(|s| s.to_string()))
}

fn num(v: Option<&EdnValue>) -> Option<f32> {
    v.and_then(|x| x.as_float().map(|f| f as f32).or_else(|| x.as_integer().map(|i| i as f32)))
}

/// Validate a dance scene's EDN. Returns findings in document order; an empty
/// vec means the scene is clean (no silent corrections, no structural errors).
pub fn lint_scene(src: &str) -> Vec<Lint> {
    let mut out = Vec::new();
    let root = match root_map(src) {
        Some(m) => m,
        None => {
            out.push(Lint {
                severity: Severity::Error,
                path: "<root>".into(),
                message: "top-level form is not an EDN map".into(),
            });
            return out;
        }
    };

    let warn = |out: &mut Vec<Lint>, path: &str, msg: String| {
        out.push(Lint { severity: Severity::Warn, path: path.into(), message: msg });
    };
    let enum_check =
        |out: &mut Vec<Lint>, path: &str, val: Option<&EdnValue>, known: &[&str], what: &str| {
            if let Some(n) = name(val) {
                if !known.contains(&n.as_str()) {
                    warn(
                        out,
                        path,
                        format!("unknown {what} `{n}` — falls back to a default; expected one of {known:?}"),
                    );
                }
            }
        };
    let range_check = |out: &mut Vec<Lint>, path: &str, val: Option<&EdnValue>, lo: f32, hi: f32| {
        if let Some(x) = num(val) {
            if x < lo || x > hi {
                warn(out, path, format!("value {x} out of range [{lo}, {hi}] — clamped"));
            }
        }
    };

    // ── :dance/show ─────────────────────────────────────────────────────────
    if let Some(show) = mget(&root, "dance/show").and_then(|v| v.as_map()) {
        enum_check(&mut out, "dance/show.stage", mget(show, "stage"), STAGES, "stage");
        if let Some(b) = num(mget(show, "bpm")) {
            if b <= 0.0 {
                warn(&mut out, "dance/show.bpm", format!("bpm {b} must be > 0 — defaults to 128"));
            }
        }
        range_check(&mut out, "dance/show.swing", mget(show, "swing"), -0.5, 0.5);
    }

    // ── :dance/setlist + collect cue tags ───────────────────────────────────
    let mut cue_tags: BTreeSet<String> = BTreeSet::new();
    let tracks = mget(&root, "dance/setlist").and_then(|v| v.as_vector());
    match tracks {
        None | Some([]) => warn(
            &mut out,
            "dance/setlist",
            "empty or missing setlist — the show has no tracks to play".into(),
        ),
        Some(ts) => {
            for (i, t) in ts.iter().enumerate() {
                let Some(tm) = t.as_map() else { continue };
                let p = format!("dance/setlist[{i}]");
                enum_check(&mut out, &format!("{p}.dance"), mget(tm, "dance"), DANCES, "dance preset");
                if let Some(cues) = mget(tm, "cues").and_then(|v| v.as_vector()) {
                    for (j, c) in cues.iter().enumerate() {
                        let Some(cm) = c.as_map() else { continue };
                        enum_check(
                            &mut out,
                            &format!("{p}.cues[{j}].kind"),
                            mget(cm, "kind"),
                            CUE_KINDS,
                            "cue kind",
                        );
                        // beat-0 cues never fire: cue dispatch is open-closed
                        // `(prev, beat]` and `prev` starts at 0 (see
                        // setlist::cues_between). Use :beat ≥ 1 for reactions.
                        if num(mget(cm, "beat")).map_or(false, |b| b <= 0.0) {
                            warn(
                                &mut out,
                                &format!("{p}.cues[{j}].beat"),
                                "cue at :beat 0 never fires (dispatch is open-closed (prev, beat]); use :beat ≥ 1".into(),
                            );
                        }
                        if let Some(tag) = mget(cm, "tag").and_then(|v| v.as_string()) {
                            cue_tags.insert(tag.to_string());
                        }
                    }
                }
            }
        }
    }

    // ── :dance/lighting ─────────────────────────────────────────────────────
    if let Some(cues) = mget(&root, "dance/lighting").and_then(|v| v.as_vector()) {
        for (i, c) in cues.iter().enumerate() {
            let Some(cm) = c.as_map() else { continue };
            let p = format!("dance/lighting[{i}]");
            enum_check(&mut out, &format!("{p}.fixture"), mget(cm, "fixture"), FIXTURES, "fixture");
            range_check(&mut out, &format!("{p}.intensity"), mget(cm, "intensity"), 0.0, 1.0);
            // envelope may be a bare keyword; map forms ({:pulse d}) are fine.
            if let Some(n) = name(mget(cm, "envelope")) {
                if !ENVELOPES.contains(&n.as_str()) {
                    warn(&mut out, &format!("{p}.envelope"), format!("unknown envelope `{n}` — defaults to :hold"));
                }
            }
        }
    }

    // ── :dance/vj ───────────────────────────────────────────────────────────
    if let Some(steps) = mget(&root, "dance/vj").and_then(|v| v.as_vector()) {
        for (i, s) in steps.iter().enumerate() {
            let Some(sm) = s.as_map() else { continue };
            let p = format!("dance/vj[{i}]");
            enum_check(&mut out, &format!("{p}.pattern"), mget(sm, "pattern"), VJ_PATTERNS, "pattern");
            // palette: named const is checked; an inline map is fine.
            if let Some(n) = name(mget(sm, "palette")) {
                if !PALETTES.contains(&n.as_str()) {
                    warn(&mut out, &format!("{p}.palette"), format!("unknown palette `{n}` — defaults to :cool-wave"));
                }
            }
        }
    }

    // ── :dance/triggers (+ dangling-tag check) ──────────────────────────────
    if let Some(trigs) = mget(&root, "dance/triggers").and_then(|v| v.as_vector()) {
        for (i, t) in trigs.iter().enumerate() {
            let Some(tm) = t.as_map() else { continue };
            let p = format!("dance/triggers[{i}]");
            match name(mget(tm, "on")) {
                None => warn(&mut out, &format!("{p}.on"), "trigger missing `:on` — it will never fire".into()),
                Some(n) if !TRIGGER_ON.contains(&n.as_str()) => warn(
                    &mut out,
                    &format!("{p}.on"),
                    format!("unknown trigger `:on {n}` — it will never fire; expected one of {TRIGGER_ON:?}"),
                ),
                _ => {}
            }
            if let Some(tag) = mget(tm, "tag").and_then(|v| v.as_string()) {
                if !cue_tags.contains(tag) {
                    warn(
                        &mut out,
                        &format!("{p}.tag"),
                        format!("dangling `:tag {tag:?}` — no cue in the setlist carries it, so this trigger never fires"),
                    );
                }
            }
        }
    }

    // ── :dance/clips → collect names + validate each ────────────────────────
    let mut clip_names: BTreeSet<String> = BTreeSet::new();
    if let Some(clips) = mget(&root, "dance/clips").and_then(|v| v.as_vector()) {
        for (i, c) in clips.iter().enumerate() {
            let Some(cm) = c.as_map() else { continue };
            match mget(cm, "name").and_then(|v| v.as_string()) {
                Some(n) if !n.is_empty() => {
                    clip_names.insert(n.to_string());
                }
                _ => warn(&mut out, &format!("dance/clips[{i}].name"), "clip has no `:name`".into()),
            }
            if mget(cm, "tracks").and_then(|v| v.as_vector()).map_or(true, |t| t.is_empty()) {
                warn(&mut out, &format!("dance/clips[{i}].tracks"), "clip has no `:tracks` — nothing to play".into());
            }
        }
    }

    // ── :dance/post → post-fx chain ─────────────────────────────────────────
    if let Some(chain) = mget(&root, "dance/post").and_then(|v| v.as_vector()) {
        for (i, e) in chain.iter().enumerate() {
            let Some(em) = e.as_map() else { continue };
            // tag is `:effect` (canonical, kami-postfx-scene) or `:fx` (alias).
            match name(mget(em, "effect")).or_else(|| name(mget(em, "fx"))) {
                None => warn(&mut out, &format!("dance/post[{i}].effect"), "post effect missing `:effect`".into()),
                Some(n) if !POST_FX.contains(&n.as_str()) => warn(
                    &mut out,
                    &format!("dance/post[{i}].effect"),
                    format!("unknown post effect `{n}` — expected one of {POST_FX:?}"),
                ),
                _ => {}
            }
        }
    }

    // ── :dance/avatar (VRM) ─────────────────────────────────────────────────
    if let Some(av) = mget(&root, "dance/avatar").and_then(|v| v.as_map()) {
        if mget(av, "vrm").and_then(|v| v.as_string()).unwrap_or("").is_empty() {
            warn(&mut out, "dance/avatar.vrm", "no `:vrm` model bound — the avatar won't load".into());
        }
        if num(mget(av, "scale")).map_or(false, |s| s <= 0.0) {
            warn(&mut out, "dance/avatar.scale", "scale must be > 0".into());
        }
        // dangling clip reference: avatar :clip names a clip not in :dance/clips.
        if let Some(clip) = name(mget(av, "clip")) {
            if !clip_names.contains(&clip) {
                warn(
                    &mut out,
                    "dance/avatar.clip",
                    format!("references clip `{clip}` not defined in :dance/clips"),
                );
            }
        }
    }

    // ── :dance/live2d ───────────────────────────────────────────────────────
    if let Some(l2) = mget(&root, "dance/live2d").and_then(|v| v.as_map()) {
        if mget(l2, "model").and_then(|v| v.as_string()).unwrap_or("").is_empty() {
            warn(&mut out, "dance/live2d.model", "no `:model` bound — the Live2D avatar won't load".into());
        }
        if num(mget(l2, "scale")).map_or(false, |s| s <= 0.0) {
            warn(&mut out, "dance/live2d.scale", "scale must be > 0".into());
        }
        let mut motion_names: BTreeSet<String> = BTreeSet::new();
        if let Some(motions) = mget(l2, "motions").and_then(|v| v.as_vector()) {
            for (i, m) in motions.iter().enumerate() {
                let Some(mm) = m.as_map() else { continue };
                if let Some(n) = mget(mm, "name").and_then(|v| v.as_string()) {
                    motion_names.insert(n.to_string());
                }
                // a motion needs either a :file or inline :keys to play.
                let has_file = !mget(mm, "file").and_then(|v| v.as_string()).unwrap_or("").is_empty();
                let has_keys = mget(mm, "keys").and_then(|v| v.as_vector()).map_or(false, |k| !k.is_empty());
                if !has_file && !has_keys {
                    warn(
                        &mut out,
                        &format!("dance/live2d.motions[{i}]"),
                        "motion has neither `:file` nor inline `:keys` — nothing to play".into(),
                    );
                }
            }
        }
        // dangling active-motion reference.
        if let Some(motion) = name(mget(l2, "motion")) {
            if !motion_names.contains(&motion) {
                warn(
                    &mut out,
                    "dance/live2d.motion",
                    format!("references motion `{motion}` not defined in :motions"),
                );
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_scene_has_no_lints() {
        let src = r#"
        {:dance/show     {:bpm 128.0 :stage :hall :swing 0.1}
         :dance/lighting [{:fixture :front-par :intensity 0.8 :envelope :hold}]
         :dance/vj       [{:pattern :stripes :palette :cool-wave}]
         :dance/triggers [{:on :drop :fx :confetti}]
         :dance/setlist  [{:title "A" :bars 8 :dance :wota
                           :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        assert!(lint_scene(src).is_empty(), "{:?}", lint_scene(src));
    }

    #[test]
    fn catches_unknown_enums() {
        let src = r#"
        {:dance/show    {:bpm 120.0 :stage :halll}
         :dance/setlist [{:title "A" :bars 8 :dance :wotaa
                          :cues [{:beat 0 :kind :drrop :tag "x"}]}]}
        "#;
        let lints = lint_scene(src);
        let paths: Vec<&str> = lints.iter().map(|l| l.path.as_str()).collect();
        assert!(paths.contains(&"dance/show.stage"), "{lints:?}");
        assert!(paths.contains(&"dance/setlist[0].dance"));
        assert!(paths.contains(&"dance/setlist[0].cues[0].kind"));
    }

    #[test]
    fn catches_range_and_empty_and_bpm() {
        let src = r#"
        {:dance/show    {:bpm 0 :stage :hall :swing 1.5}
         :dance/lighting [{:fixture :spot :intensity 2.0}]
         :dance/setlist []}
        "#;
        let lints = lint_scene(src);
        assert!(lints.iter().any(|l| l.path == "dance/show.bpm"));
        assert!(lints.iter().any(|l| l.path == "dance/show.swing"));
        assert!(lints.iter().any(|l| l.path == "dance/lighting[0].intensity"));
        assert!(lints.iter().any(|l| l.path == "dance/setlist"));
    }

    #[test]
    fn catches_dangling_trigger_tag_and_bad_on() {
        let src = r#"
        {:dance/show     {:bpm 120.0 :stage :hall}
         :dance/triggers [{:on :drop :tag "nope" :fx :x}
                          {:on :wiggle :fx :y}]
         :dance/setlist  [{:title "A" :bars 8 :dance :wota
                           :cues [{:beat 0 :kind :drop :tag "real"}]}]}
        "#;
        let lints = lint_scene(src);
        assert!(lints.iter().any(|l| l.path == "dance/triggers[0].tag"), "dangling tag");
        assert!(lints.iter().any(|l| l.path == "dance/triggers[1].on"), "unknown :on");
    }

    #[test]
    fn catches_unknown_post_fx() {
        let src = r#"
        {:dance/show   {:bpm 120.0 :stage :hall}
         :dance/post   [{:fx :bloom :intensity 0.5} {:fx :sparkle-blast} {:intensity 1.0}]
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        let lints = lint_scene(src);
        assert!(lints.iter().any(|l| l.path == "dance/post[1].effect"), "unknown fx: {lints:?}");
        assert!(lints.iter().any(|l| l.path == "dance/post[2].effect"), "missing fx");
        // the valid bloom entry produces no lint.
        assert!(!lints.iter().any(|l| l.path == "dance/post[0].effect"));
    }

    #[test]
    fn catches_beat_zero_cue() {
        let src = r#"
        {:dance/show    {:bpm 120.0 :stage :hall}
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 0 :kind :drop :tag "hook"}]}]}
        "#;
        let lints = lint_scene(src);
        assert!(
            lints.iter().any(|l| l.path == "dance/setlist[0].cues[0].beat"),
            "beat-0 cue flagged: {lints:?}"
        );
    }

    #[test]
    fn catches_unbound_avatar_and_live2d() {
        let src = r#"
        {:dance/show   {:bpm 120.0 :stage :hall}
         :dance/avatar {:scale -1.0}
         :dance/live2d {:lipsync :ParamMouthOpenY
                        :motions [{:name "idle"}]}
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        let lints = lint_scene(src);
        let paths: Vec<&str> = lints.iter().map(|l| l.path.as_str()).collect();
        assert!(paths.contains(&"dance/avatar.vrm"), "missing vrm: {lints:?}");
        assert!(paths.contains(&"dance/avatar.scale"), "bad scale");
        assert!(paths.contains(&"dance/live2d.model"), "missing model");
        assert!(paths.contains(&"dance/live2d.motions[0]"), "motion without file or keys");
    }

    #[test]
    fn catches_dangling_live2d_motion_ref() {
        let src = r#"
        {:dance/show   {:bpm 120.0 :stage :hall}
         :dance/live2d {:model "m" :motion "missing"
                        :motions [{:name "wave" :keys [{:t 0.0 :params {:ParamArmL 0.0}}]}]}
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        assert!(lint_scene(src).iter().any(|l| l.path == "dance/live2d.motion"), "dangling motion ref");
    }

    #[test]
    fn catches_bad_clips_and_dangling_clip_ref() {
        let src = r#"
        {:dance/show   {:bpm 120.0 :stage :hall}
         :dance/avatar {:vrm "m.vrm" :clip "missing"}
         :dance/clips  [{:name "wave" :tracks [{:bone "hips" :keys [{:t 0.0 :pos [0 0 0]}]}]}
                        {:tracks []}]
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        let lints = lint_scene(src);
        let paths: Vec<&str> = lints.iter().map(|l| l.path.as_str()).collect();
        assert!(paths.contains(&"dance/clips[1].name"), "clip without name: {lints:?}");
        assert!(paths.contains(&"dance/clips[1].tracks"), "clip without tracks");
        assert!(paths.contains(&"dance/avatar.clip"), "dangling clip ref");
    }

    #[test]
    fn valid_clip_and_ref_are_clean() {
        let src = r#"
        {:dance/show   {:bpm 120.0 :stage :hall}
         :dance/avatar {:vrm "m.vrm" :clip "wave"}
         :dance/clips  [{:name "wave" :tracks [{:bone "hips" :keys [{:t 0.0 :pos [0 0 0]}]}]}]
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        assert!(lint_scene(src).is_empty(), "{:?}", lint_scene(src));
    }

    #[test]
    fn bound_avatar_and_live2d_are_clean() {
        let src = r#"
        {:dance/show   {:bpm 120.0 :stage :hall}
         :dance/avatar {:vrm "m.vrm" :scale 1.0}
         :dance/live2d {:model "haru.model3.json" :motions [{:name "idle" :file "idle.motion3.json"}]}
         :dance/setlist [{:title "A" :bars 8 :dance :wota
                          :cues [{:beat 1 :kind :drop :tag "hook"}]}]}
        "#;
        assert!(lint_scene(src).is_empty(), "{:?}", lint_scene(src));
    }

    #[test]
    fn non_map_root_is_an_error() {
        let lints = lint_scene("[1 2 3]");
        assert_eq!(lints.len(), 1);
        assert_eq!(lints[0].severity, Severity::Error);
    }

    /// The committed reference scene must stay lint-clean — no silent
    /// defaults, no dangling tags. Guards authors against typos in the example.
    #[test]
    fn authored_example_is_lint_clean() {
        const EXAMPLE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
        let lints = lint_scene(EXAMPLE);
        assert!(lints.is_empty(), "example scene has lints: {lints:?}");
    }
}
