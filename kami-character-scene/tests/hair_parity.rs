//! Parity tests: the shipped EDN must faithfully reproduce kami-character's compiled-in
//! hair-style presets — every field, exact f32 equality, exact HairType. This is the whole
//! point of the data tier (ADR-0038): EDN becomes the source of truth with *behaviour
//! unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a `HairStyle` rebuilt from
//! `hair.edn` against `HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,afro}()`
//! (called here, not transcribed). Because those fns use `..Self::default()`, the EDN
//! must reproduce the RESOLVED values — the omitted fields are the resolved defaults; the
//! oracle is read field-by-field so any drift fails.
//!
//! `kami_character::HairStyle` derives only `Debug, Clone, Serialize, Deserialize` (no
//! `PartialEq`), so we project both the loaded style and the oracle into the `PartialEq`
//! mirror [`HairStyleSpec`] and compare those — exactly like `kami-postfx-scene`'s
//! `EffectSpec`. `kami-character` is left untouched.
//!
//! Preset values are exact decimal literals (0.7, 0.93, -0.2, …), all representable in
//! f32, so parity is asserted with exact `==` on the whole spec.

use kami_character::HairStyle;
use kami_character_scene::{
    builtin_hair_style, hair_style_from_edn, hair_styles_from_edn, shipped_hair_style,
    shipped_hair_styles, Error, HairStyleSpec, ALL_HAIR_STYLE_NAMES, HAIR_EDN,
};

/// Map a style name to the REAL Rust builder result (the oracle source).
fn oracle(name: &str) -> HairStyle {
    match name {
        "blonde-long" => HairStyle::blonde_long(),
        "dark-short" => HairStyle::dark_short(),
        "red-wavy" => HairStyle::red_wavy(),
        "brown-curly" => HairStyle::brown_curly(),
        "afro" => HairStyle::afro(),
        other => panic!("unknown style {other}"),
    }
}

/// Assert the EDN-rebuilt style equals the hardcoded `HairStyle::*()` oracle — every
/// field (exact f32 equality; HairType exact), via the `PartialEq` mirror.
fn assert_style_eq(name: &str, loaded: &HairStyle) {
    let o = oracle(name);
    let got = HairStyleSpec::from_hair_style(loaded);
    let want = HairStyleSpec::from_hair_style(&o);

    // Field-by-field, so a single-field drift names the field in the failure.
    assert_eq!(got.style, want.style, "{name}: style (HairType)");
    assert_eq!(got.length, want.length, "{name}: length");
    assert_eq!(got.density, want.density, "{name}: density");
    assert_eq!(got.volume, want.volume, "{name}: volume");
    assert_eq!(got.curl, want.curl, "{name}: curl");
    assert_eq!(got.part_side, want.part_side, "{name}: part_side");
    assert_eq!(got.bangs_length, want.bangs_length, "{name}: bangs_length");
    assert_eq!(got.bangs_width, want.bangs_width, "{name}: bangs_width");
    assert_eq!(got.color, want.color, "{name}: color");
    assert_eq!(
        got.highlight_color, want.highlight_color,
        "{name}: highlight_color"
    );
    assert_eq!(
        got.highlight_ratio, want.highlight_ratio,
        "{name}: highlight_ratio"
    );
    assert_eq!(got.root_darken, want.root_darken, "{name}: root_darken");
    assert_eq!(got.head_radius, want.head_radius, "{name}: head_radius");
    assert_eq!(
        got.head_center_y, want.head_center_y,
        "{name}: head_center_y"
    );

    // And the whole spec equals the oracle's (exact f32 equality on every field).
    assert_eq!(got, want, "{name}: full HairStyle parity");
}

/// For each shipped style, the rebuilt `HairStyle` == the value from the REAL Rust
/// `HairStyle::*()` builder — every field.
#[test]
fn hair_styles_edn_matches_builtin() {
    let loaded = hair_styles_from_edn(HAIR_EDN).expect("hair.edn parse");
    assert_eq!(loaded.len(), 5, "all styles present in EDN");

    for name in ALL_HAIR_STYLE_NAMES {
        assert_style_eq(name, &loaded[name]);

        // The `builtin_hair_style` oracle helper agrees with what we read off the builders.
        let built = builtin_hair_style(name).expect("builtin style");
        assert_eq!(
            HairStyleSpec::from_hair_style(&loaded[name]),
            HairStyleSpec::from_hair_style(&built),
            "{name}: EDN == builtin_hair_style()"
        );
    }

    // The shipped-styles convenience loader yields the same thing.
    let shipped = shipped_hair_styles().expect("shipped styles");
    for name in ALL_HAIR_STYLE_NAMES {
        assert_eq!(
            HairStyleSpec::from_hair_style(&shipped[name]),
            HairStyleSpec::from_hair_style(&loaded[name]),
            "{name}: shipped == loaded"
        );
    }
}

/// `hair_style_from_edn` / `shipped_hair_style` rebuild one style identical to the builder.
#[test]
fn single_style_from_edn_matches() {
    for name in ALL_HAIR_STYLE_NAMES {
        let got = hair_style_from_edn(HAIR_EDN, name).expect("style");
        assert_style_eq(name, &got);
        let shipped = shipped_hair_style(name).expect("shipped style");
        assert_style_eq(name, &shipped);
    }
}

/// blonde-long is exactly `HairStyle::default()` (the `blonde_long()` body is
/// `Self::default()`), so the EDN must reproduce the resolved Default.
#[test]
fn blonde_long_is_default() {
    let loaded = shipped_hair_style("blonde-long").expect("blonde-long");
    assert_eq!(
        HairStyleSpec::from_hair_style(&loaded),
        HairStyleSpec::from_hair_style(&HairStyle::default()),
        "blonde-long == HairStyle::default()"
    );
}

/// Tolerant parse: a missing scalar key falls back to the resolved default (the
/// `..Self::default()` semantics in EDN form).
#[test]
fn missing_key_falls_back_to_default() {
    // Only :style given; every other field must resolve to the Default.
    let loaded =
        hair_style_from_edn("{:character/hair-styles {:partial {:style :wavy}}}", "partial")
            .expect("partial");
    let d = HairStyle::default();
    let got = HairStyleSpec::from_hair_style(&loaded);
    assert_eq!(got.style, kami_character::HairType::Wavy, "explicit :style");
    assert_eq!(got.length, d.length, "missing :length → default");
    assert_eq!(got.color, d.color, "missing :color → default");
    assert_eq!(got.head_radius, d.head_radius, "missing :head-radius → default");
    assert_eq!(
        got.head_center_y, d.head_center_y,
        "missing :head-center-y → default"
    );
    // A fully-empty map → the full Default.
    let empty = hair_style_from_edn("{:character/hair-styles {:e {}}}", "e").expect("empty");
    assert_eq!(
        HairStyleSpec::from_hair_style(&empty),
        HairStyleSpec::from_hair_style(&d),
        "empty style map == HairStyle::default()"
    );
}

/// Tolerant-parse errors: unknown style → error, non-map root → error, missing table →
/// error.
#[test]
fn tolerant_parse_errors() {
    assert!(matches!(
        hair_style_from_edn(HAIR_EDN, "rainbow-mohawk"),
        Err(Error::StyleNotFound(_))
    ));
    assert!(matches!(hair_styles_from_edn("123"), Err(Error::NotAMap)));
    assert!(matches!(hair_styles_from_edn("{:x 1}"), Err(Error::NoTable)));
}
