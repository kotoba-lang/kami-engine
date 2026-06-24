# kami-postfx-scene

Data-tier crate that makes `kami-postfx` **data-driven**: the **post-processing
pipeline presets** (nintendo / retro / final-fantasy / baminiku-character) live as
**canonical EDN** here, loaded into the real `kami_postfx::PostFxPipeline` struct at
startup.

It is the post-fx sibling of [`kami-vehicle-scene`](../kami-vehicle-scene),
[`kami-atmosphere-scene`](../kami-atmosphere-scene),
[`kami-terrain-scene`](../kami-terrain-scene) and
[`kami-vegetation-scene`](../kami-vegetation-scene) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **hot fullscreen passes / GPU uniform packing stay native
Rust; only init-time CONFIG/DATA moves to EDN.** `kami-postfx`'s per-frame effect
passes and `*Params` uniform structs (`BloomParams`, `SSAOParams`, `GodRaysParams`, …)
stay in Rust untouched. But a **post-fx preset** is read **once** when the pipeline is
assembled at boot — it just lists which effects, in which order, with which params.
That recipe is config, so it moves out of the hardcoded
`PostFxPipeline::{nintendo,retro,final_fantasy,baminiku_character}()` builders into
EDN that an author (or a fork, or Datomic) can edit without recompiling.

This crate is **additive**: `kami-postfx`'s compiled-in preset builders are *not*
deleted. They remain the `builtin_preset()` fallback **and** the parity oracle — every
effect + every param in the shipped EDN is asserted equal to the hardcoded Rust it
mirrors, **in pipeline order**, so the EDN is the source of truth while behaviour is
provably unchanged.

```
kami-postfx  (per-frame passes + *Params, hardcoded nintendo()…baminiku_character() = oracle/fallback)  ← unchanged
      ▲ path dep
kami-postfx-scene  (this crate: data/postfx.edn  +  from_edn loaders)                                    ← additive
      ▲ path dep
(apps — assemble a PostFxPipeline from this EDN at boot)
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/postfx.edn`](data/postfx.edn) | `:postfx/presets` (`:nintendo` `:retro` `:final-fantasy` `:baminiku-character`) | `PostFxPipeline::{nintendo,retro,final_fantasy,baminiku_character}()` |

### Schema

Each preset is an **ordered vector** (the pipeline order is load-bearing — effects
compose front-to-back). Each effect map is tagged by `:effect` (a keyword id naming
the `PostEffect` variant); the remaining keys are that variant's fields, hyphenated.

```clojure
{:postfx/presets
 {:nintendo
  [{:effect :bloom    :threshold 0.8 :intensity 0.3 :radius 4.0}
   {:effect :outline  :color [0.15 0.15 0.15 1.0] :width 1.5 :depth-threshold 0.1}
   {:effect :vignette :intensity 0.15 :radius 0.8}]
  :retro              [{:effect :pixelate :pixel-size 4.0} {:effect :crt ...}]
  :final-fantasy      [...]    ; 10 effects
  :baminiku-character [...]}}  ; 6 effects
```

`:effect` keyword id → `PostEffect` variant + its fields:

| `:effect` id | Variant | Fields (EDN → Rust) |
|---|---|---|
| `:bloom`                | `Bloom`               | `:threshold` `:intensity` `:radius` |
| `:outline`              | `Outline`             | `:color` (`[r g b a]`) `:width` `:depth-threshold` |
| `:vignette`             | `Vignette`            | `:intensity` `:radius` |
| `:crt`                  | `CRT`                 | `:scanline-intensity` `:curvature` |
| `:color-grade`          | `ColorGrade`          | `:lift` `:gamma` `:gain` (each `[r g b]`) |
| `:pixelate`             | `Pixelate`            | `:pixel-size` |
| `:ssao`                 | `SSAO`                | `:radius` `:bias` `:intensity` `:samples` (`u32`) |
| `:depth-of-field`       | `DepthOfField`        | `:focal-distance` `:focal-range` `:bokeh-radius` `:bokeh-shape` (`u32`) |
| `:ssr`                  | `SSR`                 | `:max-distance` `:steps` (`u32`) `:thickness` `:fade-edge` |
| `:aces-tonemap`         | `ACESTonemap`         | `:exposure` `:curve` (`u32`) |
| `:film-grain`           | `FilmGrain`           | `:intensity` `:size` |
| `:chromatic-aberration` | `ChromaticAberration` | `:intensity` `:samples` (`u32`) |
| `:god-rays`             | `GodRays`             | `:density` `:weight` `:decay` `:exposure` `:light-pos` (`[x y]`) |

Keys are authored with hyphens (`:depth-threshold`) — idiomatic EDN — and the loader
maps each to the matching field on the `PostEffect` variant. Numbers coerce int→float
/ u32; vectors are read with `kami_scene::vec3` and `[f32;2]` / `[f32;4]` siblings. An
unknown `:effect` id (or a missing `:effect` tag) is `Error::UnknownEffect`.

## API

```rust
use kami_postfx_scene as scene;

// All presets, straight from the shipped postfx.edn.
let presets = scene::shipped_presets()?;            // BTreeMap<String, PostFxPipeline>

// One preset as a real engine pipeline (== PostFxPipeline::nintendo(), proven by tests).
let p: kami_postfx::PostFxPipeline = scene::shipped_preset("nintendo")?;

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::presets_from_edn(my_edn)?;       // id → PostFxPipeline
let ff    = scene::preset_from_edn(my_edn, "final-fantasy")?;
```

Loaders: `presets_from_edn` / `preset_from_edn` / `shipped_presets` /
`shipped_preset`, plus `effect_from_map(&map) -> PostEffect` (one effect) and
`effect_id(&PostEffect)` (the inverse keyword id). The comparable projection
`EffectSpec` (a `PartialEq` mirror of `PostEffect`, since the engine enum derives only
`Debug, Clone`) plus `EffectSpec::{from_post_effect,to_post_effect,effect_id}` and
`pipeline_specs(&pipeline) -> Vec<EffectSpec>` let parity assert every effect/field in
order. The oracle / fallback is `builtin_preset(name)`, built from the Rust
`PostFxPipeline::{nintendo,…,baminiku_character}()`. `POSTFX_EDN` is the shipped string
(`include_str!`), so consumers can bake it into a wasm bundle. `ALL_PRESET_NAMES` lists
the four ids.

> No visibility change to `kami-postfx` was needed: `PostFxPipeline.effects` is already
> a public `Vec<PostEffect>`, so the loader rebuilds via `new()` + `add(..)` and parity
> reads `.effects` directly. `kami-postfx` is left untouched.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-postfx-scene
```

- `tests/postfx_parity.rs::presets_edn_matches_builtin` — for each of the four presets,
  the pipeline rebuilt from `postfx.edn` equals the REAL Rust
  `PostFxPipeline::{nintendo,retro,final_fantasy,baminiku_character}()` (called, not
  transcribed) — every effect + every param, **in order** (`Vec<EffectSpec>` compared
  element-by-element with exact f32 `==`).
- `single_preset_from_edn_matches` — `preset_from_edn` rebuilds each preset identical to
  the hardcoded builder.
- `effectspec_round_trips_through_post_effect` — the `EffectSpec` ↔ `PostEffect`
  projection round-trips losslessly.
- `tolerant_parse_errors` + unit tests cover tolerant parse: unknown effect →
  `UnknownEffect`, unknown preset → `PresetNotFound`, non-map root → `NotAMap`, missing
  table → `NoPresets`, effect-id round-trip, preset effect counts (3 / 2 / 10 / 6).

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is
the authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run
the native test suite via the workspace alias: `cargo test-native -p kami-postfx-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
