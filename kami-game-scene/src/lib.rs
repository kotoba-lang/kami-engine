//! kami-game-scene — EDN authoring surface for `kami-game` ANIMATION-STATE presets
//! (Nintendo-style juicy motion).
//!
//! The data-tier counterpart of `kami-postfx-scene` / `kami-character-scene` (and the
//! `kami-vehicle-scene` family) for the animation system: it turns canonical
//! `:game/animations` EDN (an ordered vector of `:clip`-tagged clip maps per preset)
//! into the real [`kami_game::animation::AnimationState`] engine struct, rebuilt the
//! same way the hardcoded preset factories assemble it
//! ([`AnimationState::new`](kami_game::animation::AnimationState::new) +
//! [`with`](kami_game::animation::AnimationState::with) in order). It re-uses the
//! tolerant `kami-scene` accessors the same way games parse `scene.edn` (missing keys
//! fall back to defaults, namespaced keywords match on `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038 / ADR-0046)
//!
//! The hot per-frame integrator (`AnimationClip::tick`, the sine/elastic easing math)
//! stays native Rust (`kami-game::animation`). An animation preset is **init-time
//! CONFIG** — the clip list, their params, and their *initial* runtime state
//! (`timer: 0.0`, `phase: Wait`, `angle: 0.0`, …) read once when the `AnimationState`
//! is constructed — so it is safe to move to EDN. `kami-game` itself stays untouched;
//! the EDN dependency lives only here. The compiled-in
//! `AnimationState::{skibidi_idle,grimace_wobble,item_pickup,sigma_idle,ohio_glitch}()`
//! factories remain as the [`builtin_animation`] fallback and are parity-tested against
//! the shipped EDN ([`ANIMATIONS_EDN`]).
//!
//! ## EDN shape (see `data/animations.edn`)
//!
//! ```edn
//! {:game/animations
//!  {:skibidi-idle [{:clip :head-bob :rise-height 2.0 :rise-time 1.0 :hold-time 0.5
//!                   :drop-time 0.5 :wait-time 2.0 :timer 0.0 :phase :wait}
//!                  {:clip :spinning :speed 3.0 :angle 0.0}]
//!   :grimace-wobble [...] :item-pickup [...] :sigma-idle [] :ohio-glitch [...]}}
//! ```
//!
//! Each preset is an **ordered vector** (clip order is load-bearing — the outputs
//! combine front-to-back via `AnimationOutput::combine`). Each clip map is tagged by
//! `:clip` (a keyword id naming the [`AnimationClip`](kami_game::animation::AnimationClip)
//! variant); the rest of the keys are that variant's fields, hyphenated. The HeadBob
//! `:phase` field is itself a keyword id for the
//! [`HeadBobPhase`](kami_game::animation::HeadBobPhase) sub-enum (`:rise` / `:hold` /
//! `:drop` / `:wait`). Unknown `:clip` ids are an [`Error::UnknownClip`].

use std::collections::BTreeMap;

use kami_game::animation::{AnimationClip, AnimationState, HeadBobPhase};
use kami_scene::{mget, num, root_map, EdnValue};

/// The canonical animation-preset CONFIG shipped with this crate (the preset table).
/// This is the source of truth; the compiled-in preset factories are the parity-tested
/// mirror.
pub const ANIMATIONS_EDN: &str = include_str!("../data/animations.edn");

/// Names of the presets shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Keeping this list here (not in `kami-game`) keeps the engine crate
/// untouched. Order mirrors the preset-factory declaration order in `animation.rs`.
pub const ALL_ANIMATION_NAMES: [&str; 5] = [
    "skibidi-idle",
    "grimace-wobble",
    "item-pickup",
    "sigma-idle",
    "ohio-glitch",
];

/// Errors raised while loading animation-preset CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("animations EDN root is not a map")]
    NotAMap,
    /// The `:game/animations` table was missing or not a map.
    #[error("`:game/animations` missing or not a map")]
    NoTable,
    /// The requested preset id was missing under `:game/animations`.
    #[error("animation `{0}` not found under `:game/animations`")]
    AnimationNotFound(String),
    /// A clip map carried a `:clip` id with no matching [`AnimationClip`] variant (or no
    /// `:clip` key at all).
    #[error("unknown animation clip `{0}`")]
    UnknownClip(String),
}

// ── small typed accessors over the tolerant kami-scene helpers ──
//
// kami-scene ships `num` (f32); animation clips also need `u32` (Glitch seed) and `bool`
// (SquashStretch active) and the fixed `[f32; 3]` scale vectors. These mirror the rest of
// the data tier's "absent / malformed degrades to a default" tolerance.

/// Read an `f32` field; `0.0` when absent / non-numeric (via `num`).
fn f32_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> f32 {
    num(mget(m, key))
}

/// Read a `u32` field, coercing via `num` then rounding; `0` when absent / non-numeric.
fn u32_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> u32 {
    match mget(m, key) {
        Some(v) => num(Some(v)).round() as u32,
        None => 0,
    }
}

/// Read a `bool` field; `false` when absent / non-boolean.
fn bool_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> bool {
    mget(m, key).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Read a 3-vector `[x y z]` field; missing components default to `0.0`, non-vector →
/// zeros.
fn vec3_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> [f32; 3] {
    let s = mget(m, key).and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(0.0);
    [g(0), g(1), g(2)]
}

/// Map a HeadBob `:phase` keyword id → the [`HeadBobPhase`] sub-enum variant. Unknown /
/// absent ids fall back to `Wait` (the hardcoded `skibidi_idle` initial state), so a
/// missing phase degrades the same tolerant way as the rest of the data tier.
fn head_bob_phase_from_id(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> HeadBobPhase {
    match mget(m, key).and_then(kami_scene::kw_key).as_deref() {
        Some("rise") => HeadBobPhase::Rise,
        Some("hold") => HeadBobPhase::Hold,
        Some("drop") => HeadBobPhase::Drop,
        _ => HeadBobPhase::Wait,
    }
}

/// The hyphenated `:phase` keyword id for a [`HeadBobPhase`] variant (inverse of
/// [`head_bob_phase_from_id`]; also the source of truth for [`PhaseSpec`]).
pub fn head_bob_phase_id(p: &HeadBobPhase) -> &'static str {
    match p {
        HeadBobPhase::Rise => "rise",
        HeadBobPhase::Hold => "hold",
        HeadBobPhase::Drop => "drop",
        HeadBobPhase::Wait => "wait",
    }
}

/// The hyphenated `:clip` keyword id for an [`AnimationClip`] variant. Inverse of the
/// match in [`clip_from_map`]; also the source of truth for [`ClipSpec::clip_id`].
pub fn clip_id(c: &AnimationClip) -> &'static str {
    match c {
        AnimationClip::Bobbing { .. } => "bobbing",
        AnimationClip::Spinning { .. } => "spinning",
        AnimationClip::SquashStretch { .. } => "squash-stretch",
        AnimationClip::Wobble { .. } => "wobble",
        AnimationClip::PopIn { .. } => "pop-in",
        AnimationClip::HeadBob { .. } => "head-bob",
        AnimationClip::PulseGlow { .. } => "pulse-glow",
        AnimationClip::Glitch { .. } => "glitch",
    }
}

/// Build one real [`AnimationClip`] from a clip map tagged by `:clip`.
///
/// Every field is read with the tolerant accessors, so a key a map omits degrades to the
/// same zero/default the rest of the data tier uses. A `:clip` id with no matching variant
/// (or a missing `:clip`) is [`Error::UnknownClip`].
pub fn clip_from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Result<AnimationClip, Error> {
    let id = mget(m, "clip")
        .and_then(kami_scene::kw_key)
        .ok_or_else(|| Error::UnknownClip("<missing>".to_string()))?;

    let c = match id.as_str() {
        "bobbing" => AnimationClip::Bobbing {
            amplitude: f32_of(m, "amplitude"),
            frequency: f32_of(m, "frequency"),
            phase: f32_of(m, "phase"),
        },
        "spinning" => AnimationClip::Spinning {
            speed: f32_of(m, "speed"),
            angle: f32_of(m, "angle"),
        },
        "squash-stretch" => AnimationClip::SquashStretch {
            squash_scale: vec3_of(m, "squash-scale"),
            stretch_scale: vec3_of(m, "stretch-scale"),
            duration: f32_of(m, "duration"),
            timer: f32_of(m, "timer"),
            active: bool_of(m, "active"),
        },
        "wobble" => AnimationClip::Wobble {
            intensity: f32_of(m, "intensity"),
            speed: f32_of(m, "speed"),
            phase: f32_of(m, "phase"),
        },
        "pop-in" => AnimationClip::PopIn {
            target_scale: vec3_of(m, "target-scale"),
            duration: f32_of(m, "duration"),
            timer: f32_of(m, "timer"),
            overshoot: f32_of(m, "overshoot"),
        },
        "head-bob" => AnimationClip::HeadBob {
            rise_height: f32_of(m, "rise-height"),
            rise_time: f32_of(m, "rise-time"),
            hold_time: f32_of(m, "hold-time"),
            drop_time: f32_of(m, "drop-time"),
            wait_time: f32_of(m, "wait-time"),
            timer: f32_of(m, "timer"),
            phase: head_bob_phase_from_id(m, "phase"),
        },
        "pulse-glow" => AnimationClip::PulseGlow {
            min_scale: f32_of(m, "min-scale"),
            max_scale: f32_of(m, "max-scale"),
            speed: f32_of(m, "speed"),
            phase: f32_of(m, "phase"),
        },
        "glitch" => AnimationClip::Glitch {
            interval: f32_of(m, "interval"),
            timer: f32_of(m, "timer"),
            intensity: f32_of(m, "intensity"),
            seed: u32_of(m, "seed"),
        },
        other => return Err(Error::UnknownClip(other.to_string())),
    };
    Ok(c)
}

/// A structurally-comparable mirror of [`HeadBobPhase`] (which derives no `PartialEq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseSpec {
    Rise,
    Hold,
    Drop,
    Wait,
}

impl PhaseSpec {
    /// Project a real [`HeadBobPhase`] into the comparable [`PhaseSpec`].
    pub fn from_phase(p: &HeadBobPhase) -> Self {
        match p {
            HeadBobPhase::Rise => PhaseSpec::Rise,
            HeadBobPhase::Hold => PhaseSpec::Hold,
            HeadBobPhase::Drop => PhaseSpec::Drop,
            HeadBobPhase::Wait => PhaseSpec::Wait,
        }
    }

    /// Reconstruct the real engine [`HeadBobPhase`] from this spec.
    pub fn to_phase(self) -> HeadBobPhase {
        match self {
            PhaseSpec::Rise => HeadBobPhase::Rise,
            PhaseSpec::Hold => HeadBobPhase::Hold,
            PhaseSpec::Drop => HeadBobPhase::Drop,
            PhaseSpec::Wait => HeadBobPhase::Wait,
        }
    }
}

/// A structurally-comparable mirror of [`AnimationClip`].
///
/// `kami_game::animation::AnimationClip` derives only `Debug, Clone` (plus serde — no
/// `PartialEq`), so the data tier cannot assert `loaded == skibidi_idle()` directly
/// without touching the engine crate. `ClipSpec` is a `PartialEq` projection of each
/// variant + its fields (with [`PhaseSpec`] standing in for the `HeadBobPhase` sub-enum),
/// so parity (every field, in order) is asserted here additively, with `kami-game` left
/// untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipSpec {
    Bobbing { amplitude: f32, frequency: f32, phase: f32 },
    Spinning { speed: f32, angle: f32 },
    SquashStretch {
        squash_scale: [f32; 3],
        stretch_scale: [f32; 3],
        duration: f32,
        timer: f32,
        active: bool,
    },
    Wobble { intensity: f32, speed: f32, phase: f32 },
    PopIn { target_scale: [f32; 3], duration: f32, timer: f32, overshoot: f32 },
    HeadBob {
        rise_height: f32,
        rise_time: f32,
        hold_time: f32,
        drop_time: f32,
        wait_time: f32,
        timer: f32,
        phase: PhaseSpec,
    },
    PulseGlow { min_scale: f32, max_scale: f32, speed: f32, phase: f32 },
    Glitch { interval: f32, timer: f32, intensity: f32, seed: u32 },
}

impl ClipSpec {
    /// Project a real [`AnimationClip`] into the comparable [`ClipSpec`] (field-for-field).
    pub fn from_clip(c: &AnimationClip) -> Self {
        match c {
            AnimationClip::Bobbing { amplitude, frequency, phase } => ClipSpec::Bobbing {
                amplitude: *amplitude,
                frequency: *frequency,
                phase: *phase,
            },
            AnimationClip::Spinning { speed, angle } => ClipSpec::Spinning {
                speed: *speed,
                angle: *angle,
            },
            AnimationClip::SquashStretch {
                squash_scale,
                stretch_scale,
                duration,
                timer,
                active,
            } => ClipSpec::SquashStretch {
                squash_scale: *squash_scale,
                stretch_scale: *stretch_scale,
                duration: *duration,
                timer: *timer,
                active: *active,
            },
            AnimationClip::Wobble { intensity, speed, phase } => ClipSpec::Wobble {
                intensity: *intensity,
                speed: *speed,
                phase: *phase,
            },
            AnimationClip::PopIn { target_scale, duration, timer, overshoot } => ClipSpec::PopIn {
                target_scale: *target_scale,
                duration: *duration,
                timer: *timer,
                overshoot: *overshoot,
            },
            AnimationClip::HeadBob {
                rise_height,
                rise_time,
                hold_time,
                drop_time,
                wait_time,
                timer,
                phase,
            } => ClipSpec::HeadBob {
                rise_height: *rise_height,
                rise_time: *rise_time,
                hold_time: *hold_time,
                drop_time: *drop_time,
                wait_time: *wait_time,
                timer: *timer,
                phase: PhaseSpec::from_phase(phase),
            },
            AnimationClip::PulseGlow { min_scale, max_scale, speed, phase } => ClipSpec::PulseGlow {
                min_scale: *min_scale,
                max_scale: *max_scale,
                speed: *speed,
                phase: *phase,
            },
            AnimationClip::Glitch { interval, timer, intensity, seed } => ClipSpec::Glitch {
                interval: *interval,
                timer: *timer,
                intensity: *intensity,
                seed: *seed,
            },
        }
    }

    /// The hyphenated `:clip` id of the variant this spec mirrors.
    pub fn clip_id(&self) -> &'static str {
        clip_id(&self.to_clip())
    }

    /// Reconstruct the real engine [`AnimationClip`] from this spec (inverse of
    /// [`ClipSpec::from_clip`]).
    pub fn to_clip(&self) -> AnimationClip {
        match self {
            ClipSpec::Bobbing { amplitude, frequency, phase } => AnimationClip::Bobbing {
                amplitude: *amplitude,
                frequency: *frequency,
                phase: *phase,
            },
            ClipSpec::Spinning { speed, angle } => AnimationClip::Spinning {
                speed: *speed,
                angle: *angle,
            },
            ClipSpec::SquashStretch {
                squash_scale,
                stretch_scale,
                duration,
                timer,
                active,
            } => AnimationClip::SquashStretch {
                squash_scale: *squash_scale,
                stretch_scale: *stretch_scale,
                duration: *duration,
                timer: *timer,
                active: *active,
            },
            ClipSpec::Wobble { intensity, speed, phase } => AnimationClip::Wobble {
                intensity: *intensity,
                speed: *speed,
                phase: *phase,
            },
            ClipSpec::PopIn { target_scale, duration, timer, overshoot } => AnimationClip::PopIn {
                target_scale: *target_scale,
                duration: *duration,
                timer: *timer,
                overshoot: *overshoot,
            },
            ClipSpec::HeadBob {
                rise_height,
                rise_time,
                hold_time,
                drop_time,
                wait_time,
                timer,
                phase,
            } => AnimationClip::HeadBob {
                rise_height: *rise_height,
                rise_time: *rise_time,
                hold_time: *hold_time,
                drop_time: *drop_time,
                wait_time: *wait_time,
                timer: *timer,
                phase: phase.to_phase(),
            },
            ClipSpec::PulseGlow { min_scale, max_scale, speed, phase } => AnimationClip::PulseGlow {
                min_scale: *min_scale,
                max_scale: *max_scale,
                speed: *speed,
                phase: *phase,
            },
            ClipSpec::Glitch { interval, timer, intensity, seed } => AnimationClip::Glitch {
                interval: *interval,
                timer: *timer,
                intensity: *intensity,
                seed: *seed,
            },
        }
    }
}

/// Project a whole animation state's clips into the comparable [`ClipSpec`] list (order
/// preserved). The parity contract compares these element-by-element.
pub fn animation_specs(s: &AnimationState) -> Vec<ClipSpec> {
    s.animations.iter().map(ClipSpec::from_clip).collect()
}

/// The compiled-in fallback / parity oracle: the real
/// `AnimationState::{skibidi_idle,grimace_wobble,item_pickup,sigma_idle,ohio_glitch}()`.
/// Returns `None` for an unknown name. This is what the shipped EDN is parity-tested
/// against.
pub fn builtin_animation(name: &str) -> Option<AnimationState> {
    Some(match name {
        "skibidi-idle" => AnimationState::skibidi_idle(),
        "grimace-wobble" => AnimationState::grimace_wobble(),
        "item-pickup" => AnimationState::item_pickup(),
        "sigma-idle" => AnimationState::sigma_idle(),
        "ohio-glitch" => AnimationState::ohio_glitch(),
        _ => return None,
    })
}

/// Build one preset's state from its EDN clip-vector: `AnimationState::new()` +
/// `.with(clip)` in order, exactly the way the hardcoded factories assemble it.
fn animation_from_vec(clips: &[EdnValue]) -> Result<AnimationState, Error> {
    let mut s = AnimationState::new();
    for v in clips {
        let m = v
            .as_map()
            .ok_or_else(|| Error::UnknownClip("<not-a-map>".to_string()))?;
        s = s.with(clip_from_map(m)?);
    }
    Ok(s)
}

/// Parse the whole `:game/animations` table from EDN `src` into a map keyed by the
/// (hyphenated) preset id, each value the rebuilt [`AnimationState`].
pub fn animations_from_edn(src: &str) -> Result<BTreeMap<String, AnimationState>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "game/animations")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kami_scene::kw_key(k) else { continue };
        let Some(vec) = v.as_vector() else { continue };
        by_id.insert(id, animation_from_vec(vec)?);
    }
    Ok(by_id)
}

/// Look up & rebuild a single preset state by (hyphenated) id from EDN `src`. Errors if
/// the table or the named preset is absent.
pub fn animation_from_edn(src: &str, name: &str) -> Result<AnimationState, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "game/animations")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable)?;
    let vec = table
        .iter()
        .find_map(|(k, v)| (kami_scene::kw_key(k).as_deref() == Some(name)).then_some(v))
        .and_then(|v| v.as_vector())
        .ok_or_else(|| Error::AnimationNotFound(name.to_string()))?;
    animation_from_vec(vec)
}

/// Convenience: load & rebuild all presets from the crate-shipped [`ANIMATIONS_EDN`].
pub fn shipped_animations() -> Result<BTreeMap<String, AnimationState>, Error> {
    animations_from_edn(ANIMATIONS_EDN)
}

/// Convenience: load & rebuild one preset from the shipped EDN.
pub fn shipped_animation(name: &str) -> Result<AnimationState, Error> {
    animation_from_edn(ANIMATIONS_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_animations() {
        let a = shipped_animations().expect("animations.edn parse");
        assert_eq!(a.len(), 5);
        for name in ALL_ANIMATION_NAMES {
            assert!(a.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn animation_lengths_match_builtin() {
        // Clip counts (and thus order positions) match the hardcoded factories.
        assert_eq!(shipped_animation("skibidi-idle").unwrap().animations.len(), 2);
        assert_eq!(shipped_animation("grimace-wobble").unwrap().animations.len(), 2);
        assert_eq!(shipped_animation("item-pickup").unwrap().animations.len(), 3);
        assert_eq!(shipped_animation("sigma-idle").unwrap().animations.len(), 0);
        assert_eq!(shipped_animation("ohio-glitch").unwrap().animations.len(), 1);
    }

    #[test]
    fn unknown_builtin_animation_is_none() {
        assert!(builtin_animation("does-not-exist").is_none());
    }

    #[test]
    fn unknown_animation_from_edn_is_an_error() {
        assert!(matches!(
            animation_from_edn(ANIMATIONS_EDN, "rizz-idle"),
            Err(Error::AnimationNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(animations_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            animations_from_edn("{:other 1}"),
            Err(Error::NoTable)
        ));
    }

    #[test]
    fn unknown_clip_is_an_error() {
        assert!(matches!(
            animations_from_edn("{:game/animations {:p [{:clip :no-such-clip}]}}"),
            Err(Error::UnknownClip(_))
        ));
    }

    #[test]
    fn clip_id_round_trips() {
        for name in ALL_ANIMATION_NAMES {
            for c in &builtin_animation(name).unwrap().animations {
                let spec = ClipSpec::from_clip(c);
                assert_eq!(spec.clip_id(), clip_id(c));
            }
        }
    }
}
