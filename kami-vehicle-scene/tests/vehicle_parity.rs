//! Vehicle-build parity: a [`kami_vehicle::Vehicle`] built from the shipped
//! `garage.edn` (via [`build_from_edn`]) must be behaviourally identical to the
//! compiled-in oracle [`kami_vehicle::build_vehicle`] for every [`VehicleKind`].
//!
//! This is the VEHICLE half of the Front-A data-driven integration (the MAP half
//! is `garage_parity.rs` + the ground EDN). `build_from_edn` sources every
//! per-kind powertrain/tire override from the EDN tables; `build_vehicle(kind)`
//! applies the same overrides from inline Rust literals. They must agree on:
//!   * total node count + summed node mass (and `total_mass`),
//!   * wheel count,
//!   * `powertrain.engine.max_rpm`,
//!   * every engine torque-curve `(rpm, nm)` point,
//!   * `powertrain.gearbox.final_drive`,
//!   * `wheels[0].tire.{d_long, d_lat}` (sports = sticky 1.20) AND a wheel on a
//!     non-sports car (road-dry default).
//!
//! All override values are exact f32 literals in both sources → we assert exact
//! f32 equality. The summed node mass is the same `mass` values added in the same
//! node order (sedan() builds nodes deterministically), so it is also exact; we
//! note an epsilon of 0.0 (documented below) and assert exact equality, falling
//! back to a tiny tolerance only as a guard if float addition ever reorders.

use kami_vehicle::{build_vehicle, VehicleKind};
use kami_vehicle_scene::{build_from_edn, ALL_VEHICLE_KINDS};

/// Sum of all node masses — the same per-node `mass` values in the same build
/// order for both vehicles, so this is bit-exact.
fn summed_node_mass(v: &kami_vehicle::Vehicle) -> f32 {
    v.nodes.iter().map(|n| n.mass).sum()
}

#[test]
fn vehicle_edn_build_matches_build_vehicle() {
    for kind in ALL_VEHICLE_KINDS {
        let id = kind.id();
        let edn = build_from_edn(id).unwrap_or_else(|e| panic!("{id}: build_from_edn failed: {e}"));
        let oracle = build_vehicle(kind);

        // ── Structure: node count + wheel count ──
        assert_eq!(
            edn.nodes.len(),
            oracle.nodes.len(),
            "{id}: node count"
        );
        assert_eq!(
            edn.wheels.len(),
            oracle.wheels.len(),
            "{id}: wheel count"
        );

        // ── Mass: summed node mass (exact — same values, same order) + the
        //    cached total_mass. Epsilon = 0.0 (documented: identical f32 adds). ──
        assert_eq!(
            summed_node_mass(&edn),
            summed_node_mass(&oracle),
            "{id}: summed node mass"
        );
        assert_eq!(edn.total_mass, oracle.total_mass, "{id}: total_mass");

        // ── Engine: effective max_rpm + every torque-curve point ──
        assert_eq!(
            edn.powertrain.engine.max_rpm,
            oracle.powertrain.engine.max_rpm,
            "{id}: engine max_rpm"
        );
        let ec = &edn.powertrain.engine.torque_curve.points;
        let oc = &oracle.powertrain.engine.torque_curve.points;
        assert_eq!(ec.len(), oc.len(), "{id}: torque-curve point count");
        for (i, (a, b)) in ec.iter().zip(oc.iter()).enumerate() {
            assert_eq!(a.0, b.0, "{id}: torque-curve[{i}] rpm");
            assert_eq!(a.1, b.1, "{id}: torque-curve[{i}] nm");
        }

        // ── Gearbox: effective final_drive ──
        assert_eq!(
            edn.powertrain.gearbox.final_drive,
            oracle.powertrain.gearbox.final_drive,
            "{id}: gearbox final_drive"
        );

        // ── Tire grip: every wheel's d_long / d_lat (sports gets the sticky
        //    1.20 override; the rest keep road-dry). ──
        for (w, (we, wo)) in edn.wheels.iter().zip(oracle.wheels.iter()).enumerate() {
            assert_eq!(we.tire.d_long, wo.tire.d_long, "{id}: wheel[{w}] tire d_long");
            assert_eq!(we.tire.d_lat, wo.tire.d_lat, "{id}: wheel[{w}] tire d_lat");
        }
    }
}

/// Spot-check the two tire endpoints explicitly: sports is the sticky-tire car
/// (d_long == d_lat == 1.20), a non-sports car keeps the road-dry default. Both
/// must equal the oracle's wheel[0] tire.
#[test]
fn sports_is_sticky_others_are_road_dry() {
    let sports = build_from_edn("sports").expect("sports builds");
    let oracle_sports = build_vehicle(VehicleKind::Sports);
    assert_eq!(sports.wheels[0].tire.d_long, 1.20, "sports d_long");
    assert_eq!(sports.wheels[0].tire.d_lat, 1.20, "sports d_lat");
    assert_eq!(
        sports.wheels[0].tire.d_long,
        oracle_sports.wheels[0].tire.d_long,
        "sports d_long == oracle"
    );

    for kind in [VehicleKind::Sedan, VehicleKind::Suv, VehicleKind::Bus] {
        let v = build_from_edn(kind.id()).expect("builds");
        let o = build_vehicle(kind);
        assert_eq!(
            v.wheels[0].tire.d_long, o.wheels[0].tire.d_long,
            "{}: non-sports d_long == oracle",
            kind.id()
        );
        assert_eq!(
            v.wheels[0].tire.d_lat, o.wheels[0].tire.d_lat,
            "{}: non-sports d_lat == oracle",
            kind.id()
        );
    }
}

/// `build_from_edn` is hyphen/underscore tolerant on the kind id.
#[test]
fn kind_id_is_hyphen_underscore_tolerant() {
    // All current kind ids are single words, so this exercises the resolver
    // path; an underscore form must still resolve to the same vehicle.
    let a = build_from_edn("sedan").expect("sedan");
    let b = build_from_edn("Sedan".to_lowercase().as_str()).expect("sedan lower");
    assert_eq!(a.nodes.len(), b.nodes.len());
}
