# kotoba-lang/kami-atmosphere-scene

Zero-dep portable `.cljc` — restored from the legacy `kami-engine/kami-atmosphere-scene` Rust
crate (deleted in kotoba-lang/kami-engine PR #82, "Remove Rust workspace from kami-engine") as
part of the **clj-wgsl migration** (ADR-2607010930, `com-junkawasaki/root`).

## Status

Restored. Ports the full original crate (`src/lib.rs`, recovered from commit
`a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`) to zero-dep portable CLJC:

- `src/atmosphere_scene.cljk` (namespace `atmosphere-scene`) — the EDN authoring surface for
  `kotoba-lang/atmosphere` weather config. Parses `:weather/presets` EDN via
  `kotoba-lang/scene`'s tolerant accessors (`scene/mget` / `scene/num` / `scene/root-map` /
  `scene/kw-key`) and merges each preset's fields onto `atmosphere/default-weather` (the CLJC
  mirror of the original `kami_atmosphere::Weather::default()` shape) — any key a preset omits
  keeps the `atmosphere` default, never a hardcoded/transcribed value.
- `resources/weather.edn` — the canonical preset table (`overcast` / `clear`), byte-identical to
  the original crate's `data/weather.edn`. Also embedded as a literal string constant
  (`atmosphere-scene/weather-edn`) directly in the source so the namespace loads identically on
  the JVM and in ClojureScript without resource-loading/`slurp` portability concerns.

## Dependency relationship

This crate is the data-tier counterpart of `kami-vehicle-scene` for the sky/weather system:

- **`kotoba-lang/scene`** — tolerant EDN accessor primitives (`kw-key`/`mget`/`num`/`vec3`/
  `root-map`): missing keys fall back to defaults, namespaced keywords match on `ns/name`,
  numbers coerce int<->float.
- **`kotoba-lang/atmosphere`** — the CLJC port of the native `kami-atmosphere` engine: provides
  `default-weather` (the merge base) and the `weather-overcast`/`weather-clear` compiled-in
  presets, which this crate's `builtin-preset` uses as the fallback/parity oracle.

`atmosphere-scene` depends on both; it does not modify or vendor either.

## EDN key -> `atmosphere` map path

| EDN key             | `atmosphere` path          |
|----------------------|-----------------------------|
| `:cloud-coverage`    | `[:clouds :coverage]`       |
| `:cloud-density`     | `[:clouds :density]`        |
| `:cloud-altitude`    | `[:clouds :altitude]`       |
| `:cloud-sharpness`   | `[:clouds :sharpness]`      |
| `:wind-speed`        | `[:wind :speed]`             |
| `:wind-gust`         | `[:wind :gust-intensity]`   |
| `:time`              | `[:day-night :time]`        |

## Public API

- `weather-edn` — the shipped EDN source (literal string constant).
- `weather->preset` / `preset->weather` — convert between a real `atmosphere` Weather map and a
  flat WeatherPreset spec (`:cloud-coverage` etc).
- `weather-preset-defaults` — the default WeatherPreset spec, derived from
  `atmosphere/default-weather`.
- `presets-from-edn` / `preset-weather-from-edn` — parse presets (or one preset) from arbitrary
  EDN source, throwing `ex-info` (`:atmosphere-scene/error` of `:not-a-map` / `:no-presets` /
  `:preset-not-found`) on failure.
- `shipped-presets` / `shipped-weather` — convenience loaders against the shipped `weather-edn`.
- `builtin-preset` — the compiled-in fallback/parity oracle (`atmosphere/weather-overcast` /
  `atmosphere/weather-clear`), by name.
- `resolve-weather` — executor-edge resolver: shipped EDN first, builtin fallback on parse
  failure, `nil` for an unknown name.
- `all-preset-names` — `["overcast" "clear"]`.

All 8 original Rust `#[test]`s ported 1:1 to `test/atmosphere_scene_test.cljk` (+1 smoke test) —
9 tests / 22 assertions, 0 failures.

## Develop

```bash
kbb -M:test
```
