//! Parity test: the shipped battle-royale EDN must reproduce kami-game's compiled-in
//! `default_storm_phases()` and `consumable_pool()` entry-for-entry, IN ORDER (ADR-0046)
//! — EDN as source of truth, behaviour unchanged.
//!
//! The oracles are the REAL Rust (`builtin_storm_phases` / `builtin_consumables`, not
//! transcribed). `StormPhaseConfig` / `ConsumableDef` derive `Serialize` (no `PartialEq`),
//! so each side is compared structurally via `serde_json`.

use kami_game_scene::battle_royale::{
    builtin_consumables, builtin_storm_phases, builtin_weapons, consumables_from_edn,
    storm_phases_from_edn, weapons_from_edn, CONSUMABLES_EDN, STORM_PHASES_EDN, WEAPONS_EDN,
};

#[test]
fn storm_phases_edn_matches_builtin() {
    let loaded = storm_phases_from_edn(STORM_PHASES_EDN).expect("storm edn parses");
    let builtin = builtin_storm_phases();
    assert_eq!(loaded.len(), builtin.len(), "phase count");
    assert_eq!(loaded.len(), 8, "all 8 phases present");
    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(
            serde_json::to_value(g).unwrap(),
            serde_json::to_value(w).unwrap(),
            "phase[{i}]"
        );
    }
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&builtin).unwrap(),
        "full storm-phase parity (ordered)"
    );
}

#[test]
fn consumables_edn_matches_builtin() {
    let loaded = consumables_from_edn(CONSUMABLES_EDN).expect("consumables edn parses");
    let builtin = builtin_consumables();
    assert_eq!(loaded.len(), builtin.len(), "consumable count");
    assert_eq!(loaded.len(), 11, "all 11 consumables present");
    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(
            serde_json::to_value(g).unwrap(),
            serde_json::to_value(w).unwrap(),
            "consumable[{i}] ({})",
            w.name
        );
    }
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&builtin).unwrap(),
        "full consumable parity (ordered)"
    );
}

#[test]
fn weapons_edn_matches_builtin() {
    let loaded = weapons_from_edn(WEAPONS_EDN).expect("weapons edn parses");
    let builtin = builtin_weapons();
    assert_eq!(loaded.len(), builtin.len(), "weapon count");
    assert_eq!(loaded.len(), 25, "all 25 weapons present");
    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(
            serde_json::to_value(g).unwrap(),
            serde_json::to_value(w).unwrap(),
            "weapon[{i}] ({} {})",
            w.name,
            serde_json::to_value(&w.rarity).unwrap()
        );
    }
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&builtin).unwrap(),
        "full weapon-pool parity (ordered)"
    );
}
