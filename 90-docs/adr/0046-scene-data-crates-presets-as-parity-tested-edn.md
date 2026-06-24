# ADR-0046 — `*-scene` data crates: hardcoded presets become parity-tested EDN

- Status: accepted
- Date: 2026-06-24
- Builds on: ADR-0038 (Rust base + CLJ/Datomic game layer — hot native, config EDN),
  ADR-0040 (everything describable is EDN/Datomic), ADR-0043 (kami-live `DanceScene::from_edn`)

## Context

ADR-0040 drew one line through the engine: a thing that **describes** is EDN data; a thing
that **computes per element** stays native Rust. The render graph, materials, and tuning
constants already live as EDN.

But a large class of *description* was still trapped in Rust: **hardcoded preset tables** —
`match`/builder fns that return a config struct. Examples that shipped as code, not data:

- `kami-vehicle`: the 8 `SurfaceKind` grip coefficients, the `MapGround::demo_circuit` zones,
  the 6-car garage + powertrain/tire tuning.
- `kami-atmosphere`: `Weather::overcast()/clear()`.
- `kami-terrain`: `BiomePreset::{plains,quarry,desert,tundra}` (heightmap + splat + palette).
- `kami-vegetation`: the 7 `TaxonomicProfile` species.
- `kami-postfx`: the 4 `PostFxPipeline` presets (ordered effect lists).
- `kami-autodrive`: per-`VehicleClass` `VehicleLimits` + `AutopilotConfig`.
- `kami-character`: the 5 `HairStyle` presets.

These are init/build-time **config**, not the 2 kHz hot path. By the ADR-0040 rule they
should be EDN — but moving them must be **safe** (zero behaviour change, no risk to the hot
engine) and **consistent** (one recipe, not a bespoke loader per crate). `kami-scene`
(tolerant EDN accessors) and `kami-live` (`DanceScene::from_edn`, ADR-0043) already pointed
the way: a thin *data crate* over the engine crate.

## Decision

For each engine subsystem with hardcoded preset config, add a sibling **data-tier crate**
`kami-<sub>-scene` that makes the presets canonical EDN, loaded into the *real* engine
structs. The recipe is fixed:

1. **Canonical EDN** in `kami-<sub>-scene/data/*.edn`, shipped via `include_str!` as a
   `*_EDN` const. This is the source of truth.
2. **`from_edn` loaders** built on `kami_scene::{root_map, mget, num, vec3, kw_key}` —
   tolerant (missing key → default), hyphen-keyword ids (`:asphalt-dry`), int↔float
   coercion. They return the engine's own structs (`SurfaceTable`, `VehicleLimits`,
   `PostFxPipeline`, `HairStyle`, …), reconstructing enums via a `*_from_id` map.
3. **The engine crate is untouched.** Its hardcoded builders stay as `builtin*()` — both the
   runtime **fallback** (load fails → builtin) **and** the **parity oracle**.
4. **Parity tests are the contract.** Every shipped EDN value is asserted `==` the value from
   the *real Rust* (call `SurfaceKind::coefficients()`, `Weather::overcast()`,
   `VehicleClass::Car.limits()`, … — never transcribe). So the EDN is authoritative *and*
   behaviour is provably unchanged. Where the engine struct lacks `PartialEq`, the data crate
   carries a local `*Spec` `PartialEq` mirror (see `kami-postfx-scene`, `kami-character-scene`).
5. **Additive, isolated.** New crate + one `[workspace] members` line. No visibility changes
   to the engine crate where its preset API is already `pub` (it always was, for all 8).
6. **Verify** with `cargo test-native -p kami-<sub>-scene` (the workspace `.cargo` config
   defaults `build.target` to wasm32; the `test-native` alias runs the suite natively).

The same EDN a native loader reads is plain data a CLJS/Datomic authoring brain can produce,
fork, and `as-of` — the ADR-0040 substrate, now covering preset config too.

### Catalog (delivered)

| Data crate | Engine | EDN | Migrated |
|---|---|---|---|
| `kami-live` (ADR-0043) | kami-vrm/skeleton | — | VRM dance-scene `from_edn` (the precedent) |
| `kami-vehicle-scene` | kami-vehicle | ground.edn, garage.edn | 8 surfaces + demo-circuit map; 6-car garage + powertrain/tire. Consumed by `kami-app-car-sim` (driver.etzhayyim.com) at boot. |
| `kami-atmosphere-scene` | kami-atmosphere | weather.edn | overcast / clear |
| `kami-terrain-scene` | kami-terrain | biomes.edn | plains / quarry / desert / tundra (heightmap + splat + palette) |
| `kami-vegetation-scene` | kami-vegetation | vegetation.edn | 7 taxonomic species |
| `kami-postfx-scene` | kami-postfx | postfx.edn | 4 post-FX presets (ordered effect lists) |
| `kami-autodrive-scene` | kami-autodrive | classes.edn | per-class `VehicleLimits` + `AutopilotConfig` (the drive.gftd.ai autonomy stack) |
| `kami-character-scene` | kami-character | hair.edn | 5 hair styles |

## Consequences

- **Authoring/forking without recompiling.** A scene/Datomic snapshot can swap a car, re-tune
  grip, design a track, repaint a biome, or restyle hair as data.
- **No runtime risk.** The builtin remains the fallback; the parity test guarantees the EDN
  equals it, so a drift is a failing test, not a shipped regression.
- **One recipe.** Onboarding a new subsystem is mechanical: read the preset fns, mirror a
  sibling crate, parity-test against the real Rust. (This ADR is that recipe.)
- **Engine crates stay pure.** No EDN dependency leaks into the hot crates; the data crate
  owns `kami-scene` + `thiserror`.
- **Remaining / future:** any subsystem with a hardcoded preset table is a candidate (e.g.
  `kami-cam` stock, `kami-character` rig/anim-blueprint, `kami-game` npc). Add a `*-scene`
  crate by the recipe above. The cross-platform *executor* adopting the EDN at the GPU/native
  edge (vs. reading the builtin) proceeds per subsystem, additively (ADR-0044 pattern).
