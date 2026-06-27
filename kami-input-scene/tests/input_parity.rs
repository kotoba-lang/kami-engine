//! Parity tests: the shipped EDN must faithfully reproduce kami-input's compiled-in
//! default input-binding maps — every `(key-code, action)` pair, in order. This is
//! the whole point of the data tier (ADR-0038 / ADR-0040): EDN becomes the source of
//! truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares an `InputMap` rebuilt from
//! `input.edn` against `InputMap::{default_fps,default_graph}()` (called here, not
//! transcribed). Order is load-bearing because `InputMap::resolve` is first-match, so
//! the comparison walks the bindings vec index-by-index.
//!
//! `kami_input::InputMap` derives no `PartialEq` (it is just `pub bindings:
//! Vec<(String, Action)>`), so we compare the `bindings` vecs element-by-element.
//! `kami_input::Action` derives `PartialEq, Eq, Copy`, so each pair compares with
//! `==`. `kami-input` is left untouched.

use kami_input::InputMap;
use kami_input_scene::{
    ALL_MAP_NAMES, Error, INPUT_EDN, builtin_input_map, input_map_from_edn, input_maps_from_edn,
    shipped_input_map, shipped_input_maps,
};

/// Map a map name to the REAL Rust builder result (the oracle source).
fn oracle(name: &str) -> InputMap {
    match name {
        "fps" => InputMap::default_fps(),
        "graph" => InputMap::default_graph(),
        other => panic!("unknown map {other}"),
    }
}

/// Assert two `InputMap`s have identical bindings — same length, and every
/// `(key-code, action)` pair equal, in order. `InputMap` derives no `PartialEq`, so
/// we walk the `bindings` vec (`Action` is `PartialEq`; `String` is `PartialEq`).
fn assert_map_eq(name: &str, loaded: &InputMap, want: &InputMap) {
    assert_eq!(
        loaded.bindings.len(),
        want.bindings.len(),
        "{name}: binding count"
    );
    for (i, (got, exp)) in loaded.bindings.iter().zip(want.bindings.iter()).enumerate() {
        assert_eq!(got.0, exp.0, "{name}: binding[{i}] key-code");
        assert_eq!(got.1, exp.1, "{name}: binding[{i}] action");
        // The whole pair, in order (tuple == uses String == + Action ==).
        assert_eq!(got, exp, "{name}: binding[{i}] full (key, action) pair");
    }
    // The whole bindings vec, in order.
    assert_eq!(
        loaded.bindings, want.bindings,
        "{name}: full InputMap.bindings parity"
    );
}

/// For each shipped map, the rebuilt `InputMap` == the value from the REAL Rust
/// `InputMap::*()` builder — every binding, in order.
#[test]
fn input_maps_edn_matches_builtin() {
    let loaded = input_maps_from_edn(INPUT_EDN).expect("input.edn parse");
    assert_eq!(loaded.len(), 2, "all maps present in EDN");

    for name in ALL_MAP_NAMES {
        assert_map_eq(name, &loaded[name], &oracle(name));

        // The `builtin_input_map` oracle helper agrees with what we read off the builders.
        let built = builtin_input_map(name).expect("builtin map");
        assert_map_eq(name, &loaded[name], &built);
    }

    // The shipped-maps convenience loader yields the same thing.
    let shipped = shipped_input_maps().expect("shipped maps");
    for name in ALL_MAP_NAMES {
        assert_map_eq(name, &shipped[name], &loaded[name]);
    }
}

/// `input_map_from_edn` / `shipped_input_map` rebuild one map identical to the builder.
#[test]
fn single_map_from_edn_matches() {
    for name in ALL_MAP_NAMES {
        let got = input_map_from_edn(INPUT_EDN, name).expect("map");
        assert_map_eq(name, &got, &oracle(name));
        let shipped = shipped_input_map(name).expect("shipped map");
        assert_map_eq(name, &shipped, &oracle(name));
    }
}

/// The shipped `:fps` and `:graph` maps each have the expected 12 bindings, and the
/// first-match `resolve` semantics still hold on the EDN-rebuilt map.
#[test]
fn shipped_maps_have_expected_counts_and_resolve() {
    let fps = shipped_input_map("fps").expect("fps");
    let graph = shipped_input_map("graph").expect("graph");
    assert_eq!(fps.bindings.len(), 12, "fps has 12 bindings");
    assert_eq!(graph.bindings.len(), 12, "graph has 12 bindings");

    // Resolve (first-match) behaves identically to the oracle.
    assert_eq!(fps.resolve("KeyW"), InputMap::default_fps().resolve("KeyW"));
    assert_eq!(
        fps.resolve("Escape"),
        InputMap::default_fps().resolve("Escape")
    );
    assert_eq!(
        graph.resolve("Equal"),
        InputMap::default_graph().resolve("Equal")
    );
    assert_eq!(
        graph.resolve("NumpadSubtract"),
        InputMap::default_graph().resolve("NumpadSubtract")
    );
    assert_eq!(fps.resolve("KeyX"), None, "unbound key → None");
}

/// Tolerant-parse errors: unknown map → error, unknown action → error, non-map root →
/// error, missing table → error.
#[test]
fn tolerant_parse_errors() {
    assert!(matches!(
        input_map_from_edn(INPUT_EDN, "vehicle"),
        Err(Error::MapNotFound(_))
    ));
    assert!(matches!(
        input_map_from_edn("{:input/maps {:m [[\"KeyW\" :teleport]]}}", "m"),
        Err(Error::UnknownAction(a)) if a == "teleport"
    ));
    assert!(matches!(input_maps_from_edn("123"), Err(Error::NotAMap)));
    assert!(matches!(input_maps_from_edn("{:x 1}"), Err(Error::NoTable)));
}
