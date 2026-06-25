//! Parity tests: the shipped EDN must faithfully reproduce kami-atmosphere's
//! compiled-in weather presets. This is the whole point of the data tier (ADR-0038) —
//! EDN becomes the source of truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a value loaded from
//! `weather.edn` against `Weather::overcast()` / `Weather::clear()` (called here, not
//! transcribed). Preset values are exact decimal literals (0.95, 1.0, 250.0, 1.2,
//! 8.0, 0.45, 0.42 / 0.25, 0.6, 4.0, 0.4) — all representable in f32 — so parity is
//! asserted with a tiny epsilon (1e-6) purely to guard against int/float coercion
//! noise; exact equality also holds.

use kami_atmosphere::Weather;
use kami_atmosphere_scene::{
    builtin_preset, preset_to_weather, presets_from_edn, shipped_presets, WeatherPreset,
    ALL_PRESET_NAMES, WEATHER_EDN,
};

const EPS: f32 = 1e-6;

/// Assert every field of an EDN-loaded preset equals the same field read off the
/// hardcoded `Weather` oracle.
fn assert_preset_eq(name: &str, edn: &WeatherPreset, oracle: &Weather) {
    let o = WeatherPreset::from_weather(oracle);
    let pairs: [(&str, f32, f32); 7] = [
        ("cloud_coverage", edn.cloud_coverage, o.cloud_coverage),
        ("cloud_density", edn.cloud_density, o.cloud_density),
        ("cloud_altitude", edn.cloud_altitude, o.cloud_altitude),
        ("cloud_sharpness", edn.cloud_sharpness, o.cloud_sharpness),
        ("wind_speed", edn.wind_speed, o.wind_speed),
        ("wind_gust", edn.wind_gust, o.wind_gust),
        ("time", edn.time, o.time),
    ];
    for (field, a, b) in pairs {
        assert!(
            (a - b).abs() < EPS,
            "{name}: {field} {a} != {b} (oracle)"
        );
    }
    // And the whole spec equals the oracle-derived spec (exact f32 equality).
    assert_eq!(*edn, o, "{name}: full WeatherPreset parity");
}

/// For each shipped preset (overcast, clear), every field loaded from weather.edn ==
/// the value from the Rust `Weather::overcast()` / `Weather::clear()`.
#[test]
fn weather_edn_matches_builtin() {
    let loaded = presets_from_edn(WEATHER_EDN).expect("weather.edn presets parse");
    assert_eq!(loaded.len(), 2, "both presets present in EDN");

    let overcast = Weather::overcast();
    assert_preset_eq("overcast", &loaded["overcast"], &overcast);

    let clear = Weather::clear();
    assert_preset_eq("clear", &loaded["clear"], &clear);

    // The `builtin_preset` oracle helper agrees with what we read off the structs.
    for name in ALL_PRESET_NAMES {
        let built = builtin_preset(name).expect("builtin preset");
        assert_eq!(loaded[name], built, "{name}: EDN == builtin_preset()");
    }

    // The shipped-presets convenience loader yields the same thing.
    let shipped = shipped_presets().expect("shipped presets");
    assert_eq!(shipped["overcast"], loaded["overcast"]);
    assert_eq!(shipped["clear"], loaded["clear"]);
}

/// `preset_to_weather` reconstructs a `Weather` whose touched fields equal the
/// hardcoded preset's — the real engine struct, behaviourally identical.
#[test]
fn preset_to_weather_matches_hardcoded() {
    for (name, oracle) in [("overcast", Weather::overcast()), ("clear", Weather::clear())] {
        let loaded = presets_from_edn(WEATHER_EDN).unwrap();
        let w = preset_to_weather(&loaded[name]);

        assert!((w.clouds.coverage - oracle.clouds.coverage).abs() < EPS, "{name}: coverage");
        assert!((w.clouds.density - oracle.clouds.density).abs() < EPS, "{name}: density");
        assert!((w.clouds.altitude - oracle.clouds.altitude).abs() < EPS, "{name}: altitude");
        assert!((w.clouds.sharpness - oracle.clouds.sharpness).abs() < EPS, "{name}: sharpness");
        assert!((w.wind.speed - oracle.wind.speed).abs() < EPS, "{name}: wind speed");
        assert!(
            (w.wind.gust_intensity - oracle.wind.gust_intensity).abs() < EPS,
            "{name}: gust"
        );
        assert!((w.day_night.time - oracle.day_night.time).abs() < EPS, "{name}: time");
    }
}

/// `clear` omits `:cloud-altitude` / `:cloud-sharpness` / `:wind-gust` in the EDN, and
/// the hardcoded `Weather::clear()` likewise leaves them at `CloudSystem` /
/// `WindSystem` defaults — so the partial merge must reproduce those defaults exactly.
#[test]
fn clear_omitted_fields_inherit_defaults() {
    let loaded = presets_from_edn(WEATHER_EDN).unwrap();
    let clear = &loaded["clear"];
    let d = WeatherPreset::defaults();

    // These three are NOT set by Weather::clear() — they must equal the engine default.
    assert_eq!(clear.cloud_altitude, d.cloud_altitude, "clear altitude = default");
    assert_eq!(clear.cloud_sharpness, d.cloud_sharpness, "clear sharpness = default");
    assert_eq!(clear.wind_gust, d.wind_gust, "clear gust = default");

    // Cross-check the oracle agrees the hardcoded clear() left them at default.
    let oracle = WeatherPreset::from_weather(&Weather::clear());
    assert_eq!(oracle.cloud_altitude, d.cloud_altitude);
    assert_eq!(oracle.cloud_sharpness, d.cloud_sharpness);
    assert_eq!(oracle.wind_gust, d.wind_gust);
}
