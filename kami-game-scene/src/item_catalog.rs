//! Item-catalog data tier — kami-game's Sabiotoshi (rust-restoration game) item catalog
//! (`sabiotoshi::default_item_catalog()`) as parity-tested EDN.
//!
//! The restoration simulation (rust removal, tool effectiveness, scoring) stays native
//! Rust; only the init-time **description** — the item table (SDF model, rust zones,
//! disassembly steps, PBR metal, CPC/UNSPSC codes) — moves to EDN (ADR-0046 / ADR-0038).
//! [`items_from_edn`] rebuilds the real [`kami_game::sabiotoshi::RestorableItem`] list,
//! asserted item-for-item `==` the compiled-in `default_item_catalog()` in
//! `tests/item_catalog_parity.rs`.
//!
//! `RestorableItem` / `RustZone3D` / `DisassemblyStep` derive no `PartialEq` (and hold
//! `glam::Vec3`), so the data tier carries `PartialEq` spec mirrors with `[f32; 3]` coords.

use std::collections::BTreeMap;

use glam::Vec3;
use kami_game::sabiotoshi::{
    default_item_catalog, DisassemblyStep, RestorableItem, RustType, RustZone3D, ToolKind,
};
use kami_scene::{kw_key, mget, num, root_map, vec3, EdnValue};

/// The canonical item-catalog CONFIG shipped with this crate.
pub const ITEM_CATALOG_EDN: &str = include_str!("../data/item_catalog.edn");

/// Errors raised while loading the item catalog from EDN.
#[derive(Debug, thiserror::Error)]
pub enum ItemCatalogError {
    /// The EDN source did not parse to a top-level map.
    #[error("item-catalog EDN root is not a map")]
    NotAMap,
    /// The `:game/item-catalog` table was missing or not a vector.
    #[error("`:game/item-catalog` missing or not a vector")]
    NoTable,
}

type Map = BTreeMap<EdnValue, EdnValue>;

// ── enum id maps ─────────────────────────────────────────────────────────────────────

/// The hyphenated keyword id for a [`RustType`] variant.
pub fn rust_type_id(t: RustType) -> &'static str {
    match t {
        RustType::Surface => "surface",
        RustType::Deep => "deep",
        RustType::Pitted => "pitted",
        RustType::Patina => "patina",
    }
}
/// Inverse of [`rust_type_id`]; unknown → `Surface` (tolerant).
pub fn rust_type_from_id(id: &str) -> RustType {
    match id {
        "deep" => RustType::Deep,
        "pitted" => RustType::Pitted,
        "patina" => RustType::Patina,
        _ => RustType::Surface,
    }
}

/// The hyphenated keyword id for a [`ToolKind`] variant.
pub fn tool_kind_id(t: ToolKind) -> &'static str {
    match t {
        ToolKind::PressureWasher => "pressure-washer",
        ToolKind::WireBrush => "wire-brush",
        ToolKind::Sandpaper => "sandpaper",
        ToolKind::ChemicalSolvent => "chemical-solvent",
        ToolKind::PolishingCloth => "polishing-cloth",
        ToolKind::Ultrasonic => "ultrasonic",
    }
}
/// Inverse of [`tool_kind_id`]; unknown → `PressureWasher` (tolerant).
pub fn tool_kind_from_id(id: &str) -> ToolKind {
    match id {
        "wire-brush" => ToolKind::WireBrush,
        "sandpaper" => ToolKind::Sandpaper,
        "chemical-solvent" => ToolKind::ChemicalSolvent,
        "polishing-cloth" => ToolKind::PolishingCloth,
        "ultrasonic" => ToolKind::Ultrasonic,
        _ => ToolKind::PressureWasher,
    }
}

// ── PartialEq spec mirrors ───────────────────────────────────────────────────────────

/// PartialEq mirror of [`RustZone3D`] (Vec3 → `[f32; 3]`, enum → id).
#[derive(Debug, Clone, PartialEq)]
pub struct RustZoneSpec {
    pub id: String,
    pub center: [f32; 3],
    pub extent: [f32; 3],
    pub rust_type: String,
    pub initial_level: f32,
    pub current_level: f32,
    pub nerf_grid_idx: Option<usize>,
}

/// PartialEq mirror of [`DisassemblyStep`].
#[derive(Debug, Clone, PartialEq)]
pub struct DisassemblyStepSpec {
    pub id: String,
    pub name: String,
    pub name_ja: String,
    pub part_ids: Vec<String>,
    pub revealed_zones: Vec<String>,
    pub completed: bool,
    pub required_tool: Option<String>,
    pub detach_offset: [f32; 3],
}

/// PartialEq mirror of [`RestorableItem`].
#[derive(Debug, Clone, PartialEq)]
pub struct ItemSpec {
    pub id: String,
    pub name: String,
    pub name_ja: String,
    pub difficulty: u8,
    pub base_score: u32,
    pub perfect_bonus: u32,
    pub sdf_desc: String,
    pub entity_ids: Vec<String>,
    pub zones: Vec<RustZoneSpec>,
    pub disassembly_steps: Vec<DisassemblyStepSpec>,
    pub cpc_code: String,
    pub unspsc_code: String,
    pub metal_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
}

// ── tolerant readers ─────────────────────────────────────────────────────────────────

fn str_at(m: &Map, key: &str) -> String {
    mget(m, key).and_then(|v| v.as_string()).unwrap_or("").to_string()
}
fn int_at(m: &Map, key: &str) -> i64 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0)
}
fn strings(m: &Map, key: &str) -> Vec<String> {
    mget(m, key)
        .and_then(|v| v.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| v.as_string().map(str::to_string))
        .collect()
}
fn maps_at<'a>(m: &'a Map, key: &str) -> Vec<&'a Map> {
    mget(m, key)
        .and_then(|v| v.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| v.as_map())
        .collect()
}

// ── spec ← oracle (real engine struct) ───────────────────────────────────────────────

impl RustZoneSpec {
    pub fn from_zone(z: &RustZone3D) -> Self {
        Self {
            id: z.id.clone(),
            center: z.center.to_array(),
            extent: z.extent.to_array(),
            rust_type: rust_type_id(z.rust_type).to_string(),
            initial_level: z.initial_level,
            current_level: z.current_level,
            nerf_grid_idx: z.nerf_grid_idx,
        }
    }
    pub fn from_map(m: &Map) -> Self {
        let initial = num(mget(m, "initial-level"));
        Self {
            id: str_at(m, "id"),
            center: vec3(mget(m, "center")),
            extent: vec3(mget(m, "extent")),
            rust_type: mget(m, "rust-type").and_then(kw_key).unwrap_or_default(),
            initial_level: initial,
            // current-level defaults to initial-level (matches the builtin).
            current_level: mget(m, "current-level").map(|v| num(Some(v))).unwrap_or(initial),
            nerf_grid_idx: mget(m, "nerf")
                .and_then(|v| v.as_integer())
                .map(|i| i.max(0) as usize),
        }
    }
}

impl DisassemblyStepSpec {
    pub fn from_step(s: &DisassemblyStep) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            name_ja: s.name_ja.clone(),
            part_ids: s.part_ids.clone(),
            revealed_zones: s.revealed_zones.clone(),
            completed: s.completed,
            required_tool: s.required_tool.map(|t| tool_kind_id(t).to_string()),
            detach_offset: s.detach_offset.to_array(),
        }
    }
    pub fn from_map(m: &Map) -> Self {
        Self {
            id: str_at(m, "id"),
            name: str_at(m, "name"),
            name_ja: str_at(m, "name-ja"),
            part_ids: strings(m, "part-ids"),
            revealed_zones: strings(m, "revealed-zones"),
            completed: mget(m, "completed").and_then(|v| v.as_bool()).unwrap_or(false),
            required_tool: mget(m, "required-tool").and_then(kw_key),
            detach_offset: vec3(mget(m, "detach-offset")),
        }
    }
}

impl ItemSpec {
    pub fn from_item(it: &RestorableItem) -> Self {
        Self {
            id: it.id.clone(),
            name: it.name.clone(),
            name_ja: it.name_ja.clone(),
            difficulty: it.difficulty,
            base_score: it.base_score,
            perfect_bonus: it.perfect_bonus,
            sdf_desc: it.sdf_desc.clone(),
            entity_ids: it.entity_ids.clone(),
            zones: it.zones.iter().map(RustZoneSpec::from_zone).collect(),
            disassembly_steps: it
                .disassembly_steps
                .iter()
                .map(DisassemblyStepSpec::from_step)
                .collect(),
            cpc_code: it.cpc_code.clone(),
            unspsc_code: it.unspsc_code.clone(),
            metal_color: it.metal_color,
            metallic: it.metallic,
            roughness: it.roughness,
        }
    }
    pub fn from_map(m: &Map) -> Self {
        Self {
            id: str_at(m, "id"),
            name: str_at(m, "name"),
            name_ja: str_at(m, "name-ja"),
            difficulty: int_at(m, "difficulty").clamp(0, 255) as u8,
            base_score: int_at(m, "base-score").max(0) as u32,
            perfect_bonus: int_at(m, "perfect-bonus").max(0) as u32,
            sdf_desc: str_at(m, "sdf-desc"),
            entity_ids: strings(m, "entity-ids"),
            zones: maps_at(m, "zones").into_iter().map(RustZoneSpec::from_map).collect(),
            disassembly_steps: maps_at(m, "disassembly-steps")
                .into_iter()
                .map(DisassemblyStepSpec::from_map)
                .collect(),
            cpc_code: str_at(m, "cpc-code"),
            unspsc_code: str_at(m, "unspsc-code"),
            metal_color: vec3(mget(m, "metal-color")),
            metallic: num(mget(m, "metallic")),
            roughness: num(mget(m, "roughness")),
        }
    }
}

// ── spec → real engine struct ────────────────────────────────────────────────────────

fn spec_to_zone(s: &RustZoneSpec) -> RustZone3D {
    RustZone3D {
        id: s.id.clone(),
        center: Vec3::from_array(s.center),
        extent: Vec3::from_array(s.extent),
        rust_type: rust_type_from_id(&s.rust_type),
        initial_level: s.initial_level,
        current_level: s.current_level,
        nerf_grid_idx: s.nerf_grid_idx,
    }
}
fn spec_to_step(s: &DisassemblyStepSpec) -> DisassemblyStep {
    DisassemblyStep {
        id: s.id.clone(),
        name: s.name.clone(),
        name_ja: s.name_ja.clone(),
        part_ids: s.part_ids.clone(),
        revealed_zones: s.revealed_zones.clone(),
        completed: s.completed,
        required_tool: s.required_tool.as_deref().map(tool_kind_from_id),
        detach_offset: Vec3::from_array(s.detach_offset),
    }
}
/// Reconstruct the real [`RestorableItem`] from an [`ItemSpec`].
pub fn spec_to_item(s: &ItemSpec) -> RestorableItem {
    RestorableItem {
        id: s.id.clone(),
        name: s.name.clone(),
        name_ja: s.name_ja.clone(),
        difficulty: s.difficulty,
        base_score: s.base_score,
        perfect_bonus: s.perfect_bonus,
        sdf_desc: s.sdf_desc.clone(),
        entity_ids: s.entity_ids.clone(),
        zones: s.zones.iter().map(spec_to_zone).collect(),
        disassembly_steps: s.disassembly_steps.iter().map(spec_to_step).collect(),
        cpc_code: s.cpc_code.clone(),
        unspsc_code: s.unspsc_code.clone(),
        metal_color: s.metal_color,
        metallic: s.metallic,
        roughness: s.roughness,
    }
}

// ── public API ───────────────────────────────────────────────────────────────────────

/// Parse the `:game/item-catalog` table from EDN `src` into ordered [`ItemSpec`]s.
pub fn item_specs_from_edn(src: &str) -> Result<Vec<ItemSpec>, ItemCatalogError> {
    let root = root_map(src).ok_or(ItemCatalogError::NotAMap)?;
    let items = mget(&root, "game/item-catalog")
        .and_then(|v| v.as_vector())
        .ok_or(ItemCatalogError::NoTable)?;
    Ok(items
        .iter()
        .filter_map(|e| e.as_map().map(ItemSpec::from_map))
        .collect())
}

/// Parse the table from EDN `src` into the real [`RestorableItem`] list.
pub fn items_from_edn(src: &str) -> Result<Vec<RestorableItem>, ItemCatalogError> {
    Ok(item_specs_from_edn(src)?.iter().map(spec_to_item).collect())
}

/// The compiled-in oracle: `default_item_catalog()` projected into specs.
pub fn builtin_item_specs() -> Vec<ItemSpec> {
    default_item_catalog().iter().map(ItemSpec::from_item).collect()
}

/// Convenience: the items from the crate-shipped [`ITEM_CATALOG_EDN`].
pub fn shipped_items() -> Result<Vec<RestorableItem>, ItemCatalogError> {
    items_from_edn(ITEM_CATALOG_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_four_items() {
        let specs = item_specs_from_edn(ITEM_CATALOG_EDN).expect("item_catalog.edn parses");
        assert_eq!(specs.len(), 4);
        assert_eq!(specs.len(), builtin_item_specs().len());
        let zones: usize = specs.iter().map(|i| i.zones.len()).sum();
        assert_eq!(zones, 15, "15 rust zones across the catalog");
    }

    #[test]
    fn enum_ids_round_trip() {
        for id in ["surface", "deep", "pitted", "patina"] {
            assert_eq!(rust_type_id(rust_type_from_id(id)), id);
        }
        for id in ["pressure-washer", "wire-brush", "sandpaper", "chemical-solvent", "polishing-cloth", "ultrasonic"] {
            assert_eq!(tool_kind_id(tool_kind_from_id(id)), id);
        }
    }

    #[test]
    fn current_level_defaults_to_initial() {
        let specs = item_specs_from_edn(ITEM_CATALOG_EDN).unwrap();
        for it in &specs {
            for z in &it.zones {
                assert_eq!(z.current_level, z.initial_level, "{}/{}", it.id, z.id);
            }
        }
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(item_specs_from_edn("42"), Err(ItemCatalogError::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            item_specs_from_edn("{:x 1}"),
            Err(ItemCatalogError::NoTable)
        ));
    }
}
