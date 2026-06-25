# kami-terrain-scene

Data-tier crate that makes `kami-terrain` **data-driven**: the terrain biome
presets (plains / quarry / desert / tundra) — each a bundle of FBM heightmap
params + splatmap thresholds + material colour palette — live as **canonical EDN**
here, loaded into the real `kami_terrain` structs at startup.

It is the terrain sibling of [`kami-vehicle-scene`](../kami-vehicle-scene) and
[`kami-atmosphere-scene`](../kami-atmosphere-scene) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **hot heightmap / splatmap / chunk-mesh generation stays
native Rust; only init-time CONFIG/DATA moves to EDN.** `kami-terrain`'s FBM noise,
splatmap blend, chunk meshing and water run per-frame / per-chunk and stay in Rust
untouched. But a **biome preset** is read **once** when a chunk is generated — it
just seeds a `HeightmapConfig` + `SplatThresholds` + `MaterialPalette` the native
generators then consume. That seed is config, so it moves out of the hardcoded
`BiomePreset::{heightmap, splat_thresholds, palette}` methods into EDN that an author
(or a fork, or Datomic) can edit without recompiling.

This crate is **additive**: `kami-terrain`'s compiled-in `BiomePreset` enum and its
methods are *not* deleted. They remain the `builtin_biome()` fallback **and** the
parity oracle — every value in the shipped EDN is asserted equal to the hardcoded
Rust it mirrors, so the EDN is the source of truth while behaviour is provably
unchanged.

```
kami-terrain  (per-chunk gen, hardcoded BiomePreset methods = oracle/fallback)  ← unchanged
      ▲ path dep
kami-terrain-scene  (this crate: data/biomes.edn  +  from_edn loaders)          ← additive
      ▲ path dep
(open-world apps — seed biome config from this EDN at boot)
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/biomes.edn`](data/biomes.edn) | `:terrain/biomes` (`:plains`, `:quarry`, `:desert`, `:tundra`) | `BiomePreset::{heightmap, splat_thresholds, palette}` |

### Schema

```clojure
{:terrain/biomes
 {:plains {:heightmap {:max-height  80.0
                       :frequency   0.008
                       :octaves     7
                       :lacunarity  2.0
                       :persistence 0.5}
           :splat {:sand-line 15.0 :snow-line 100.0 :rock-slope 0.4}
           :palette {:base [[0.28 0.52 0.15] [0.45 0.40 0.35]
                            [0.76 0.69 0.50] [0.92 0.93 0.95]]
                     :tip  [[0.42 0.68 0.22] [0.55 0.50 0.45]
                            [0.85 0.78 0.60] [1.00 1.00 1.00]]}}
  :quarry {...} :desert {...} :tundra {...}}}
```

Keys are authored with hyphens (`:max-height`) — idiomatic EDN — and the loader maps
each to the matching public field on `HeightmapConfig` / `SplatThresholds` /
`MaterialPalette`:

| EDN key | Rust field |
|---|---|
| `:heightmap/:max-height`  | `HeightmapConfig.max_height` |
| `:heightmap/:frequency`   | `HeightmapConfig.frequency` |
| `:heightmap/:octaves`     | `HeightmapConfig.octaves` (u32) |
| `:heightmap/:lacunarity`  | `HeightmapConfig.lacunarity` |
| `:heightmap/:persistence` | `HeightmapConfig.persistence` |
| `:splat/:sand-line`       | `SplatThresholds.sand_line` |
| `:splat/:snow-line`       | `SplatThresholds.snow_line` |
| `:splat/:rock-slope`      | `SplatThresholds.rock_slope` |
| `:palette/:base`          | `MaterialPalette.base` (4 × `[r g b]`) |
| `:palette/:tip`           | `MaterialPalette.tip`  (4 × `[r g b]`) |

The heightmap `seed` is **not** stored in EDN — it is supplied per-call to
`to_heightmap_config(seed)`, exactly as `BiomePreset::heightmap(seed)` takes it. Any
heightmap key a biome omits inherits `HeightmapConfig::default()` (read, never
transcribed), so a partial merge is provably the same. (The shipped biomes set all
heightmap fields; the merge contract is exercised by the tolerant-parse tests.)

## API

```rust
use kami_terrain_scene as scene;

// All biomes, straight from the shipped biomes.edn.
let biomes = scene::shipped_biomes()?;            // BTreeMap<String, BiomeSpec>

// One biome as the real engine structs (== BiomePreset methods, proven by tests).
let spec = scene::shipped_biome("quarry")?;
let hc   = spec.to_heightmap_config(seed);        // kami_terrain::HeightmapConfig
let st   = spec.to_splat_thresholds();            // kami_terrain::SplatThresholds
let mp   = spec.to_material_palette();            // kami_terrain::MaterialPalette

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::biomes_from_edn(my_edn)?;      // id → BiomeSpec
let spec  = scene::biome_from_edn(my_edn, "desert")?;
```

Loaders: `biomes_from_edn` / `biome_from_edn` / `shipped_biomes` / `shipped_biome`,
plus the converters `BiomeSpec::{to_heightmap_config, to_splat_thresholds,
to_material_palette}`. The oracle / fallback is `builtin_biome(name)`, built from the
Rust `BiomePreset`. `BIOMES_EDN` is the shipped string (`include_str!`) and
`ALL_BIOME_NAMES` lists the four biomes, so consumers can bake the table into a wasm
bundle.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-terrain-scene
```

- `tests/biome_parity.rs::biomes_edn_matches_builtin` — for each biome, every field
  loaded from `biomes.edn` equals the value read off the REAL Rust `BiomePreset`
  methods (called, not transcribed). Preset values are exact f32-representable
  decimals; parity uses a tiny `1e-6` epsilon to absorb int/float coercion, and exact
  `==` on the whole `BiomeSpec` also holds.
- `converters_match_hardcoded` — `to_heightmap_config(seed)` /
  `to_splat_thresholds()` / `to_material_palette()` reconstruct the real engine
  structs equal to the hardcoded `BiomePreset` methods, threading the per-call seed
  unchanged.
- `omitted_heightmap_fields_inherit_defaults` — a biome that omits heightmap keys
  reproduces the engine `HeightmapConfig::default()`, the tolerant-merge contract.
- Unit tests cover tolerant parse: missing key → default merge, int → float / u32
  coercion, unknown biome → `BiomeNotFound`, non-map root → `NotAMap`, missing table →
  `NoBiomes`.

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is
the authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run
the native test suite via the workspace alias: `cargo test-native -p kami-terrain-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
