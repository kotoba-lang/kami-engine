# kami-vehicle-scene

EDN authoring surface for `kami-vehicle` GROUND + GARAGE/POWERTRAIN
CONFIG. Restored as zero-dependency portable `.cljc` from the legacy
`kami-vehicle-scene` Rust crate in `kotoba-lang/kami-engine` (deleted in
PR #82 "Remove Rust workspace from kami-engine"; source recoverable at
commit `a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`), per ADR-2607010930.

## What it does

Two namespaces, one per original Rust module:

- **`vehicle-scene`** (`src/vehicle_scene.cljk`, 267 lines) — ported
  from `src/lib.rs`. Turns canonical `:ground/surfaces` EDN (an 8-entry
  friction/grip/tint/name table) and `:ground/map` EDN (the
  `demo-circuit` zone map) into duck-typed `SurfaceParams` /
  `MapGround`/`SurfaceZone` maps, plus `surface-at` (last-matching-zone
  wins, so a carved-out ice/sand/mud patch overrides the broader asphalt
  strip it sits inside).
- **`vehicle-scene.garage`** (`src/vehicle_scene/garage.cljk`, 521
  lines) — ported from `src/garage.rs`. Turns canonical `:vehicle/*` EDN
  (4 engine torque-curve presets, 1 gearbox preset, 2 Pacejka tire
  presets, and 6 garage vehicle `SedanSpec`s — sedan/hatchback/suv/
  sports/pickup/bus) into duck-typed `EngineSpec`/`GearboxSpec`/
  `TireSpec`/`LayoutSpec`/`GarageSpec` maps, `build-from-spec`/
  `build-from-edn` (resolve a vehicle kind's per-kind powertrain/tire
  overrides — effective max-rpm, gearbox final-drive, sticky tire
  d-long/d-lat — into a build PLAN), and the `builtin-*`/`*-builtin`
  compiled-in mirrors.

Both re-use the tolerant `scene` accessors (`kotoba-lang/scene`) the
same way a kami-clj game parses `scene.edn`: missing keys fall back to
defaults, namespaced keywords match on `ns/name`, integers coerce to
doubles.

The surface grip table, the demo-circuit zone map, and the garage/
powertrain presets are all **init-time / build-time CONFIG** — read once
at load or vehicle-construction time, never touched by a 2 kHz physics
solver (ADR-0038, inherited from the original crate) — so they are safe
to author as data.

## Duck-typing decision (no hard dependency on `kotoba-lang/kami-vehicle`)

`kami-vehicle-scene`'s Rust source paired with `kami-vehicle` (via
`kami-vehicle = { path = "../kami-vehicle" }` in the original
`Cargo.toml`) — the domain crate owning `SurfaceKind`, `MapGround`,
`Engine`, `Gearbox`, `PacejkaParams`, `DrivelineLayout`, and the
soft-body `Vehicle`/`SedanSpec` builders. At the time this crate was
restored, `kotoba-lang/kami-vehicle` was being restored **in parallel**
by another migration pass and was not guaranteed to exist yet.

Rather than block on it, this port **locally duck-types** every shape it
needs as a plain CLJC map, inferred entirely from `kami-vehicle-scene`'s
own Rust source (struct field usage, `#[test]` assertions) — no
`kami-vehicle` source was consulted or required:

- `SurfaceParams`, `SurfaceZone`/`MapGround` (`vehicle-scene`)
- `Engine` (via `to-engine`), `Gearbox` (via `to-gearbox`),
  `PacejkaParams` (via `to-pacejka`), `DrivelineLayout` (via
  `layout-from-value`/`LayoutSpec`), `SedanSpec` (via `to-sedan-spec`)
  (`vehicle-scene.garage`)

`deps.edn` depends only on `kotoba-lang/scene`. A future pass can
refactor these `to-*` conversions to build real
`kotoba-lang/kami-vehicle` records once that crate is stable and its
struct shapes are confirmed.

**Scope note — no soft-body physics build.** The original
`build_from_spec`/`build_from_edn` called
`kami_vehicle::models::sedan::sedan(&spec)` to assemble a full soft-body
node-graph `Vehicle` — physics-engine logic living entirely in
`kami-vehicle`, not in `kami-vehicle-scene` itself, and not inferable
from this crate's own source. This port's `build-from-spec`/
`build-from-edn` instead resolve a **build PLAN**
(`{:name :sedan-spec :engine :gearbox :tire}`) with every per-kind
override already applied — i.e. everything the original
`kami-vehicle-scene` crate itself owned (EDN -> resolved CONFIG),
stopping exactly at the boundary where it delegated to
`kami_vehicle::models::sedan::sedan()`.

**Builtin/parity oracle note.** The original crate's `builtin_*()`
functions and `GarageSpec::builtin(kind)` read real `kami-vehicle`
constructors (`TorqueCurve::na_2_0_gasoline()`, `Gearbox::manual_6()`,
`PacejkaParams::road_dry()`, `VehicleKind::spec()`,
`kami_vehicle::build_vehicle(kind)`) as the parity oracle the shipped
EDN was tested against. Those aren't available here either, but the
original crate's own parity tests (`garage_edn_matches_builtin`,
`surfaces_edn_matches_builtin`, etc.) *guarantee* the shipped EDN is
numerically identical to those compiled-in builders — so this port's
`builtin-engine`/`builtin-gearbox`/`builtin-tire`/`garage-spec-builtin`/
`builtin-surface-table`/`builtin-demo-circuit` simply re-load the shipped
EDN rather than hand-transcribing a second, independent copy of the same
numbers.

## Shipped data

`resources/kami_vehicle_scene/ground.edn` and
`resources/kami_vehicle_scene/garage.edn` hold the canonical
`:ground/*` and `:vehicle/*` tables (byte-for-byte the same content as
the original crate's `data/ground.edn` and `data/garage.edn`). The same
text is embedded as the `vehicle-scene/ground-edn` and
`vehicle-scene.garage/garage-edn` string constants — the CLJC analogue
of the original's `include_str!(...)` — so both namespaces do no runtime
file IO and stay portable to cljs/wasm.

## Dependency

- [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene) — the
  tolerant EDN accessors (`kw-key`, `mget`, `num`, `vec3`, `root-map`)
  used to parse both EDN tables. Pinned at commit
  `b0ca0ba9134dc8e57ebcb1d82d51829456d8b703`.

## Tests

`test/vehicle_scene_test.cljk` (91 lines) ports all 5 original
`#[test]`s from `src/lib.rs`, adapts `tests/parity.rs`'s 2 tests to
shipped-EDN self-consistency checks (no real `kami_vehicle` oracle
available), plus a namespace-loads smoke test — 8 tests.

`test/vehicle_scene/garage_test.cljk` (142 lines) ports all 3 original
`#[test]`s from `src/garage.rs`, adapts `tests/garage_parity.rs`'s 4
tests and `tests/vehicle_parity.rs`'s 3 tests to shipped-EDN
self-consistency checks, plus a differential-resolution test and a
namespace-loads smoke test — 12 tests.

**20 tests / 193 assertions, 0 failures, 0 errors** (`kbb -M:test`).
