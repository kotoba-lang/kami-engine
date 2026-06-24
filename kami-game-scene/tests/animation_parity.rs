//! Parity tests: the shipped EDN must faithfully reproduce kami-game's compiled-in
//! animation-state presets — every clip, every field (including the initial runtime state:
//! `timer: 0.0`, `phase: Wait`, `angle: 0.0`, …), IN ORDER. This is the whole point of the
//! data tier (ADR-0038/0046): EDN becomes the source of truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares an `AnimationState` rebuilt from
//! `animations.edn` against
//! `AnimationState::{skibidi_idle,grimace_wobble,item_pickup,sigma_idle,ohio_glitch}()`
//! (called here, not transcribed).
//!
//! `kami_game::animation::{AnimationClip,AnimationState,HeadBobPhase}` derive only
//! `Debug, Clone` (+ serde — no `PartialEq`), so we project both the loaded state and the
//! oracle state into the `PartialEq` mirror [`ClipSpec`] (via `animation_specs`) and
//! compare the two `Vec<ClipSpec>` — element-for-element, order preserved. No accessor was
//! needed: `AnimationState.animations` is already a public `Vec<AnimationClip>`, so the
//! data tier reads it directly and `kami-game` is left untouched.
//!
//! Preset values are exact decimal literals (2.0, 0.5, 0.05, 0.15, …), all representable in
//! f32, so parity is asserted with exact `==` on the whole spec list.

use kami_game::animation::AnimationState;
use kami_game_scene::{
    animation_from_edn, animation_specs, animations_from_edn, builtin_animation, shipped_animations,
    ClipSpec, Error, ALL_ANIMATION_NAMES, ANIMATIONS_EDN,
};

/// Map a preset name to the REAL Rust factory result (the oracle source).
fn oracle(name: &str) -> AnimationState {
    match name {
        "skibidi-idle" => AnimationState::skibidi_idle(),
        "grimace-wobble" => AnimationState::grimace_wobble(),
        "item-pickup" => AnimationState::item_pickup(),
        "sigma-idle" => AnimationState::sigma_idle(),
        "ohio-glitch" => AnimationState::ohio_glitch(),
        other => panic!("unknown preset {other}"),
    }
}

/// Assert the EDN-rebuilt state equals the hardcoded `AnimationState::*()` oracle: same
/// clip count, and every clip (each field) in the same order.
fn assert_animation_eq(name: &str, loaded: &AnimationState) {
    let o = oracle(name);

    let got: Vec<ClipSpec> = animation_specs(loaded);
    let want: Vec<ClipSpec> = animation_specs(&o);

    // Same number of clips, in the combine order.
    assert_eq!(got.len(), want.len(), "{name}: clip count");

    // Every clip (variant + every field, incl. initial runtime state), position by position.
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g, w, "{name}: clip[{i}] (variant + fields, in order)");
    }

    // And the whole ordered clip list equals the oracle's (exact f32 / u32 / bool equality).
    assert_eq!(got, want, "{name}: full animation parity (ordered)");
}

/// For each shipped preset, the rebuilt state == the value from the REAL Rust
/// `AnimationState::*()` factory — every clip + every field, in order.
#[test]
fn animations_edn_matches_builtin() {
    let loaded = animations_from_edn(ANIMATIONS_EDN).expect("animations.edn parse");
    assert_eq!(loaded.len(), 5, "all presets present in EDN");

    for name in ALL_ANIMATION_NAMES {
        assert_animation_eq(name, &loaded[name]);

        // The `builtin_animation` oracle helper agrees with what we read off the factories.
        let built = builtin_animation(name).expect("builtin animation");
        assert_eq!(
            animation_specs(&loaded[name]),
            animation_specs(&built),
            "{name}: EDN == builtin_animation()"
        );
    }

    // The shipped-animations convenience loader yields the same thing.
    let shipped = shipped_animations().expect("shipped animations");
    for name in ALL_ANIMATION_NAMES {
        assert_eq!(
            animation_specs(&shipped[name]),
            animation_specs(&loaded[name]),
            "{name}: shipped == loaded"
        );
    }
}

/// `animation_from_edn` rebuilds one preset identical to the hardcoded factory.
#[test]
fn single_animation_from_edn_matches() {
    for name in ALL_ANIMATION_NAMES {
        let got = animation_from_edn(ANIMATIONS_EDN, name).expect("animation");
        assert_animation_eq(name, &got);
    }
}

/// The `ClipSpec` round-trip (`to_clip`) reconstructs the real engine `AnimationClip`, so
/// the rebuilt state equals the oracle through the projection.
#[test]
fn clipspec_round_trips_through_clip() {
    for name in ALL_ANIMATION_NAMES {
        let o = oracle(name);
        for c in &o.animations {
            let spec = ClipSpec::from_clip(c);
            let back = spec.to_clip();
            assert_eq!(
                ClipSpec::from_clip(&back),
                spec,
                "{name}: ClipSpec round-trips through AnimationClip"
            );
        }
    }
}

/// Tolerant-parse errors: unknown clip → error, unknown preset → error, non-map root →
/// error, missing table → error.
#[test]
fn tolerant_parse_errors() {
    assert!(matches!(
        animation_from_edn(ANIMATIONS_EDN, "rizz-idle"),
        Err(Error::AnimationNotFound(_))
    ));
    assert!(matches!(
        animations_from_edn("{:game/animations {:p [{:clip :bogus-clip}]}}"),
        Err(Error::UnknownClip(_))
    ));
    assert!(matches!(animations_from_edn("123"), Err(Error::NotAMap)));
    assert!(matches!(animations_from_edn("{:x 1}"), Err(Error::NoTable)));
}
