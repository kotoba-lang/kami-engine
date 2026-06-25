# kami-autodrive-scene

Data-tier crate that makes `kami-autodrive` **data-driven**: the **per-vehicle-class
presets** of the drive.gftd.ai autonomy (GNC) stack — the kinematic envelope per
class (`car` / `ship` / `drone` / `aircraft`) and the autopilot tuning that wraps it
— live as **canonical EDN** here, loaded into the real
`kami_autodrive::VehicleLimits` / `AutopilotConfig` structs at startup.

It is the autonomy sibling of [`kami-vehicle-scene`](../kami-vehicle-scene),
[`kami-atmosphere-scene`](../kami-atmosphere-scene),
[`kami-terrain-scene`](../kami-terrain-scene),
[`kami-vegetation-scene`](../kami-vegetation-scene) and
[`kami-postfx-scene`](../kami-postfx-scene) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **the hot GNC loop (perception / planning / control) stays
native Rust; only init-time CONFIG/DATA moves to EDN.** `kami-autodrive`'s
per-tick `Autopilot::step` (occupancy grid, A* planner, pure-pursuit + PID) stays
in Rust untouched. But a **per-class preset** is read **once** when a plant + an
`Autopilot` are constructed at boot — `VehicleClass::limits()` and
`AutopilotConfig::for_class(class)`. That recipe is config, so it moves out of the
hardcoded match arms into EDN that an author (or a fork, or Datomic) can edit
without recompiling.

This crate is **additive**: `kami-autodrive`'s compiled-in `limits()` /
`for_class()` are *not* deleted. They remain the `builtin_limits()` /
`builtin_autopilot()` fallback **and** the parity oracle — every field in the
shipped EDN is asserted equal to the hardcoded Rust it mirrors, so the EDN is the
source of truth while behaviour is provably unchanged.

```
kami-autodrive  (GNC loop + hardcoded limits()/for_class() = oracle/fallback)  ← unchanged
      ▲ path dep
kami-autodrive-scene  (this crate: data/classes.edn  +  from_edn loaders)      ← additive
      ▲ path dep
(apps — build a plant + Autopilot from this EDN at boot)
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/classes.edn`](data/classes.edn) | `:autodrive/limits` (`:car` `:ship` `:drone` `:aircraft`) | `VehicleClass::limits()` |
| [`data/classes.edn`](data/classes.edn) | `:autodrive/autopilot` (same four ids) | `AutopilotConfig::for_class()` |

### Schema

Keys are the four class ids; field keywords are hyphenated (`:max-speed` →
`max_speed`). Ints coerce to floats; `[lo hi]` vectors read as `(f32, f32)` bands.

```clojure
{:autodrive/limits
 {:car {:max-speed 25.0 :max-accel 4.0 :max-decel 8.0 :wheelbase 2.7
        :max-steer 0.61 :turn-radius-ref 4.5 :footprint-radius 1.3}
  :ship {..} :drone {..} :aircraft {..}}
 :autodrive/autopilot
 {:car {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
        :goal-tol 1.3 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
        :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
        :recovery-ticks 60}
  ;; only the aircraft loiters:
  :aircraft {.. :loiter-radius 200.0}}}
```

`:autodrive/limits` — every field of `VehicleLimits`:

| `:field` | Rust field | meaning |
|---|---|---|
| `:max-speed`        | `max_speed`        | cruising speed ceiling (m/s) |
| `:max-accel`        | `max_accel`        | forward acceleration ceiling (m/s²) |
| `:max-decel`        | `max_decel`        | braking deceleration ceiling (m/s², positive) |
| `:wheelbase`        | `wheelbase`        | effective turning length (m) |
| `:max-steer`        | `max_steer`        | steering angle ceiling (rad) |
| `:turn-radius-ref`  | `turn_radius_ref`  | effective min turning radius (m) for the pursuit controller |
| `:footprint-radius` | `footprint_radius` | collision footprint radius (m) for C-space inflation |

`:autodrive/autopilot` — every field of `AutopilotConfig` except `limits`, which is
resolved from the matching `:autodrive/limits` entry (exactly as `for_class` pulls
it from `class.limits()`): `:grid-half-extent` `:grid-res` `:z-band` (`[lo hi]`)
`:replan-period` (`u32`) `:goal-tol` `:emergency-cone` `:lateral-accel`
`:brake-margin` `:dynamic-obstacles` (`bool`) `:camera-z-band` (`[lo hi]`)
`:stuck-limit` (`u32`) `:recovery-ticks` (`u32`) `:loiter-radius` (absent = a normal
stopping vehicle / `None`; present only for the aircraft).

`:goal-tol` is authored as the **resolved** value `footprint_radius.max(1.0)`
(car 1.3 / ship 6.0 / drone 1.0 / aircraft 8.0), matching `for_class`. An unknown
class id is `Error::ClassNotFound`.

## API

```rust
use kami_autodrive_scene as scene;

// All per-class limits, straight from the shipped classes.edn.
let limits = scene::shipped_limits()?;               // BTreeMap<String, VehicleLimits>

// One class as a real engine struct (== VehicleClass::Car.limits(), proven by tests).
let car: kami_autodrive::VehicleLimits = scene::shipped_limits_for("car")?;

// Autopilot config per class (limits resolved from the limits table):
let ap = scene::shipped_autopilot_for("aircraft")?;  // AutopilotConfig (loiters)

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::limits_from_edn(my_edn)?;         // id → VehicleLimits
let class = scene::class_from_id("drone");           // VehicleClass::Drone
```

Loaders: `limits_from_edn` / `limits_for_from_edn` / `shipped_limits` /
`shipped_limits_for` (+ `limits_specs_from_edn` for the `LimitsSpec` form, and
`to_vehicle_limits`), plus the autopilot loaders `autopilot_from_edn` /
`autopilot_for_from_edn` / `shipped_autopilot` / `shipped_autopilot_for` (+
`autopilot_specs_from_edn` → `AutopilotSpec`). `class_from_id` / `try_class_from_id`
/ `class_id` map between ids and `VehicleClass`. The oracle / fallback is
`builtin_limits(class)` / `builtin_autopilot(class)`, built from the Rust
`VehicleClass::limits()` / `AutopilotConfig::for_class()`. `CLASSES_EDN` is the
shipped string (`include_str!`) so consumers can bake it into a wasm bundle;
`ALL_CLASS_NAMES` lists the four ids.

> No visibility change to `kami-autodrive` was needed: `VehicleLimits` and
> `AutopilotConfig` already have all-public fields, so the loader builds them with
> struct literals and parity reads them directly. `kami-autodrive` is left
> untouched. `AutopilotConfig` derives only `Debug, Clone` (no `PartialEq`), so
> autopilot parity compares via the `PartialEq` mirror `AutopilotSpec` (every field
> except `limits`) plus the `limits` field directly.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-autodrive-scene
```

- `tests/class_parity.rs::limits_edn_matches_builtin` — for each of the four
  classes, every `VehicleLimits` field loaded from `classes.edn` equals the REAL
  Rust `VehicleClass::<X>.limits()` (called, not transcribed), exact f32 `==`.
- `autopilot_edn_matches_builtin` — every `AutopilotConfig` field (via the
  `AutopilotSpec` mirror + the `limits` field) equals `AutopilotConfig::for_class()`.
- `single_limits_from_edn_matches` — `shipped_limits_for` resolves each class
  identical to the hardcoded `limits()`.
- `tolerant_parse_errors` + unit tests cover tolerant parse: unknown class →
  `ClassNotFound`, non-map root → `NotAMap`, missing table → `NoTable`, missing
  field → default (0.0).

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN
is the authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`.
Run the native test suite via the workspace alias: `cargo test-native -p
kami-autodrive-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
