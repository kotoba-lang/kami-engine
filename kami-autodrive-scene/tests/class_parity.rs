//! Parity tests: the shipped EDN must faithfully reproduce kami-autodrive's
//! compiled-in per-vehicle-class presets — every `VehicleLimits` field, and every
//! `AutopilotConfig` field. This is the whole point of the data tier (ADR-0038):
//! EDN becomes the source of truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a value loaded from
//! `classes.edn` against `VehicleClass::<X>.limits()` /
//! `AutopilotConfig::for_class(<X>)` (CALLED here, never transcribed).
//!
//! `kami_autodrive::VehicleLimits` derives `PartialEq`, so limits parity is a
//! direct `==`. `AutopilotConfig` derives only `Debug, Clone` (no `PartialEq`), so
//! the autopilot parity projects both the loaded config and the oracle config into
//! the `PartialEq` mirror `AutopilotSpec` and compares those, plus asserts the
//! resolved `limits` field equals the oracle's directly.
//!
//! Preset values are exact decimal literals (25.0, 0.61, 0.025-free here…), all
//! representable in f32, so parity is asserted with exact `==` (no epsilon needed).

use kami_autodrive::{AutopilotConfig, VehicleClass, VehicleLimits};
use kami_autodrive_scene::{
    autopilot_from_edn, builtin_autopilot, builtin_limits, class_id, limits_from_edn,
    shipped_autopilot, shipped_autopilot_for, shipped_limits, shipped_limits_for, AutopilotSpec,
    Error, ALL_CLASS_NAMES, CLASSES_EDN,
};

/// The four classes, paired with their id, as the iteration source.
const CLASSES: [VehicleClass; 4] = [
    VehicleClass::Car,
    VehicleClass::Ship,
    VehicleClass::Drone,
    VehicleClass::Aircraft,
];

/// Assert two `VehicleLimits` are field-for-field equal (exact f32 ==).
fn assert_limits_eq(name: &str, got: &VehicleLimits, want: &VehicleLimits) {
    assert_eq!(got.max_speed, want.max_speed, "{name}: max_speed");
    assert_eq!(got.max_accel, want.max_accel, "{name}: max_accel");
    assert_eq!(got.max_decel, want.max_decel, "{name}: max_decel");
    assert_eq!(got.wheelbase, want.wheelbase, "{name}: wheelbase");
    assert_eq!(got.max_steer, want.max_steer, "{name}: max_steer");
    assert_eq!(got.turn_radius_ref, want.turn_radius_ref, "{name}: turn_radius_ref");
    assert_eq!(got.footprint_radius, want.footprint_radius, "{name}: footprint_radius");
    // And the whole struct (VehicleLimits derives PartialEq).
    assert_eq!(got, want, "{name}: full VehicleLimits parity");
}

/// For each class, every `VehicleLimits` field loaded from `classes.edn` equals
/// the value from the REAL Rust `VehicleClass::<X>.limits()`.
#[test]
fn limits_edn_matches_builtin() {
    let loaded = limits_from_edn(CLASSES_EDN).expect("classes.edn parse");
    assert_eq!(loaded.len(), 4, "all classes present in EDN");

    for class in CLASSES {
        let id = class_id(class);
        let got = loaded.get(id).expect("class in EDN");
        let want = class.limits(); // REAL Rust oracle (called, not transcribed)
        assert_limits_eq(id, got, &want);

        // The `builtin_limits` oracle helper agrees with the direct call.
        assert_eq!(builtin_limits(class), want, "{id}: builtin_limits == limits()");
    }

    // The shipped-limits convenience loader yields the same thing.
    let shipped = shipped_limits().expect("shipped limits");
    for name in ALL_CLASS_NAMES {
        assert_eq!(shipped[name], loaded[name], "{name}: shipped == loaded");
    }
}

/// `shipped_limits_for` resolves one class identical to the hardcoded `limits()`.
#[test]
fn single_limits_from_edn_matches() {
    for class in CLASSES {
        let id = class_id(class);
        let got = shipped_limits_for(id).expect("limits for class");
        assert_limits_eq(id, &got, &class.limits());
    }
}

/// Assert two `AutopilotConfig` are field-for-field equal. `AutopilotConfig` has no
/// `PartialEq`, so compare the `PartialEq` projection `AutopilotSpec` (every field
/// except `limits`) AND the `limits` field directly.
fn assert_autopilot_eq(name: &str, got: &AutopilotConfig, want: &AutopilotConfig) {
    assert_limits_eq(name, &got.limits, &want.limits);
    assert_eq!(
        AutopilotSpec::from_config(got),
        AutopilotSpec::from_config(want),
        "{name}: full AutopilotConfig parity (every non-limits field)"
    );
    // Spot-check the load-bearing derived fields explicitly for a clear failure.
    assert_eq!(got.goal_tol, want.goal_tol, "{name}: goal_tol");
    assert_eq!(got.loiter_radius, want.loiter_radius, "{name}: loiter_radius");
    assert_eq!(got.grid_half_extent, want.grid_half_extent, "{name}: grid_half_extent");
    assert_eq!(got.z_band, want.z_band, "{name}: z_band");
    assert_eq!(got.camera_z_band, want.camera_z_band, "{name}: camera_z_band");
    assert_eq!(got.dynamic_obstacles, want.dynamic_obstacles, "{name}: dynamic_obstacles");
    assert_eq!(got.stuck_limit, want.stuck_limit, "{name}: stuck_limit");
    assert_eq!(got.recovery_ticks, want.recovery_ticks, "{name}: recovery_ticks");
}

/// For each class, every `AutopilotConfig` field loaded from `classes.edn` equals
/// the value from the REAL Rust `AutopilotConfig::for_class(<X>)`.
#[test]
fn autopilot_edn_matches_builtin() {
    let loaded = autopilot_from_edn(CLASSES_EDN).expect("classes.edn parse");
    assert_eq!(loaded.len(), 4, "all classes present in EDN");

    for class in CLASSES {
        let id = class_id(class);
        let got = loaded.get(id).expect("class in EDN");
        let want = AutopilotConfig::for_class(class); // REAL Rust oracle
        assert_autopilot_eq(id, got, &want);

        // The `builtin_autopilot` oracle helper agrees with the direct call.
        assert_autopilot_eq(id, &builtin_autopilot(class), &want);
    }

    // The shipped-autopilot convenience loaders agree.
    let shipped = shipped_autopilot().expect("shipped autopilot");
    for class in CLASSES {
        let id = class_id(class);
        assert_autopilot_eq(id, &shipped[id], &AutopilotConfig::for_class(class));
        let one = shipped_autopilot_for(id).expect("one config");
        assert_autopilot_eq(id, &one, &AutopilotConfig::for_class(class));
    }
}

/// Tolerant-parse errors: unknown class → error, non-map root → error, missing
/// table → error; a missing field degrades to its default rather than panicking.
#[test]
fn tolerant_parse_errors() {
    // Unknown class id.
    assert!(matches!(
        shipped_limits_for("submarine"),
        Err(Error::ClassNotFound(_))
    ));
    // Non-map root.
    assert!(matches!(limits_from_edn("123"), Err(Error::NotAMap)));
    assert!(matches!(autopilot_from_edn("123"), Err(Error::NotAMap)));
    // Missing table.
    assert!(matches!(limits_from_edn("{:x 1}"), Err(Error::NoTable("limits"))));
    // `autopilot_from_edn` resolves the limits table first (to attach each
    // config's `limits`), so a fully-empty doc surfaces the limits table as
    // missing before the autopilot table.
    assert!(matches!(
        autopilot_from_edn("{:x 1}"),
        Err(Error::NoTable("limits"))
    ));
    // With a limits table present but no autopilot table, the autopilot table is
    // the one reported missing.
    assert!(matches!(
        autopilot_from_edn("{:autodrive/limits {:car {:max-speed 1.0}}}"),
        Err(Error::NoTable("autopilot"))
    ));

    // Missing key → default/inherit (0.0), not a panic.
    let partial = limits_from_edn("{:autodrive/limits {:car {:max-speed 9.0}}}")
        .expect("partial limits parse");
    assert_eq!(partial["car"].max_speed, 9.0);
    assert_eq!(partial["car"].footprint_radius, 0.0); // absent → default
}
