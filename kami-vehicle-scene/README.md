# kami-vehicle-scene

Data-tier crate that makes `kami-vehicle` **data-driven**: the car-sim's surface grip
table, the multi-surface circuit map, the garage of vehicles, and the powertrain / tire
tuning all live as **canonical EDN** here, loaded into the real engine structs at startup.

It is the vehicle sibling of [`kami-live`](../kami-live) (`DanceScene::from_edn`) — a thin
`from_edn` layer over [`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **hot physics stays native Rust; only init-time CONFIG/DATA moves to
EDN.** `kami-vehicle`'s XPBD soft-body solver, Pacejka tire model, and powertrain integration
run 2 kHz and stay in Rust untouched. But the *numbers* those run on — grip coefficients,
zone layouts, vehicle dimensions, torque curves, gear ratios — are read **once** at load /
build time. Those are config, so they move out of `match` arms into EDN that an author (or a
fork, or Datomic) can edit without recompiling.

This crate is **additive**: `kami-vehicle`'s compiled-in `SurfaceKind` enum,
`MapGround::demo_circuit()`, and `garage::build()` are *not* deleted. They remain the
`builtin()` fallback **and** the parity oracle — every value in the shipped EDN is asserted
equal to the hardcoded Rust it mirrors, so the EDN is the source of truth while behaviour is
provably unchanged.

```
kami-vehicle  (hot solver, hardcoded builders = oracle/fallback)   ← unchanged
      ▲ path dep
kami-vehicle-scene  (this crate: data/*.edn  +  from_edn loaders)   ← additive
      ▲ path dep
kami-app-car-sim   (driver.etzhayyim.com — consumes the EDN at boot)
```

## Data (the source of truth)

| File | Tables | Mirrors |
|---|---|---|
| [`data/ground.edn`](data/ground.edn) | `:ground/surfaces` (8 presets: friction-mu / grip-modifier / tint / name), `:ground/map :demo-circuit` (default + 9 zones) | `SurfaceKind::{coefficients,tint,display_name}`, `MapGround::demo_circuit()` |
| [`data/garage.edn`](data/garage.edn) | `:vehicle/engines` (4), `:vehicle/gearboxes` (`manual-6`), `:vehicle/tires` (`road-dry`/`road-wet`), `:vehicle/garage` (6 cars + per-kind overrides) | `VehicleKind::spec()` + `garage::build()` powertrain/tire overrides |

Keyword ids use hyphens (`:asphalt-dry`, `:na-2-0-gasoline`); the loaders translate to the
engine's underscore ids (`asphalt_dry`) — see `surface_kind_from_value`.

## API

```rust
use kami_vehicle_scene as scene;

// Ground: surface table + the demo circuit, straight from the shipped ground.edn.
let surfaces = scene::shipped_surface_table()?;          // SurfaceTable (id → params)
let map      = scene::shipped_demo_circuit()?;           // kami_vehicle::MapGround
let mu       = surfaces.get(SurfaceKind::Ice).friction_mu;

// Vehicle: build a real soft-body car from garage.edn (behaviourally identical to
// kami_vehicle::build_vehicle(kind) — proven by tests/vehicle_parity.rs).
let car = scene::build_from_edn("sports")?;              // kami_vehicle::Vehicle

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::SurfaceTable::from_edn(my_edn)?;
let garage = scene::garage_from_edn(my_edn)?;            // id → GarageSpec
```

Loaders: `SurfaceTable::from_edn` / `map_from_edn` / `garage_from_edn` /
`engines_from_edn` / `gearboxes_from_edn` / `tires_from_edn` / `build_from_spec` /
`build_from_edn`. Each has a `builtin()` / `builtin_*` counterpart built from the Rust enum —
the fallback and the parity oracle. `GROUND_EDN` / `GARAGE_EDN` are the shipped strings
(`include_str!`), so consumers can bake them into a wasm bundle.

## Consumed by the real app

`kami-app-car-sim` (live at `driver.etzhayyim.com`) builds **both** its circuit and its
vehicle from this crate's EDN at boot, with the hardcoded builder as a fallback:

```rust
let map = kami_vehicle_scene::shipped_demo_circuit()
    .unwrap_or_else(|_| MapGround::demo_circuit());
let car = kami_vehicle_scene::build_from_edn(kind.id())
    .unwrap_or_else(|_| build(kind));
```

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-vehicle-scene     # 17 tests
```

- `tests/parity.rs` — every surface's `(friction_mu, grip_modifier)`, tint, and name match
  the enum; `demo-circuit` zones + `surface_at()` match `MapGround::demo_circuit()`.
- `tests/garage_parity.rs` — each of the 6 vehicles' SedanSpec geometry/mass, engine curve +
  rpm, gearbox final-drive, and tire coeffs match `spec()` / `build_vehicle()`.
- `tests/vehicle_parity.rs` — a full car `build_from_edn(kind)` equals `build_vehicle(kind)`
  on node/wheel counts, summed mass, `total_mass`, engine `max_rpm` + every torque point,
  gearbox final-drive, and per-wheel tire `d_long`/`d_lat`.

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is the
authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run the
native test suite via the workspace alias: `cargo test-native -p kami-vehicle-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
