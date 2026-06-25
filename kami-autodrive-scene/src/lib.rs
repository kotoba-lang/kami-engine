//! kami-autodrive-scene — EDN authoring surface for `kami-autodrive`
//! PER-VEHICLE-CLASS PRESETS (the drive.gftd.ai autonomy stack's per-class config).
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` /
//! `kami-terrain-scene` / `kami-vegetation-scene` / `kami-postfx-scene` for the
//! autonomy (GNC) stack: it turns canonical `:autodrive/limits` EDN (per-class
//! kinematic envelopes) and `:autodrive/autopilot` EDN (per-class autopilot
//! tuning) into the real [`kami_autodrive::VehicleLimits`] /
//! [`kami_autodrive::AutopilotConfig`] engine structs, re-using the tolerant
//! `kami-scene` accessors the same way games parse `scene.edn` (missing keys fall
//! back to defaults, namespaced keywords match on `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! The hot GNC loop (perception / planning / control) stays native Rust
//! (`kami-autodrive`). A per-class preset is **init-time CONFIG** — read once when
//! a plant + [`kami_autodrive::Autopilot`] are constructed at boot
//! (`VehicleClass::limits()`, `AutopilotConfig::for_class(class)`) — so it is safe
//! to move to EDN. `kami-autodrive` itself stays untouched; the EDN dependency
//! lives only here. The compiled-in `VehicleClass::limits()` /
//! `AutopilotConfig::for_class()` remain as the [`builtin_limits`] /
//! [`builtin_autopilot`] fallback and are parity-tested against the shipped EDN
//! ([`CLASSES_EDN`]).
//!
//! ## EDN shape (see `data/classes.edn`)
//!
//! ```edn
//! {:autodrive/limits
//!  {:car {:max-speed 25.0 :max-accel 4.0 :max-decel 8.0 :wheelbase 2.7
//!         :max-steer 0.61 :turn-radius-ref 4.5 :footprint-radius 1.3}
//!   :ship {..} :drone {..} :aircraft {..}}
//!  :autodrive/autopilot
//!  {:car {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] ..} ..}}
//! ```
//!
//! Keys are the four class ids (`car`/`ship`/`drone`/`aircraft`); field keywords
//! are hyphenated (`:max-speed` → `max_speed`). An unknown class id is
//! [`Error::ClassNotFound`].

use std::collections::BTreeMap;

use kami_autodrive::{AutopilotConfig, VehicleClass, VehicleLimits};
use kami_scene::{kw_key, mget, num, root_map, EdnValue};

/// The canonical per-class preset CONFIG shipped with this crate (both tables).
/// This is the source of truth; the compiled-in presets are the parity-tested
/// mirror.
pub const CLASSES_EDN: &str = include_str!("../data/classes.edn");

/// The four class ids — the iteration source for `builtin`/parity. Order mirrors
/// the `enum VehicleClass` declaration order.
pub const ALL_CLASS_NAMES: [&str; 4] = ["car", "ship", "drone", "aircraft"];

/// Errors raised while loading per-class preset CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("autodrive EDN root is not a map")]
    NotAMap,
    /// A `:autodrive/<table>` table was missing or not a map.
    #[error("`:autodrive/{0}` missing or not a map")]
    NoTable(&'static str),
    /// The requested class id had no entry in the table (or is not one of the
    /// four known classes).
    #[error("class `{0}` not found")]
    ClassNotFound(String),
}

// ── small typed accessors over the tolerant kami-scene helpers ──

/// Read an `f32` field; `0.0` when absent / non-numeric (via `num`).
fn f32_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> f32 {
    num(mget(m, key))
}

/// Read a `u32` field, coercing via `num` then rounding; `0` when absent.
fn u32_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> u32 {
    match mget(m, key) {
        Some(v) => num(Some(v)).round() as u32,
        None => 0,
    }
}

/// Read a `bool` field; `false` when absent / non-bool.
fn bool_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> bool {
    mget(m, key).and_then(|x| x.as_bool()).unwrap_or(false)
}

/// Read a 2-tuple `[lo hi]`; missing components default to `0.0`.
fn pair_of(m: &BTreeMap<EdnValue, EdnValue>, key: &str) -> (f32, f32) {
    let s = mget(m, key).and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(0.0);
    (g(0), g(1))
}

/// Map a class id to the `VehicleClass` enum variant (hyphen/underscore tolerant,
/// case-insensitive on the bare name). Falls back to [`VehicleClass::Car`] for an
/// unknown id (mirroring the tolerant data-tier style); use [`try_class_from_id`]
/// for a checked lookup.
pub fn class_from_id(id: &str) -> VehicleClass {
    try_class_from_id(id).unwrap_or(VehicleClass::Car)
}

/// Checked class-id → [`VehicleClass`] lookup; `None` for an unknown id.
pub fn try_class_from_id(id: &str) -> Option<VehicleClass> {
    match id.to_ascii_lowercase().replace('_', "-").as_str() {
        "car" => Some(VehicleClass::Car),
        "ship" => Some(VehicleClass::Ship),
        "drone" => Some(VehicleClass::Drone),
        "aircraft" => Some(VehicleClass::Aircraft),
        _ => None,
    }
}

/// The hyphenated class id for a [`VehicleClass`] (inverse of [`try_class_from_id`]).
pub fn class_id(class: VehicleClass) -> &'static str {
    match class {
        VehicleClass::Car => "car",
        VehicleClass::Ship => "ship",
        VehicleClass::Drone => "drone",
        VehicleClass::Aircraft => "aircraft",
    }
}

// ── :autodrive/limits ─────────────────────────────────────────────────────────

/// The EDN-loaded mirror of a [`kami_autodrive::VehicleLimits`] preset — every
/// field of the kinematic envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitsSpec {
    pub max_speed: f32,
    pub max_accel: f32,
    pub max_decel: f32,
    pub wheelbase: f32,
    pub max_steer: f32,
    pub turn_radius_ref: f32,
    pub footprint_radius: f32,
}

impl LimitsSpec {
    /// Build from a real [`VehicleLimits`] (the `builtin_*()` oracle).
    pub fn from_limits(l: &VehicleLimits) -> Self {
        Self {
            max_speed: l.max_speed,
            max_accel: l.max_accel,
            max_decel: l.max_decel,
            wheelbase: l.wheelbase,
            max_steer: l.max_steer,
            turn_radius_ref: l.turn_radius_ref,
            footprint_radius: l.footprint_radius,
        }
    }

    /// Produce the real [`VehicleLimits`] this spec describes.
    pub fn to_vehicle_limits(&self) -> VehicleLimits {
        VehicleLimits {
            max_speed: self.max_speed,
            max_accel: self.max_accel,
            max_decel: self.max_decel,
            wheelbase: self.wheelbase,
            max_steer: self.max_steer,
            turn_radius_ref: self.turn_radius_ref,
            footprint_radius: self.footprint_radius,
        }
    }

    /// Parse one limits map (every field via the tolerant accessors).
    fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        Self {
            max_speed: f32_of(m, "max-speed"),
            max_accel: f32_of(m, "max-accel"),
            max_decel: f32_of(m, "max-decel"),
            wheelbase: f32_of(m, "wheelbase"),
            max_steer: f32_of(m, "max-steer"),
            turn_radius_ref: f32_of(m, "turn-radius-ref"),
            footprint_radius: f32_of(m, "footprint-radius"),
        }
    }
}

/// Free-function form: turn a [`LimitsSpec`] into the real [`VehicleLimits`].
pub fn to_vehicle_limits(spec: &LimitsSpec) -> VehicleLimits {
    spec.to_vehicle_limits()
}

/// The compiled-in fallback / parity oracle: the real `VehicleClass::limits()`.
/// This is what the shipped EDN is parity-tested against.
pub fn builtin_limits(class: VehicleClass) -> VehicleLimits {
    class.limits()
}

/// Resolve the `:autodrive/<table>` sub-map of the root, or [`Error::NoTable`].
fn autodrive_table<'a>(
    root: &'a BTreeMap<EdnValue, EdnValue>,
    table: &'static str,
) -> Result<&'a BTreeMap<EdnValue, EdnValue>, Error> {
    mget(root, &format!("autodrive/{table}"))
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable(table))
}

/// Parse the whole `:autodrive/limits` table from EDN `src` into a map keyed by
/// the (hyphenated) class id, each value the loaded [`LimitsSpec`].
pub fn limits_specs_from_edn(src: &str) -> Result<BTreeMap<String, LimitsSpec>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = autodrive_table(&root, "limits")?;
    let mut out = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };
        out.insert(id, LimitsSpec::from_map(m));
    }
    Ok(out)
}

/// Parse the `:autodrive/limits` table into `id -> VehicleLimits` (the real engine
/// struct), the data-driven counterpart of `VehicleClass::limits()`.
pub fn limits_from_edn(src: &str) -> Result<BTreeMap<String, VehicleLimits>, Error> {
    Ok(limits_specs_from_edn(src)?
        .into_iter()
        .map(|(id, spec)| (id, spec.to_vehicle_limits()))
        .collect())
}

/// Look up one class's [`VehicleLimits`] (hyphen/underscore tolerant) from EDN.
pub fn limits_for_from_edn(src: &str, name: &str) -> Result<VehicleLimits, Error> {
    let id = name.to_ascii_lowercase().replace('_', "-");
    let table = limits_from_edn(src)?;
    table
        .get(&id)
        .or_else(|| table.get(name))
        .copied()
        .ok_or_else(|| Error::ClassNotFound(name.to_string()))
}

/// Convenience: load all per-class limits from the crate-shipped [`CLASSES_EDN`].
pub fn shipped_limits() -> Result<BTreeMap<String, VehicleLimits>, Error> {
    limits_from_edn(CLASSES_EDN)
}

/// Convenience: load one class's limits from the shipped EDN.
pub fn shipped_limits_for(name: &str) -> Result<VehicleLimits, Error> {
    limits_for_from_edn(CLASSES_EDN, name)
}

// ── :autodrive/autopilot ──────────────────────────────────────────────────────

/// The EDN-loaded mirror of an [`AutopilotConfig`] preset's tunable fields. The
/// `limits` field of the real `AutopilotConfig` is sourced from the matching
/// `:autodrive/limits` entry at build time (`for_class` pulls it from
/// `class.limits()`), so it is NOT duplicated here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutopilotSpec {
    pub grid_half_extent: f32,
    pub grid_res: f32,
    pub z_band: (f32, f32),
    pub replan_period: u32,
    pub goal_tol: f32,
    pub emergency_cone: f32,
    pub lateral_accel: f32,
    pub brake_margin: f32,
    pub dynamic_obstacles: bool,
    pub camera_z_band: (f32, f32),
    pub stuck_limit: u32,
    pub recovery_ticks: u32,
    /// `None` = a normal stopping vehicle; `Some(r)` = orbit the goal at radius r.
    pub loiter_radius: Option<f32>,
}

impl AutopilotSpec {
    /// Build from a real [`AutopilotConfig`] (the `builtin_*()` oracle). The
    /// `limits` field is intentionally dropped — it is rebuilt from the class's
    /// `:autodrive/limits` entry.
    pub fn from_config(c: &AutopilotConfig) -> Self {
        Self {
            grid_half_extent: c.grid_half_extent,
            grid_res: c.grid_res,
            z_band: c.z_band,
            replan_period: c.replan_period,
            goal_tol: c.goal_tol,
            emergency_cone: c.emergency_cone,
            lateral_accel: c.lateral_accel,
            brake_margin: c.brake_margin,
            dynamic_obstacles: c.dynamic_obstacles,
            camera_z_band: c.camera_z_band,
            stuck_limit: c.stuck_limit,
            recovery_ticks: c.recovery_ticks,
            loiter_radius: c.loiter_radius,
        }
    }

    /// Produce the real [`AutopilotConfig`] this spec describes, attaching the
    /// supplied `limits` (resolved from the matching `:autodrive/limits` entry).
    pub fn to_autopilot_config(&self, limits: VehicleLimits) -> AutopilotConfig {
        AutopilotConfig {
            limits,
            grid_half_extent: self.grid_half_extent,
            grid_res: self.grid_res,
            z_band: self.z_band,
            replan_period: self.replan_period,
            goal_tol: self.goal_tol,
            emergency_cone: self.emergency_cone,
            lateral_accel: self.lateral_accel,
            brake_margin: self.brake_margin,
            dynamic_obstacles: self.dynamic_obstacles,
            camera_z_band: self.camera_z_band,
            stuck_limit: self.stuck_limit,
            recovery_ticks: self.recovery_ticks,
            loiter_radius: self.loiter_radius,
        }
    }

    /// Parse one autopilot map (every field via the tolerant accessors).
    fn from_map(m: &BTreeMap<EdnValue, EdnValue>) -> Self {
        Self {
            grid_half_extent: f32_of(m, "grid-half-extent"),
            grid_res: f32_of(m, "grid-res"),
            z_band: pair_of(m, "z-band"),
            replan_period: u32_of(m, "replan-period"),
            goal_tol: f32_of(m, "goal-tol"),
            emergency_cone: f32_of(m, "emergency-cone"),
            lateral_accel: f32_of(m, "lateral-accel"),
            brake_margin: f32_of(m, "brake-margin"),
            dynamic_obstacles: bool_of(m, "dynamic-obstacles"),
            camera_z_band: pair_of(m, "camera-z-band"),
            stuck_limit: u32_of(m, "stuck-limit"),
            recovery_ticks: u32_of(m, "recovery-ticks"),
            // Absent `:loiter-radius` = a normal stopping vehicle (None).
            loiter_radius: mget(m, "loiter-radius").map(|x| num(Some(x))),
        }
    }
}

/// The compiled-in fallback / parity oracle: the real
/// `AutopilotConfig::for_class()`. This is what the shipped EDN is parity-tested
/// against.
pub fn builtin_autopilot(class: VehicleClass) -> AutopilotConfig {
    AutopilotConfig::for_class(class)
}

/// Parse the whole `:autodrive/autopilot` table from EDN `src` into a map keyed by
/// the (hyphenated) class id, each value the loaded [`AutopilotSpec`] (without its
/// `limits` — resolve those from [`limits_specs_from_edn`]).
pub fn autopilot_specs_from_edn(src: &str) -> Result<BTreeMap<String, AutopilotSpec>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = autodrive_table(&root, "autopilot")?;
    let mut out = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };
        out.insert(id, AutopilotSpec::from_map(m));
    }
    Ok(out)
}

/// Parse the `:autodrive/autopilot` table into `id -> AutopilotConfig` (the real
/// engine struct), resolving each class's `limits` from the `:autodrive/limits`
/// table in the same EDN. The data-driven counterpart of
/// `AutopilotConfig::for_class()`.
pub fn autopilot_from_edn(src: &str) -> Result<BTreeMap<String, AutopilotConfig>, Error> {
    let limits = limits_from_edn(src)?;
    let specs = autopilot_specs_from_edn(src)?;
    let mut out = BTreeMap::new();
    for (id, spec) in specs {
        // Resolve this class's limits from the limits table; error if absent so a
        // mismatched/partial EDN doesn't silently ship a zeroed envelope.
        let l = limits
            .get(&id)
            .copied()
            .ok_or_else(|| Error::ClassNotFound(id.clone()))?;
        out.insert(id, spec.to_autopilot_config(l));
    }
    Ok(out)
}

/// Look up one class's [`AutopilotConfig`] (hyphen/underscore tolerant) from EDN.
pub fn autopilot_for_from_edn(src: &str, name: &str) -> Result<AutopilotConfig, Error> {
    let id = name.to_ascii_lowercase().replace('_', "-");
    let table = autopilot_from_edn(src)?;
    table
        .get(&id)
        .or_else(|| table.get(name))
        .cloned()
        .ok_or_else(|| Error::ClassNotFound(name.to_string()))
}

/// Convenience: load all per-class autopilot configs from the shipped [`CLASSES_EDN`].
pub fn shipped_autopilot() -> Result<BTreeMap<String, AutopilotConfig>, Error> {
    autopilot_from_edn(CLASSES_EDN)
}

/// Convenience: load one class's autopilot config from the shipped EDN.
pub fn shipped_autopilot_for(name: &str) -> Result<AutopilotConfig, Error> {
    autopilot_for_from_edn(CLASSES_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_classes() {
        let l = shipped_limits().expect("classes.edn parse");
        assert_eq!(l.len(), 4);
        for name in ALL_CLASS_NAMES {
            assert!(l.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn class_id_round_trips() {
        for name in ALL_CLASS_NAMES {
            let c = try_class_from_id(name).expect("known class");
            assert_eq!(class_id(c), name);
        }
        assert!(try_class_from_id("submarine").is_none());
        assert_eq!(class_from_id("submarine"), VehicleClass::Car); // tolerant fallback
        assert_eq!(class_from_id("AIRCRAFT"), VehicleClass::Aircraft);
    }

    #[test]
    fn unknown_class_from_edn_is_an_error() {
        assert!(matches!(
            limits_for_from_edn(CLASSES_EDN, "submarine"),
            Err(Error::ClassNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(limits_specs_from_edn("42"), Err(Error::NotAMap)));
        assert!(matches!(autopilot_specs_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            limits_specs_from_edn("{:other 1}"),
            Err(Error::NoTable("limits"))
        ));
        assert!(matches!(
            autopilot_specs_from_edn("{:other 1}"),
            Err(Error::NoTable("autopilot"))
        ));
    }

    #[test]
    fn missing_field_defaults_to_zero() {
        // A class map missing :max-decel degrades to 0.0 (tolerant parse), not a panic.
        let edn = "{:autodrive/limits {:car {:max-speed 25.0}}}";
        let l = limits_from_edn(edn).expect("partial parse");
        let car = l.get("car").expect("car present");
        assert_eq!(car.max_speed, 25.0);
        assert_eq!(car.max_decel, 0.0); // absent → default
    }

    #[test]
    fn only_aircraft_loiters() {
        let ap = shipped_autopilot().expect("autopilot parse");
        assert_eq!(ap["aircraft"].loiter_radius, Some(200.0));
        for name in ["car", "ship", "drone"] {
            assert_eq!(ap[name].loiter_radius, None, "{name} does not loiter");
        }
    }
}
