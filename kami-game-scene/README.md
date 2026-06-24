# kami-game-scene

Data-tier crate that makes `kami-game`'s **animation system** data-driven: the
**Nintendo-style animation-state presets** (skibidi-idle / grimace-wobble /
item-pickup / sigma-idle / ohio-glitch) live as **canonical EDN** here, loaded into
the real `kami_game::animation::AnimationState` struct at startup.

It is the animation sibling of [`kami-postfx-scene`](../kami-postfx-scene) and
[`kami-character-scene`](../kami-character-scene) (and the
[`kami-vehicle-scene`](../kami-vehicle-scene) family) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors. It mirrors
**kami-postfx-scene** most closely: a preset is an **ordered list** of tagged enum
variants, and parity uses a local `PartialEq` mirror because the engine enum derives
none.

## Why (ADR-0038 / ADR-0046)

The architecture rule: **the hot per-frame integrator stays native Rust; only
init-time CONFIG/DATA moves to EDN.** `kami-game::animation`'s `AnimationClip::tick`
(the sine / elastic-ease / glitch-hash math, run every frame) stays in Rust untouched.
But an **animation preset** is read **once** when the `AnimationState` is constructed —
it just lists which clips, in which order, with which params **and which initial
runtime state** (`timer: 0.0`, `phase: Wait`, `angle: 0.0`). That recipe is config, so
it moves out of the hardcoded
`AnimationState::{skibidi_idle,grimace_wobble,item_pickup,sigma_idle,ohio_glitch}()`
factories into EDN an author (or a fork, or Datomic) can edit without recompiling.

This crate is **additive**: `kami-game`'s compiled-in preset factories are *not*
deleted. They remain the `builtin_animation()` fallback **and** the parity oracle —
every clip + every field in the shipped EDN is asserted equal to the hardcoded Rust it
mirrors, **in clip order**, so the EDN is the source of truth while behaviour is
provably unchanged.

```
kami-game::animation  (per-frame AnimationClip::tick + the hardcoded skibidi_idle()…ohio_glitch() = oracle/fallback)  ← unchanged
      ▲ path dep
kami-game-scene  (this crate: data/animations.edn  +  from_edn loaders)                                              ← additive
      ▲ path dep
(apps — assemble an AnimationState from this EDN at boot)
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/animations.edn`](data/animations.edn) | `:game/animations` (`:skibidi-idle` `:grimace-wobble` `:item-pickup` `:sigma-idle` `:ohio-glitch`) | `AnimationState::{skibidi_idle,grimace_wobble,item_pickup,sigma_idle,ohio_glitch}()` |

### Schema

Each preset is an **ordered vector** (clip order is load-bearing — the per-clip outputs
combine front-to-back via `AnimationOutput::combine`). Each clip map is tagged by
`:clip` (a keyword id naming the `AnimationClip` variant); the remaining keys are that
variant's fields, hyphenated.

```clojure
{:game/animations
 {:skibidi-idle
  [{:clip :head-bob :rise-height 2.0 :rise-time 1.0 :hold-time 0.5 :drop-time 0.5
    :wait-time 2.0 :timer 0.0 :phase :wait}
   {:clip :spinning :speed 3.0 :angle 0.0}]
  :grimace-wobble [{:clip :wobble ...} {:clip :bobbing ...}]
  :item-pickup    [{:clip :bobbing ...} {:clip :spinning ...} {:clip :pulse-glow ...}]
  :sigma-idle     []   ; completely still — that is the point
  :ohio-glitch    [{:clip :glitch :interval 0.1 :timer 0.0 :intensity 0.15 :seed 42}]}}
```

`:clip` keyword id → `AnimationClip` variant + its fields:

| `:clip` id | Variant | Fields (EDN → Rust) |
|---|---|---|
| `:bobbing`        | `Bobbing`       | `:amplitude` `:frequency` `:phase` |
| `:spinning`       | `Spinning`      | `:speed` `:angle` |
| `:squash-stretch` | `SquashStretch` | `:squash-scale` (`[x y z]`) `:stretch-scale` (`[x y z]`) `:duration` `:timer` `:active` (`bool`) |
| `:wobble`         | `Wobble`        | `:intensity` `:speed` `:phase` |
| `:pop-in`         | `PopIn`         | `:target-scale` (`[x y z]`) `:duration` `:timer` `:overshoot` |
| `:head-bob`       | `HeadBob`       | `:rise-height` `:rise-time` `:hold-time` `:drop-time` `:wait-time` `:timer` `:phase` (sub-enum keyword) |
| `:pulse-glow`     | `PulseGlow`     | `:min-scale` `:max-scale` `:speed` `:phase` |
| `:glitch`         | `Glitch`        | `:interval` `:timer` `:intensity` `:seed` (`u32`) |

The HeadBob `:phase` field is the `HeadBobPhase` **sub-enum** as a keyword id:
`:rise` / `:hold` / `:drop` / `:wait`. Variants mix CONFIG fields (`rise-height`, `speed`)
with **initial RUNTIME-STATE** fields (`:timer 0.0`, `:angle 0.0`, `:phase :wait`,
`:active false`) — the EDN captures the exact initial values the factory produces.

Keys are authored with hyphens (`:rise-height`) — idiomatic EDN — and the loader maps each
to the matching field. Numbers coerce int→float / u32. An unknown `:clip` id (or a missing
`:clip` tag) is `Error::UnknownClip`.

## API

```rust
use kami_game_scene as scene;

// All presets, straight from the shipped animations.edn.
let presets = scene::shipped_animations()?;   // BTreeMap<String, AnimationState>

// One preset as a real engine state (== AnimationState::skibidi_idle(), proven by tests).
let s: kami_game::animation::AnimationState = scene::shipped_animation("skibidi-idle")?;

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::animations_from_edn(my_edn)?;            // id → AnimationState
let glitch = scene::animation_from_edn(my_edn, "ohio-glitch")?;
```

Loaders: `animations_from_edn` / `animation_from_edn` / `shipped_animations` /
`shipped_animation`, plus `clip_from_map(&map) -> AnimationClip` (one clip) and
`clip_id(&AnimationClip)` (the inverse keyword id). The comparable projection `ClipSpec`
(a `PartialEq` mirror of `AnimationClip`, since the engine enum derives only `Debug, Clone`
+ serde) plus `ClipSpec::{from_clip,to_clip,clip_id}`, the `PhaseSpec` mirror of
`HeadBobPhase`, and `animation_specs(&state) -> Vec<ClipSpec>` let parity assert every
clip/field in order. The oracle / fallback is `builtin_animation(name)`, built from the
Rust factories. `ANIMATIONS_EDN` is the shipped string (`include_str!`), so consumers can
bake it into a wasm bundle. `ALL_ANIMATION_NAMES` lists the five ids.

> No visibility change to `kami-game` was needed: `animation` is `pub mod animation`
> (reached via the fully-qualified `kami_game::animation::*` path; it is not re-exported at
> the crate root), and `AnimationState` / `AnimationClip` / `HeadBobPhase` + all their
> fields + the preset factories are already `pub`. `AnimationState.animations` is a public
> `Vec<AnimationClip>`, so the loader rebuilds via `new()` + `.with(..)` and parity reads
> `.animations` directly. `kami-game` is left untouched.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-game-scene
```

- `tests/animation_parity.rs::animations_edn_matches_builtin` — for each of the five
  presets, the state rebuilt from `animations.edn` equals the REAL Rust
  `AnimationState::{skibidi_idle,grimace_wobble,item_pickup,sigma_idle,ohio_glitch}()`
  (called, not transcribed) — every clip + every field (incl. initial runtime state),
  **in order** (`Vec<ClipSpec>` compared element-by-element with exact `==`).
- `single_animation_from_edn_matches` — `animation_from_edn` rebuilds each preset identical
  to the hardcoded factory.
- `clipspec_round_trips_through_clip` — the `ClipSpec` ↔ `AnimationClip` projection
  round-trips losslessly.
- `tolerant_parse_errors` + unit tests cover tolerant parse: unknown clip → `UnknownClip`,
  unknown preset → `AnimationNotFound`, non-map root → `NotAMap`, missing table →
  `NoTable`, clip-id round-trip, preset clip counts (2 / 2 / 3 / 0 / 1).

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is the
authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run the
native test suite via the workspace alias: `cargo test-native -p kami-game-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
