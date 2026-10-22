# kami-terrain-scene

Data-tier crate that makes [`kotoba-lang/terrain`](https://github.com/kotoba-lang/terrain)
**data-driven**: the terrain biome presets (plains / quarry / desert / tundra) — each a
bundle of FBM heightmap params + splatmap thresholds + material colour palette — and the
default Gerstner ocean waves live as **canonical EDN**, loaded into the real `terrain`
shapes at startup.

Restored from the legacy `kami-terrain-scene` Rust crate
(`kotoba-lang/kami-engine`, deleted in PR #82 "Remove Rust workspace from kami-engine",
recovered at commit `a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`) as zero-dependency
portable CLJC, part of the clj-wgsl migration (ADR-2607010930, `com-junkawasaki/root`).

It is the terrain sibling of `kami-vehicle-scene` / `kami-atmosphere-scene` — a thin
`from-edn` layer over [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene)'s
tolerant EDN accessors (`mget` / `num` / `vec3` / `root-map` / `kw-key`).

## Why (ADR-0038)

The architecture rule: **hot heightmap / splatmap / chunk-mesh generation stays native
(Rust originally, now `kotoba-lang/terrain` CLJC); only init-time CONFIG/DATA moves to
EDN.** `terrain`'s FBM noise, splatmap blend, chunk meshing and water stay untouched. A
**biome preset** is read **once** when a chunk is generated — it just seeds a
heightmap-config + splat-thresholds + material-palette map the native generators then
consume. That seed is config, so it moves out of the hardcoded `terrain.biome` functions
into EDN an author (or a fork, or Datomic) can edit without recompiling.

This crate is **additive**: `terrain.biome`'s compiled-in `biome-heightmap-config` /
`biome-splat-thresholds` / `biome-palette` functions are not deleted. They remain the
`builtin-biome` fallback **and** the parity oracle — every value in the shipped EDN is
asserted equal to the value those functions return, so the EDN is the source of truth
while behaviour is provably unchanged.

```
terrain          (per-chunk gen, terrain.biome functions = oracle/fallback)  <- unchanged
      ^ dep
terrain-scene     (this crate: resources/biomes.edn + resources/waves.edn
                   + from-edn loaders)                                       <- additive
      ^ dep
(open-world apps — seed biome/wave config from this EDN at boot)
```

## Why it matters: the missing piece for two other restorations

This crate is referenced **by name** (`kami_terrain_scene::resolve_biome`) in both
[`kami-app-isekai`](https://github.com/kotoba-lang/kami-app-isekai) and
[`kami-app-quarry-walk`](https://github.com/kotoba-lang/kami-app-quarry-walk) — both
already restored earlier in this migration. Those crates call
`resolve_biome("plains")` / `resolve_biome("quarry")` to look up a biome preset by name,
falling back to `kami-terrain`'s compiled-in `BiomePreset` if the lookup fails, but both
restorations deliberately scoped this crate **out** of their own port (native WASM/wgpu
orchestration only, documented in their own READMEs). `terrain-scene/resolve-biome` here
is that missing piece — it lets a future integration actually wire a biome name up to
real, retunable EDN config.

## Modules restored

| Namespace | Restored from | Lines | Role |
|---|---|---|---|
| [`terrain-scene`](src/terrain_scene.cljk) | `src/lib.rs` | 398 | Biome CONFIG: `HeightmapSpec`/`SplatSpec`/`PaletteSpec`/`BiomeSpec`, EDN loaders, `builtin-biome` oracle, `resolve-biome` |
| [`terrain-scene.waves`](src/terrain_scene/waves.cljk) | `src/waves.rs` | 111 | Default Gerstner ocean-wave table as EDN, `builtin-waves` oracle |

Data: [`resources/biomes.edn`](resources/biomes.edn) (`:terrain/biomes` table: plains /
quarry / desert / tundra) and [`resources/waves.edn`](resources/waves.edn)
(`:terrain/waves` — 4 Gerstner wave trains), both also embedded as literal strings in
the corresponding namespace (`biomes-edn` / `waves-edn`) so this crate loads identically
on the JVM and in ClojureScript, kept byte-identical to the `resources/` copies.

## Schema

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

The heightmap `:seed` is **not** stored in EDN — it is supplied per-call to
`biome-spec->heightmap-config`, mirroring the original `BiomePreset::heightmap(seed)`.
Any heightmap key a biome omits inherits `terrain/default-heightmap-config`'s value for
that key (read at runtime, never transcribed), so a partial merge is provably the same.

## API

```clojure
(require '[terrain-scene :as terrain-scene]
         '[terrain-scene.waves :as waves])

;; All biomes, straight from the shipped biomes.edn.
(def biomes (terrain-scene/shipped-biomes))            ; {"plains" BiomeSpec ...}

;; One biome as the real terrain shapes (== terrain.biome functions, proven by tests).
(def spec (terrain-scene/shipped-biome "quarry"))
(def hc   (terrain-scene/biome-spec->heightmap-config spec seed))  ; terrain heightmap-config map
(def st   (terrain-scene/biome-spec->splat-thresholds spec))       ; terrain splat-thresholds map
(def mp   (terrain-scene/biome-spec->material-palette spec))       ; terrain palette map

;; Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
(def table (terrain-scene/biomes-from-edn my-edn))                 ; id -> BiomeSpec
(def d     (terrain-scene/biome-from-edn my-edn "desert"))

;; Executor-edge: what kami-app-isekai / kami-app-quarry-walk would call.
(terrain-scene/resolve-biome "plains")   ; => BiomeSpec, EDN-driven, builtin fallback
(terrain-scene/resolve-biome "quarry")   ; => BiomeSpec
(terrain-scene/resolve-biome "volcano")  ; => nil (unknown)

;; Default ocean waves as EDN -> real terrain wave maps.
(waves/shipped-waves)      ; => [wave wave wave wave], == (terrain/default-waves)
```

Loaders: `biomes-from-edn` / `biome-from-edn` / `shipped-biomes` / `shipped-biome`, plus
the converters `biome-spec->heightmap-config` / `biome-spec->splat-thresholds` /
`biome-spec->material-palette`. The oracle/fallback is `builtin-biome`, built from the
real `terrain.biome` functions. `biomes-edn` is the shipped string and `all-biome-names`
lists the four biomes. `terrain-scene.waves/waves-from-edn` / `shipped-waves` /
`builtin-waves` mirror the same pattern for the wave table.

## Tests (parity = the correctness contract)

```bash
kbb -M:test
```

20 tests / 235 assertions, 0 failures, 0 errors. Every applicable original Rust
`#[test]` (from both `src/lib.rs` + `src/waves.rs` `#[cfg(test)]` modules and
`tests/biome_parity.rs` + `tests/waves_parity.rs`) is ported 1:1, plus two
namespace-loads smoke tests and a test asserting `(resolve-biome "plains")` /
`(resolve-biome "quarry")` both resolve — the exact two names `kami-app-isekai` and
`kami-app-quarry-walk` call.

- `biomes-edn-matches-builtin` — for each biome, every field loaded from `biomes.edn`
  equals the value read off the REAL `terrain.biome` functions (called, not
  transcribed). Parity uses a `1e-6` epsilon to absorb int/float coercion noise; exact
  equality on the whole `BiomeSpec` also holds.
- `converters-match-hardcoded` — `biome-spec->heightmap-config` (seed threaded
  per-call) / `biome-spec->splat-thresholds` / `biome-spec->material-palette`
  reconstruct the real `terrain` shapes equal to the hardcoded `terrain.biome`
  functions.
- `omitted-heightmap-fields-inherit-defaults` — a biome that omits heightmap keys
  reproduces `terrain/default-heightmap-config`'s values, the tolerant-merge contract.
- `waves-edn-matches-builtin` — the shipped `waves.edn`, in order, equals
  `terrain/default-waves`.
- Unit tests cover tolerant parse: missing key -> default merge, int -> float / octave
  coercion, unknown biome -> `ex-info` `:biome-not-found`, non-map root -> `:not-a-map`,
  missing table -> `:no-biomes` / `:no-table`.

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is
the authoritative copy, pinned to the engine's behaviour.

## Dependencies

- [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene) — tolerant EDN accessors
  (`mget` / `num` / `vec3` / `root-map` / `kw-key`).
- [`kotoba-lang/terrain`](https://github.com/kotoba-lang/terrain) — the default
  heightmap-config shape, the compiled-in `terrain.biome` presets (oracle), and
  `terrain/default-waves` (oracle).

## License

Apache-2.0 / MIT (matches the original workspace).
