//! Biological taxonomy → procedural generation parameters.
//!
//! Instead of hardcoding 5 species meshes, we derive generation patterns
//! from a hierarchical botanical classification. Adding a new species
//! = choosing enum values + parameter tuning (no new mesh function).

use serde::{Deserialize, Serialize};

/// Plant division (highest rank that shapes morphology).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Division {
    /// Mosses, liverworts — non-vascular, low mat-forming.
    Bryophyta,
    /// Ferns, horsetails — vascular, spore-producing, pinnate fronds.
    Pteridophyta,
    /// Conifers, cycads, ginkgo — naked seeds, needle-like leaves, cones.
    Gymnospermae,
    /// Flowering plants — most diverse, varied morphology.
    Angiospermae,
}

/// Overall plant architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrowthHabit {
    /// Narrow-leaved grasses (Poaceae) — tussock / turf.
    Grass,
    /// Soft non-woody stem, low (< 1m).
    Herb,
    /// Multi-stemmed woody plant, < 5m (Bush / shrub).
    Shrub,
    /// Single dominant trunk, > 5m.
    Tree,
    /// Water-storing stem (cactus / succulent).
    Succulent,
    /// Ground-hugging mat (moss / low cover).
    Mat,
    /// Climbing vine / liana.
    Climber,
}

/// Leaf arrangement pattern on the stem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeafArrangement {
    /// Single leaf per node, staggered.
    Alternate,
    /// Two leaves per node, opposite each other.
    Opposite,
    /// Three+ leaves per node at same height.
    Whorled,
    /// Cluster at the base (like dandelion).
    Rosette,
    /// All leaves from base (like grass).
    Basal,
    /// No discrete leaves (cactus spines, moss).
    None,
}

/// Leaf / foliage unit shape (drives per-leaf mesh geometry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeafShape {
    /// Long + narrow, parallel veins (grass blade).
    Linear,
    /// Pointed ends, lance-shaped.
    Lanceolate,
    /// Egg-shaped, broadest near base.
    Ovate,
    /// Fingered / hand-shaped (maple).
    Palmate,
    /// Compound, feather-like (fern).
    Pinnate,
    /// Stiff narrow (conifer).
    Needle,
    /// Thick, fleshy (succulent).
    Succulent,
    /// Scale-like (cypress, moss).
    Scale,
}

/// Overall silhouette shape of the foliage mass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanopyShape {
    /// Upward blades (grass / tussock).
    Blade,
    /// Radial fan from base (fern).
    Fan,
    /// Rounded / globe (bush, broadleaf tree).
    Dome,
    /// Tapered pyramid (conifer).
    Cone,
    /// Palm crown — radial fronds at top.
    Radial,
    /// Column / spire (cactus, cypress).
    Column,
    /// Flat carpet (moss, creeping).
    Carpet,
}

/// Full procedural generation profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomicProfile {
    pub common_name: &'static str,
    pub division: Division,
    pub habit: GrowthHabit,
    pub arrangement: LeafArrangement,
    pub leaf_shape: LeafShape,
    pub canopy: CanopyShape,

    // Mesh generation parameters (derived but tunable)
    /// Height range [min, max] in local units (scaled at instance time).
    pub height_range: [f32; 2],
    /// Stem/trunk radius at base.
    pub stem_radius_base: f32,
    /// Stem radius at top (< base for tapering).
    pub stem_radius_top: f32,
    /// Number of leaf units / blades / fronds.
    pub leaf_count: u32,
    /// Per-leaf size (in unit mesh).
    pub leaf_size: f32,
    /// Base → leaf color gradient (RGB to RGB).
    pub color_base: [f32; 3],
    pub color_tip: [f32; 3],
}

// ── Preset catalog ──

/// Grass (Poaceae — Angiospermae, Grass habit).
pub fn grass() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "grass",
        division: Division::Angiospermae,
        habit: GrowthHabit::Grass,
        arrangement: LeafArrangement::Basal,
        leaf_shape: LeafShape::Linear,
        canopy: CanopyShape::Blade,
        height_range: [0.7, 1.4],
        stem_radius_base: 0.0, // no distinct stem
        stem_radius_top: 0.0,
        leaf_count: 3,
        leaf_size: 0.18,
        color_base: [0.18, 0.42, 0.08],
        color_tip: [0.42, 0.68, 0.15],
    }
}

/// Fern (Pteridophyta, Herb habit).
pub fn fern() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "fern",
        division: Division::Pteridophyta,
        habit: GrowthHabit::Herb,
        arrangement: LeafArrangement::Alternate,
        leaf_shape: LeafShape::Pinnate,
        canopy: CanopyShape::Fan,
        height_range: [0.8, 1.5],
        stem_radius_base: 0.04,
        stem_radius_top: 0.02,
        leaf_count: 5, // leaflet pairs
        leaf_size: 0.35,
        color_base: [0.12, 0.28, 0.04],
        color_tip: [0.3, 0.55, 0.12],
    }
}

/// Palm tree (Angiospermae, Tree habit, radial canopy).
pub fn palm() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "palm",
        division: Division::Angiospermae,
        habit: GrowthHabit::Tree,
        arrangement: LeafArrangement::Whorled,
        leaf_shape: LeafShape::Pinnate,
        canopy: CanopyShape::Radial,
        height_range: [0.85, 1.25],
        stem_radius_base: 0.08,
        stem_radius_top: 0.06,
        leaf_count: 7,
        leaf_size: 0.55,
        color_base: [0.35, 0.22, 0.08],
        color_tip: [0.18, 0.45, 0.1],
    }
}

/// Conifer (Gymnospermae, Tree habit, cone canopy).
pub fn conifer() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "conifer",
        division: Division::Gymnospermae,
        habit: GrowthHabit::Tree,
        arrangement: LeafArrangement::Whorled,
        leaf_shape: LeafShape::Needle,
        canopy: CanopyShape::Cone,
        height_range: [0.7, 1.3],
        stem_radius_base: 0.09,
        stem_radius_top: 0.03,
        leaf_count: 3, // cone layers
        leaf_size: 0.42,
        color_base: [0.25, 0.18, 0.08],
        color_tip: [0.12, 0.3, 0.08],
    }
}

/// Broadleaf bush (Angiospermae, Shrub habit, dome canopy).
pub fn bush() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "bush",
        division: Division::Angiospermae,
        habit: GrowthHabit::Shrub,
        arrangement: LeafArrangement::Alternate,
        leaf_shape: LeafShape::Ovate,
        canopy: CanopyShape::Dome,
        height_range: [0.8, 1.4],
        stem_radius_base: 0.06,
        stem_radius_top: 0.04,
        leaf_count: 6, // layered leaf discs
        leaf_size: 0.33,
        color_base: [0.15, 0.28, 0.06],
        color_tip: [0.28, 0.48, 0.1],
    }
}

/// Columnar cactus (Angiospermae, Succulent habit, Column canopy).
pub fn cactus() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "cactus",
        division: Division::Angiospermae,
        habit: GrowthHabit::Succulent,
        arrangement: LeafArrangement::None,
        leaf_shape: LeafShape::Succulent,
        canopy: CanopyShape::Column,
        height_range: [0.6, 1.3],
        stem_radius_base: 0.22,
        stem_radius_top: 0.18,
        leaf_count: 0,
        leaf_size: 0.0,
        color_base: [0.22, 0.38, 0.18],
        color_tip: [0.32, 0.52, 0.22],
    }
}

/// Ground moss (Bryophyta, Mat habit, carpet canopy).
pub fn moss() -> TaxonomicProfile {
    TaxonomicProfile {
        common_name: "moss",
        division: Division::Bryophyta,
        habit: GrowthHabit::Mat,
        arrangement: LeafArrangement::None,
        leaf_shape: LeafShape::Scale,
        canopy: CanopyShape::Carpet,
        height_range: [0.15, 0.25],
        stem_radius_base: 0.0,
        stem_radius_top: 0.0,
        leaf_count: 1,
        leaf_size: 0.45,
        color_base: [0.16, 0.30, 0.08],
        color_tip: [0.32, 0.54, 0.14],
    }
}

/// Standard catalog of 7 profiles keyed to SpeciesId + 2 extensions.
pub fn default_catalog() -> Vec<TaxonomicProfile> {
    vec![grass(), fern(), palm(), conifer(), bush(), cactus(), moss()]
}

// ── Remote catalog (seibutsu.etzhayyim.com bridge) ──────────────────────────────
//
// `kami-vegetation` has no networking by design (Rust core, runs in WASM).
// The browser shell fetches `app.etzhayyim.apps.seibutsu.renderProfile` for each
// species DID and feeds the resulting JSON through `OwnedTaxonomicProfile::
// from_json_str`. The owned variant decouples the engine from the static
// `&'static str` `common_name` of `TaxonomicProfile` so dynamic species can
// be loaded at runtime.

/// Owned variant — `String` common_name so the profile can come from a
/// `seibutsu.renderProfile` XRPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedTaxonomicProfile {
    pub common_name: String,
    pub division: Division,
    pub habit: GrowthHabit,
    #[serde(default = "default_arrangement")]
    pub arrangement: LeafArrangement,
    pub leaf_shape: LeafShape,
    pub canopy: CanopyShape,
    pub height_range: [f32; 2],
    #[serde(default)]
    pub stem_radius_base: f32,
    #[serde(default)]
    pub stem_radius_top: f32,
    #[serde(default)]
    pub leaf_count: u32,
    #[serde(default)]
    pub leaf_size: f32,
    pub color_base: [f32; 3],
    pub color_tip: [f32; 3],
}

fn default_arrangement() -> LeafArrangement {
    LeafArrangement::None
}

impl OwnedTaxonomicProfile {
    /// Parse a single `seibutsu.renderProfile` response (camelCase JSON).
    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        // Accept the camelCase wire format used by app.etzhayyim.apps.seibutsu.renderProfile
        // by mapping it through a lenient intermediate.
        let v: serde_json::Value = serde_json::from_str(s)?;
        let take_f = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let take_u = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let take_s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let arr3 = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_array())
                .map(|a| {
                    let mut out = [0.0f32; 3];
                    for (i, n) in a.iter().take(3).enumerate() {
                        out[i] = n.as_f64().unwrap_or(0.0) as f32;
                    }
                    out
                })
                .unwrap_or([0.0; 3])
        };
        let arr2 = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_array())
                .map(|a| {
                    let mut out = [0.0f32; 2];
                    for (i, n) in a.iter().take(2).enumerate() {
                        out[i] = n.as_f64().unwrap_or(0.0) as f32;
                    }
                    out
                })
                .unwrap_or([0.0, 1.0])
        };
        Ok(OwnedTaxonomicProfile {
            common_name: take_s("commonName"),
            division: parse_division(&take_s("division")),
            habit: parse_habit(&take_s("habit")),
            arrangement: parse_arrangement(&take_s("arrangement")),
            leaf_shape: parse_leaf_shape(&take_s("leafShape")),
            canopy: parse_canopy(&take_s("canopy")),
            height_range: arr2("heightRange"),
            stem_radius_base: take_f("stemRadiusBase"),
            stem_radius_top: take_f("stemRadiusTop"),
            leaf_count: take_u("leafCount"),
            leaf_size: take_f("leafSize"),
            color_base: arr3("colorBase"),
            color_tip: arr3("colorTip"),
        })
    }
}

fn parse_division(s: &str) -> Division {
    match s {
        "bryophyta" => Division::Bryophyta,
        "pteridophyta" => Division::Pteridophyta,
        "gymnospermae" => Division::Gymnospermae,
        _ => Division::Angiospermae,
    }
}
fn parse_habit(s: &str) -> GrowthHabit {
    match s {
        "grass" => GrowthHabit::Grass,
        "herb" => GrowthHabit::Herb,
        "shrub" => GrowthHabit::Shrub,
        "tree" => GrowthHabit::Tree,
        "succulent" => GrowthHabit::Succulent,
        "mat" => GrowthHabit::Mat,
        "climber" => GrowthHabit::Climber,
        _ => GrowthHabit::Herb,
    }
}
fn parse_arrangement(s: &str) -> LeafArrangement {
    match s {
        "alternate" => LeafArrangement::Alternate,
        "opposite" => LeafArrangement::Opposite,
        "whorled" => LeafArrangement::Whorled,
        "rosette" => LeafArrangement::Rosette,
        "basal" => LeafArrangement::Basal,
        _ => LeafArrangement::None,
    }
}
fn parse_leaf_shape(s: &str) -> LeafShape {
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
fn parse_canopy(s: &str) -> CanopyShape {
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

/// Runtime catalog: built either from the static presets or from
/// `seibutsu.renderProfile` JSON responses fetched by the WASM shell.
#[derive(Debug, Clone, Default)]
pub struct RemoteCatalog {
    pub profiles: Vec<OwnedTaxonomicProfile>,
}

impl RemoteCatalog {
    pub fn from_default() -> Self {
        Self {
            profiles: default_catalog()
                .into_iter()
                .map(|p| OwnedTaxonomicProfile {
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
                })
                .collect(),
        }
    }

    /// Push one `renderProfile` response into the catalog.
    pub fn push_json(&mut self, s: &str) -> Result<(), serde_json::Error> {
        self.profiles.push(OwnedTaxonomicProfile::from_json_str(s)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_complete() {
        let c = default_catalog();
        assert_eq!(c.len(), 7);
        for p in &c {
            assert!(p.height_range[0] < p.height_range[1]);
            assert!(p.height_range[0] > 0.0);
        }
    }

    #[test]
    fn conifer_is_gymnosperm_with_needles() {
        let c = conifer();
        assert_eq!(c.division, Division::Gymnospermae);
        assert_eq!(c.leaf_shape, LeafShape::Needle);
        assert_eq!(c.canopy, CanopyShape::Cone);
    }

    #[test]
    fn moss_is_bryophyta_carpet() {
        let m = moss();
        assert_eq!(m.division, Division::Bryophyta);
        assert_eq!(m.canopy, CanopyShape::Carpet);
        assert!(m.height_range[1] < 0.3);
    }

    #[test]
    fn cactus_has_no_leaves() {
        let c = cactus();
        assert_eq!(c.habit, GrowthHabit::Succulent);
        assert_eq!(c.arrangement, LeafArrangement::None);
        assert_eq!(c.leaf_count, 0);
    }

    #[test]
    fn remote_catalog_round_trip() {
        let json = r#"{
          "commonName": "bamboo",
          "division": "angiospermae",
          "habit": "grass",
          "arrangement": "basal",
          "leafShape": "linear",
          "canopy": "blade",
          "heightRange": [1.5, 4.0],
          "stemRadiusBase": 0.05,
          "stemRadiusTop": 0.04,
          "leafCount": 6,
          "leafSize": 0.4,
          "colorBase": [0.18, 0.42, 0.08],
          "colorTip": [0.5, 0.7, 0.2]
        }"#;
        let p = OwnedTaxonomicProfile::from_json_str(json).expect("parse");
        assert_eq!(p.common_name, "bamboo");
        assert_eq!(p.division, Division::Angiospermae);
        assert_eq!(p.habit, GrowthHabit::Grass);
        assert_eq!(p.canopy, CanopyShape::Blade);
        assert_eq!(p.leaf_count, 6);
    }

    #[test]
    fn remote_catalog_seeds_from_default() {
        let cat = RemoteCatalog::from_default();
        assert_eq!(cat.profiles.len(), 7);
    }
}
