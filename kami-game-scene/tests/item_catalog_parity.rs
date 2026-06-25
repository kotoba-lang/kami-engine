//! Parity test: the shipped `item_catalog.edn` must reproduce kami-game's compiled-in
//! `sabiotoshi::default_item_catalog()` item-for-item, zone-for-zone, step-for-step, IN
//! ORDER (ADR-0046) — EDN as source of truth, behaviour unchanged.
//!
//! The oracle is the REAL Rust: `default_item_catalog()` (via `builtin_item_specs`, not
//! transcribed). `RestorableItem` / `RustZone3D` / `DisassemblyStep` derive no `PartialEq`
//! (and hold `glam::Vec3`), so both sides are projected into the `PartialEq` `ItemSpec`
//! mirror (Vec3 → `[f32; 3]`) and compared as ordered vectors.

use kami_game_scene::item_catalog::{
    builtin_item_specs, item_specs_from_edn, spec_to_item, ItemSpec, ITEM_CATALOG_EDN,
};

#[test]
fn item_catalog_edn_matches_builtin() {
    let loaded = item_specs_from_edn(ITEM_CATALOG_EDN).expect("item_catalog.edn parses");
    let builtin = builtin_item_specs();

    assert_eq!(loaded.len(), builtin.len(), "item count");
    assert_eq!(loaded.len(), 4, "all 4 items present");

    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(g.zones.len(), w.zones.len(), "item[{i}] ({}) zone count", w.id);
        for (j, (gz, wz)) in g.zones.iter().zip(w.zones.iter()).enumerate() {
            assert_eq!(gz, wz, "item[{i}].zone[{j}] (center/extent/rust-type/levels/nerf)");
        }
        assert_eq!(
            g.disassembly_steps, w.disassembly_steps,
            "item[{i}] ({}) disassembly steps",
            w.id
        );
        assert_eq!(g, w, "item[{i}] ({}) full parity", w.id);
    }

    assert_eq!(loaded, builtin, "full item-catalog parity (ordered)");
}

/// `spec_to_item` reconstructs the real `RestorableItem` whose re-projected spec equals
/// the oracle's.
#[test]
fn spec_round_trips_through_item() {
    let loaded = item_specs_from_edn(ITEM_CATALOG_EDN).unwrap();
    let builtin = builtin_item_specs();
    for (spec, want) in loaded.iter().zip(builtin.iter()) {
        let item = spec_to_item(spec);
        assert_eq!(
            ItemSpec::from_item(&item),
            *want,
            "{}: RestorableItem round-trips through ItemSpec",
            want.id
        );
    }
}
