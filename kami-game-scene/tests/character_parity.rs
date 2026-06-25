//! Parity test: the shipped `brainrot_characters.edn` must reproduce kami-game's
//! compiled-in `island_gen::brainrot_characters()` character-for-character, IN ORDER
//! (ADR-0046) — EDN as source of truth, behaviour unchanged.
//!
//! The oracle is the REAL Rust: `brainrot_characters()` (via `builtin_characters`, not
//! transcribed). `CharacterDef` / `CharacterAppearance` derive `Serialize` but not
//! `PartialEq`, so the rebuilt list is compared structurally via `serde_json`.

use kami_game_scene::character::{builtin_characters, characters_from_edn, BRAINROT_CHARACTERS_EDN};

#[test]
fn characters_edn_matches_builtin() {
    let loaded = characters_from_edn(BRAINROT_CHARACTERS_EDN).expect("brainrot_characters.edn parses");
    let builtin = builtin_characters();

    assert_eq!(loaded.len(), builtin.len(), "character count");
    assert_eq!(loaded.len(), 7, "all 7 characters present");

    // Per-character JSON parity (clear failure messages by id), then whole-list parity.
    for (i, (g, w)) in loaded.iter().zip(builtin.iter()).enumerate() {
        assert_eq!(
            serde_json::to_value(g).unwrap(),
            serde_json::to_value(w).unwrap(),
            "character[{i}] ({}) — id/name/role/appearance/spawn-points",
            w.id
        );
    }

    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&builtin).unwrap(),
        "full brainrot-characters parity (ordered)"
    );
}
