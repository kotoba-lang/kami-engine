//! kami-postfx-scene — EDN authoring surface for `kami-postfx`
//! POST-PROCESSING PIPELINE presets.
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` /
//! `kami-terrain-scene` / `kami-vegetation-scene` for the post-processing system: it
//! turns canonical `:postfx/presets` EDN (an ordered vector of `:effect`-tagged
//! effect maps per preset) into the real [`kami_postfx::PostFxPipeline`] engine
//! struct, rebuilt the same way the hardcoded presets are (`PostFxPipeline::new()` +
//! `add(effect)` in order). It re-uses the tolerant `kami-scene` accessors the same
//! way games parse `scene.edn` (missing keys fall back to defaults, namespaced
//! keywords match on `ns/name`, ints coerce to floats / u32).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot fullscreen passes / GPU uniform packing (`BloomParams`, `SSAOParams`, …) stay
//! native Rust (`kami-postfx`). A post-fx preset is **init-time CONFIG** — read once
//! when the pipeline is assembled at boot — so it is safe to move to EDN.
//! `kami-postfx` itself stays untouched; the EDN dependency lives only here. The
//! compiled-in `PostFxPipeline::{nintendo,retro,final_fantasy,baminiku_character}()`
//! builders remain as the [`builtin_preset`] fallback and are parity-tested against
//! the shipped EDN ([`crate::POSTFX_EDN`]).
//!
//! ## EDN shape (see `data/postfx.edn`)
//!
//! ```edn
//! {:postfx/presets
//!  {:nintendo [{:effect :bloom :threshold 0.8 :intensity 0.3 :radius 4.0}
//!              {:effect :outline :color [0.15 0.15 0.15 1.0] :width 1.5 :depth-threshold 0.1}
//!              {:effect :vignette :intensity 0.15 :radius 0.8}]
//!   :retro [...] :final-fantasy [...] :baminiku-character [...]}}
//! ```
//!
//! Each preset is an **ordered vector** (the pipeline order is load-bearing). Each
//! effect map is tagged by `:effect` (a keyword id naming the [`kami_postfx::PostEffect`]
//! variant); the rest of the keys are that variant's fields, hyphenated. Unknown
//! `:effect` ids are an [`Error::UnknownEffect`].

use std::collections::BTreeMap;

use kami_postfx::{PostEffect, PostFxPipeline};
use kami_scene::{mget, num, root_map, vec3, EdnValue};

/// The canonical post-processing preset CONFIG shipped with this crate (the preset
/// table). This is the source of truth; the compiled-in presets are the parity-tested
/// mirror.
pub const POSTFX_EDN: &str = include_str!("../data/postfx.edn");

/// Names of the presets shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Keeping this list here (not in `kami-postfx`) keeps the engine
/// crate untouched. Order mirrors the `impl PostFxPipeline` declaration order.
pub const ALL_PRESET_NAMES: [&str; 4] =
    ["nintendo", "retro", "final-fantasy", "baminiku-character"];

/// Errors raised while loading post-fx preset CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("postfx EDN root is not a map")]
    NotAMap,
    /// The `:postfx/presets` table was missing or not a map.
    #[error("`:postfx/presets` missing or not a map")]
    NoPresets,
    /// The requested preset id was missing under `:postfx/presets`.
    #[error("preset `{0}` not found under `:postfx/presets`")]
    PresetNotFound(String),
    /// An effect map carried an `:effect` id with no matching [`PostEffect`] variant
    /// (or no `:effect` key at all).
    #[error("unknown post effect `{0}`")]
    UnknownEffect(String),
}

// ── small typed accessors over the tolerant kami-scene helpers ──
//
// kami-scene ships `num` (f32) / `vec3` ([f32;3]); post-fx also needs u32 and the
// fixed [f32;2] / [f32;4] vectors. These mirror `vec3`'s "pad short, zero a
// non-vector" tolerance so absent / malformed config degrades the same way.

/// Read a `u32` field, coercing via `num` then rounding; `0` when absent / non-numeric.
fn u32_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> u32 {
    match mget(m, key) {
        Some(v) => num(Some(v)).round() as u32,
        None => 0,
    }
}

/// Read an `f32` field; `0.0` when absent / non-numeric (via `num`).
fn f32_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> f32 {
    num(mget(m, key))
}

/// Read a 2-vector `[x y]`; missing components default to `0.0`, non-vector → `[0,0]`.
fn vec2_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> [f32; 2] {
    let s = mget(m, key).and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(0.0);
    [g(0), g(1)]
}

/// Read a 3-vector `[r g b]` field via the shared `kami_scene::vec3`.
fn vec3_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> [f32; 3] {
    vec3(mget(m, key))
}

/// Read a 4-vector `[r g b a]`; missing components default to `0.0`, non-vector → zeros.
fn vec4_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> [f32; 4] {
    let s = mget(m, key).and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(0.0);
    [g(0), g(1), g(2), g(3)]
}

/// The hyphenated `:effect` keyword id for a [`PostEffect`] variant. Inverse of the
/// match in [`effect_from_map`]; also the source of truth for [`EffectSpec::effect_id`].
pub fn effect_id(e: &PostEffect) -> &'static str {
    match e {
        PostEffect::Bloom { .. } => "bloom",
        PostEffect::Outline { .. } => "outline",
        PostEffect::Vignette { .. } => "vignette",
        PostEffect::CRT { .. } => "crt",
        PostEffect::ColorGrade { .. } => "color-grade",
        PostEffect::Pixelate { .. } => "pixelate",
        PostEffect::SSAO { .. } => "ssao",
        PostEffect::DepthOfField { .. } => "depth-of-field",
        PostEffect::SSR { .. } => "ssr",
        PostEffect::ACESTonemap { .. } => "aces-tonemap",
        PostEffect::FilmGrain { .. } => "film-grain",
        PostEffect::ChromaticAberration { .. } => "chromatic-aberration",
        PostEffect::GodRays { .. } => "god-rays",
    }
}

/// Build one real [`PostEffect`] from an effect map tagged by `:effect`.
///
/// Every field is read with the tolerant accessors, so a key a map omits degrades to
/// the same zero/default the rest of the data tier uses. An `:effect` id with no
/// matching variant (or a missing `:effect`) is [`Error::UnknownEffect`].
pub fn effect_from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Result<PostEffect, Error> {
    let id = mget(m, "effect")
        .and_then(kami_scene::kw_key)
        .ok_or_else(|| Error::UnknownEffect("<missing>".to_string()))?;

    let e = match id.as_str() {
        "bloom" => PostEffect::Bloom {
            threshold: f32_of(m, "threshold"),
            intensity: f32_of(m, "intensity"),
            radius: f32_of(m, "radius"),
        },
        "outline" => PostEffect::Outline {
            color: vec4_of(m, "color"),
            width: f32_of(m, "width"),
            depth_threshold: f32_of(m, "depth-threshold"),
        },
        "vignette" => PostEffect::Vignette {
            intensity: f32_of(m, "intensity"),
            radius: f32_of(m, "radius"),
        },
        "crt" => PostEffect::CRT {
            scanline_intensity: f32_of(m, "scanline-intensity"),
            curvature: f32_of(m, "curvature"),
        },
        "color-grade" => PostEffect::ColorGrade {
            lift: vec3_of(m, "lift"),
            gamma: vec3_of(m, "gamma"),
            gain: vec3_of(m, "gain"),
        },
        "pixelate" => PostEffect::Pixelate {
            pixel_size: f32_of(m, "pixel-size"),
        },
        "ssao" => PostEffect::SSAO {
            radius: f32_of(m, "radius"),
            bias: f32_of(m, "bias"),
            intensity: f32_of(m, "intensity"),
            samples: u32_of(m, "samples"),
        },
        "depth-of-field" => PostEffect::DepthOfField {
            focal_distance: f32_of(m, "focal-distance"),
            focal_range: f32_of(m, "focal-range"),
            bokeh_radius: f32_of(m, "bokeh-radius"),
            bokeh_shape: u32_of(m, "bokeh-shape"),
        },
        "ssr" => PostEffect::SSR {
            max_distance: f32_of(m, "max-distance"),
            steps: u32_of(m, "steps"),
            thickness: f32_of(m, "thickness"),
            fade_edge: f32_of(m, "fade-edge"),
        },
        "aces-tonemap" => PostEffect::ACESTonemap {
            exposure: f32_of(m, "exposure"),
            curve: u32_of(m, "curve"),
        },
        "film-grain" => PostEffect::FilmGrain {
            intensity: f32_of(m, "intensity"),
            size: f32_of(m, "size"),
        },
        "chromatic-aberration" => PostEffect::ChromaticAberration {
            intensity: f32_of(m, "intensity"),
            samples: u32_of(m, "samples"),
        },
        "god-rays" => PostEffect::GodRays {
            density: f32_of(m, "density"),
            weight: f32_of(m, "weight"),
            decay: f32_of(m, "decay"),
            exposure: f32_of(m, "exposure"),
            light_pos: vec2_of(m, "light-pos"),
        },
        other => return Err(Error::UnknownEffect(other.to_string())),
    };
    Ok(e)
}

/// A structurally-comparable mirror of [`PostEffect`].
///
/// `kami_postfx::PostEffect` derives only `Debug, Clone` (no `PartialEq`), so the
/// data tier cannot assert `loaded == nintendo()` directly without touching the
/// engine crate. `EffectSpec` is a `PartialEq` projection of each variant + its
/// fields, so parity (every field, in order) is asserted here additively, with
/// `kami-postfx` left untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectSpec {
    Bloom { threshold: f32, intensity: f32, radius: f32 },
    Outline { color: [f32; 4], width: f32, depth_threshold: f32 },
    Vignette { intensity: f32, radius: f32 },
    Crt { scanline_intensity: f32, curvature: f32 },
    ColorGrade { lift: [f32; 3], gamma: [f32; 3], gain: [f32; 3] },
    Pixelate { pixel_size: f32 },
    Ssao { radius: f32, bias: f32, intensity: f32, samples: u32 },
    DepthOfField { focal_distance: f32, focal_range: f32, bokeh_radius: f32, bokeh_shape: u32 },
    Ssr { max_distance: f32, steps: u32, thickness: f32, fade_edge: f32 },
    AcesTonemap { exposure: f32, curve: u32 },
    FilmGrain { intensity: f32, size: f32 },
    ChromaticAberration { intensity: f32, samples: u32 },
    GodRays { density: f32, weight: f32, decay: f32, exposure: f32, light_pos: [f32; 2] },
}

impl EffectSpec {
    /// Project a real [`PostEffect`] into the comparable [`EffectSpec`] (field-for-field).
    pub fn from_post_effect(e: &PostEffect) -> Self {
        match *e {
            PostEffect::Bloom { threshold, intensity, radius } => {
                EffectSpec::Bloom { threshold, intensity, radius }
            }
            PostEffect::Outline { color, width, depth_threshold } => {
                EffectSpec::Outline { color, width, depth_threshold }
            }
            PostEffect::Vignette { intensity, radius } => EffectSpec::Vignette { intensity, radius },
            PostEffect::CRT { scanline_intensity, curvature } => {
                EffectSpec::Crt { scanline_intensity, curvature }
            }
            PostEffect::ColorGrade { lift, gamma, gain } => {
                EffectSpec::ColorGrade { lift, gamma, gain }
            }
            PostEffect::Pixelate { pixel_size } => EffectSpec::Pixelate { pixel_size },
            PostEffect::SSAO { radius, bias, intensity, samples } => {
                EffectSpec::Ssao { radius, bias, intensity, samples }
            }
            PostEffect::DepthOfField { focal_distance, focal_range, bokeh_radius, bokeh_shape } => {
                EffectSpec::DepthOfField { focal_distance, focal_range, bokeh_radius, bokeh_shape }
            }
            PostEffect::SSR { max_distance, steps, thickness, fade_edge } => {
                EffectSpec::Ssr { max_distance, steps, thickness, fade_edge }
            }
            PostEffect::ACESTonemap { exposure, curve } => {
                EffectSpec::AcesTonemap { exposure, curve }
            }
            PostEffect::FilmGrain { intensity, size } => EffectSpec::FilmGrain { intensity, size },
            PostEffect::ChromaticAberration { intensity, samples } => {
                EffectSpec::ChromaticAberration { intensity, samples }
            }
            PostEffect::GodRays { density, weight, decay, exposure, light_pos } => {
                EffectSpec::GodRays { density, weight, decay, exposure, light_pos }
            }
        }
    }

    /// The hyphenated `:effect` id of the variant this spec mirrors.
    pub fn effect_id(&self) -> &'static str {
        effect_id(&self.to_post_effect())
    }

    /// Reconstruct the real engine [`PostEffect`] from this spec (inverse of
    /// [`EffectSpec::from_post_effect`]).
    pub fn to_post_effect(&self) -> PostEffect {
        match *self {
            EffectSpec::Bloom { threshold, intensity, radius } => {
                PostEffect::Bloom { threshold, intensity, radius }
            }
            EffectSpec::Outline { color, width, depth_threshold } => {
                PostEffect::Outline { color, width, depth_threshold }
            }
            EffectSpec::Vignette { intensity, radius } => PostEffect::Vignette { intensity, radius },
            EffectSpec::Crt { scanline_intensity, curvature } => {
                PostEffect::CRT { scanline_intensity, curvature }
            }
            EffectSpec::ColorGrade { lift, gamma, gain } => {
                PostEffect::ColorGrade { lift, gamma, gain }
            }
            EffectSpec::Pixelate { pixel_size } => PostEffect::Pixelate { pixel_size },
            EffectSpec::Ssao { radius, bias, intensity, samples } => {
                PostEffect::SSAO { radius, bias, intensity, samples }
            }
            EffectSpec::DepthOfField { focal_distance, focal_range, bokeh_radius, bokeh_shape } => {
                PostEffect::DepthOfField { focal_distance, focal_range, bokeh_radius, bokeh_shape }
            }
            EffectSpec::Ssr { max_distance, steps, thickness, fade_edge } => {
                PostEffect::SSR { max_distance, steps, thickness, fade_edge }
            }
            EffectSpec::AcesTonemap { exposure, curve } => {
                PostEffect::ACESTonemap { exposure, curve }
            }
            EffectSpec::FilmGrain { intensity, size } => PostEffect::FilmGrain { intensity, size },
            EffectSpec::ChromaticAberration { intensity, samples } => {
                PostEffect::ChromaticAberration { intensity, samples }
            }
            EffectSpec::GodRays { density, weight, decay, exposure, light_pos } => {
                PostEffect::GodRays { density, weight, decay, exposure, light_pos }
            }
        }
    }
}

/// Project a whole pipeline's effects into the comparable [`EffectSpec`] list (order
/// preserved). The parity contract compares these element-by-element.
pub fn pipeline_specs(p: &PostFxPipeline) -> Vec<EffectSpec> {
    p.effects.iter().map(EffectSpec::from_post_effect).collect()
}

/// The compiled-in fallback / parity oracle: the real
/// `PostFxPipeline::{nintendo,retro,final_fantasy,baminiku_character}()`. Returns
/// `None` for an unknown name. This is what the shipped EDN is parity-tested against.
pub fn builtin_preset(name: &str) -> Option<PostFxPipeline> {
    Some(match name {
        "nintendo" => PostFxPipeline::nintendo(),
        "retro" => PostFxPipeline::retro(),
        "final-fantasy" => PostFxPipeline::final_fantasy(),
        "baminiku-character" => PostFxPipeline::baminiku_character(),
        _ => return None,
    })
}

/// Build one preset's pipeline from its EDN effect-vector: `PostFxPipeline::new()` +
/// `add(effect)` in order, exactly the way the hardcoded builders assemble it.
fn pipeline_from_vec(effects: &[EdnValue]) -> Result<PostFxPipeline, Error> {
    let mut p = PostFxPipeline::new();
    for v in effects {
        let m = v.as_map().ok_or_else(|| Error::UnknownEffect("<not-a-map>".to_string()))?;
        p.add(effect_from_map(m)?);
    }
    Ok(p)
}

/// Parse the whole `:postfx/presets` table from EDN `src` into a map keyed by the
/// (hyphenated) preset id, each value the rebuilt [`PostFxPipeline`].
pub fn presets_from_edn(src: &str) -> Result<BTreeMap<String, PostFxPipeline>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let presets = mget(&root, "postfx/presets")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoPresets)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in presets.iter() {
        let Some(id) = kami_scene::kw_key(k) else { continue };
        let Some(vec) = v.as_vector() else { continue };
        by_id.insert(id, pipeline_from_vec(vec)?);
    }
    Ok(by_id)
}

/// Look up & rebuild a single preset pipeline by (hyphenated) id from EDN `src`.
/// Errors if the table or the named preset is absent.
pub fn preset_from_edn(src: &str, name: &str) -> Result<PostFxPipeline, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let presets = mget(&root, "postfx/presets")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoPresets)?;
    let vec = presets
        .iter()
        .find_map(|(k, v)| {
            (kami_scene::kw_key(k).as_deref() == Some(name)).then_some(v)
        })
        .and_then(|v| v.as_vector())
        .ok_or_else(|| Error::PresetNotFound(name.to_string()))?;
    pipeline_from_vec(vec)
}

/// Convenience: load & rebuild all presets from the crate-shipped [`POSTFX_EDN`].
pub fn shipped_presets() -> Result<BTreeMap<String, PostFxPipeline>, Error> {
    presets_from_edn(POSTFX_EDN)
}

/// Convenience: load & rebuild one preset from the shipped EDN.
pub fn shipped_preset(name: &str) -> Result<PostFxPipeline, Error> {
    preset_from_edn(POSTFX_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_presets() {
        let p = shipped_presets().expect("postfx.edn parse");
        assert_eq!(p.len(), 4);
        for name in ALL_PRESET_NAMES {
            assert!(p.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn preset_lengths_match_builtin() {
        // Effect counts (and thus order positions) match the hardcoded pipelines.
        assert_eq!(shipped_preset("nintendo").unwrap().effects.len(), 3);
        assert_eq!(shipped_preset("retro").unwrap().effects.len(), 2);
        assert_eq!(shipped_preset("final-fantasy").unwrap().effects.len(), 10);
        assert_eq!(shipped_preset("baminiku-character").unwrap().effects.len(), 6);
    }

    #[test]
    fn rebuilt_pipeline_is_enabled() {
        // PostFxPipeline::new() sets enabled = true, like every hardcoded preset.
        assert!(shipped_preset("nintendo").unwrap().enabled);
    }

    #[test]
    fn unknown_builtin_preset_is_none() {
        assert!(builtin_preset("does-not-exist").is_none());
    }

    #[test]
    fn unknown_preset_from_edn_is_an_error() {
        assert!(matches!(
            preset_from_edn(POSTFX_EDN, "cinematic"),
            Err(Error::PresetNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(presets_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_presets_table_is_an_error() {
        assert!(matches!(presets_from_edn("{:other 1}"), Err(Error::NoPresets)));
    }

    #[test]
    fn unknown_effect_is_an_error() {
        assert!(matches!(
            presets_from_edn("{:postfx/presets {:p [{:effect :no-such-fx}]}}"),
            Err(Error::UnknownEffect(_))
        ));
    }

    #[test]
    fn effect_id_round_trips() {
        for name in ALL_PRESET_NAMES {
            for e in &builtin_preset(name).unwrap().effects {
                let spec = EffectSpec::from_post_effect(e);
                assert_eq!(spec.effect_id(), effect_id(e));
            }
        }
    }
}
