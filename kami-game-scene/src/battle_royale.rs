//! Battle-royale data tier — kami-game's storm-phase schedule
//! (`battle_royale::default_storm_phases()`) and consumable pool
//! (`battle_royale::consumable_pool()`) as parity-tested EDN.
//!
//! The storm simulation and heal/shield logic stay native Rust; only the init-time
//! **descriptions** — the phase schedule and the consumable item table — move to EDN
//! (ADR-0046 / ADR-0038). [`storm_phases_from_edn`] / [`consumables_from_edn`] rebuild the
//! real [`StormPhaseConfig`] / [`ConsumableDef`] lists, asserted `==` the compiled-in
//! oracles in `tests/battle_royale_parity.rs`.
//!
//! `StormPhaseConfig` / `ConsumableDef` derive `Serialize`, so parity is a structural JSON
//! comparison — no `PartialEq` mirror needed; only the keyword-id → enum maps live here.

use std::collections::BTreeMap;

use kami_game::battle_royale::{
    consumable_pool, default_storm_phases, weapon_pool, ConsumableDef, ConsumableType,
    StormPhaseConfig, WeaponDef, WeaponType,
};
use kami_game::inventory::Rarity;
use kami_scene::{kw_key, mget, num, root_map, EdnValue};

/// The canonical storm-phase CONFIG shipped with this crate.
pub const STORM_PHASES_EDN: &str = include_str!("../data/battle_royale_storm.edn");
/// The canonical consumable-pool CONFIG shipped with this crate.
pub const CONSUMABLES_EDN: &str = include_str!("../data/battle_royale_consumables.edn");
/// The canonical weapon-pool CONFIG shipped with this crate (generated from the oracle).
pub const WEAPONS_EDN: &str = include_str!("../data/battle_royale_weapons.edn");

/// Errors raised while loading battle-royale CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum BattleRoyaleError {
    /// The EDN source did not parse to a top-level map.
    #[error("battle-royale EDN root is not a map")]
    NotAMap,
    /// The expected table key was missing or not a vector.
    #[error("`{0}` missing or not a vector")]
    NoTable(&'static str),
}

type Map = BTreeMap<EdnValue, EdnValue>;

// ── enum id maps ─────────────────────────────────────────────────────────────────────

/// The hyphenated keyword id for a [`Rarity`].
pub fn rarity_id(r: Rarity) -> &'static str {
    match r {
        Rarity::Common => "common",
        Rarity::Uncommon => "uncommon",
        Rarity::Rare => "rare",
        Rarity::Epic => "epic",
        Rarity::Legendary => "legendary",
    }
}
/// Inverse of [`rarity_id`]; unknown → `Common` (tolerant).
pub fn rarity_from_id(id: &str) -> Rarity {
    match id {
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "epic" => Rarity::Epic,
        "legendary" => Rarity::Legendary,
        _ => Rarity::Common,
    }
}

/// The hyphenated keyword id for a [`ConsumableType`].
pub fn consumable_type_id(t: ConsumableType) -> &'static str {
    match t {
        ConsumableType::SmallShield => "small-shield",
        ConsumableType::LargeShield => "large-shield",
        ConsumableType::MiniHP => "mini-hp",
        ConsumableType::Medkit => "medkit",
        ConsumableType::Chug => "chug",
        ConsumableType::SmallFry => "small-fry",
        ConsumableType::Flopper => "flopper",
        ConsumableType::ShieldFish => "shield-fish",
        ConsumableType::GrimaceShake => "grimace-shake",
        ConsumableType::GyattEnergy => "gyatt-energy",
        ConsumableType::OhioMilk => "ohio-milk",
    }
}
/// Inverse of [`consumable_type_id`]; unknown → `SmallShield` (tolerant).
pub fn consumable_type_from_id(id: &str) -> ConsumableType {
    match id {
        "large-shield" => ConsumableType::LargeShield,
        "mini-hp" => ConsumableType::MiniHP,
        "medkit" => ConsumableType::Medkit,
        "chug" => ConsumableType::Chug,
        "small-fry" => ConsumableType::SmallFry,
        "flopper" => ConsumableType::Flopper,
        "shield-fish" => ConsumableType::ShieldFish,
        "grimace-shake" => ConsumableType::GrimaceShake,
        "gyatt-energy" => ConsumableType::GyattEnergy,
        "ohio-milk" => ConsumableType::OhioMilk,
        _ => ConsumableType::SmallShield,
    }
}

/// The hyphenated keyword id for a [`WeaponType`].
pub fn weapon_type_id(t: WeaponType) -> &'static str {
    match t {
        WeaponType::AssaultRifle => "assault-rifle",
        WeaponType::Shotgun => "shotgun",
        WeaponType::SMG => "smg",
        WeaponType::SniperRifle => "sniper-rifle",
        WeaponType::Pistol => "pistol",
        WeaponType::RocketLauncher => "rocket-launcher",
        WeaponType::GrenadeLauncher => "grenade-launcher",
    }
}
/// Inverse of [`weapon_type_id`]; unknown → `AssaultRifle` (tolerant).
pub fn weapon_type_from_id(id: &str) -> WeaponType {
    match id {
        "shotgun" => WeaponType::Shotgun,
        "smg" => WeaponType::SMG,
        "sniper-rifle" => WeaponType::SniperRifle,
        "pistol" => WeaponType::Pistol,
        "rocket-launcher" => WeaponType::RocketLauncher,
        "grenade-launcher" => WeaponType::GrenadeLauncher,
        _ => WeaponType::AssaultRifle,
    }
}

// ── readers ──────────────────────────────────────────────────────────────────────────

fn str_at(m: &Map, key: &str) -> String {
    mget(m, key).and_then(|v| v.as_string()).unwrap_or("").to_string()
}
fn u16_at(m: &Map, key: &str) -> u16 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0).clamp(0, u16::MAX as i64) as u16
}
fn u8_at(m: &Map, key: &str) -> u8 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0).clamp(0, 255) as u8
}
fn kw_at(m: &Map, key: &str) -> String {
    mget(m, key).and_then(kw_key).unwrap_or_default()
}

fn storm_phase_from_map(m: &Map) -> StormPhaseConfig {
    StormPhaseConfig {
        phase_index: u8_at(m, "phase"),
        wait_seconds: num(mget(m, "wait")),
        shrink_seconds: num(mget(m, "shrink")),
        end_radius: num(mget(m, "end-radius")),
        damage_per_second: num(mget(m, "dps")),
    }
}

fn consumable_from_map(m: &Map) -> ConsumableDef {
    ConsumableDef {
        consumable_type: consumable_type_from_id(&kw_at(m, "type")),
        name: str_at(m, "name"),
        rarity: rarity_from_id(&kw_at(m, "rarity")),
        use_time: num(mget(m, "use-time")),
        hp_restore: u16_at(m, "hp-restore"),
        shield_restore: u16_at(m, "shield-restore"),
        hp_cap: u16_at(m, "hp-cap"),
        shield_cap: u16_at(m, "shield-cap"),
        stack_size: u16_at(m, "stack"),
    }
}

fn weapon_from_map(m: &Map) -> WeaponDef {
    WeaponDef {
        weapon_type: weapon_type_from_id(&kw_at(m, "type")),
        name: str_at(m, "name"),
        rarity: rarity_from_id(&kw_at(m, "rarity")),
        damage: u16_at(m, "damage"),
        headshot_multiplier: num(mget(m, "headshot-mult")),
        fire_rate: num(mget(m, "fire-rate")),
        magazine_size: u16_at(m, "magazine"),
        reload_time: num(mget(m, "reload-time")),
        spread: num(mget(m, "spread")),
        damage_falloff: num(mget(m, "damage-falloff")),
        range: num(mget(m, "range")),
        projectile_speed: num(mget(m, "projectile-speed")),
    }
}

fn table<'a>(src: &str, key: &'static str) -> Result<Vec<Map>, BattleRoyaleError> {
    let root = root_map(src).ok_or(BattleRoyaleError::NotAMap)?;
    let entries = mget(&root, key)
        .and_then(|v| v.as_vector())
        .ok_or(BattleRoyaleError::NoTable(key))?;
    Ok(entries.iter().filter_map(|e| e.as_map().cloned()).collect())
}

/// Parse the `:battle-royale/storm-phases` table from EDN `src`.
pub fn storm_phases_from_edn(src: &str) -> Result<Vec<StormPhaseConfig>, BattleRoyaleError> {
    Ok(table(src, "battle-royale/storm-phases")?
        .iter()
        .map(storm_phase_from_map)
        .collect())
}

/// Parse the `:battle-royale/consumables` table from EDN `src`.
pub fn consumables_from_edn(src: &str) -> Result<Vec<ConsumableDef>, BattleRoyaleError> {
    Ok(table(src, "battle-royale/consumables")?
        .iter()
        .map(consumable_from_map)
        .collect())
}

/// Parse the `:battle-royale/weapons` table from EDN `src`.
pub fn weapons_from_edn(src: &str) -> Result<Vec<WeaponDef>, BattleRoyaleError> {
    Ok(table(src, "battle-royale/weapons")?
        .iter()
        .map(weapon_from_map)
        .collect())
}

/// The compiled-in oracle: `default_storm_phases()`.
pub fn builtin_storm_phases() -> Vec<StormPhaseConfig> {
    default_storm_phases()
}
/// The compiled-in oracle: `consumable_pool()`.
pub fn builtin_consumables() -> Vec<ConsumableDef> {
    consumable_pool()
}
/// The compiled-in oracle: `weapon_pool()`.
pub fn builtin_weapons() -> Vec<WeaponDef> {
    weapon_pool()
}

/// Convenience: storm phases from the crate-shipped [`STORM_PHASES_EDN`].
pub fn shipped_storm_phases() -> Result<Vec<StormPhaseConfig>, BattleRoyaleError> {
    storm_phases_from_edn(STORM_PHASES_EDN)
}
/// Convenience: consumables from the crate-shipped [`CONSUMABLES_EDN`].
pub fn shipped_consumables() -> Result<Vec<ConsumableDef>, BattleRoyaleError> {
    consumables_from_edn(CONSUMABLES_EDN)
}
/// Convenience: weapons from the crate-shipped [`WEAPONS_EDN`].
pub fn shipped_weapons() -> Result<Vec<WeaponDef>, BattleRoyaleError> {
    weapons_from_edn(WEAPONS_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_counts() {
        assert_eq!(shipped_storm_phases().unwrap().len(), 8);
        assert_eq!(shipped_consumables().unwrap().len(), 11);
        assert_eq!(shipped_weapons().unwrap().len(), 25);
    }

    #[test]
    fn enum_ids_round_trip() {
        for id in ["common", "uncommon", "rare", "epic", "legendary"] {
            assert_eq!(rarity_id(rarity_from_id(id)), id);
        }
        for id in [
            "small-shield", "large-shield", "mini-hp", "medkit", "chug", "small-fry", "flopper",
            "shield-fish", "grimace-shake", "gyatt-energy", "ohio-milk",
        ] {
            assert_eq!(consumable_type_id(consumable_type_from_id(id)), id);
        }
        for id in [
            "assault-rifle", "shotgun", "smg", "sniper-rifle", "pistol", "rocket-launcher",
            "grenade-launcher",
        ] {
            assert_eq!(weapon_type_id(weapon_type_from_id(id)), id);
        }
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(storm_phases_from_edn("42"), Err(BattleRoyaleError::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            consumables_from_edn("{:x 1}"),
            Err(BattleRoyaleError::NoTable(_))
        ));
    }
}
