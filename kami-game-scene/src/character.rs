//! Brainrot-character data tier — `kami-game`'s parametric character definitions
//! (`island_gen::brainrot_characters()`) as parity-tested EDN.
//!
//! Mesh / scene generation stays native Rust; only the init-time **description** — the
//! per-character `CharacterDef` (JSON-LD id/name/role + `gftd:kami/character` appearance +
//! spawn points) — moves to EDN (ADR-0046 / ADR-0038). [`characters_from_edn`] rebuilds the
//! real [`kami_game::scene::CharacterDef`] list, asserted `==` the compiled-in
//! `brainrot_characters()` (via serde) in `tests/character_parity.rs`.
//!
//! `CharacterDef` / `CharacterAppearance` derive `Serialize`, so parity uses a structural
//! JSON comparison — no `PartialEq` mirror needed.

use std::collections::BTreeMap;

use kami_game::island_gen::brainrot_characters;
use kami_game::scene::{CharacterAppearance, CharacterDef};
use kami_scene::{mget, num, root_map, EdnValue};

/// The canonical brainrot-character CONFIG shipped with this crate.
pub const BRAINROT_CHARACTERS_EDN: &str = include_str!("../data/brainrot_characters.edn");

/// Errors raised while loading the brainrot-character table from EDN.
#[derive(Debug, thiserror::Error)]
pub enum CharacterError {
    /// The EDN source did not parse to a top-level map.
    #[error("brainrot-characters EDN root is not a map")]
    NotAMap,
    /// The `:game/brainrot-characters` table was missing or not a vector.
    #[error("`:game/brainrot-characters` missing or not a vector")]
    NoTable,
}

type Map = BTreeMap<EdnValue, EdnValue>;

fn str_at(m: &Map, key: &str) -> String {
    mget(m, key).and_then(|v| v.as_string()).unwrap_or("").to_string()
}
fn opt_str(m: &Map, key: &str) -> Option<String> {
    mget(m, key).and_then(|v| v.as_string()).map(str::to_string)
}
fn strings(m: &Map, key: &str) -> Vec<String> {
    mget(m, key)
        .and_then(|v| v.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| v.as_string().map(str::to_string))
        .collect()
}

fn appearance_from_map(m: &Map) -> CharacterAppearance {
    CharacterAppearance {
        face: str_at(m, "face"),
        skin_hue: num(mget(m, "skin-hue")),
        skin_lightness: num(mget(m, "skin-lightness")),
        eye: str_at(m, "eye"),
        eye_color_hue: num(mget(m, "eye-color-hue")),
        eye_size: num(mget(m, "eye-size")),
        nose: str_at(m, "nose"),
        mouth: str_at(m, "mouth"),
        mouth_size: num(mget(m, "mouth-size")),
        hair: str_at(m, "hair"),
        hair_color_hue: num(mget(m, "hair-color-hue")),
        hair_color_lightness: num(mget(m, "hair-color-lightness")),
        body: str_at(m, "body"),
        height: num(mget(m, "height")),
        accessory1: str_at(m, "accessory1"),
        accessory2: str_at(m, "accessory2"),
    }
}

/// Build one [`CharacterDef`] from its EDN map (tolerant: missing → default / None).
pub fn character_from_map(m: &Map) -> CharacterDef {
    let appearance = mget(m, "appearance")
        .and_then(|v| v.as_map())
        .map(appearance_from_map)
        .unwrap_or_else(|| appearance_from_map(&BTreeMap::new()));
    CharacterDef {
        ld_type: opt_str(m, "ld-type"),
        id: str_at(m, "id"),
        name: str_at(m, "name"),
        role: opt_str(m, "role"),
        appearance,
        spawn_points: strings(m, "spawn-points"),
    }
}

/// Parse the `:game/brainrot-characters` table from EDN `src` into real [`CharacterDef`]s.
pub fn characters_from_edn(src: &str) -> Result<Vec<CharacterDef>, CharacterError> {
    let root = root_map(src).ok_or(CharacterError::NotAMap)?;
    let entries = mget(&root, "game/brainrot-characters")
        .and_then(|v| v.as_vector())
        .ok_or(CharacterError::NoTable)?;
    Ok(entries
        .iter()
        .filter_map(|e| e.as_map().map(character_from_map))
        .collect())
}

/// The compiled-in oracle: `brainrot_characters()`.
pub fn builtin_characters() -> Vec<CharacterDef> {
    brainrot_characters()
}

/// Convenience: the characters from the crate-shipped [`BRAINROT_CHARACTERS_EDN`].
pub fn shipped_characters() -> Result<Vec<CharacterDef>, CharacterError> {
    characters_from_edn(BRAINROT_CHARACTERS_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_seven_characters() {
        let cs = characters_from_edn(BRAINROT_CHARACTERS_EDN).expect("parses");
        assert_eq!(cs.len(), 7);
        assert_eq!(cs.len(), builtin_characters().len());
        assert_eq!(cs[0].id, "char-skibidi-commander");
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(characters_from_edn("42"), Err(CharacterError::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            characters_from_edn("{:x 1}"),
            Err(CharacterError::NoTable)
        ));
    }
}
