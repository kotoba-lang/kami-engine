//! kami-atmosphere-scene — EDN authoring surface for `kami-atmosphere` WEATHER CONFIG.
//!
//! The data-tier counterpart of `kami-vehicle-scene` for the sky/weather system: it
//! turns canonical `:weather/presets` EDN into real `kami_atmosphere::Weather`
//! instances, re-using the tolerant `kami-scene` accessors the same way games parse
//! `scene.edn` (missing keys fall back to defaults, namespaced keywords match on
//! `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot rendering / wind / cloud simulation stays native Rust (`kami-atmosphere`). A
//! weather preset is **init-time CONFIG** — read once when the scene boots to seed a
//! [`kami_atmosphere::Weather`] snapshot, which the per-frame `tick` then evolves — so
//! it is safe to move to EDN. `kami-atmosphere` itself stays "pure Rust + glam +
//! bytemuck, no edn dep"; the EDN dependency lives only here. The compiled-in
//! [`kami_atmosphere::Weather::overcast`] / [`kami_atmosphere::Weather::clear`]
//! presets remain as the [`builtin_preset`] fallback and are parity-tested against the
//! shipped EDN ([`crate::WEATHER_EDN`]).
//!
//! ## EDN shape (see `data/weather.edn`)
//!
//! ```edn
//! {:weather/presets
//!  {:overcast {:cloud-coverage 0.95 :cloud-density 1.0 :cloud-altitude 250.0
//!              :cloud-sharpness 1.2 :wind-speed 8.0 :wind-gust 0.45 :time 0.42}
//!   :clear    {:cloud-coverage 0.25 :cloud-density 0.6 :wind-speed 4.0 :time 0.4}}}
//! ```
//!
//! ## Hyphen field keys → Rust fields
//!
//! Keys are authored with hyphens (`:cloud-coverage`) — idiomatic EDN. The loader
//! maps each to the matching public field on `Weather` / `CloudSystem` /
//! `WindSystem` / `DayNightCycle`:
//!
//! | EDN key | Rust field |
//! |---|---|
//! | `:cloud-coverage`  | `clouds.coverage` |
//! | `:cloud-density`   | `clouds.density` |
//! | `:cloud-altitude`  | `clouds.altitude` |
//! | `:cloud-sharpness` | `clouds.sharpness` |
//! | `:wind-speed`      | `wind.speed` |
//! | `:wind-gust`       | `wind.gust_intensity` |
//! | `:time`            | `day_night.time` |
//!
//! Any key a preset omits inherits the engine's `Default` (taken from
//! [`kami_atmosphere::Weather::default`], never transcribed here), so a partial EDN
//! map merges onto the default exactly as the hardcoded preset leaves fields untouched.

use std::collections::BTreeMap;

use kami_atmosphere::Weather;
use kami_scene::{mget, num, root_map};

/// The canonical weather CONFIG shipped with this crate (the preset table).
/// This is the source of truth; the compiled-in presets are the parity-tested mirror.
pub const WEATHER_EDN: &str = include_str!("../data/weather.edn");

/// Errors raised while loading weather CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("weather EDN root is not a map")]
    NotAMap,
    /// The `:weather/presets` table was missing or not a map.
    #[error("`:weather/presets` missing or not a map")]
    NoPresets,
    /// The requested preset id was missing under `:weather/presets`.
    #[error("preset `{0}` not found under `:weather/presets`")]
    PresetNotFound(String),
}

/// One weather preset — the EDN-loaded mirror of the fields a hardcoded
/// `Weather::overcast()` / `Weather::clear()` sets. Every field is the value to
/// apply onto a [`Weather::default`] base; a field absent from the EDN keeps the
/// default (see [`WeatherPreset::from_map`]), so this is a *full* (merged) spec.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherPreset {
    /// Cloud coverage [0, 1] (0 = clear, 1 = overcast) — `clouds.coverage`.
    pub cloud_coverage: f32,
    /// Cloud density / opacity — `clouds.density`.
    pub cloud_density: f32,
    /// Cloud altitude (world units above sea level) — `clouds.altitude`.
    pub cloud_altitude: f32,
    /// Cloud edge sharpness — `clouds.sharpness`.
    pub cloud_sharpness: f32,
    /// Base wind speed (m/s) — `wind.speed`.
    pub wind_speed: f32,
    /// Gust intensity [0, 1] — `wind.gust_intensity`.
    pub wind_gust: f32,
    /// Time of day [0, 1) where 0.5 = noon — `day_night.time`.
    pub time: f32,
}

impl WeatherPreset {
    /// Build the spec from the compiled-in [`Weather`] oracle: read every field this
    /// preset describes straight off the real engine struct. This is what the EDN is
    /// parity-tested against.
    pub fn from_weather(w: &Weather) -> Self {
        Self {
            cloud_coverage: w.clouds.coverage,
            cloud_density: w.clouds.density,
            cloud_altitude: w.clouds.altitude,
            cloud_sharpness: w.clouds.sharpness,
            wind_speed: w.wind.speed,
            wind_gust: w.wind.gust_intensity,
            time: w.day_night.time,
        }
    }

    /// The default spec: every field read from [`Weather::default`]. Used as the merge
    /// base so a partial EDN preset only overrides the keys it actually carries.
    pub fn defaults() -> Self {
        Self::from_weather(&Weather::default())
    }

    /// Build a spec from one preset's EDN map, merging present keys onto
    /// [`WeatherPreset::defaults`]. Absent keys keep the engine default (mirroring the
    /// hardcoded presets, which leave untouched fields at their `Default`).
    pub fn from_map(m: &BTreeMap<kami_scene::EdnValue, kami_scene::EdnValue>) -> Self {
        let d = Self::defaults();
        // `num` returns 0.0 for an absent key, so guard each with the default.
        let or = |key: &str, fallback: f32| match mget(m, key) {
            Some(v) => num(Some(v)),
            None => fallback,
        };
        Self {
            cloud_coverage: or("cloud-coverage", d.cloud_coverage),
            cloud_density: or("cloud-density", d.cloud_density),
            cloud_altitude: or("cloud-altitude", d.cloud_altitude),
            cloud_sharpness: or("cloud-sharpness", d.cloud_sharpness),
            wind_speed: or("wind-speed", d.wind_speed),
            wind_gust: or("wind-gust", d.wind_gust),
            time: or("time", d.time),
        }
    }
}

/// Apply a [`WeatherPreset`] onto a fresh [`Weather::default`], yielding the real
/// engine struct — behaviourally identical to the hardcoded `Weather::overcast()` /
/// `Weather::clear()` (proven by `tests/weather_parity.rs`).
pub fn preset_to_weather(p: &WeatherPreset) -> Weather {
    let mut w = Weather::default();
    w.clouds.coverage = p.cloud_coverage;
    w.clouds.density = p.cloud_density;
    w.clouds.altitude = p.cloud_altitude;
    w.clouds.sharpness = p.cloud_sharpness;
    w.wind.speed = p.wind_speed;
    w.wind.gust_intensity = p.wind_gust;
    w.day_night.time = p.time;
    w
}

/// The compiled-in fallback / parity oracle: build a [`WeatherPreset`] straight from
/// the hardcoded `Weather::overcast()` / `Weather::clear()`. Returns `None` for an
/// unknown name. This is what the shipped EDN is parity-tested against.
pub fn builtin_preset(name: &str) -> Option<WeatherPreset> {
    let w = match name {
        "overcast" => Weather::overcast(),
        "clear" => Weather::clear(),
        _ => return None,
    };
    Some(WeatherPreset::from_weather(&w))
}

/// Names of the presets shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Keeping this list here (not in `kami-atmosphere`) keeps the
/// engine crate untouched.
pub const ALL_PRESET_NAMES: [&str; 2] = ["overcast", "clear"];

/// Parse the whole `:weather/presets` table from EDN `src` into a map keyed by the
/// (hyphenated) preset id, each value the merged [`WeatherPreset`].
pub fn presets_from_edn(src: &str) -> Result<BTreeMap<String, WeatherPreset>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let presets = mget(&root, "weather/presets")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoPresets)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in presets.iter() {
        let Some(id) = kami_scene::kw_key(k) else {
            continue;
        };
        let Some(m) = v.as_map() else { continue };
        by_id.insert(id, WeatherPreset::from_map(m));
    }
    Ok(by_id)
}

/// Look up a single preset by (hyphenated) id from EDN `src`, returning the real
/// [`Weather`]. Errors if the table or the named preset is absent.
pub fn preset_weather_from_edn(src: &str, name: &str) -> Result<Weather, Error> {
    presets_from_edn(src)?
        .get(name)
        .map(preset_to_weather)
        .ok_or_else(|| Error::PresetNotFound(name.to_string()))
}

/// Convenience: load all presets from the crate-shipped [`WEATHER_EDN`].
pub fn shipped_presets() -> Result<BTreeMap<String, WeatherPreset>, Error> {
    presets_from_edn(WEATHER_EDN)
}

/// Convenience: load one preset from the shipped EDN as a real [`Weather`].
pub fn shipped_weather(name: &str) -> Result<Weather, Error> {
    preset_weather_from_edn(WEATHER_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_both_presets() {
        let p = shipped_presets().expect("weather.edn presets parse");
        assert_eq!(p.len(), 2);
        for name in ALL_PRESET_NAMES {
            assert!(p.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn unknown_builtin_preset_is_none() {
        assert!(builtin_preset("does-not-exist").is_none());
    }

    #[test]
    fn unknown_preset_from_edn_is_an_error() {
        // Weather has no Debug, so match the Err variant directly rather than unwrap_err.
        match preset_weather_from_edn(WEATHER_EDN, "monsoon") {
            Err(Error::PresetNotFound(_)) => {}
            other => panic!("expected PresetNotFound, got {:?}", other.err()),
        }
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(presets_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_presets_table_is_an_error() {
        assert!(matches!(presets_from_edn("{:other 1}"), Err(Error::NoPresets)));
    }

    #[test]
    fn missing_key_falls_back_to_default() {
        // A preset that only sets coverage: every other field inherits the default.
        let p = presets_from_edn("{:weather/presets {:p {:cloud-coverage 0.5}}}").unwrap();
        let spec = &p["p"];
        let d = WeatherPreset::defaults();
        assert_eq!(spec.cloud_coverage, 0.5);
        assert_eq!(spec.cloud_density, d.cloud_density, "absent → default density");
        assert_eq!(spec.cloud_altitude, d.cloud_altitude, "absent → default altitude");
        assert_eq!(spec.wind_speed, d.wind_speed, "absent → default wind speed");
        assert_eq!(spec.time, d.time, "absent → default time");
    }

    #[test]
    fn int_coerces_to_float() {
        // `:wind-speed 6` (an int) coerces to 6.0 via kami-scene `num`.
        let p = presets_from_edn("{:weather/presets {:p {:wind-speed 6}}}").unwrap();
        assert_eq!(p["p"].wind_speed, 6.0);
    }
}
