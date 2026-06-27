//! kami-cam-scene — EDN authoring surface for `kami-cam` STOCK MATERIAL presets.
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` for the
//! CAM stock library: it turns canonical `:cam/materials` EDN into real
//! [`kami_cam::CamMaterial`] instances, re-using the tolerant [`kami_scene`] accessors
//! the same way games parse `scene.edn` (missing keys fall back, hyphen-keyword ids,
//! ints coerce to floats).
//!
//! ## Why this is safe (ADR-0046 / ADR-0038)
//!
//! A material preset is **init-time CONFIG** — a feed/speed lookup (`density`,
//! `hardness`) read once when a CAM job seeds its `Stock`, never on a per-element hot
//! path — so it is safe to move to EDN. `kami-cam` itself stays edn-free; the EDN
//! dependency lives only here. The compiled-in [`kami_cam::CamMaterial::aluminum_6061`]
//! … presets remain as the [`builtin_material`] fallback and are parity-tested against
//! the shipped EDN ([`MATERIALS_EDN`], `tests/materials_parity.rs`).
//!
//! ## EDN shape (see `data/materials.edn`)
//!
//! ```edn
//! {:cam/materials
//!  {:aluminum-6061 {:name "Aluminum 6061-T6" :density 2.70 :hardness 95.0}
//!   :steel-1045    {:name "Steel 1045"       :density 7.87 :hardness 163.0}
//!   ...}}
//! ```
//!
//! `CamMaterial` derives no `PartialEq`, so the data crate carries a local
//! [`CamMaterialSpec`] `PartialEq` mirror (the same pattern as `kami-postfx-scene` /
//! `kami-character-scene`) used to assert parity.

use std::collections::BTreeMap;

use kami_cam::CamMaterial;
use kami_scene::{EdnValue, kw_key, mget, root_map};

/// The canonical stock-material CONFIG shipped with this crate (the preset table).
/// This is the source of truth; the compiled-in presets are the parity-tested mirror.
pub const MATERIALS_EDN: &str = include_str!("../data/materials.edn");

/// Errors raised while loading stock-material CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("materials EDN root is not a map")]
    NotAMap,
    /// The `:cam/materials` table was missing or not a map.
    #[error("`:cam/materials` missing or not a map")]
    NoMaterials,
    /// The requested material id was missing under `:cam/materials`.
    #[error("material `{0}` not found under `:cam/materials`")]
    MaterialNotFound(String),
}

/// EDN-loaded mirror of [`kami_cam::CamMaterial`] (which derives no `PartialEq`). Used
/// to parity-assert the shipped EDN against the real `CamMaterial::*()` builders.
#[derive(Debug, Clone, PartialEq)]
pub struct CamMaterialSpec {
    /// Human-readable material name (e.g. `"Aluminum 6061-T6"`).
    pub name: String,
    /// Density in g/cm^3.
    pub density: f64,
    /// Brinell hardness (HB).
    pub hardness: f64,
}

impl CamMaterialSpec {
    /// Build the spec from the compiled-in [`CamMaterial`] oracle: read every field
    /// straight off the real engine struct. This is what the EDN is parity-tested
    /// against.
    pub fn from_cam_material(m: &CamMaterial) -> Self {
        Self {
            name: m.name.clone(),
            density: m.density,
            hardness: m.hardness,
        }
    }

    /// Build a spec from one material's EDN map. Tolerant: an absent `:name` → `""`,
    /// an absent / non-numeric `:density` / `:hardness` → `0.0` (the shipped EDN sets
    /// all three, so the parity test pins the real values). Integers coerce to floats.
    pub fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        let name = mget(m, "name")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let f64_or_zero = |key: &str| {
            mget(m, key)
                .and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
                .unwrap_or(0.0)
        };
        Self {
            name,
            density: f64_or_zero("density"),
            hardness: f64_or_zero("hardness"),
        }
    }
}

/// Reconstruct the real [`kami_cam::CamMaterial`] from a [`CamMaterialSpec`] —
/// behaviourally identical to the hardcoded `CamMaterial::aluminum_6061()` … (proven
/// by `tests/materials_parity.rs`).
pub fn spec_to_cam_material(s: &CamMaterialSpec) -> CamMaterial {
    CamMaterial::new(s.name.clone(), s.density, s.hardness)
}

/// The compiled-in fallback / parity oracle: build a [`CamMaterialSpec`] straight from
/// the hardcoded `CamMaterial::*()` builder. Returns `None` for an unknown id. This is
/// what the shipped EDN is parity-tested against.
pub fn builtin_material(id: &str) -> Option<CamMaterialSpec> {
    let m = match id {
        "aluminum-6061" => CamMaterial::aluminum_6061(),
        "steel-1045" => CamMaterial::steel_1045(),
        "titanium-ti6al4v" => CamMaterial::titanium_ti6al4v(),
        "abs-plastic" => CamMaterial::abs_plastic(),
        "wood-oak" => CamMaterial::wood_oak(),
        _ => return None,
    };
    Some(CamMaterialSpec::from_cam_material(&m))
}

/// Ids of the materials shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Kept here, not in `kami-cam`, so the engine crate stays untouched.
pub const ALL_MATERIAL_IDS: [&str; 5] = [
    "aluminum-6061",
    "steel-1045",
    "titanium-ti6al4v",
    "abs-plastic",
    "wood-oak",
];

/// Parse the whole `:cam/materials` table from EDN `src` into a map keyed by the
/// (hyphenated) material id, each value the loaded [`CamMaterialSpec`].
pub fn materials_from_edn(src: &str) -> Result<BTreeMap<String, CamMaterialSpec>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let materials = mget(&root, "cam/materials")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoMaterials)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in materials.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };
        by_id.insert(id, CamMaterialSpec::from_map(m));
    }
    Ok(by_id)
}

/// Look up a single material by (hyphenated) id from EDN `src`, returning the real
/// [`CamMaterial`]. Errors if the table or the named material is absent.
pub fn material_from_edn(src: &str, id: &str) -> Result<CamMaterial, Error> {
    materials_from_edn(src)?
        .get(id)
        .map(spec_to_cam_material)
        .ok_or_else(|| Error::MaterialNotFound(id.to_string()))
}

/// Convenience: load all materials from the crate-shipped [`MATERIALS_EDN`].
pub fn shipped_materials() -> Result<BTreeMap<String, CamMaterialSpec>, Error> {
    materials_from_edn(MATERIALS_EDN)
}

/// Convenience: load one material from the shipped EDN as a real [`CamMaterial`].
pub fn shipped_material(id: &str) -> Result<CamMaterial, Error> {
    material_from_edn(MATERIALS_EDN, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_materials() {
        let m = shipped_materials().expect("materials.edn parses");
        assert_eq!(m.len(), 5);
        for id in ALL_MATERIAL_IDS {
            assert!(m.contains_key(id), "{id} present in EDN");
        }
    }

    #[test]
    fn unknown_builtin_material_is_none() {
        assert!(builtin_material("unobtainium").is_none());
    }

    #[test]
    fn unknown_material_from_edn_is_an_error() {
        assert!(matches!(
            material_from_edn(MATERIALS_EDN, "unobtainium"),
            Err(Error::MaterialNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(materials_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_materials_table_is_an_error() {
        assert!(matches!(
            materials_from_edn("{:other 1}"),
            Err(Error::NoMaterials)
        ));
    }

    #[test]
    fn int_coerces_to_float() {
        // `:density 7` (an int) coerces to 7.0.
        let m = materials_from_edn("{:cam/materials {:x {:name \"X\" :density 7 :hardness 100}}}")
            .unwrap();
        assert_eq!(m["x"].density, 7.0);
        assert_eq!(m["x"].hardness, 100.0);
    }

    #[test]
    fn missing_fields_fall_back() {
        let m = materials_from_edn("{:cam/materials {:bare {:name \"Bare\"}}}").unwrap();
        assert_eq!(m["bare"].name, "Bare");
        assert_eq!(m["bare"].density, 0.0, "absent density → 0");
        assert_eq!(m["bare"].hardness, 0.0, "absent hardness → 0");
    }
}
