//! Parity tests: the shipped EDN must faithfully reproduce kami-terrain's compiled-in
//! biome presets. This is the whole point of the data tier (ADR-0038) — EDN becomes the
//! source of truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a value loaded from
//! `biomes.edn` against `BiomePreset::{heightmap, splat_thresholds, palette}` (called
//! here, not transcribed). Preset values are exact decimal literals (e.g. 0.008, 2.1,
//! 0.45, 0.28…) — all representable in f32 — so parity is asserted with a tiny epsilon
//! (1e-6) purely to guard against int/float coercion noise; exact equality also holds.

use kami_terrain::BiomePreset;
use kami_terrain_scene::{
    biomes_from_edn, builtin_biome, shipped_biomes, BiomeSpec, HeightmapSpec, ALL_BIOME_NAMES,
    BIOMES_EDN,
};

const EPS: f32 = 1e-6;

/// Map a biome name to the real `BiomePreset` (the oracle source).
fn preset(name: &str) -> BiomePreset {
    match name {
        "plains" => BiomePreset::Plains,
        "quarry" => BiomePreset::Quarry,
        "desert" => BiomePreset::Desert,
        "tundra" => BiomePreset::Tundra,
        other => panic!("unknown biome {other}"),
    }
}

/// Assert every field of an EDN-loaded biome equals the same field read off the
/// hardcoded `BiomePreset` oracle.
fn assert_biome_eq(name: &str, edn: &BiomeSpec) {
    let p = preset(name);
    let oracle = BiomeSpec::from_preset(p);

    // Heightmap (seed is supplied per-call, not stored — compare the seedless fields).
    let hm = &edn.heightmap;
    let oh = &oracle.heightmap;
    assert!((hm.max_height - oh.max_height).abs() < EPS, "{name}: max_height");
    assert!((hm.frequency - oh.frequency).abs() < EPS, "{name}: frequency");
    assert_eq!(hm.octaves, oh.octaves, "{name}: octaves");
    assert!((hm.lacunarity - oh.lacunarity).abs() < EPS, "{name}: lacunarity");
    assert!((hm.persistence - oh.persistence).abs() < EPS, "{name}: persistence");

    // Splat thresholds.
    let s = &edn.splat;
    let os = &oracle.splat;
    assert!((s.sand_line - os.sand_line).abs() < EPS, "{name}: sand_line");
    assert!((s.snow_line - os.snow_line).abs() < EPS, "{name}: snow_line");
    assert!((s.rock_slope - os.rock_slope).abs() < EPS, "{name}: rock_slope");

    // Palette (4 base + 4 tip RGB).
    for layer in 0..4 {
        for ch in 0..3 {
            assert!(
                (edn.palette.base[layer][ch] - oracle.palette.base[layer][ch]).abs() < EPS,
                "{name}: base[{layer}][{ch}]"
            );
            assert!(
                (edn.palette.tip[layer][ch] - oracle.palette.tip[layer][ch]).abs() < EPS,
                "{name}: tip[{layer}][{ch}]"
            );
        }
    }

    // And the whole spec equals the oracle-derived spec (exact f32 equality).
    assert_eq!(*edn, oracle, "{name}: full BiomeSpec parity");
}

/// For each shipped biome (plains, quarry, desert, tundra), every field loaded from
/// biomes.edn == the value from the Rust `BiomePreset` methods.
#[test]
fn biomes_edn_matches_builtin() {
    let loaded = biomes_from_edn(BIOMES_EDN).expect("biomes.edn parse");
    assert_eq!(loaded.len(), 4, "all biomes present in EDN");

    for name in ALL_BIOME_NAMES {
        assert_biome_eq(name, &loaded[name]);

        // The `builtin_biome` oracle helper agrees with what we read off the methods.
        let built = builtin_biome(name).expect("builtin biome");
        assert_eq!(loaded[name], built, "{name}: EDN == builtin_biome()");
    }

    // The shipped-biomes convenience loader yields the same thing.
    let shipped = shipped_biomes().expect("shipped biomes");
    for name in ALL_BIOME_NAMES {
        assert_eq!(shipped[name], loaded[name], "{name}: shipped == loaded");
    }
}

/// The converters reconstruct the real engine structs, behaviourally identical to the
/// hardcoded `BiomePreset` methods. The heightmap takes the same seed per-call.
#[test]
fn converters_match_hardcoded() {
    let loaded = biomes_from_edn(BIOMES_EDN).unwrap();
    let seed = 123.5_f32; // arbitrary per-call seed; must thread through unchanged.

    for name in ALL_BIOME_NAMES {
        let p = preset(name);
        let spec = &loaded[name];

        let hc = spec.to_heightmap_config(seed);
        let oh = p.heightmap(seed);
        assert!((hc.seed - oh.seed).abs() < EPS, "{name}: seed threaded");
        assert!((hc.max_height - oh.max_height).abs() < EPS, "{name}: hc max_height");
        assert!((hc.frequency - oh.frequency).abs() < EPS, "{name}: hc frequency");
        assert_eq!(hc.octaves, oh.octaves, "{name}: hc octaves");
        assert!((hc.lacunarity - oh.lacunarity).abs() < EPS, "{name}: hc lacunarity");
        assert!((hc.persistence - oh.persistence).abs() < EPS, "{name}: hc persistence");

        let st = spec.to_splat_thresholds();
        let ot = p.splat_thresholds();
        assert!((st.sand_line - ot.sand_line).abs() < EPS, "{name}: st sand_line");
        assert!((st.snow_line - ot.snow_line).abs() < EPS, "{name}: st snow_line");
        assert!((st.rock_slope - ot.rock_slope).abs() < EPS, "{name}: st rock_slope");

        let mp = spec.to_material_palette();
        let op = p.palette();
        assert_eq!(mp.base, op.base, "{name}: palette base");
        assert_eq!(mp.tip, op.tip, "{name}: palette tip");
    }
}

/// A biome whose `:heightmap` omits keys reproduces the engine `HeightmapConfig`
/// default for those keys — the tolerant merge contract.
#[test]
fn omitted_heightmap_fields_inherit_defaults() {
    let loaded =
        biomes_from_edn("{:terrain/biomes {:p {:heightmap {:max-height 50.0}}}}").unwrap();
    let hm = &loaded["p"].heightmap;
    let d = HeightmapSpec::defaults();
    assert_eq!(hm.max_height, 50.0);
    assert_eq!(hm.frequency, d.frequency, "absent → default frequency");
    assert_eq!(hm.octaves, d.octaves, "absent → default octaves");
    assert_eq!(hm.lacunarity, d.lacunarity, "absent → default lacunarity");
    assert_eq!(hm.persistence, d.persistence, "absent → default persistence");
}
