# kami-vegetation-scene

Data-tier crate that makes `kami-vegetation` **data-driven**: the per-species
**taxonomic profile presets** (grass / fern / palm / conifer / bush / cactus / moss)
live as **canonical EDN** here, loaded into the real
`kami_vegetation::taxonomy::TaxonomicProfile` struct at startup.

It is the vegetation sibling of [`kami-vehicle-scene`](../kami-vehicle-scene),
[`kami-atmosphere-scene`](../kami-atmosphere-scene) and
[`kami-terrain-scene`](../kami-terrain-scene) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **hot mesh generation / placement / cull stays native Rust;
only init-time CONFIG/DATA moves to EDN.** `kami-vegetation`'s
`mesh_from_profile(&TaxonomicProfile)`, Poisson-disk placement, and WASM-cached cull
run per-frame / per-chunk and stay in Rust untouched. But a **taxonomic profile** is
read **once** when a species mesh is generated — it just parameterizes the canopy
mesh switch (`leaf_count` / `leaf_size` / `stem_radius` / `CanopyShape`). That seed is
config, so it moves out of the hardcoded
`taxonomy::{grass,fern,palm,conifer,bush,cactus,moss}()` builders into EDN that an
author (or a fork, or Datomic, or the `seibutsu.renderProfile` XRPC bridge) can edit
without recompiling.

This crate is **additive**: `kami-vegetation`'s compiled-in profile builders are *not*
deleted. They remain the `builtin_profile()` fallback **and** the parity oracle —
every value in the shipped EDN is asserted equal to the hardcoded Rust it mirrors, so
the EDN is the source of truth while behaviour is provably unchanged.

```
kami-vegetation  (per-frame mesh/cull, hardcoded grass()…moss() = oracle/fallback)  ← unchanged
      ▲ path dep
kami-vegetation-scene  (this crate: data/vegetation.edn  +  from_edn loaders)        ← additive
      ▲ path dep
(open-world apps — seed TaxonomicProfile from this EDN at boot)
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/vegetation.edn`](data/vegetation.edn) | `:vegetation/profiles` (`:grass` `:fern` `:palm` `:conifer` `:bush` `:cactus` `:moss`) | `taxonomy::{grass,fern,palm,conifer,bush,cactus,moss}()` |

### Schema

```clojure
{:vegetation/profiles
 {:grass {:common-name      "grass"
          :division         :angiospermae   ; enum id
          :habit            :grass          ; enum id
          :arrangement      :basal          ; enum id
          :leaf-shape       :linear         ; enum id
          :canopy           :blade          ; enum id → CanopyShape
          :height-range     [0.7 1.4]
          :stem-radius-base 0.0
          :stem-radius-top  0.0
          :leaf-count       3               ; u32
          :leaf-size        0.18
          :color-base       [0.18 0.42 0.08]
          :color-tip        [0.42 0.68 0.15]}
  :fern {...} :palm {...} :conifer {...} :bush {...} :cactus {...} :moss {...}}}
```

Keys are authored with hyphens (`:stem-radius-base`) — idiomatic EDN — and the loader
maps each to the matching public field on `TaxonomicProfile`:

| EDN key | Rust field |
|---|---|
| `:common-name`      | `common_name` (`&'static str`, resolved via the known-name table) |
| `:division`         | `division` (`Division` enum, via `division_from_id`) |
| `:habit`            | `habit` (`GrowthHabit` enum, via `habit_from_id`) |
| `:arrangement`      | `arrangement` (`LeafArrangement` enum, via `arrangement_from_id`) |
| `:leaf-shape`       | `leaf_shape` (`LeafShape` enum, via `leaf_shape_from_id`) |
| `:canopy`           | `canopy` (`CanopyShape` enum, via `canopy_from_id`) |
| `:height-range`     | `height_range` (`[f32; 2]`) |
| `:stem-radius-base` | `stem_radius_base` |
| `:stem-radius-top`  | `stem_radius_top` |
| `:leaf-count`       | `leaf_count` (`u32`) |
| `:leaf-size`        | `leaf_size` |
| `:color-base`       | `color_base` (`[r g b]`) |
| `:color-tip`        | `color_tip` (`[r g b]`) |

`:canopy` is a keyword id mapped to `CanopyShape` (7 variants:
`:blade :fan :radial :cone :dome :column :carpet`). A key a profile **omits** inherits
the engine `moss()` fallback (the same profile the `from_json_str` bridge defaults to
for unknown enum ids), read off the real Rust — never transcribed — so the partial
merge is provably the same.

## API

```rust
use kami_vegetation_scene as scene;

// All profiles, straight from the shipped vegetation.edn.
let profiles = scene::shipped_profiles()?;          // BTreeMap<String, ProfileSpec>

// One profile as a real engine struct (== taxonomy::grass(), proven by tests).
let p: kami_vegetation::taxonomy::TaxonomicProfile = scene::shipped_profile("grass")?
    .to_taxonomic_profile();

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::profiles_from_edn(my_edn)?;      // id → ProfileSpec
let spec  = scene::profile_from_edn(my_edn, "fern")?;
```

Loaders: `profiles_from_edn` / `profile_from_edn` / `shipped_profiles` /
`shipped_profile`, plus `to_taxonomic_profile(&spec)` (and `ProfileSpec::
to_taxonomic_profile`). Enum id maps: `canopy_from_id` / `id_from_canopy` /
`division_from_id` / `habit_from_id` / `arrangement_from_id` / `leaf_shape_from_id`.
The oracle / fallback is `builtin_profile(name)`, built from the Rust
`taxonomy::{grass,…,moss}()`. `VEGETATION_EDN` is the shipped string (`include_str!`),
so consumers can bake it into a wasm bundle. `ALL_PROFILE_NAMES` lists the seven ids.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-vegetation-scene
```

- `tests/profile_parity.rs::profiles_edn_matches_builtin` — for each of the seven
  profiles, every field loaded from `vegetation.edn` equals the value read off the REAL
  Rust `taxonomy::{grass,…,moss}()` (called, not transcribed). Preset values are exact
  f32-representable decimals; parity uses exact `==` on the whole `ProfileSpec` (a tiny
  `1e-6` epsilon also guards the float scalars against int/float coercion). `leaf_count`
  (`u32`) and the `CanopyShape` / taxonomy enums are compared exactly.
- `converter_matches_hardcoded` — the reconstructed `TaxonomicProfile`'s every field
  matches the hardcoded builder.
- `omitted_fields_inherit_defaults` — a profile that omits keys reproduces the engine
  `moss()` default for those keys (the tolerant merge contract).
- `tolerant_parse_errors` + unit tests cover tolerant parse: missing key → default
  merge, int → float / u32 coercion, unknown profile → `ProfileNotFound`, non-map root
  → `NotAMap`, missing table → `NoProfiles`, canopy id round-trip.

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is
the authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run
the native test suite via the workspace alias: `cargo test-native -p kami-vegetation-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
