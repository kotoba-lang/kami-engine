# kami-atmosphere-scene

Data-tier crate that makes `kami-atmosphere` **data-driven**: the sky/weather
system's named weather presets (overcast / clear) live as **canonical EDN** here,
loaded into the real `kami_atmosphere::Weather` struct at startup.

It is the atmosphere sibling of [`kami-vehicle-scene`](../kami-vehicle-scene)
(`SurfaceTable::from_edn`, `build_from_edn`) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **hot rendering / wind / cloud simulation stays native Rust;
only init-time CONFIG/DATA moves to EDN.** `kami-atmosphere`'s sky shading, day/night
cycle, wind gusting, and cloud scroll run per-frame and stay in Rust untouched. But a
**weather preset** is read **once** when a scene boots — it just seeds a `Weather`
snapshot the per-frame `tick` then evolves. That seed is config, so it moves out of
the hardcoded `Weather::overcast()` / `Weather::clear()` builders into EDN that an
author (or a fork, or Datomic) can edit without recompiling.

This crate is **additive**: `kami-atmosphere`'s compiled-in `Weather::overcast()` and
`Weather::clear()` are *not* deleted. They remain the `builtin_preset()` fallback
**and** the parity oracle — every value in the shipped EDN is asserted equal to the
hardcoded Rust it mirrors, so the EDN is the source of truth while behaviour is
provably unchanged.

```
kami-atmosphere  (per-frame sim, hardcoded overcast()/clear() = oracle/fallback)  ← unchanged
      ▲ path dep
kami-atmosphere-scene  (this crate: data/weather.edn  +  from_edn loaders)        ← additive
      ▲ path dep
(open-world apps — seed Weather from this EDN at boot)
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/weather.edn`](data/weather.edn) | `:weather/presets` (`:overcast`, `:clear`) | `Weather::overcast()`, `Weather::clear()` |

### Schema

```clojure
{:weather/presets
 {:overcast {:cloud-coverage  0.95
             :cloud-density   1.0
             :cloud-altitude  250.0
             :cloud-sharpness 1.2
             :wind-speed      8.0
             :wind-gust       0.45
             :time            0.42}
  :clear    {:cloud-coverage 0.25
             :cloud-density  0.6
             :wind-speed     4.0
             :time           0.4}}}
```

Keys are authored with hyphens (`:cloud-coverage`) — idiomatic EDN — and the loader
maps each to the matching public field on `Weather` / `CloudSystem` / `WindSystem` /
`DayNightCycle`:

| EDN key | Rust field |
|---|---|
| `:cloud-coverage`  | `clouds.coverage` |
| `:cloud-density`   | `clouds.density` |
| `:cloud-altitude`  | `clouds.altitude` |
| `:cloud-sharpness` | `clouds.sharpness` |
| `:wind-speed`      | `wind.speed` |
| `:wind-gust`       | `wind.gust_intensity` |
| `:time`            | `day_night.time` |

A key a preset **omits** inherits the engine `Default` (e.g. `:clear` omits
`:cloud-altitude` / `:cloud-sharpness` / `:wind-gust`), exactly as the hardcoded
`Weather::clear()` leaves those fields untouched. The default is read from
`Weather::default()` (never transcribed), so the partial merge is provably the same.

## API

```rust
use kami_atmosphere_scene as scene;

// All presets, straight from the shipped weather.edn.
let presets = scene::shipped_presets()?;          // BTreeMap<String, WeatherPreset>

// One preset as a real engine struct (== Weather::overcast(), proven by tests).
let w: kami_atmosphere::Weather = scene::shipped_weather("overcast")?;

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let table = scene::presets_from_edn(my_edn)?;     // id → WeatherPreset
let w     = scene::preset_weather_from_edn(my_edn, "clear")?;
```

Loaders: `presets_from_edn` / `preset_weather_from_edn` / `shipped_presets` /
`shipped_weather`, plus `preset_to_weather(&WeatherPreset) -> Weather`. The oracle /
fallback is `builtin_preset(name)`, built from the Rust `Weather::overcast()` /
`clear()`. `WEATHER_EDN` is the shipped string (`include_str!`), so consumers can bake
it into a wasm bundle.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-atmosphere-scene
```

- `tests/weather_parity.rs::weather_edn_matches_builtin` — for each preset, every
  field loaded from `weather.edn` equals the value read off the REAL Rust
  `Weather::overcast()` / `Weather::clear()` (called, not transcribed). Preset values
  are exact f32-representable decimals; parity uses a tiny `1e-6` epsilon to absorb
  int/float coercion, and exact `==` on the whole `WeatherPreset` also holds.
- `preset_to_weather_matches_hardcoded` — the reconstructed `Weather`'s touched fields
  match the hardcoded preset.
- `clear_omitted_fields_inherit_defaults` — the three keys `:clear` omits reproduce
  the engine default, matching what the hardcoded `clear()` leaves untouched.
- Unit tests cover tolerant parse: missing key → default merge, int → float coercion,
  unknown preset → `PresetNotFound`, non-map root → `NotAMap`, missing table →
  `NoPresets`.

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is
the authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run
the native test suite via the workspace alias: `cargo test-native -p kami-atmosphere-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
