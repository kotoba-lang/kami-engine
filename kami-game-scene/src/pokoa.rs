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

use kami_game::pokoa::{
    pokoa_dex, pokoa_items, EvolutionTrigger, ItemDef, ItemType, PokoaType, SpeciesDef,
};
use kami_scene::{kw_key, mget, root_map, EdnValue};

/// The canonical Pokoa-dex CONFIG shipped with this crate (generated from the oracle).
pub const POKOA_DEX_EDN: &str = include_str!("../data/pokoa_dex.edn");
/// The canonical Pokoa item-shop CONFIG shipped with this crate (generated from the oracle).
pub const POKOA_ITEMS_EDN: &str = include_str!("../data/pokoa_items.edn");

/// Errors raised while loading the Pokoa dex from EDN.
#[derive(Debug, thiserror::Error)]
pub enum PokoaError {
    /// The EDN source did not parse to a top-level map.
    #[error("pokoa-dex EDN root is not a map")]
    NotAMap,
    /// The expected table key was missing or not a vector.
    #[error("`{0}` missing or not a vector")]
    NoTable(&'static str),
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
        .ok_or(PokoaError::NoTable("game/pokoa-dex"))?;
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

// ── item shop ────────────────────────────────────────────────────────────────────────

fn u32_at(m: &Map, key: &str) -> u32 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0).clamp(0, u32::MAX as i64) as u32
}

/// PartialEq mirror of [`kami_game::pokoa::ItemType`] (owned; `&'static str` → `String`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemTypeSpec {
    Pokeball { catch_modifier: u8 },
    Potion { heal_amount: u16 },
    Revive { hp_pct: u8 },
    EvolutionItem { item_id: String },
    KeyItem { name: String },
    /// Fallback for an unknown `:kind` (never produced from the oracle).
    Unknown,
}

impl ItemTypeSpec {
    fn from_item_type(t: &ItemType) -> Self {
        match t {
            ItemType::Pokeball { catch_modifier } => Self::Pokeball { catch_modifier: *catch_modifier },
            ItemType::Potion { heal_amount } => Self::Potion { heal_amount: *heal_amount },
            ItemType::Revive { hp_pct } => Self::Revive { hp_pct: *hp_pct },
            ItemType::EvolutionItem { item_id } => Self::EvolutionItem { item_id: item_id.to_string() },
            ItemType::KeyItem { name } => Self::KeyItem { name: name.to_string() },
        }
    }
    fn from_map(m: &Map) -> Self {
        match mget(m, "kind").and_then(kw_key).as_deref() {
            Some("pokeball") => Self::Pokeball { catch_modifier: u8_at(m, "catch-modifier") },
            Some("potion") => Self::Potion { heal_amount: u16_at(m, "heal-amount") },
            Some("revive") => Self::Revive { hp_pct: u8_at(m, "hp-pct") },
            Some("evolution-item") => Self::EvolutionItem { item_id: str_at(m, "item-id") },
            Some("key-item") => Self::KeyItem { name: str_at(m, "name") },
            _ => Self::Unknown,
        }
    }
}

/// PartialEq mirror of [`kami_game::pokoa::ItemDef`] (owned).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDefSpec {
    pub id: String,
    pub name: String,
    pub item_type: ItemTypeSpec,
    pub price: u32,
}

impl ItemDefSpec {
    /// Read the spec straight off the real engine item (the parity oracle source).
    pub fn from_item(it: &ItemDef) -> Self {
        Self {
            id: it.id.to_string(),
            name: it.name.to_string(),
            item_type: ItemTypeSpec::from_item_type(&it.item_type),
            price: it.price,
        }
    }
    /// Build a spec from one item's EDN map.
    pub fn from_map(m: &Map) -> Self {
        Self {
            id: str_at(m, "id"),
            name: str_at(m, "name"),
            item_type: mget(m, "type")
                .and_then(|v| v.as_map())
                .map(ItemTypeSpec::from_map)
                .unwrap_or(ItemTypeSpec::Unknown),
            price: u32_at(m, "price"),
        }
    }
}

/// Parse the `:game/pokoa-items` table from EDN `src` into ordered [`ItemDefSpec`]s.
pub fn item_specs_from_edn(src: &str) -> Result<Vec<ItemDefSpec>, PokoaError> {
    let root = root_map(src).ok_or(PokoaError::NotAMap)?;
    let items = mget(&root, "game/pokoa-items")
        .and_then(|v| v.as_vector())
        .ok_or(PokoaError::NoTable("game/pokoa-items"))?;
    Ok(items
        .iter()
        .filter_map(|e| e.as_map().map(ItemDefSpec::from_map))
        .collect())
}

/// The compiled-in oracle: `pokoa_items()` projected into specs.
pub fn builtin_item_specs() -> Vec<ItemDefSpec> {
    pokoa_items().iter().map(ItemDefSpec::from_item).collect()
}

/// Convenience: the item shop from the crate-shipped [`POKOA_ITEMS_EDN`].
pub fn shipped_item_specs() -> Result<Vec<ItemDefSpec>, PokoaError> {
    item_specs_from_edn(POKOA_ITEMS_EDN)
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
        assert!(matches!(dex_specs_from_edn("{:x 1}"), Err(PokoaError::NoTable(_))));
        assert!(matches!(item_specs_from_edn("{:x 1}"), Err(PokoaError::NoTable(_))));
    }

    #[test]
    fn shipped_has_ten_items() {
        let specs = item_specs_from_edn(POKOA_ITEMS_EDN).expect("pokoa_items.edn parses");
        assert_eq!(specs.len(), 10);
        assert_eq!(specs.len(), builtin_item_specs().len());
        assert_eq!(specs[0].id, "pokoa-ball");
    }
}
