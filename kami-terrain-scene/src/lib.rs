//! kami-terrain-scene — EDN authoring surface for `kami-terrain` BIOME CONFIG.
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` for
//! the terrain biome system: it turns canonical `:terrain/biomes` EDN into the real
//! `kami_terrain` engine structs ([`kami_terrain::HeightmapConfig`],
//! [`kami_terrain::SplatThresholds`], [`kami_terrain::MaterialPalette`]), re-using the
//! tolerant `kami-scene` accessors the same way games parse `scene.edn` (missing keys
//! fall back to defaults, namespaced keywords match on `ns/name`, ints coerce to
//! floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot heightmap / splatmap / chunk-mesh generation stays native Rust
//! (`kami-terrain`). A biome preset is **init-time CONFIG** — read once when a chunk
//! is generated to seed an FBM [`kami_terrain::HeightmapConfig`] + the splatmap
//! [`kami_terrain::SplatThresholds`] + the shader [`kami_terrain::MaterialPalette`] —
//! so it is safe to move to EDN. `kami-terrain` itself stays "pure Rust + serde, no
//! edn dep"; the EDN dependency lives only here. The compiled-in
//! [`kami_terrain::BiomePreset`] enum + its `heightmap()` / `splat_thresholds()` /
//! `palette()` methods remain as the [`builtin_biome`] fallback and are parity-tested
//! against the shipped EDN ([`crate::BIOMES_EDN`]).
//!
//! ## EDN shape (see `data/biomes.edn`)
//!
//! ```edn
//! {:terrain/biomes
//!  {:plains {:heightmap {:max-height 80.0 :frequency 0.008 :octaves 7
//!                        :lacunarity 2.0 :persistence 0.5}
//!            :splat {:sand-line 15.0 :snow-line 100.0 :rock-slope 0.4}
//!            :palette {:base [[r g b] ...4] :tip [[r g b] ...4]}}
//!   :quarry {...} :desert {...} :tundra {...}}}
//! ```
//!
//! ## Hyphen field keys → Rust fields
//!
//! Keys are authored with hyphens (`:max-height`) — idiomatic EDN. The loader maps
//! each to the matching public field on `HeightmapConfig` / `SplatThresholds` /
//! `MaterialPalette`:
//!
//! | EDN key | Rust field |
//! |---|---|
//! | `:heightmap/:max-height`  | `HeightmapConfig.max_height` |
//! | `:heightmap/:frequency`   | `HeightmapConfig.frequency` |
//! | `:heightmap/:octaves`     | `HeightmapConfig.octaves` (u32) |
//! | `:heightmap/:lacunarity`  | `HeightmapConfig.lacunarity` |
//! | `:heightmap/:persistence` | `HeightmapConfig.persistence` |
//! | `:splat/:sand-line`       | `SplatThresholds.sand_line` |
//! | `:splat/:snow-line`       | `SplatThresholds.snow_line` |
//! | `:splat/:rock-slope`      | `SplatThresholds.rock_slope` |
//! | `:palette/:base`          | `MaterialPalette.base` (4 × `[r g b]`) |
//! | `:palette/:tip`           | `MaterialPalette.tip`  (4 × `[r g b]`) |
//!
//! The heightmap `seed` is **not** stored in EDN — it is supplied per-call to
//! [`BiomeSpec::to_heightmap_config`], mirroring `BiomePreset::heightmap(seed)`. Any
//! heightmap key a biome omits inherits [`kami_terrain::HeightmapConfig::default`]
//! (never transcribed here).
//!
//! ## Also in this crate
//!
//! [`waves`] — the default Gerstner ocean waves (`water::default_waves()`) as
//! parity-tested EDN (ADR-0046).

/// Default Gerstner ocean waves (`water::default_waves()`) as EDN → real `GerstnerWave`s.
pub mod waves;

use std::collections::BTreeMap;

use kami_scene::{EdnValue, mget, num, root_map, vec3};
use kami_terrain::{BiomePreset, HeightmapConfig, MaterialPalette, SplatThresholds};

/// The canonical biome CONFIG shipped with this crate (the preset table).
/// This is the source of truth; the compiled-in presets are the parity-tested mirror.
pub const BIOMES_EDN: &str = include_str!("../data/biomes.edn");

/// Names of the biomes shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Keeping this list here (not in `kami-terrain`) keeps the engine
/// crate untouched. Order mirrors `BiomePreset` declaration order.
pub const ALL_BIOME_NAMES: [&str; 4] = ["plains", "quarry", "desert", "tundra"];

/// Errors raised while loading biome CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("biomes EDN root is not a map")]
    NotAMap,
    /// The `:terrain/biomes` table was missing or not a map.
    #[error("`:terrain/biomes` missing or not a map")]
    NoBiomes,
    /// The requested biome id was missing under `:terrain/biomes`.
    #[error("biome `{0}` not found under `:terrain/biomes`")]
    BiomeNotFound(String),
}

/// Heightmap sub-spec — the EDN-loaded mirror of the fields a hardcoded
/// `BiomePreset::heightmap(seed)` sets (minus `seed`, supplied per-call). A field
/// absent from the EDN keeps [`HeightmapConfig::default`], so this is a *full*
/// (merged) spec.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightmapSpec {
    /// Maximum terrain height in world units — `HeightmapConfig.max_height`.
    pub max_height: f32,
    /// FBM noise frequency — `HeightmapConfig.frequency`.
    pub frequency: f32,
    /// FBM octaves — `HeightmapConfig.octaves`.
    pub octaves: u32,
    /// FBM lacunarity — `HeightmapConfig.lacunarity`.
    pub lacunarity: f32,
    /// FBM persistence — `HeightmapConfig.persistence`.
    pub persistence: f32,
}

impl HeightmapSpec {
    /// Read every field off a real [`HeightmapConfig`] (the parity oracle / default base).
    pub fn from_config(c: &HeightmapConfig) -> Self {
        Self {
            max_height: c.max_height,
            frequency: c.frequency,
            octaves: c.octaves,
            lacunarity: c.lacunarity,
            persistence: c.persistence,
        }
    }

    /// The default sub-spec: every field read from [`HeightmapConfig::default`].
    pub fn defaults() -> Self {
        Self::from_config(&HeightmapConfig::default())
    }

    /// Build from one biome's `:heightmap` EDN map, merging present keys onto
    /// [`HeightmapSpec::defaults`].
    pub fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        let d = Self::defaults();
        let or = |key: &str, fallback: f32| match mget(m, key) {
            Some(v) => num(Some(v)),
            None => fallback,
        };
        Self {
            max_height: or("max-height", d.max_height),
            frequency: or("frequency", d.frequency),
            octaves: match mget(m, "octaves") {
                Some(v) => num(Some(v)).round() as u32,
                None => d.octaves,
            },
            lacunarity: or("lacunarity", d.lacunarity),
            persistence: or("persistence", d.persistence),
        }
    }
}

/// Splatmap-thresholds sub-spec — the EDN-loaded mirror of a hardcoded
/// `BiomePreset::splat_thresholds()`.
#[derive(Debug, Clone, PartialEq)]
pub struct SplatSpec {
    /// Sand line (max height for sand) — `SplatThresholds.sand_line`.
    pub sand_line: f32,
    /// Snow line (min height for snow) — `SplatThresholds.snow_line`.
    pub snow_line: f32,
    /// Rock slope threshold — `SplatThresholds.rock_slope`.
    pub rock_slope: f32,
}

impl SplatSpec {
    /// Read every field off a real [`SplatThresholds`] (the parity oracle).
    pub fn from_thresholds(t: &SplatThresholds) -> Self {
        Self {
            sand_line: t.sand_line,
            snow_line: t.snow_line,
            rock_slope: t.rock_slope,
        }
    }

    /// Build from one biome's `:splat` EDN map (all three keys present in shipped data;
    /// absent keys read `0.0` via `num`).
    pub fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        Self {
            sand_line: num(mget(m, "sand-line")),
            snow_line: num(mget(m, "snow-line")),
            rock_slope: num(mget(m, "rock-slope")),
        }
    }
}

/// Material-palette sub-spec — the EDN-loaded mirror of a hardcoded
/// `BiomePreset::palette()`: 4 base + 4 tip RGB colours (grass / rock / sand / snow).
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteSpec {
    /// (grass, rock, sand, snow) base colours, RGB [0,1] — `MaterialPalette.base`.
    pub base: [[f32; 3]; 4],
    /// (grass, rock, sand, snow) tip/accent colours — `MaterialPalette.tip`.
    pub tip: [[f32; 3]; 4],
}

impl PaletteSpec {
    /// Read every colour off a real [`MaterialPalette`] (the parity oracle).
    pub fn from_palette(p: &MaterialPalette) -> Self {
        Self {
            base: p.base,
            tip: p.tip,
        }
    }

    /// Build from one biome's `:palette` EDN map. `:base` / `:tip` are each a vector of
    /// four `[r g b]` vectors; missing entries default to `[0,0,0]` via `vec3`.
    pub fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        Self {
            base: read_layers(mget(m, "base")),
            tip: read_layers(mget(m, "tip")),
        }
    }
}

/// Read a 4-layer colour array (`[[r g b] [r g b] [r g b] [r g b]]`); missing layers
/// default to `[0,0,0]`.
fn read_layers(v: Option<&EdnValue>) -> [[f32; 3]; 4] {
    let rows = v.and_then(|x| x.as_vector());
    let g = |i: usize| match rows {
        Some(rows) => vec3(rows.get(i)),
        None => [0.0, 0.0, 0.0],
    };
    [g(0), g(1), g(2), g(3)]
}

/// One biome — the EDN-loaded mirror of the per-biome config a hardcoded
/// `BiomePreset` returns (`heightmap` + `splat_thresholds` + `palette`).
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeSpec {
    /// FBM heightmap params (seed supplied per-call).
    pub heightmap: HeightmapSpec,
    /// Splatmap generation thresholds.
    pub splat: SplatSpec,
    /// Material colour palette.
    pub palette: PaletteSpec,
}

impl BiomeSpec {
    /// Build the spec from the compiled-in [`BiomePreset`] oracle: read every field
    /// straight off the real engine methods. This is what the EDN is parity-tested
    /// against. A fixed `seed = 0.0` is used to read the heightmap (the spec carries no
    /// seed — it is supplied per-call to [`BiomeSpec::to_heightmap_config`]).
    pub fn from_preset(p: BiomePreset) -> Self {
        Self {
            heightmap: HeightmapSpec::from_config(&p.heightmap(0.0)),
            splat: SplatSpec::from_thresholds(&p.splat_thresholds()),
            palette: PaletteSpec::from_palette(&p.palette()),
        }
    }

    /// Build a spec from one biome's EDN map (`{:heightmap {..} :splat {..} :palette {..}}`).
    pub fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        let sub = |key: &str| mget(m, key).and_then(|v| v.as_map());
        Self {
            heightmap: sub("heightmap")
                .map(HeightmapSpec::from_map)
                .unwrap_or_else(HeightmapSpec::defaults),
            splat: sub("splat")
                .map(SplatSpec::from_map)
                .unwrap_or_else(|| SplatSpec {
                    sand_line: 0.0,
                    snow_line: 0.0,
                    rock_slope: 0.0,
                }),
            palette: sub("palette")
                .map(PaletteSpec::from_map)
                .unwrap_or_else(|| PaletteSpec {
                    base: [[0.0; 3]; 4],
                    tip: [[0.0; 3]; 4],
                }),
        }
    }

    /// Convert the heightmap sub-spec into the real [`HeightmapConfig`], taking the
    /// `seed` per-call — behaviourally identical to `BiomePreset::heightmap(seed)`.
    pub fn to_heightmap_config(&self, seed: f32) -> HeightmapConfig {
        HeightmapConfig {
            seed,
            max_height: self.heightmap.max_height,
            frequency: self.heightmap.frequency,
            octaves: self.heightmap.octaves,
            lacunarity: self.heightmap.lacunarity,
            persistence: self.heightmap.persistence,
        }
    }

    /// Convert the splat sub-spec into the real [`SplatThresholds`] — behaviourally
    /// identical to `BiomePreset::splat_thresholds()`.
    pub fn to_splat_thresholds(&self) -> SplatThresholds {
        SplatThresholds {
            sand_line: self.splat.sand_line,
            snow_line: self.splat.snow_line,
            rock_slope: self.splat.rock_slope,
        }
    }

    /// Convert the palette sub-spec into the real [`MaterialPalette`] — behaviourally
    /// identical to `BiomePreset::palette()`.
    pub fn to_material_palette(&self) -> MaterialPalette {
        MaterialPalette {
            base: self.palette.base,
            tip: self.palette.tip,
        }
    }
}

/// The compiled-in fallback / parity oracle: build a [`BiomeSpec`] straight from the
/// hardcoded `BiomePreset`. Returns `None` for an unknown name. This is what the
/// shipped EDN is parity-tested against.
pub fn builtin_biome(name: &str) -> Option<BiomeSpec> {
    let preset = match name {
        "plains" => BiomePreset::Plains,
        "quarry" => BiomePreset::Quarry,
        "desert" => BiomePreset::Desert,
        "tundra" => BiomePreset::Tundra,
        _ => return None,
    };
    Some(BiomeSpec::from_preset(preset))
}

/// Parse the whole `:terrain/biomes` table from EDN `src` into a map keyed by the
/// (hyphenated) biome id, each value the merged [`BiomeSpec`].
pub fn biomes_from_edn(src: &str) -> Result<BTreeMap<String, BiomeSpec>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let biomes = mget(&root, "terrain/biomes")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoBiomes)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in biomes.iter() {
        let Some(id) = kami_scene::kw_key(k) else {
            continue;
        };
        let Some(m) = v.as_map() else { continue };
        by_id.insert(id, BiomeSpec::from_map(m));
    }
    Ok(by_id)
}

/// Look up a single biome by (hyphenated) id from EDN `src`. Errors if the table or the
/// named biome is absent.
pub fn biome_from_edn(src: &str, name: &str) -> Result<BiomeSpec, Error> {
    biomes_from_edn(src)?
        .remove(name)
        .ok_or_else(|| Error::BiomeNotFound(name.to_string()))
}

/// Convenience: load all biomes from the crate-shipped [`BIOMES_EDN`].
pub fn shipped_biomes() -> Result<BTreeMap<String, BiomeSpec>, Error> {
    biomes_from_edn(BIOMES_EDN)
}

/// Convenience: load one biome from the shipped EDN.
pub fn shipped_biome(name: &str) -> Result<BiomeSpec, Error> {
    biome_from_edn(BIOMES_EDN, name)
}

/// Executor-edge resolver (ADR-0044/0046): resolve a named biome to a [`BiomeSpec`],
/// loading from the shipped [`BIOMES_EDN`] and falling back to the compiled-in
/// [`BiomePreset`] only if the EDN fails to parse/resolve. Returns `None` for an unknown
/// biome name.
///
/// A native/GPU consumer (e.g. a `kami-app-*` terrain build) calls this instead of passing
/// a hardcoded `BiomePreset` to the pipeline — so the heightmap/splat/palette are *data*
/// (parity-tested here), retunable without recompiling. `BiomeSpec`'s
/// [`to_heightmap_config`](BiomeSpec::to_heightmap_config) /
/// [`to_splat_thresholds`](BiomeSpec::to_splat_thresholds) /
/// [`to_material_palette`](BiomeSpec::to_material_palette) then yield the real engine structs.
pub fn resolve_biome(name: &str) -> Option<BiomeSpec> {
    match shipped_biome(name) {
        Ok(b) => Some(b),
        Err(_) => builtin_biome(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Executor-edge proof: `resolve_biome` is driven by the shipped EDN (equals
    /// `shipped_biome`), with the builtin only as fallback, and `None` for unknowns.
    #[test]
    fn resolve_biome_is_driven_by_edn() {
        for name in ALL_BIOME_NAMES {
            assert_eq!(
                resolve_biome(name).expect("biome resolves"),
                shipped_biome(name).expect("biomes.edn resolves"),
                "{name}: resolve_biome driven by biomes.edn"
            );
        }
        assert!(resolve_biome("volcano").is_none(), "unknown → None");
    }

    #[test]
    fn shipped_has_all_biomes() {
        let b = shipped_biomes().expect("biomes.edn parse");
        assert_eq!(b.len(), 4);
        for name in ALL_BIOME_NAMES {
            assert!(b.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn unknown_builtin_biome_is_none() {
        assert!(builtin_biome("does-not-exist").is_none());
    }

    #[test]
    fn unknown_biome_from_edn_is_an_error() {
        assert!(matches!(
            biome_from_edn(BIOMES_EDN, "jungle"),
            Err(Error::BiomeNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(biomes_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_biomes_table_is_an_error() {
        assert!(matches!(
            biomes_from_edn("{:other 1}"),
            Err(Error::NoBiomes)
        ));
    }

    #[test]
    fn missing_heightmap_key_falls_back_to_default() {
        // A biome whose heightmap only sets max-height: every other field inherits the
        // engine HeightmapConfig default.
        let b = biomes_from_edn("{:terrain/biomes {:p {:heightmap {:max-height 50.0}}}}").unwrap();
        let hm = &b["p"].heightmap;
        let d = HeightmapSpec::defaults();
        assert_eq!(hm.max_height, 50.0);
        assert_eq!(hm.frequency, d.frequency, "absent → default frequency");
        assert_eq!(hm.octaves, d.octaves, "absent → default octaves");
        assert_eq!(hm.lacunarity, d.lacunarity, "absent → default lacunarity");
        assert_eq!(
            hm.persistence, d.persistence,
            "absent → default persistence"
        );
    }

    #[test]
    fn int_octaves_coerces_to_u32() {
        let b = biomes_from_edn("{:terrain/biomes {:p {:heightmap {:octaves 8}}}}").unwrap();
        assert_eq!(b["p"].heightmap.octaves, 8);
    }

    #[test]
    fn int_threshold_coerces_to_float() {
        // `:sand-line 7` (an int) coerces to 7.0 via kami-scene `num`.
        let b = biomes_from_edn("{:terrain/biomes {:p {:splat {:sand-line 7}}}}").unwrap();
        assert_eq!(b["p"].splat.sand_line, 7.0);
    }
}
