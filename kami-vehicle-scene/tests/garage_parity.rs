//! Parity tests: the shipped `garage.edn` must faithfully reproduce
//! kami-vehicle's compiled-in GARAGE specs + POWERTRAIN/tire tables. This is the
//! Front-A premise — EDN becomes the source of truth with *behaviour unchanged*.
//!
//! Each test compares EDN-loaded values to the ACTUAL Rust source of truth, all
//! reached through public API:
//!   * `VehicleKind::spec()`            — the SedanSpec geometry / mass / layout
//!   * `kami_vehicle::build_vehicle(k)` — the per-kind powertrain/tire OVERRIDES
//!     that only `garage::build()` applies (effective max_rpm, final_drive,
//!     sticky tire d_long/d_lat). `spec()` alone does NOT carry these, so the
//!     built Vehicle's `powertrain`/`wheels` are the closest public oracle.
//!   * `TorqueCurve::{na_2_0_gasoline,turbo_2_0}()`, the inline pickup/bus
//!     curves from build(), `Gearbox::manual_6()`, `PacejkaParams::{road_dry,
//!     road_wet}()` — the preset constructors.
//!
//! All literals in the Rust source are exact f32 → we assert exact equality
//! (no tolerance) unless noted.

use kami_vehicle::{
    build_vehicle, DrivelineLayout, Gearbox, PacejkaParams, TorqueCurve, VehicleKind,
};
use kami_vehicle_scene::{
    builtin_engine, builtin_gearbox, builtin_tire, engines_from_edn, garage_from_edn,
    gearboxes_from_edn, tires_from_edn, GarageSpec, LayoutSpec, ALL_VEHICLE_KINDS, GARAGE_EDN,
};

/// For each of the 6 VehicleKinds, every SedanSpec field loaded from EDN must
/// equal the value from the Rust `spec()`, and every per-kind powertrain/tire
/// OVERRIDE must equal the value from the built Vehicle (the public oracle).
#[test]
fn garage_edn_matches_builtin() {
    let loaded = garage_from_edn(GARAGE_EDN).expect("garage.edn parses");
    assert_eq!(loaded.len(), 6, "all 6 vehicles present in EDN");

    for kind in ALL_VEHICLE_KINDS {
        let id = kind.id();
        let edn = loaded.get(id).unwrap_or_else(|| panic!("EDN missing {id}"));
        let spec = kind.spec();

        // ── SedanSpec geometry / mass (exact f32 literals) ──
        assert_eq!(edn.wheelbase, spec.wheelbase, "{id}: wheelbase");
        assert_eq!(edn.track_width, spec.track_width, "{id}: track_width");
        assert_eq!(edn.ride_height, spec.ride_height, "{id}: ride_height");
        assert_eq!(edn.roof_height, spec.roof_height, "{id}: roof_height");
        assert_eq!(edn.overhang_front, spec.overhang_front, "{id}: overhang_front");
        assert_eq!(edn.overhang_rear, spec.overhang_rear, "{id}: overhang_rear");
        assert_eq!(edn.mass_chassis, spec.mass_chassis, "{id}: mass_chassis");
        assert_eq!(edn.mass_engine, spec.mass_engine, "{id}: mass_engine");
        assert_eq!(edn.mass_cabin, spec.mass_cabin, "{id}: mass_cabin");
        assert_eq!(edn.wheel_radius, spec.wheel_radius, "{id}: wheel_radius");
        assert_eq!(edn.wheel_width, spec.wheel_width, "{id}: wheel_width");
        assert_eq!(edn.turbo, spec.turbo, "{id}: turbo");

        // Driveline layout (incl. the AWD front_split).
        assert_eq!(
            edn.layout,
            LayoutSpec::from_layout(spec.layout),
            "{id}: layout"
        );

        // ── Per-kind powertrain/tire OVERRIDES, oracle = build_vehicle(kind) ──
        let v = build_vehicle(kind);

        // Effective final_drive (build() overrides per kind).
        assert_eq!(
            edn.final_drive,
            v.powertrain.gearbox.final_drive,
            "{id}: final_drive"
        );

        // Effective max_rpm. The EDN authors `:max-rpm` only when build()
        // overrides it; when omitted (sedan/suv) the GarageSpec.max_rpm is the
        // 0.0 "use engine preset" sentinel. Reconcile: the EDN's effective
        // max_rpm = override if present, else the engine preset's own max_rpm.
        let engine_preset = builtin_engine(&edn.engine)
            .unwrap_or_else(|| panic!("{id}: unknown engine id {}", edn.engine));
        let edn_effective_max_rpm = if edn.max_rpm != 0.0 {
            edn.max_rpm
        } else {
            engine_preset.max_rpm
        };
        assert_eq!(
            edn_effective_max_rpm, v.powertrain.engine.max_rpm,
            "{id}: effective max_rpm"
        );

        // Effective tire d_long / d_lat (sports gets the 1.20 sticky override).
        let tire0 = v.wheels[0].tire;
        let base = PacejkaParams::road_dry();
        let edn_d_long = edn.tire_d_long.unwrap_or(base.d_long);
        let edn_d_lat = edn.tire_d_lat.unwrap_or(base.d_lat);
        assert_eq!(edn_d_long, tire0.d_long, "{id}: tire d_long");
        assert_eq!(edn_d_lat, tire0.d_lat, "{id}: tire d_lat");

        // The whole struct equals the builtin mirror assembled from Rust.
        assert_eq!(edn, &GarageSpec::builtin(kind), "{id}: full GarageSpec parity");
    }

    // Sports is the one that should carry the sticky-tire override.
    let sports = loaded.get("sports").unwrap();
    assert_eq!(sports.tire_d_long, Some(1.20), "sports d_long override");
    assert_eq!(sports.tire_d_lat, Some(1.20), "sports d_lat override");

    // SUV is the AWD car.
    let suv = loaded.get("suv").unwrap();
    assert_eq!(suv.layout, LayoutSpec::Awd { front_split: 0.45 }, "suv AWD");
    // …and round-trips back to the real DrivelineLayout.
    assert!(matches!(
        suv.layout.to_layout(),
        DrivelineLayout::Awd { front_split } if front_split == 0.45
    ));
}

/// Each engine preset referenced by a garage car must reproduce the Rust preset:
/// torque-curve points + idle/max rpm + inertia/friction.
#[test]
fn engines_edn_matches_builtin() {
    let engines = engines_from_edn(GARAGE_EDN).expect("engines parse");
    assert_eq!(engines.len(), 4, "4 engine presets in EDN");

    for (id, edn) in &engines {
        let builtin = builtin_engine(id).unwrap_or_else(|| panic!("unknown engine {id}"));
        assert_eq!(edn.idle_rpm, builtin.idle_rpm, "{id}: idle_rpm");
        assert_eq!(edn.max_rpm, builtin.max_rpm, "{id}: max_rpm");
        assert_eq!(edn.inertia, builtin.inertia, "{id}: inertia");
        assert_eq!(edn.friction, builtin.friction, "{id}: friction");
        assert_eq!(
            edn.torque_curve.len(),
            builtin.torque_curve.len(),
            "{id}: torque-curve point count"
        );
        for (i, (a, b)) in edn.torque_curve.iter().zip(builtin.torque_curve.iter()).enumerate() {
            assert_eq!(a.0, b.0, "{id}: curve[{i}] rpm");
            assert_eq!(a.1, b.1, "{id}: curve[{i}] nm");
        }
        // Full struct parity.
        assert_eq!(edn, &builtin, "{id}: full EngineSpec parity");
    }

    // Direct spot-check vs the named preset constructors (not transcription).
    let na = &engines["na-2-0-gasoline"];
    assert_eq!(na.torque_curve, TorqueCurve::na_2_0_gasoline().points);
    let turbo = &engines["turbo-2-0"];
    assert_eq!(turbo.torque_curve, TorqueCurve::turbo_2_0().points);

    // The pickup/bus curves are inline in garage.rs build() — oracle = the
    // effective curve on the built Vehicle.
    let pickup_v = build_vehicle(VehicleKind::Pickup);
    assert_eq!(
        engines["pickup-v6"].torque_curve,
        pickup_v.powertrain.engine.torque_curve.points,
        "pickup-v6 curve == build_vehicle(Pickup) curve"
    );
    assert_eq!(
        engines["pickup-v6"].max_rpm, pickup_v.powertrain.engine.max_rpm,
        "pickup-v6 max_rpm"
    );
    let bus_v = build_vehicle(VehicleKind::Bus);
    assert_eq!(
        engines["bus-diesel"].torque_curve,
        bus_v.powertrain.engine.torque_curve.points,
        "bus-diesel curve == build_vehicle(Bus) curve"
    );
    assert_eq!(
        engines["bus-diesel"].max_rpm, bus_v.powertrain.engine.max_rpm,
        "bus-diesel max_rpm"
    );
}

/// The gearbox preset must reproduce `Gearbox::manual_6()`: ratios + final_drive
/// + inertia + shift_time.
#[test]
fn gearbox_edn_matches_builtin() {
    let boxes = gearboxes_from_edn(GARAGE_EDN).expect("gearboxes parse");
    let edn = &boxes["manual-6"];
    let builtin = builtin_gearbox("manual-6").unwrap();
    let g = Gearbox::manual_6();

    assert_eq!(edn.ratios, g.ratios, "manual-6 ratios");
    assert_eq!(edn.final_drive, g.final_drive, "manual-6 final_drive (base)");
    assert_eq!(edn.inertia, g.inertia, "manual-6 inertia");
    assert_eq!(edn.shift_time, g.shift_time, "manual-6 shift_time");
    assert_eq!(edn, &builtin, "full GearboxSpec parity");

    // to_gearbox round-trips to a real Gearbox with the same parametric fields.
    let rebuilt = edn.to_gearbox();
    assert_eq!(rebuilt.ratios, g.ratios);
    assert_eq!(rebuilt.final_drive, g.final_drive);
    assert_eq!(rebuilt.shift_time, g.shift_time);
}

/// Each tire preset must reproduce its `PacejkaParams` preset — all 8 coeffs —
/// and the per-car sticky d_long/d_lat overrides must match the built Vehicle.
#[test]
fn tires_edn_matches_builtin() {
    let tires = tires_from_edn(GARAGE_EDN).expect("tires parse");
    assert_eq!(tires.len(), 2, "road-dry + road-wet in EDN");

    for (id, p) in [
        ("road-dry", PacejkaParams::road_dry()),
        ("road-wet", PacejkaParams::road_wet()),
    ] {
        let edn = &tires[id];
        let builtin = builtin_tire(id).unwrap();
        assert_eq!(edn.b_long, p.b_long, "{id}: b_long");
        assert_eq!(edn.c_long, p.c_long, "{id}: c_long");
        assert_eq!(edn.d_long, p.d_long, "{id}: d_long");
        assert_eq!(edn.e_long, p.e_long, "{id}: e_long");
        assert_eq!(edn.b_lat, p.b_lat, "{id}: b_lat");
        assert_eq!(edn.c_lat, p.c_lat, "{id}: c_lat");
        assert_eq!(edn.d_lat, p.d_lat, "{id}: d_lat");
        assert_eq!(edn.e_lat, p.e_lat, "{id}: e_lat");
        assert_eq!(edn, &builtin, "{id}: full TireSpec parity");
    }

    // Per-car sticky-tire override: sports d_long/d_lat == 1.20, and that equals
    // the actual tire on the built sports car.
    let garage = garage_from_edn(GARAGE_EDN).expect("garage parse");
    let sports = &garage["sports"];
    assert_eq!(sports.tire_d_long, Some(1.20));
    assert_eq!(sports.tire_d_lat, Some(1.20));

    let v = build_vehicle(VehicleKind::Sports);
    assert_eq!(v.wheels[0].tire.d_long, 1.20, "built sports d_long");
    assert_eq!(v.wheels[0].tire.d_lat, 1.20, "built sports d_lat");

    // Non-sports cars keep the road-dry d_long/d_lat (no override) on the
    // actual built Vehicle.
    for kind in [VehicleKind::Sedan, VehicleKind::Suv, VehicleKind::Bus] {
        let v = build_vehicle(kind);
        assert_eq!(
            v.wheels[0].tire.d_long,
            PacejkaParams::road_dry().d_long,
            "{}: keeps road-dry d_long",
            kind.id()
        );
    }
}
