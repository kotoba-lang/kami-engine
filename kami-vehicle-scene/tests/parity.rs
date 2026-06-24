//! Parity tests: the shipped EDN must faithfully reproduce kami-vehicle's
//! compiled-in surface table and demo-circuit map. This is the whole point of
//! Front A — EDN becomes the source of truth with *behaviour unchanged*.

use kami_vehicle::{MapGround, SurfaceKind};
use kami_vehicle_scene::{
    map_from_edn, shipped_demo_circuit, SurfaceParams, SurfaceTable, ALL_SURFACE_KINDS, GROUND_EDN,
};

/// For every `SurfaceKind`, the EDN-loaded params must equal the hardcoded
/// `coefficients()` / `tint()` / `display_name()`.
#[test]
fn surfaces_edn_matches_builtin() {
    let loaded = SurfaceTable::from_edn(GROUND_EDN).expect("ground.edn surfaces parse");
    assert_eq!(loaded.len(), 8, "all 8 surfaces present in EDN");

    for kind in ALL_SURFACE_KINDS {
        let edn = loaded.get(kind);
        let builtin = SurfaceParams::from_kind(kind);

        let (mu, grip) = kind.coefficients();
        assert!(
            (edn.friction_mu - mu).abs() < 1e-6,
            "{}: friction_mu {} != {}",
            kind.id(),
            edn.friction_mu,
            mu
        );
        assert!(
            (edn.grip_modifier - grip).abs() < 1e-6,
            "{}: grip_modifier {} != {}",
            kind.id(),
            edn.grip_modifier,
            grip
        );
        let t = kind.tint();
        for i in 0..3 {
            assert!(
                (edn.tint[i] - t[i]).abs() < 1e-6,
                "{}: tint[{}] {} != {}",
                kind.id(),
                i,
                edn.tint[i],
                t[i]
            );
        }
        assert_eq!(edn.name, kind.display_name(), "{}: name", kind.id());

        // And the whole struct equals the builtin mirror.
        assert_eq!(edn, builtin, "{}: full SurfaceParams parity", kind.id());
    }

    // The shipped-table convenience loader yields the same thing.
    let shipped = kami_vehicle_scene::shipped_surface_table().expect("shipped table");
    assert_eq!(shipped.get(SurfaceKind::Ice), loaded.get(SurfaceKind::Ice));
}

/// `map_from_edn(..,"demo-circuit")` must reproduce `MapGround::demo_circuit()`
/// exactly: same default, same zone count, same per-zone bounds + surface.
#[test]
fn demo_circuit_edn_matches_builtin() {
    let edn: MapGround = map_from_edn(GROUND_EDN, "demo-circuit").expect("demo-circuit parses");
    let builtin = MapGround::demo_circuit();

    assert_eq!(edn.default, builtin.default, "default surface");
    assert_eq!(
        edn.zones.len(),
        builtin.zones.len(),
        "zone count ({} vs {})",
        edn.zones.len(),
        builtin.zones.len()
    );

    for (i, (a, b)) in edn.zones.iter().zip(builtin.zones.iter()).enumerate() {
        assert!((a.x_min - b.x_min).abs() < 1e-6, "zone {i}: x_min");
        assert!((a.x_max - b.x_max).abs() < 1e-6, "zone {i}: x_max");
        assert!((a.z_min - b.z_min).abs() < 1e-6, "zone {i}: z_min");
        assert!((a.z_max - b.z_max).abs() < 1e-6, "zone {i}: z_max");
        assert_eq!(a.surface, b.surface, "zone {i}: surface");
    }

    // The convenience loader agrees.
    let shipped = shipped_demo_circuit().expect("shipped demo-circuit");
    assert_eq!(shipped.zones.len(), builtin.zones.len());

    // Behavioural spot-check: surface_at() agrees at several probe points
    // (the actual point of the data — same physics regions).
    for &(x, z) in &[
        (0.0, 0.0),    // main asphalt
        (0.0, 35.0),   // ice patch
        (20.0, 0.0),   // sand
        (-20.0, 0.0),  // mud
        (0.0, 200.0),  // off-map → default grass
    ] {
        assert_eq!(
            edn.surface_at(x, z),
            builtin.surface_at(x, z),
            "surface_at({x},{z})"
        );
    }
}
