//! kami-vehicle-scene — EDN authoring surface for `kami-vehicle` ground CONFIG.
//!
//! The data-tier counterpart of `kami-live` for the car-sim: it turns canonical
//! `:ground/*` EDN (the surface grip table + the demo-circuit zone map) into the
//! real `kami_vehicle` engine structs, re-using the tolerant `kami-scene`
//! accessors the same way games parse `scene.edn` (missing keys fall back to
//! defaults, namespaced keywords match on `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot physics stays native Rust (`kami-vehicle`). The surface coefficients and
//! the map zones are **init-time CONFIG** — read once at load, never touched by
//! the 2 kHz solver — so they are safe to move to EDN. `kami-vehicle` itself
//! stays "pure Rust + glam + serde, no edn dep"; the EDN dependency lives only
//! here. The compiled-in [`SurfaceKind`] enum + [`MapGround::demo_circuit`]
//! remain as the [`SurfaceTable::builtin`] fallback and are parity-tested
//! against the shipped EDN ([`crate::GROUND_EDN`]).
//!
//! ## EDN shape (see `data/ground.edn`)
//!
//! ```edn
//! {:ground/surfaces
//!  {:asphalt-dry {:friction-mu 1.00 :grip-modifier 1.00 :tint [0.20 0.20 0.22] :name "Dry Asphalt"}
//!   ...all 8...}
//!  :ground/map
//!  {:demo-circuit
//!   {:default :grass
//!    :zones [{:x-min -4.0 :x-max 4.0 :z-min -100.0 :z-max 100.0 :surface :asphalt-dry} ...]}}}
//! ```
//!
//! ## Hyphen ↔ underscore
//!
//! Surface ids are authored with hyphens (`:asphalt-dry`) — idiomatic EDN — but
//! [`kami_vehicle::SurfaceKind::from_id`] expects underscores (`asphalt_dry`).
//! The loader replaces `'-'` with `'_'` before calling `from_id` (see
//! [`surface_id_from_kw`]). Unknown ids fall back to `AsphaltDry`, mirroring
//! `from_id`.

use std::collections::BTreeMap;

use kami_scene::{mget, num, root_map, vec3, EdnValue};
use kami_vehicle::{MapGround, SurfaceKind, SurfaceZone};

mod garage;
pub use garage::{
    build_from_edn, build_from_spec, builtin_engine, builtin_gearbox, builtin_tire,
    differential_from_id, engines_from_edn, garage_from_edn, gearboxes_from_edn, shipped_engines,
    shipped_garage, tires_from_edn, EngineSpec, GarageSpec, GearboxSpec, LayoutSpec, TireSpec,
    ALL_VEHICLE_KINDS, GARAGE_EDN,
};

/// The canonical ground CONFIG shipped with this crate (surface table + maps).
/// This is the source of truth; the compiled-in enum is the parity-tested mirror.
pub const GROUND_EDN: &str = include_str!("../data/ground.edn");

/// Errors raised while loading ground CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("ground EDN root is not a map")]
    NotAMap,
    /// The `:ground/surfaces` table was missing or not a map.
    #[error("`:ground/surfaces` missing or not a map")]
    NoSurfaces,
    /// The requested map id was missing under `:ground/map`.
    #[error("map `{0}` not found under `:ground/map`")]
    MapNotFound(String),
    /// A `:vehicle/*` table was missing or not a map.
    #[error("`:vehicle/{0}` missing or not a map")]
    NoTable(&'static str),
}

/// The four parameters that describe one ground surface — the EDN-loaded mirror
/// of [`SurfaceKind`]'s `coefficients()` / `tint()` / `display_name()`.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceParams {
    /// Coulomb friction multiplier (1.0 = dry-asphalt baseline).
    pub friction_mu: f32,
    /// Pacejka peak-grip modifier (1.0 = dry).
    pub grip_modifier: f32,
    /// Visual tint for renderer overlays (RGB 0-1).
    pub tint: [f32; 3],
    /// Human-readable display name.
    pub name: String,
}

impl SurfaceParams {
    /// Build from the compiled-in enum (the `builtin()` fallback source).
    pub fn from_kind(kind: SurfaceKind) -> Self {
        let (friction_mu, grip_modifier) = kind.coefficients();
        Self {
            friction_mu,
            grip_modifier,
            tint: kind.tint(),
            name: kind.display_name().to_string(),
        }
    }
}

/// A lookup table from surface id (underscore form, e.g. `"asphalt_dry"`) to its
/// [`SurfaceParams`]. Loaded from EDN, or constructed from the compiled-in enum.
#[derive(Debug, Clone, Default)]
pub struct SurfaceTable {
    by_id: BTreeMap<String, SurfaceParams>,
}

impl SurfaceTable {
    /// The compiled-in fallback: every [`SurfaceKind`] variant's hardcoded
    /// coefficients / tint / name. This is what the EDN is parity-tested against.
    pub fn builtin() -> SurfaceTable {
        let mut by_id = BTreeMap::new();
        for kind in ALL_SURFACE_KINDS {
            by_id.insert(kind.id().to_string(), SurfaceParams::from_kind(kind));
        }
        SurfaceTable { by_id }
    }

    /// Parse a `SurfaceTable` from the `:ground/surfaces` map in EDN `src`.
    pub fn from_edn(src: &str) -> Result<SurfaceTable, Error> {
        let root = root_map(src).ok_or(Error::NotAMap)?;
        let surfaces = mget(&root, "ground/surfaces")
            .and_then(|v| v.as_map())
            .ok_or(Error::NoSurfaces)?;

        let mut by_id = BTreeMap::new();
        for (k, v) in surfaces.iter() {
            let Some(id) = surface_id_from_kw(k) else {
                continue;
            };
            let Some(m) = v.as_map() else { continue };
            // Default the display name to the id when absent.
            let name = mget(m, "name")
                .and_then(|x| x.as_string())
                .map(|s| s.to_string())
                .unwrap_or_else(|| id.clone());
            by_id.insert(
                id,
                SurfaceParams {
                    friction_mu: num(mget(m, "friction-mu")),
                    grip_modifier: num(mget(m, "grip-modifier")),
                    tint: vec3(mget(m, "tint")),
                    name,
                },
            );
        }
        Ok(SurfaceTable { by_id })
    }

    /// Look up by underscore id string (`"asphalt_dry"`). Falls back to the
    /// `AsphaltDry` entry (mirroring `from_id`) when the id is absent.
    pub fn get_by_id(&self, id: &str) -> SurfaceParams {
        self.by_id
            .get(id)
            .cloned()
            .or_else(|| self.by_id.get(SurfaceKind::AsphaltDry.id()).cloned())
            .unwrap_or_else(|| SurfaceParams::from_kind(SurfaceKind::AsphaltDry))
    }

    /// Look up the params for a [`SurfaceKind`].
    pub fn get(&self, kind: SurfaceKind) -> SurfaceParams {
        self.get_by_id(kind.id())
    }

    /// Number of surface entries.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// True when the table holds no entries.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// All [`SurfaceKind`] variants — the iteration source for `builtin()` and the
/// parity tests. Keeping this list here (not in `kami-vehicle`) keeps the engine
/// crate untouched.
pub const ALL_SURFACE_KINDS: [SurfaceKind; 8] = [
    SurfaceKind::AsphaltDry,
    SurfaceKind::AsphaltWet,
    SurfaceKind::Gravel,
    SurfaceKind::Sand,
    SurfaceKind::Snow,
    SurfaceKind::Ice,
    SurfaceKind::Mud,
    SurfaceKind::Grass,
];

/// Resolve a surface keyword KEY (or VALUE) to its underscore id string.
/// `:asphalt-dry` → `"asphalt_dry"`. Returns `None` for non-keywords.
pub fn surface_id_from_kw(k: &EdnValue) -> Option<String> {
    k.as_keyword().map(|kw| kw.0.name.replace('-', "_"))
}

/// Resolve a surface keyword VALUE (e.g. `:ice`) to a [`SurfaceKind`], mapping
/// hyphens to underscores first. Unknown / non-keyword → `AsphaltDry`.
pub fn surface_kind_from_value(v: &EdnValue) -> SurfaceKind {
    surface_id_from_kw(v)
        .map(|id| SurfaceKind::from_id(&id))
        .unwrap_or(SurfaceKind::AsphaltDry)
}

/// Build a real [`MapGround`] from the `:ground/map :<map-id>` EDN in `src`.
///
/// `map_id` is the hyphenated keyword name as authored (e.g. `"demo-circuit"`).
/// `:default` and each zone's `:surface` are resolved through
/// [`surface_kind_from_value`] (hyphen → underscore → `from_id`).
pub fn map_from_edn(src: &str, map_id: &str) -> Result<MapGround, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let maps = mget(&root, "ground/map")
        .and_then(|v| v.as_map())
        .ok_or_else(|| Error::MapNotFound(map_id.to_string()))?;

    // `mget` matches on the keyword's `ns/name`; the map ids are bare keywords
    // (`:demo-circuit`) so a bare lookup string resolves them.
    let map = mget(maps, map_id)
        .and_then(|v| v.as_map())
        .ok_or_else(|| Error::MapNotFound(map_id.to_string()))?;

    let default = mget(map, "default")
        .map(surface_kind_from_value)
        .unwrap_or(SurfaceKind::AsphaltDry);

    let mut zones = Vec::new();
    for z in mget(map, "zones").and_then(|v| v.as_vector()).unwrap_or(&[]) {
        let Some(zm) = z.as_map() else { continue };
        zones.push(SurfaceZone {
            x_min: num(mget(zm, "x-min")),
            x_max: num(mget(zm, "x-max")),
            z_min: num(mget(zm, "z-min")),
            z_max: num(mget(zm, "z-max")),
            surface: mget(zm, "surface")
                .map(surface_kind_from_value)
                .unwrap_or(default),
        });
    }

    Ok(MapGround { default, zones })
}

/// Convenience: load the [`SurfaceTable`] from the crate-shipped [`GROUND_EDN`].
pub fn shipped_surface_table() -> Result<SurfaceTable, Error> {
    SurfaceTable::from_edn(GROUND_EDN)
}

/// Convenience: load the demo-circuit [`MapGround`] from the shipped EDN.
pub fn shipped_demo_circuit() -> Result<MapGround, Error> {
    map_from_edn(GROUND_EDN, "demo-circuit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_table_has_all_eight() {
        let t = SurfaceTable::builtin();
        assert_eq!(t.len(), 8);
        for kind in ALL_SURFACE_KINDS {
            let p = t.get(kind);
            assert_eq!(p.name, kind.display_name());
        }
    }

    #[test]
    fn unknown_surface_id_falls_back_to_asphalt_dry() {
        let t = SurfaceTable::builtin();
        let p = t.get_by_id("does_not_exist");
        assert_eq!(p, SurfaceParams::from_kind(SurfaceKind::AsphaltDry));
    }

    #[test]
    fn hyphen_keyword_maps_to_underscore_id() {
        // a keyword VALUE :asphalt-dry → SurfaceKind::AsphaltDry
        let m = root_map("{:s :asphalt-dry}").unwrap();
        let v = mget(&m, "s").unwrap();
        assert_eq!(surface_kind_from_value(v), SurfaceKind::AsphaltDry);
        assert_eq!(surface_id_from_kw(v).as_deref(), Some("asphalt_dry"));
    }

    #[test]
    fn missing_map_is_an_error() {
        let err = map_from_edn(GROUND_EDN, "nope").unwrap_err();
        assert!(matches!(err, Error::MapNotFound(_)));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(SurfaceTable::from_edn("42"), Err(Error::NotAMap)));
        assert!(matches!(map_from_edn("42", "demo-circuit"), Err(Error::NotAMap)));
    }
}
