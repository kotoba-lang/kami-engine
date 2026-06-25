//! Parity test: the shipped `pokoa_dex.edn` must reproduce kami-game's compiled-in
//! `pokoa::pokoa_dex()` species-for-species, IN ORDER (ADR-0046) — EDN as source of truth.
//!
//! The oracle is the REAL Rust (`builtin_dex_specs`, not transcribed). `SpeciesDef` uses
//! `&'static str` and derives no `PartialEq`, so both sides are projected into the
//! owned `PartialEq` `SpeciesDefSpec` mirror and compared as ordered vectors.

use kami_game_scene::pokoa::{
    builtin_dex_specs, builtin_item_specs, dex_specs_from_edn, item_specs_from_edn, POKOA_DEX_EDN,
    POKOA_ITEMS_EDN,
};

#[test]
fn pokoa_dex_edn_matches_builtin() {
    let loaded = dex_specs_from_edn(POKOA_DEX_EDN).expect("pokoa_dex.edn parses");
    let builtin = builtin_dex_specs();

    assert_eq!(loaded.len(), builtin.len(), "species count");
    assert_eq!(loaded.len(), 12, "all 12 species present");

    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(g.type1, w.type1, "species[{i}] ({}) type1", w.name);
        assert_eq!(g.type2, w.type2, "species[{i}] ({}) type2", w.name);
        assert_eq!(g.base_stats, w.base_stats, "species[{i}] ({}) stats", w.name);
        assert_eq!(g.evolves_to, w.evolves_to, "species[{i}] ({}) evolution", w.name);
        assert_eq!(g.learnable_moves, w.learnable_moves, "species[{i}] ({}) moves", w.name);
        assert_eq!(g, w, "species[{i}] ({}) full parity", w.name);
    }

    assert_eq!(loaded, builtin, "full pokoa-dex parity (ordered)");
}

#[test]
fn pokoa_items_edn_matches_builtin() {
    let loaded = item_specs_from_edn(POKOA_ITEMS_EDN).expect("pokoa_items.edn parses");
    let builtin = builtin_item_specs();

    assert_eq!(loaded.len(), builtin.len(), "item count");
    assert_eq!(loaded.len(), 10, "all 10 items present");

    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(g.item_type, w.item_type, "item[{i}] ({}) type", w.name);
        assert_eq!(g, w, "item[{i}] ({}) full parity", w.name);
    }

    assert_eq!(loaded, builtin, "full pokoa-items parity (ordered)");
}
