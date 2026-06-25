//! Parity tests: the shipped EDN must faithfully reproduce kami-postfx's compiled-in
//! post-processing presets — every effect, every field, IN ORDER. This is the whole
//! point of the data tier (ADR-0038): EDN becomes the source of truth with *behaviour
//! unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a pipeline rebuilt from
//! `postfx.edn` against `PostFxPipeline::{nintendo,retro,final_fantasy,
//! baminiku_character}()` (called here, not transcribed).
//!
//! `kami_postfx::PostEffect` / `PostFxPipeline` derive only `Debug, Clone` (no
//! `PartialEq`), so we project both the loaded pipeline and the oracle pipeline into
//! the `PartialEq` mirror [`EffectSpec`] (via `pipeline_specs`) and compare the two
//! `Vec<EffectSpec>` — element-for-element, order preserved. No `effects()` getter was
//! needed: `PostFxPipeline.effects` is already a public `Vec<PostEffect>`, so the data
//! tier reads it directly and `kami-postfx` is left untouched.
//!
//! Preset values are exact decimal literals (e.g. 0.025, 0.96, 0.002…), all
//! representable in f32, so parity is asserted with exact `==` on the whole spec list.

use kami_postfx::PostFxPipeline;
use kami_postfx_scene::{
    builtin_preset, pipeline_specs, presets_from_edn, preset_from_edn, shipped_presets, Error,
    EffectSpec, ALL_PRESET_NAMES, POSTFX_EDN,
};

/// Map a preset name to the REAL Rust builder result (the oracle source).
fn oracle(name: &str) -> PostFxPipeline {
    match name {
        "nintendo" => PostFxPipeline::nintendo(),
        "retro" => PostFxPipeline::retro(),
        "final-fantasy" => PostFxPipeline::final_fantasy(),
        "baminiku-character" => PostFxPipeline::baminiku_character(),
        other => panic!("unknown preset {other}"),
    }
}

/// Assert the EDN-rebuilt pipeline equals the hardcoded `PostFxPipeline::*()` oracle:
/// same effect count, same `enabled`, and every effect (each field) in the same order.
fn assert_pipeline_eq(name: &str, loaded: &PostFxPipeline) {
    let o = oracle(name);

    // `enabled` matches (PostFxPipeline::new() sets it true, like every preset).
    assert_eq!(loaded.enabled, o.enabled, "{name}: enabled");

    let got: Vec<EffectSpec> = pipeline_specs(loaded);
    let want: Vec<EffectSpec> = pipeline_specs(&o);

    // Same number of effects, in the pipeline order.
    assert_eq!(got.len(), want.len(), "{name}: effect count");

    // Every effect (variant + every field), position by position.
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g, w, "{name}: effect[{i}] (variant + fields, in order)");
    }

    // And the whole ordered effect list equals the oracle's (exact f32 equality).
    assert_eq!(got, want, "{name}: full pipeline parity (ordered)");
}

/// For each shipped preset, the rebuilt pipeline == the value from the REAL Rust
/// `PostFxPipeline::*()` builder — every effect + every param, in order.
#[test]
fn presets_edn_matches_builtin() {
    let loaded = presets_from_edn(POSTFX_EDN).expect("postfx.edn parse");
    assert_eq!(loaded.len(), 4, "all presets present in EDN");

    for name in ALL_PRESET_NAMES {
        assert_pipeline_eq(name, &loaded[name]);

        // The `builtin_preset` oracle helper agrees with what we read off the builders.
        let built = builtin_preset(name).expect("builtin preset");
        assert_eq!(
            pipeline_specs(&loaded[name]),
            pipeline_specs(&built),
            "{name}: EDN == builtin_preset()"
        );
    }

    // The shipped-presets convenience loader yields the same thing.
    let shipped = shipped_presets().expect("shipped presets");
    for name in ALL_PRESET_NAMES {
        assert_eq!(
            pipeline_specs(&shipped[name]),
            pipeline_specs(&loaded[name]),
            "{name}: shipped == loaded"
        );
    }
}

/// `preset_from_edn` rebuilds one preset identical to the hardcoded builder.
#[test]
fn single_preset_from_edn_matches() {
    for name in ALL_PRESET_NAMES {
        let got = preset_from_edn(POSTFX_EDN, name).expect("preset");
        assert_pipeline_eq(name, &got);
    }
}

/// The `EffectSpec` round-trip (`to_post_effect`) reconstructs the real engine
/// `PostEffect`, so the rebuilt pipeline equals the oracle through the projection.
#[test]
fn effectspec_round_trips_through_post_effect() {
    for name in ALL_PRESET_NAMES {
        let o = oracle(name);
        for e in &o.effects {
            let spec = EffectSpec::from_post_effect(e);
            let back = spec.to_post_effect();
            assert_eq!(
                EffectSpec::from_post_effect(&back),
                spec,
                "{name}: EffectSpec round-trips through PostEffect"
            );
        }
    }
}

/// Tolerant-parse errors: unknown effect → error, unknown preset → error, non-map root
/// → error, missing table → error.
#[test]
fn tolerant_parse_errors() {
    assert!(matches!(
        preset_from_edn(POSTFX_EDN, "cinematic"),
        Err(Error::PresetNotFound(_))
    ));
    assert!(matches!(
        presets_from_edn("{:postfx/presets {:p [{:effect :bogus-fx}]}}"),
        Err(Error::UnknownEffect(_))
    ));
    assert!(matches!(presets_from_edn("123"), Err(Error::NotAMap)));
    assert!(matches!(presets_from_edn("{:x 1}"), Err(Error::NoPresets)));
}
