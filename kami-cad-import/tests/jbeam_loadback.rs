//! Integration test — emit JBeam JSON for the demo assemblies, then
//! load it back through `kami_vehicle::jbeam::load_str` and verify the
//! resulting `Vehicle` matches the topology promised by `jbeam_emit`.
//!
//! This proves the emitter output is not just structurally well-formed
//! JSON but is *semantically valid* JBeam that the actual soft-body
//! simulator accepts.
//!
//! Settle / step physics is intentionally NOT exercised here — the
//! roadster's emitted spring constants come from the material table and
//! aren't tuned for kami-vehicle's default damping. A separate physics
//! tuning pass (Phase 2.5) is tracked in deps.toml.

use kami_cad_import::demos::{roadster_na, synth_sedan};
use kami_cad_import::jbeam_emit;
use kami_vehicle::jbeam::load_str;

#[test]
fn synth_sedan_loads_back_through_kami_vehicle() {
    let asm = synth_sedan();
    let json = jbeam_emit::emit(&asm).expect("emit");
    let vehicle = load_str(&json).expect("kami_vehicle::jbeam::load_str");

    // synth_sedan: 9 parts. Counts depend on each part's emit strategy:
    //   chassis (Chassis)          → AabbCube → 8 nodes
    //   hood / trunk (Body)        → AabbHull20 → 20 nodes each
    //   windshield (Window)        → AabbHull20 → 20 nodes
    //   engine (Powertrain)        → AabbCube → 8 nodes
    //   4 × wheel                  → WheelRing → 14 nodes each
    //
    // 8 + 20 + 20 + 20 + 8 + 56 = 132 nodes
    assert_eq!(vehicle.nodes.len(), 132, "node count");
    // 4 wheel slots
    assert_eq!(vehicle.wheels.len(), 4, "wheel count");
    // Every beam should reference real nodes (load_str would have
    // returned an error otherwise — this is just defence in depth).
    for b in &vehicle.beams {
        assert!(vehicle.nodes.iter().any(|n| n.id == b.n1), "dangling n1");
        assert!(vehicle.nodes.iter().any(|n| n.id == b.n2), "dangling n2");
    }
    // Every wheel axle pair should resolve.
    for w in &vehicle.wheels {
        assert_eq!(w.hub_nodes.len(), 2, "wheel slot has 2 axle hub_nodes");
    }
}

#[test]
fn roadster_loads_back_through_kami_vehicle() {
    let asm = roadster_na();
    let json = jbeam_emit::emit(&asm).expect("emit");
    let vehicle = load_str(&json).expect("kami_vehicle::jbeam::load_str");

    // 33-part roadster → 432 nodes / 1221 beams (matches example output).
    assert_eq!(vehicle.nodes.len(), 432, "node count");
    assert_eq!(vehicle.beams.len(), 1221, "beam count");
    assert_eq!(vehicle.wheels.len(), 4, "wheel count");

    // Spot-check: at least one node should land in each kami-vehicle
    // group we emit (body, cargo, wheel_hub, wheel_tire).
    use kami_vehicle::node::NodeGroup;
    let mut seen = [false; 5];
    for n in &vehicle.nodes {
        match n.group {
            NodeGroup::Body => seen[0] = true,
            NodeGroup::WheelHub => seen[1] = true,
            NodeGroup::WheelTire => seen[2] = true,
            NodeGroup::Cargo => seen[3] = true,
            NodeGroup::Anchor => seen[4] = true,
        }
    }
    assert!(seen[0], "no Body nodes — chassis / suspension / brake");
    assert!(seen[1], "no WheelHub nodes — wheel axles");
    assert!(seen[2], "no WheelTire nodes — wheel ring");
    assert!(seen[3], "no Cargo nodes — engine / radiator / fuel tank");
}

#[test]
fn roadster_break_groups_propagate_into_vehicle_beams() {
    // Every emitted beam carries `break_group` from its part. The
    // kami-vehicle BeamNG-style detach API consumes those groups.
    let asm = roadster_na();
    let json = jbeam_emit::emit(&asm).expect("emit");
    let vehicle = load_str(&json).expect("load");

    // We expect at least groups 1 (Chassis), 2 (Body / Window), 3
    // (Interior), 4 (Cargo: Powertrain / Electrical / Fluid), 5
    // (Suspension / Wheel / Brake) — see PartKind::default_break_group.
    use std::collections::BTreeSet;
    let groups: BTreeSet<u32> = vehicle.beams.iter().filter_map(|b| b.break_group).collect();
    assert!(
        groups.contains(&1) && groups.contains(&5),
        "missing core break groups; got {:?}",
        groups
    );
    // break_group(1) should detach a non-trivial number of beams.
    let mut v = vehicle;
    let detached = v.break_group(1);
    assert!(
        detached > 50,
        "break_group(1) should detach the chassis frame (>50 beams), got {detached}"
    );
}

#[test]
fn roadster_wheel_tire_nodes_populate_via_jbeam_loader() {
    // Phase 2 wheel-ring scaffolding emits 12 ring nodes per wheel and
    // lists them in JBeamWheel.tire_nodes; the kami-vehicle loader
    // (>= 2026-05-06) maps them into Wheel::tire_nodes so per-wheel
    // body forces and break-group attribution see the ring.
    let asm = roadster_na();
    let json = jbeam_emit::emit(&asm).expect("emit");
    let vehicle = load_str(&json).expect("load");
    for w in &vehicle.wheels {
        assert_eq!(
            w.tire_nodes.len(),
            12,
            "wheel {} should have 12 tire ring nodes, got {}",
            w.id,
            w.tire_nodes.len()
        );
        // every tire node should land in the WheelTire group.
        for &id in &w.tire_nodes {
            let n = vehicle
                .nodes
                .iter()
                .find(|n| n.id == id)
                .expect("ring node");
            assert!(matches!(n.group, kami_vehicle::node::NodeGroup::WheelTire));
        }
    }
}

#[test]
fn roadster_wheel_contact_mode_flips_to_tire_ring() {
    // Phase 2.5: when the JBeam wheel slot ships >= 8 ring nodes, the
    // loader switches `Wheel::contact_mode` to TireRing so the simulator
    // routes 60% of the Pacejka force through the contact-patch ring
    // node (centre + two neighbours).
    let asm = roadster_na();
    let json = jbeam_emit::emit(&asm).expect("emit");
    let vehicle = load_str(&json).expect("load");
    use kami_vehicle::wheel::WheelContactMode;
    for w in &vehicle.wheels {
        assert_eq!(
            w.contact_mode,
            WheelContactMode::TireRing,
            "wheel {} should be in TireRing mode (got {:?})",
            w.id,
            w.contact_mode
        );
    }
}

#[test]
fn roadster_wheel_axles_resolve_to_wheel_hub_nodes() {
    let asm = roadster_na();
    let json = jbeam_emit::emit(&asm).expect("emit");
    let vehicle = load_str(&json).expect("load");

    use kami_vehicle::node::NodeGroup;
    for w in &vehicle.wheels {
        for &axle_id in &w.hub_nodes {
            let n = vehicle
                .nodes
                .iter()
                .find(|n| n.id == axle_id)
                .expect("axle node");
            assert!(
                matches!(n.group, NodeGroup::WheelHub),
                "axle node should be in WheelHub group"
            );
        }
        // Wheel slot must carry a sensible radius / width pulled from
        // the AABB inference in jbeam_emit::wheel_axes.
        assert!(w.radius > 0.20 && w.radius < 0.40, "radius: {}", w.radius);
        assert!(w.width > 0.10 && w.width < 0.30, "width: {}", w.width);
    }
}
