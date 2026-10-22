# kami-autodrive-scene

EDN authoring surface for `kami-autodrive` PER-VEHICLE-CLASS PRESETS (the
drive.gftd.ai autonomy stack's per-class GNC config).

Restored from the legacy `kami-engine/kami-autodrive-scene` Rust crate
(deleted in `kotoba-lang/kami-engine` PR #82 "Remove Rust workspace from
kami-engine") to a zero-dependency portable `.cljc` namespace, per
ADR-2607010930 (`com-junkawasaki/root`).

## What it does

The data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` /
`kami-terrain-scene` / `kami-vegetation-scene` / `kami-postfx-scene` for the
autonomy (GNC) stack: it turns canonical `:autodrive/limits` EDN (per-class
kinematic envelopes) and `:autodrive/autopilot` EDN (per-class autopilot
tuning) into vehicle limits / autopilot config data, one preset per vehicle
class (`car`, `ship`, `drone`, `aircraft`), re-using the tolerant `scene`
(`kami-scene`) accessors the same way games parse `scene.edn` — missing keys
fall back to defaults, namespaced keywords match on `ns/name`, ints coerce to
floats.

The shipped preset config (`autodrive-scene/classes-edn`) is parity-tested
against a `builtin-limits` / `builtin-autopilot` oracle for every field of
every class.

## Dependency relationships

- **`kotoba-lang/scene`** (real dependency): provides the tolerant EDN
  accessors `kw-key` / `mget` / `num` / `root-map` used to parse
  `classes-edn`.
- **`kotoba-lang/kami-autodrive`** (duck-typed, NOT a real dependency): the
  original Rust crate depended on `kami-autodrive` for the real
  `VehicleLimits` / `AutopilotConfig` / `VehicleClass` structs and used their
  compiled-in `VehicleClass::limits()` / `AutopilotConfig::for_class()` as the
  `builtin_*()` parity oracle. `kotoba-lang/kami-autodrive` did not exist yet
  at restoration time (it was being restored in parallel by a sibling task),
  so this namespace does not depend on it. `builtin-limits` /
  `builtin-autopilot` are instead duck-typed local maps mirroring the exact
  values from the original `VehicleClass::limits()` /
  `AutopilotConfig::for_class()`, matching the pattern used for
  `kotoba-lang/kami-vehicle-scene` against a not-yet-existing `kami-vehicle`.
  `VehicleLimits` / `AutopilotConfig` are represented as plain hyphen-keyed
  maps rather than a foreign engine struct.

## Contents

- `src/autodrive_scene.cljk` — ~380 lines. Namespace `autodrive-scene`.
- `resources/classes.edn` — copy of the original `data/classes.edn` fixture
  (also inlined as `autodrive-scene/classes-edn` for zero-dependency
  portability, matching Rust's `include_str!`).
- `test/autodrive_scene_test.cljk` — all 11 original Rust `#[test]`s (7 from
  `src/lib.rs`, 4 from `tests/class_parity.rs`) ported 1:1, plus 1
  namespace-loads smoke test: 12 tests / 396 assertions, 0 failures.

## Error handling

Rust `Result<T, Error>` is ported as tagged maps rather than an exception
hierarchy: `(autodrive-scene/error? x)` distinguishes an error map
(`{:kind :not-a-map}`, `{:kind :no-table :table "limits"}`,
`{:kind :class-not-found :class "submarine"}`) from a successful result map.
