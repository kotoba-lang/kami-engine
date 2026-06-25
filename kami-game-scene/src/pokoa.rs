//! Pokoa-dex data tier — kami-game's Pokémon-style species dex (`pokoa::pokoa_dex()`)
//! as parity-tested EDN.
//!
//! The battle engine stays native Rust; the species **description** (types / base stats /
//! catch+exp / evolution / learnable moves) becomes EDN data (ADR-0046 / ADR-0040) — the
//! substrate a CLJS/Datomic brain can author and fork.
//!
//! NOTE: `SpeciesDef` uses `&'static str` fields, so its real engine struct cannot be
//! rebuilt from runtime EDN. The data tier therefore exposes an owned [`SpeciesDefSpec`]
//! mirror; it is parity-tested `==` `pokoa_dex()` projected to specs.

use std::collections::BTreeMap;

use kami_game::pokoa::{pokoa_dex, EvolutionTrigger, PokoaType, SpeciesDef};
use kami_scene::{kw_key, mget, root_map, EdnValue};

/// The canonical Pokoa-dex CONFIG shipped with this crate (generated from the oracle).
pub const POKOA_DEX_EDN: &str = include_str!("../data/pokoa_dex.edn");

/// Errors raised while loading the Pokoa dex from EDN.
#[derive(Debug, thiserror::Error)]
pub enum PokoaError {
    /// The EDN source did not parse to a top-level map.
    #[error("pokoa-dex EDN root is not a map")]
    NotAMap,
    /// The `:game/pokoa-dex` table was missing or not a vector.
    #[error("`:game/pokoa-dex` missing or not a vector")]
    NoTable,
}

type Map = BTreeMap<EdnValue, EdnValue>;

/// The hyphenated keyword id for a [`PokoaType`].
pub fn pokoa_type_id(t: PokoaType) -> &'static str {
    match t {
        PokoaType::Normal => "normal",
        PokoaType::Fire => "fire",
        PokoaType::Water => "water",
        PokoaType::Electric => "electric",
        PokoaType::Grass => "grass",
        PokoaType::Ice => "ice",
        PokoaType::Fighting => "fighting",
        PokoaType::Poison => "poison",
        PokoaType::Ground => "ground",
        PokoaType::Flying => "flying",
        PokoaType::Psychic => "psychic",
        PokoaType::Bug => "bug",
        PokoaType::Rock => "rock",
        PokoaType::Ghost => "ghost",
        PokoaType::Dragon => "dragon",
        PokoaType::Dark => "dark",
        PokoaType::Steel => "steel",
        PokoaType::Fairy => "fairy",
    }
}

/// PartialEq mirror of `Stats`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsSpec {
    pub hp: u16,
    pub atk: u16,
    pub def: u16,
    pub spa: u16,
    pub spd: u16,
    pub spe: u16,
}

/// PartialEq mirror of an evolution gate (`Option<(u16, EvolutionTrigger)>` element).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolveSpec {
    /// Target species id.
    pub to: u16,
    /// Level gate, if the trigger is `EvolutionTrigger::Level`.
    pub level: Option<u8>,
    /// Item id, if the trigger is `EvolutionTrigger::Item`.
    pub item: Option<String>,
}

/// PartialEq mirror of [`kami_game::pokoa::SpeciesDef`] (owned; `&'static str` → `String`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesDefSpec {
    pub id: u16,
    pub name: String,
    pub type1: String,
    pub type2: Option<String>,
    pub base_stats: StatsSpec,
    pub catch_rate: u8,
    pub exp_yield: u16,
    pub evolves_to: Option<EvolveSpec>,
    pub learnable_moves: Vec<(u8, String)>,
    pub description: String,
}

// ── readers ──────────────────────────────────────────────────────────────────────────

fn u16_at(m: &Map, key: &str) -> u16 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0).clamp(0, u16::MAX as i64) as u16
}
fn u8_at(m: &Map, key: &str) -> u8 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0).clamp(0, 255) as u8
}
fn str_at(m: &Map, key: &str) -> String {
    mget(m, key).and_then(|v| v.as_string()).unwrap_or("").to_string()
}

fn stats_from_map(m: &Map) -> StatsSpec {
    StatsSpec {
        hp: u16_at(m, "hp"),
        atk: u16_at(m, "atk"),
        def: u16_at(m, "def"),
        spa: u16_at(m, "spa"),
        spd: u16_at(m, "spd"),
        spe: u16_at(m, "spe"),
    }
}

fn evolve_from_map(m: &Map) -> EvolveSpec {
    EvolveSpec {
        to: u16_at(m, "to"),
        level: mget(m, "level").and_then(|v| v.as_integer()).map(|i| i.clamp(0, 255) as u8),
        item: mget(m, "item").and_then(|v| v.as_string()).map(str::to_string),
    }
}

impl StatsSpec {
    fn from_stats(s: &kami_game::pokoa::Stats) -> Self {
        Self { hp: s.hp, atk: s.atk, def: s.def, spa: s.spa, spd: s.spd, spe: s.spe }
    }
}

impl SpeciesDefSpec {
    /// Read the spec straight off the real engine species (the parity oracle source).
    pub fn from_species(s: &SpeciesDef) -> Self {
        let (t1, t2) = s.types;
        Self {
            id: s.id,
            name: s.name.to_string(),
            type1: pokoa_type_id(t1).to_string(),
            type2: t2.map(|t| pokoa_type_id(t).to_string()),
            base_stats: StatsSpec::from_stats(&s.base_stats),
            catch_rate: s.catch_rate,
            exp_yield: s.exp_yield,
            evolves_to: s.evolves_to.map(|(to, trig)| EvolveSpec {
                to,
                level: match trig {
                    EvolutionTrigger::Level(l) => Some(l),
                    EvolutionTrigger::Item(_) => None,
                },
                item: match trig {
                    EvolutionTrigger::Item(i) => Some(i.to_string()),
                    EvolutionTrigger::Level(_) => None,
                },
            }),
            learnable_moves: s
                .learnable_moves
                .iter()
                .map(|(lv, m)| (*lv, m.to_string()))
                .collect(),
            description: s.description.to_string(),
        }
    }

    /// Build a spec from one species' EDN map (tolerant).
    pub fn from_map(m: &Map) -> Self {
        let types: Vec<String> = mget(m, "types")
            .and_then(|v| v.as_vector())
            .unwrap_or(&[])
            .iter()
            .filter_map(kw_key)
            .collect();
        let evolves_to = mget(m, "evolves-to")
            .and_then(|v| v.as_map())
            .map(evolve_from_map);
        let learnable_moves = mget(m, "moves")
            .and_then(|v| v.as_vector())
            .unwrap_or(&[])
            .iter()
            .filter_map(|pair| {
                let p = pair.as_vector()?;
                let lv = p.first()?.as_integer()?.clamp(0, 255) as u8;
                let mv = p.get(1)?.as_string()?.to_string();
                Some((lv, mv))
            })
            .collect();
        Self {
            id: u16_at(m, "id"),
            name: str_at(m, "name"),
            type1: types.first().cloned().unwrap_or_default(),
            type2: types.get(1).cloned(),
            base_stats: mget(m, "stats")
                .and_then(|v| v.as_map())
                .map(stats_from_map)
                .unwrap_or(StatsSpec { hp: 0, atk: 0, def: 0, spa: 0, spd: 0, spe: 0 }),
            catch_rate: u8_at(m, "catch-rate"),
            exp_yield: u16_at(m, "exp-yield"),
            evolves_to,
            learnable_moves,
            description: str_at(m, "description"),
        }
    }
}

/// Parse the `:game/pokoa-dex` table from EDN `src` into ordered [`SpeciesDefSpec`]s.
pub fn dex_specs_from_edn(src: &str) -> Result<Vec<SpeciesDefSpec>, PokoaError> {
    let root = root_map(src).ok_or(PokoaError::NotAMap)?;
    let dex = mget(&root, "game/pokoa-dex")
        .and_then(|v| v.as_vector())
        .ok_or(PokoaError::NoTable)?;
    Ok(dex
        .iter()
        .filter_map(|e| e.as_map().map(SpeciesDefSpec::from_map))
        .collect())
}

/// The compiled-in oracle: `pokoa_dex()` projected into specs.
pub fn builtin_dex_specs() -> Vec<SpeciesDefSpec> {
    pokoa_dex().iter().map(SpeciesDefSpec::from_species).collect()
}

/// Convenience: the dex from the crate-shipped [`POKOA_DEX_EDN`].
pub fn shipped_dex_specs() -> Result<Vec<SpeciesDefSpec>, PokoaError> {
    dex_specs_from_edn(POKOA_DEX_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_twelve_species() {
        let specs = dex_specs_from_edn(POKOA_DEX_EDN).expect("pokoa_dex.edn parses");
        assert_eq!(specs.len(), 12);
        assert_eq!(specs.len(), builtin_dex_specs().len());
        assert_eq!(specs[0].name, "Toilettle");
    }

    #[test]
    fn type_ids_are_distinct() {
        let all = [
            PokoaType::Normal, PokoaType::Fire, PokoaType::Water, PokoaType::Electric,
            PokoaType::Grass, PokoaType::Ice, PokoaType::Fighting, PokoaType::Poison,
            PokoaType::Ground, PokoaType::Flying, PokoaType::Psychic, PokoaType::Bug,
            PokoaType::Rock, PokoaType::Ghost, PokoaType::Dragon, PokoaType::Dark,
            PokoaType::Steel, PokoaType::Fairy,
        ];
        let ids: std::collections::BTreeSet<_> = all.iter().map(|t| pokoa_type_id(*t)).collect();
        assert_eq!(ids.len(), 18);
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(dex_specs_from_edn("42"), Err(PokoaError::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(dex_specs_from_edn("{:x 1}"), Err(PokoaError::NoTable)));
    }
}
