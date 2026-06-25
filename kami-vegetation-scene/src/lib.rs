//! kami-vegetation-scene — EDN authoring surface for `kami-vegetation`
//! TAXONOMIC PROFILE presets.
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` /
//! `kami-terrain-scene` for the vegetation taxonomy system: it turns canonical
//! `:vegetation/profiles` EDN into the real
//! [`kami_vegetation::taxonomy::TaxonomicProfile`] engine struct, re-using the
//! tolerant `kami-scene` accessors the same way games parse `scene.edn` (missing
//! keys fall back to defaults, namespaced keywords match on `ns/name`, ints coerce
//! to floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot mesh generation / Poisson-disk placement / WASM cull stays native Rust
//! (`kami-vegetation`). A taxonomic profile is **init-time CONFIG** — read once when
//! a species mesh is generated (`mesh_from_profile(&TaxonomicProfile)` switches on
//! [`CanopyShape`] parameterized by `leaf_count` / `leaf_size` / `stem_radius`) — so
//! it is safe to move to EDN. `kami-vegetation` itself stays untouched; the EDN
//! dependency lives only here. The compiled-in
//! `taxonomy::{grass,fern,palm,conifer,bush,cactus,moss}()` builders remain as the
//! [`builtin_profile`] fallback and are parity-tested against the shipped EDN
//! ([`crate::VEGETATION_EDN`]).
//!
//! ## EDN shape (see `data/vegetation.edn`)
//!
//! ```edn
//! {:vegetation/profiles
//!  {:grass {:canopy :blade :stem-radius-base 0.0 :stem-radius-top 0.0
//!           :leaf-count 3 :leaf-size 0.18 :height-range [0.7 1.4]
//!           :color-base [r g b] :color-tip [r g b]
//!           :common-name "grass" :division :angiospermae :habit :grass
//!           :arrangement :basal :leaf-shape :linear}
//!   :fern {...} :palm {...} :conifer {...} :bush {...} :cactus {...} :moss {...}}}
//! ```
//!
//! Enum-valued fields are keyword ids mapped to the matching Rust variant via
//! [`canopy_from_id`], [`division_from_id`], [`habit_from_id`],
//! [`arrangement_from_id`], [`leaf_shape_from_id`].

use std::collections::BTreeMap;

use kami_scene::{mget, num, root_map, vec3, EdnValue};
use kami_vegetation::taxonomy::{
    bush, cactus, conifer, fern, grass, moss, palm, CanopyShape, Division, GrowthHabit,
    LeafArrangement, LeafShape, TaxonomicProfile,
};

/// The canonical taxonomic-profile CONFIG shipped with this crate (the preset table).
/// This is the source of truth; the compiled-in profiles are the parity-tested mirror.
pub const VEGETATION_EDN: &str = include_str!("../data/vegetation.edn");

/// Names of the profiles shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Keeping this list here (not in `kami-vegetation`) keeps the
/// engine crate untouched. Order mirrors `default_catalog()` declaration order.
pub const ALL_PROFILE_NAMES: [&str; 7] =
    ["grass", "fern", "palm", "conifer", "bush", "cactus", "moss"];

/// Errors raised while loading taxonomic-profile CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("vegetation EDN root is not a map")]
    NotAMap,
    /// The `:vegetation/profiles` table was missing or not a map.
    #[error("`:vegetation/profiles` missing or not a map")]
    NoProfiles,
    /// The requested profile id was missing under `:vegetation/profiles`.
    #[error("profile `{0}` not found under `:vegetation/profiles`")]
    ProfileNotFound(String),
}

// ── enum id ↔ variant maps (hyphenated keyword ids → Rust variants) ──
//
// kami-vegetation has the equivalent `parse_*` fns but they are private; mirroring
// them here keeps the engine crate untouched. The default arm matches the engine's
// (so unknown / absent ids inherit the same fallback variant the JSON bridge uses).

/// Map a `:canopy` keyword id to [`CanopyShape`] (default `Carpet`, as in the engine).
pub fn canopy_from_id(s: &str) -> CanopyShape {
    match s {
        "blade" => CanopyShape::Blade,
        "fan" => CanopyShape::Fan,
        "dome" => CanopyShape::Dome,
        "cone" => CanopyShape::Cone,
        "radial" => CanopyShape::Radial,
        "column" => CanopyShape::Column,
        _ => CanopyShape::Carpet,
    }
}

/// The hyphenated keyword id for a [`CanopyShape`] (inverse of [`canopy_from_id`]).
pub fn id_from_canopy(c: CanopyShape) -> &'static str {
    match c {
        CanopyShape::Blade => "blade",
        CanopyShape::Fan => "fan",
        CanopyShape::Dome => "dome",
        CanopyShape::Cone => "cone",
        CanopyShape::Radial => "radial",
        CanopyShape::Column => "column",
        CanopyShape::Carpet => "carpet",
    }
}

/// Map a `:division` keyword id to [`Division`] (default `Angiospermae`, as in the engine).
pub fn division_from_id(s: &str) -> Division {
    match s {
        "bryophyta" => Division::Bryophyta,
        "pteridophyta" => Division::Pteridophyta,
        "gymnospermae" => Division::Gymnospermae,
        _ => Division::Angiospermae,
    }
}

/// Map a `:habit` keyword id to [`GrowthHabit`] (default `Herb`, as in the engine).
pub fn habit_from_id(s: &str) -> GrowthHabit {
    match s {
        "grass" => GrowthHabit::Grass,
        "shrub" => GrowthHabit::Shrub,
        "tree" => GrowthHabit::Tree,
        "succulent" => GrowthHabit::Succulent,
        "mat" => GrowthHabit::Mat,
        "climber" => GrowthHabit::Climber,
        _ => GrowthHabit::Herb,
    }
}

/// Map an `:arrangement` keyword id to [`LeafArrangement`] (default `None`, as in the engine).
pub fn arrangement_from_id(s: &str) -> LeafArrangement {
    match s {
        "alternate" => LeafArrangement::Alternate,
        "opposite" => LeafArrangement::Opposite,
        "whorled" => LeafArrangement::Whorled,
        "rosette" => LeafArrangement::Rosette,
        "basal" => LeafArrangement::Basal,
        _ => LeafArrangement::None,
    }
}

/// Map a `:leaf-shape` keyword id to [`LeafShape`] (default `Scale`, as in the engine).
pub fn leaf_shape_from_id(s: &str) -> LeafShape {
    match s {
        "linear" => LeafShape::Linear,
        "lanceolate" => LeafShape::Lanceolate,
        "ovate" => LeafShape::Ovate,
        "palmate" => LeafShape::Palmate,
        "pinnate" => LeafShape::Pinnate,
        "needle" => LeafShape::Needle,
        "succulent" => LeafShape::Succulent,
        _ => LeafShape::Scale,
    }
}

/// Resolve a runtime profile name to the matching `&'static str` `common_name` that
/// [`TaxonomicProfile`] requires (its `common_name` field is `&'static str`). Known
/// names map to a static literal; anything else falls back to `""`.
fn static_common_name(s: &str) -> &'static str {
    match s {
        "grass" => "grass",
        "fern" => "fern",
        "palm" => "palm",
        "conifer" => "conifer",
        "bush" => "bush",
        "cactus" => "cactus",
        "moss" => "moss",
        _ => "",
    }
}

/// One taxonomic profile — the EDN-loaded mirror of a hardcoded
/// `taxonomy::{grass,…,moss}()` builder. A field absent from the EDN keeps the
/// [`ProfileSpec::defaults`] (the engine `moss()` fallback used by the JSON bridge),
/// so this is a *full* (merged) spec.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSpec {
    /// Human-readable species name — `TaxonomicProfile.common_name`.
    pub common_name: String,
    /// Plant division — `TaxonomicProfile.division`.
    pub division: Division,
    /// Growth habit — `TaxonomicProfile.habit`.
    pub habit: GrowthHabit,
    /// Leaf arrangement — `TaxonomicProfile.arrangement`.
    pub arrangement: LeafArrangement,
    /// Leaf unit shape — `TaxonomicProfile.leaf_shape`.
    pub leaf_shape: LeafShape,
    /// Overall foliage silhouette — `TaxonomicProfile.canopy`.
    pub canopy: CanopyShape,
    /// Height range `[min, max]` — `TaxonomicProfile.height_range`.
    pub height_range: [f32; 2],
    /// Stem/trunk radius at base — `TaxonomicProfile.stem_radius_base`.
    pub stem_radius_base: f32,
    /// Stem radius at top — `TaxonomicProfile.stem_radius_top`.
    pub stem_radius_top: f32,
    /// Number of leaf units / blades / fronds — `TaxonomicProfile.leaf_count`.
    pub leaf_count: u32,
    /// Per-leaf size — `TaxonomicProfile.leaf_size`.
    pub leaf_size: f32,
    /// Base RGB colour — `TaxonomicProfile.color_base`.
    pub color_base: [f32; 3],
    /// Tip RGB colour — `TaxonomicProfile.color_tip`.
    pub color_tip: [f32; 3],
}

impl ProfileSpec {
    /// Read every field off a real [`TaxonomicProfile`] (the parity oracle / default base).
    pub fn from_profile(p: &TaxonomicProfile) -> Self {
        Self {
            common_name: p.common_name.to_string(),
            division: p.division,
            habit: p.habit,
            arrangement: p.arrangement,
            leaf_shape: p.leaf_shape,
            canopy: p.canopy,
            height_range: p.height_range,
            stem_radius_base: p.stem_radius_base,
            stem_radius_top: p.stem_radius_top,
            leaf_count: p.leaf_count,
            leaf_size: p.leaf_size,
            color_base: p.color_base,
            color_tip: p.color_tip,
        }
    }

    /// The default sub-spec: every field read from the engine `moss()` builder — the
    /// same profile the JSON bridge falls back to for unknown enum ids. (Used as the
    /// merge base so an EDN profile that omits a field inherits a real engine value.)
    pub fn defaults() -> Self {
        Self::from_profile(&moss())
    }

    /// Build from one profile's EDN map, merging present keys onto [`ProfileSpec::defaults`].
    pub fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        let d = Self::defaults();
        let or_f = |key: &str, fallback: f32| match mget(m, key) {
            Some(v) => num(Some(v)),
            None => fallback,
        };
        let id = |key: &str| mget(m, key).and_then(kami_scene::kw_key);
        Self {
            common_name: mget(m, "common-name")
                .and_then(|v| v.as_string().map(|s| s.to_string()))
                .unwrap_or(d.common_name),
            division: match id("division") {
                Some(s) => division_from_id(&s),
                None => d.division,
            },
            habit: match id("habit") {
                Some(s) => habit_from_id(&s),
                None => d.habit,
            },
            arrangement: match id("arrangement") {
                Some(s) => arrangement_from_id(&s),
                None => d.arrangement,
            },
            leaf_shape: match id("leaf-shape") {
                Some(s) => leaf_shape_from_id(&s),
                None => d.leaf_shape,
            },
            canopy: match id("canopy") {
                Some(s) => canopy_from_id(&s),
                None => d.canopy,
            },
            height_range: match mget(m, "height-range").and_then(|v| v.as_vector()) {
                Some(rows) => [
                    num(rows.get(0)),
                    num(rows.get(1)),
                ],
                None => d.height_range,
            },
            stem_radius_base: or_f("stem-radius-base", d.stem_radius_base),
            stem_radius_top: or_f("stem-radius-top", d.stem_radius_top),
            leaf_count: match mget(m, "leaf-count") {
                Some(v) => num(Some(v)).round() as u32,
                None => d.leaf_count,
            },
            leaf_size: or_f("leaf-size", d.leaf_size),
            color_base: match mget(m, "color-base") {
                Some(_) => vec3(mget(m, "color-base")),
                None => d.color_base,
            },
            color_tip: match mget(m, "color-tip") {
                Some(_) => vec3(mget(m, "color-tip")),
                None => d.color_tip,
            },
        }
    }

    /// Convert into the real [`TaxonomicProfile`] — behaviourally identical to the
    /// hardcoded `taxonomy::{grass,…,moss}()` builder. `common_name` is resolved to a
    /// `&'static str` via the known-name table (the field requires `&'static str`).
    pub fn to_taxonomic_profile(&self) -> TaxonomicProfile {
        TaxonomicProfile {
            common_name: static_common_name(&self.common_name),
            division: self.division,
            habit: self.habit,
            arrangement: self.arrangement,
            leaf_shape: self.leaf_shape,
            canopy: self.canopy,
            height_range: self.height_range,
            stem_radius_base: self.stem_radius_base,
            stem_radius_top: self.stem_radius_top,
            leaf_count: self.leaf_count,
            leaf_size: self.leaf_size,
            color_base: self.color_base,
            color_tip: self.color_tip,
        }
    }
}

/// Convert a [`ProfileSpec`] into the real engine [`TaxonomicProfile`] — free-fn form
/// of [`ProfileSpec::to_taxonomic_profile`].
pub fn to_taxonomic_profile(spec: &ProfileSpec) -> TaxonomicProfile {
    spec.to_taxonomic_profile()
}

/// The compiled-in fallback / parity oracle: build a [`ProfileSpec`] straight from the
/// hardcoded `taxonomy::{grass,…,moss}()` builders. Returns `None` for an unknown name.
/// This is what the shipped EDN is parity-tested against.
pub fn builtin_profile(name: &str) -> Option<ProfileSpec> {
    let p = match name {
        "grass" => grass(),
        "fern" => fern(),
        "palm" => palm(),
        "conifer" => conifer(),
        "bush" => bush(),
        "cactus" => cactus(),
        "moss" => moss(),
        _ => return None,
    };
    Some(ProfileSpec::from_profile(&p))
}

/// Parse the whole `:vegetation/profiles` table from EDN `src` into a map keyed by the
/// (hyphenated) profile id, each value the merged [`ProfileSpec`].
pub fn profiles_from_edn(src: &str) -> Result<BTreeMap<String, ProfileSpec>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let profiles = mget(&root, "vegetation/profiles")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoProfiles)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in profiles.iter() {
        let Some(id) = kami_scene::kw_key(k) else {
            continue;
        };
        let Some(m) = v.as_map() else { continue };
        by_id.insert(id, ProfileSpec::from_map(m));
    }
    Ok(by_id)
}

/// Look up a single profile by (hyphenated) id from EDN `src`. Errors if the table or
/// the named profile is absent.
pub fn profile_from_edn(src: &str, name: &str) -> Result<ProfileSpec, Error> {
    profiles_from_edn(src)?
        .remove(name)
        .ok_or_else(|| Error::ProfileNotFound(name.to_string()))
}

/// Convenience: load all profiles from the crate-shipped [`VEGETATION_EDN`].
pub fn shipped_profiles() -> Result<BTreeMap<String, ProfileSpec>, Error> {
    profiles_from_edn(VEGETATION_EDN)
}

/// Convenience: load one profile from the shipped EDN.
pub fn shipped_profile(name: &str) -> Result<ProfileSpec, Error> {
    profile_from_edn(VEGETATION_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_profiles() {
        let p = shipped_profiles().expect("vegetation.edn parse");
        assert_eq!(p.len(), 7);
        for name in ALL_PROFILE_NAMES {
            assert!(p.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn unknown_builtin_profile_is_none() {
        assert!(builtin_profile("does-not-exist").is_none());
    }

    #[test]
    fn unknown_profile_from_edn_is_an_error() {
        assert!(matches!(
            profile_from_edn(VEGETATION_EDN, "bamboo"),
            Err(Error::ProfileNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(profiles_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_profiles_table_is_an_error() {
        assert!(matches!(profiles_from_edn("{:other 1}"), Err(Error::NoProfiles)));
    }

    #[test]
    fn missing_key_falls_back_to_default() {
        // A profile that only sets :leaf-count: every other field inherits the engine
        // moss() fallback (the same profile the JSON bridge uses for unknown ids).
        let p = profiles_from_edn("{:vegetation/profiles {:p {:leaf-count 9}}}").unwrap();
        let spec = &p["p"];
        let d = ProfileSpec::defaults();
        assert_eq!(spec.leaf_count, 9);
        assert_eq!(spec.leaf_size, d.leaf_size, "absent → default leaf_size");
        assert_eq!(spec.canopy, d.canopy, "absent → default canopy");
        assert_eq!(spec.stem_radius_base, d.stem_radius_base, "absent → default stem");
    }

    #[test]
    fn int_leaf_count_coerces_to_u32() {
        let p = profiles_from_edn("{:vegetation/profiles {:p {:leaf-count 6}}}").unwrap();
        assert_eq!(p["p"].leaf_count, 6);
    }

    #[test]
    fn int_field_coerces_to_float() {
        // `:leaf-size 1` (an int) coerces to 1.0 via kami-scene `num`.
        let p = profiles_from_edn("{:vegetation/profiles {:p {:leaf-size 1}}}").unwrap();
        assert_eq!(p["p"].leaf_size, 1.0);
    }

    #[test]
    fn canopy_id_round_trips() {
        for c in [
            CanopyShape::Blade,
            CanopyShape::Fan,
            CanopyShape::Dome,
            CanopyShape::Cone,
            CanopyShape::Radial,
            CanopyShape::Column,
            CanopyShape::Carpet,
        ] {
            assert_eq!(canopy_from_id(id_from_canopy(c)), c);
        }
    }
}
