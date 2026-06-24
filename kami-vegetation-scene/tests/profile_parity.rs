//! Parity tests: the shipped EDN must faithfully reproduce kami-vegetation's
//! compiled-in taxonomic-profile presets. This is the whole point of the data tier
//! (ADR-0038) — EDN becomes the source of truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a value loaded from
//! `vegetation.edn` against `taxonomy::{grass,fern,palm,conifer,bush,cactus,moss}()`
//! (called here, not transcribed). Preset values are exact decimal literals (e.g.
//! 0.18, 0.008, 0.45…) — all representable in f32 — so parity is asserted with exact
//! `==` (a tiny epsilon is also used on the float scalars to guard against int/float
//! coercion noise; exact equality also holds). `leaf_count` (u32) and the
//! `CanopyShape` / taxonomy enums are compared exactly.

use kami_vegetation::taxonomy::{
    bush, cactus, conifer, fern, grass, moss, palm, TaxonomicProfile,
};
use kami_vegetation_scene::{
    builtin_profile, profiles_from_edn, shipped_profiles, ProfileSpec, ALL_PROFILE_NAMES,
    VEGETATION_EDN,
};

const EPS: f32 = 1e-6;

/// Map a profile name to the REAL Rust builder result (the oracle source).
fn oracle(name: &str) -> TaxonomicProfile {
    match name {
        "grass" => grass(),
        "fern" => fern(),
        "palm" => palm(),
        "conifer" => conifer(),
        "bush" => bush(),
        "cactus" => cactus(),
        "moss" => moss(),
        other => panic!("unknown profile {other}"),
    }
}

/// Assert every field of an EDN-loaded profile equals the same field read off the
/// hardcoded `taxonomy::*()` oracle.
fn assert_profile_eq(name: &str, edn: &ProfileSpec) {
    let p = oracle(name);
    let o = ProfileSpec::from_profile(&p);

    // Taxonomy enums — exact.
    assert_eq!(edn.division, o.division, "{name}: division");
    assert_eq!(edn.habit, o.habit, "{name}: habit");
    assert_eq!(edn.arrangement, o.arrangement, "{name}: arrangement");
    assert_eq!(edn.leaf_shape, o.leaf_shape, "{name}: leaf_shape");
    assert_eq!(edn.canopy, o.canopy, "{name}: canopy");

    // common_name — exact string.
    assert_eq!(edn.common_name, o.common_name, "{name}: common_name");

    // Numeric scalars — exact (epsilon also holds, guards coercion noise).
    assert!((edn.stem_radius_base - o.stem_radius_base).abs() < EPS, "{name}: stem_radius_base");
    assert!((edn.stem_radius_top - o.stem_radius_top).abs() < EPS, "{name}: stem_radius_top");
    assert!((edn.leaf_size - o.leaf_size).abs() < EPS, "{name}: leaf_size");
    assert_eq!(edn.leaf_count, o.leaf_count, "{name}: leaf_count (u32 exact)");

    // height_range [min max].
    assert!((edn.height_range[0] - o.height_range[0]).abs() < EPS, "{name}: height_range[0]");
    assert!((edn.height_range[1] - o.height_range[1]).abs() < EPS, "{name}: height_range[1]");

    // Colours.
    for ch in 0..3 {
        assert!((edn.color_base[ch] - o.color_base[ch]).abs() < EPS, "{name}: color_base[{ch}]");
        assert!((edn.color_tip[ch] - o.color_tip[ch]).abs() < EPS, "{name}: color_tip[{ch}]");
    }

    // And the whole spec equals the oracle-derived spec (exact f32 equality).
    assert_eq!(*edn, o, "{name}: full ProfileSpec parity");
}

/// For each shipped profile, every field loaded from vegetation.edn == the value from
/// the Rust `taxonomy::*()` builder.
#[test]
fn profiles_edn_matches_builtin() {
    let loaded = profiles_from_edn(VEGETATION_EDN).expect("vegetation.edn parse");
    assert_eq!(loaded.len(), 7, "all profiles present in EDN");

    for name in ALL_PROFILE_NAMES {
        assert_profile_eq(name, &loaded[name]);

        // The `builtin_profile` oracle helper agrees with what we read off the builders.
        let built = builtin_profile(name).expect("builtin profile");
        assert_eq!(loaded[name], built, "{name}: EDN == builtin_profile()");
    }

    // The shipped-profiles convenience loader yields the same thing.
    let shipped = shipped_profiles().expect("shipped profiles");
    for name in ALL_PROFILE_NAMES {
        assert_eq!(shipped[name], loaded[name], "{name}: shipped == loaded");
    }
}

/// The converter reconstructs the real engine `TaxonomicProfile`, field-for-field
/// identical to the hardcoded `taxonomy::*()` builder.
#[test]
fn converter_matches_hardcoded() {
    let loaded = profiles_from_edn(VEGETATION_EDN).unwrap();

    for name in ALL_PROFILE_NAMES {
        let o = oracle(name);
        let got = loaded[name].to_taxonomic_profile();

        assert_eq!(got.common_name, o.common_name, "{name}: common_name");
        assert_eq!(got.division, o.division, "{name}: division");
        assert_eq!(got.habit, o.habit, "{name}: habit");
        assert_eq!(got.arrangement, o.arrangement, "{name}: arrangement");
        assert_eq!(got.leaf_shape, o.leaf_shape, "{name}: leaf_shape");
        assert_eq!(got.canopy, o.canopy, "{name}: canopy");
        assert_eq!(got.height_range, o.height_range, "{name}: height_range");
        assert_eq!(got.stem_radius_base, o.stem_radius_base, "{name}: stem_radius_base");
        assert_eq!(got.stem_radius_top, o.stem_radius_top, "{name}: stem_radius_top");
        assert_eq!(got.leaf_count, o.leaf_count, "{name}: leaf_count");
        assert_eq!(got.leaf_size, o.leaf_size, "{name}: leaf_size");
        assert_eq!(got.color_base, o.color_base, "{name}: color_base");
        assert_eq!(got.color_tip, o.color_tip, "{name}: color_tip");
    }
}

/// A profile whose EDN omits keys reproduces the engine `moss()` default for those
/// keys — the tolerant merge contract.
#[test]
fn omitted_fields_inherit_defaults() {
    let loaded = profiles_from_edn("{:vegetation/profiles {:p {:leaf-count 9}}}").unwrap();
    let spec = &loaded["p"];
    let d = ProfileSpec::defaults();
    assert_eq!(spec.leaf_count, 9);
    assert_eq!(spec.leaf_size, d.leaf_size, "absent → default leaf_size");
    assert_eq!(spec.canopy, d.canopy, "absent → default canopy");
    assert_eq!(spec.color_base, d.color_base, "absent → default color_base");
    assert_eq!(spec.common_name, d.common_name, "absent → default common_name");
}

/// Unknown profile → error; non-map root → error; missing table → error.
#[test]
fn tolerant_parse_errors() {
    use kami_vegetation_scene::{profile_from_edn, Error};
    assert!(matches!(
        profile_from_edn(VEGETATION_EDN, "bamboo"),
        Err(Error::ProfileNotFound(_))
    ));
    assert!(matches!(profiles_from_edn("123"), Err(Error::NotAMap)));
    assert!(matches!(profiles_from_edn("{:x 1}"), Err(Error::NoProfiles)));
}
