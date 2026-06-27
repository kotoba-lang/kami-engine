//! Miata-class roadster expressed as parametric SCAD primitives — the
//! reference vehicle for ADR 2605051430. All shapes are etzhayyim-authored
//! parametric (license MIT) so there's no third-party CAD licence
//! question. Geometry is illustrative — wheelbase / track / panel
//! proportions are roughly NA-Miata, but the dimensions are tuneable
//! parameters at the top of the function.

use crate::ingest::scad::{
    AnnotatedEntity, ScadAnnotation, ScadPrim, ScadTransform, from_annotated,
};
use crate::part::{
    Hardpoint, HardpointKind, Material, PartKind, ProvenanceSource, Supplier, VehicleAssembly,
};

pub fn roadster_na() -> VehicleAssembly {
    let wheelbase: f32 = 2.27;
    let track_f: f32 = 1.41;
    let track_r: f32 = 1.43;
    let chassis_h: f32 = 0.20;
    let belt_y: f32 = 0.55;
    let roof_y: f32 = 1.10;

    let scad_uri = "scad://gftd/roadster-na/v0.1.0";
    let scad_sha = "1".repeat(64);
    let prov = ProvenanceSource {
        uri: scad_uri.into(),
        sha256: scad_sha.clone(),
        license: "MIT".into(),
    };
    let prov_part = |path: &str| ProvenanceSource {
        uri: format!("scad://gftd/roadster-na/{}.scad", path),
        sha256: scad_sha.clone(),
        license: "MIT".into(),
    };
    let supplier_gftd = || Supplier {
        name: "gftd".into(),
        cpe: String::new(),
        mpn: String::new(),
    };

    let mut entities = Vec::<AnnotatedEntity>::new();

    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [track_f, chassis_h, wheelbase + 0.6],
        },
        transform: ScadTransform::default().translate(0.0, 0.30, 0.0),
        annotation: ScadAnnotation {
            part_id: "chassis".into(),
            display_name: Some("Chassis main + floor pan (HSS)".into()),
            kind: PartKind::Chassis,
            material: Material::SteelHss,
            mass_kg: Some(180.0),
            parent: None,
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("chassis")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [track_f, 0.05, 0.08],
        },
        transform: ScadTransform::default().translate(0.0, roof_y, 0.20),
        annotation: ScadAnnotation {
            part_id: "windshield_header".into(),
            display_name: Some("Windshield header beam".into()),
            kind: PartKind::Chassis,
            material: Material::SteelHss,
            mass_kg: Some(8.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("windshield_header")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [track_f - 0.1, 0.04, 0.95],
        },
        transform: ScadTransform::default().translate(0.0, belt_y + 0.18, wheelbase * 0.5 - 0.2),
        annotation: ScadAnnotation {
            part_id: "hood".into(),
            display_name: Some("Hood (aluminium sheet)".into()),
            kind: PartKind::Body,
            material: Material::AluminiumSheet,
            mass_kg: Some(9.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("hood")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [track_f - 0.1, 0.05, 0.85],
        },
        transform: ScadTransform::default().translate(0.0, belt_y + 0.20, -(wheelbase * 0.5)),
        annotation: ScadAnnotation {
            part_id: "trunk".into(),
            display_name: Some("Trunk lid".into()),
            kind: PartKind::Body,
            material: Material::SteelMild,
            mass_kg: Some(12.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("trunk")),
        },
    });
    for (id, x, name, src_path) in [
        ("door_l", -track_f * 0.5 + 0.05, "Door (driver)", "door_l"),
        ("door_r", track_f * 0.5 - 0.05, "Door (passenger)", "door_r"),
    ] {
        entities.push(AnnotatedEntity {
            primitive: ScadPrim::Cube {
                size: [0.05, 0.55, 0.95],
            },
            transform: ScadTransform::default().translate(x, belt_y - 0.10, 0.0),
            annotation: ScadAnnotation {
                part_id: id.into(),
                display_name: Some(name.into()),
                kind: PartKind::Body,
                material: Material::SteelMild,
                mass_kg: Some(15.0),
                parent: Some("chassis".into()),
                break_group: None,
                supplier: supplier_gftd(),
                revision: Some("0.1.0".into()),
                source: Some(prov_part(src_path)),
            },
        });
    }
    for (id, x, name) in [
        ("fender_fl", -track_f * 0.5, "Front fender L"),
        ("fender_fr", track_f * 0.5, "Front fender R"),
        ("fender_rl", -track_f * 0.5, "Rear fender L"),
        ("fender_rr", track_f * 0.5, "Rear fender R"),
    ] {
        let z = if id.contains("_f") {
            wheelbase * 0.5
        } else {
            -(wheelbase * 0.5)
        };
        entities.push(AnnotatedEntity {
            primitive: ScadPrim::Cube {
                size: [0.06, 0.30, 0.45],
            },
            transform: ScadTransform::default().translate(x, belt_y - 0.05, z),
            annotation: ScadAnnotation {
                part_id: id.into(),
                display_name: Some(name.into()),
                kind: PartKind::Body,
                material: Material::SteelMild,
                mass_kg: Some(5.5),
                parent: Some("chassis".into()),
                break_group: None,
                supplier: supplier_gftd(),
                revision: Some("0.1.0".into()),
                source: Some(prov_part(id)),
            },
        });
    }
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [track_f - 0.1, 0.55, 0.05],
        },
        transform: ScadTransform::default().translate(0.0, belt_y + 0.30, 0.45),
        annotation: ScadAnnotation {
            part_id: "windshield".into(),
            display_name: Some("Windshield (laminated)".into()),
            kind: PartKind::Window,
            material: Material::Glass,
            mass_kg: Some(8.5),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: Supplier {
                name: "AGC".into(),
                cpe: String::new(),
                mpn: "AGC-RDST-NA".into(),
            },
            revision: Some("0.1.0".into()),
            source: Some(prov_part("windshield")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [0.55, 0.55, 0.65],
        },
        transform: ScadTransform::default().translate(0.0, 0.55, 0.95),
        annotation: ScadAnnotation {
            part_id: "engine".into(),
            display_name: Some("1.6L NA inline-4 block".into()),
            kind: PartKind::Powertrain,
            material: Material::AluminiumCast,
            mass_kg: Some(102.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: Supplier {
                name: "Mazda".into(),
                cpe: String::new(),
                mpn: "B6ZE-RS".into(),
            },
            revision: Some("0.1.0".into()),
            source: Some(prov_part("engine_block")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [0.30, 0.30, 0.85],
        },
        transform: ScadTransform::default().translate(0.0, 0.45, 0.20),
        annotation: ScadAnnotation {
            part_id: "transmission".into(),
            display_name: Some("5-speed manual gearbox".into()),
            kind: PartKind::Powertrain,
            material: Material::AluminiumCast,
            mass_kg: Some(38.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: Supplier {
                name: "Mazda".into(),
                cpe: String::new(),
                mpn: "M5-NA".into(),
            },
            revision: Some("0.1.0".into()),
            source: Some(prov_part("transmission")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [0.40, 0.25, 0.40],
        },
        transform: ScadTransform::default().translate(0.0, 0.40, -wheelbase * 0.5 + 0.15),
        annotation: ScadAnnotation {
            part_id: "diff".into(),
            display_name: Some("Open differential (rear)".into()),
            kind: PartKind::Powertrain,
            material: Material::AluminiumCast,
            mass_kg: Some(26.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("diff_rear")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cylinder {
            h: 1.40,
            r1: 0.04,
            r2: 0.04,
        },
        transform: ScadTransform::default()
            .translate(0.0, 0.30, -0.40)
            .rotate_xyzw(
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
        annotation: ScadAnnotation {
            part_id: "driveshaft".into(),
            display_name: Some("Propeller shaft".into()),
            kind: PartKind::Powertrain,
            material: Material::SteelHss,
            mass_kg: Some(8.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("driveshaft")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [0.35, 0.20, 0.50],
        },
        transform: ScadTransform::default().translate(0.40, 0.42, 1.30),
        annotation: ScadAnnotation {
            part_id: "battery".into(),
            display_name: Some("12V battery".into()),
            kind: PartKind::Electrical,
            material: Material::LiIon,
            mass_kg: Some(11.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: Supplier {
                name: "Panasonic".into(),
                cpe: String::new(),
                mpn: "44B19L".into(),
            },
            revision: Some("0.1.0".into()),
            source: Some(prov_part("battery")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [0.55, 0.30, 0.35],
        },
        transform: ScadTransform::default().translate(0.0, 0.40, 1.50),
        annotation: ScadAnnotation {
            part_id: "radiator".into(),
            display_name: Some("Coolant radiator".into()),
            kind: PartKind::Fluid,
            material: Material::AluminiumSheet,
            mass_kg: Some(6.5),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: Supplier {
                name: "Denso".into(),
                cpe: String::new(),
                mpn: "DRA-1989-NA".into(),
            },
            revision: Some("0.1.0".into()),
            source: Some(prov_part("radiator")),
        },
    });
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [0.55, 0.20, 0.45],
        },
        transform: ScadTransform::default().translate(0.0, 0.30, -wheelbase * 0.5),
        annotation: ScadAnnotation {
            part_id: "fuel_tank".into(),
            display_name: Some("Fuel tank".into()),
            kind: PartKind::Fluid,
            material: Material::Plastic,
            mass_kg: Some(45.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("fuel_tank")),
        },
    });
    for (id, x, z, name) in [
        (
            "strut_fl",
            -track_f * 0.5 + 0.05,
            wheelbase * 0.5,
            "Strut FL",
        ),
        (
            "strut_fr",
            track_f * 0.5 - 0.05,
            wheelbase * 0.5,
            "Strut FR",
        ),
        (
            "strut_rl",
            -track_r * 0.5 + 0.05,
            -wheelbase * 0.5,
            "Strut RL",
        ),
        (
            "strut_rr",
            track_r * 0.5 - 0.05,
            -wheelbase * 0.5,
            "Strut RR",
        ),
    ] {
        entities.push(AnnotatedEntity {
            primitive: ScadPrim::Cylinder {
                h: 0.40,
                r1: 0.04,
                r2: 0.04,
            },
            transform: ScadTransform::default().translate(x, 0.40, z),
            annotation: ScadAnnotation {
                part_id: id.into(),
                display_name: Some(name.into()),
                kind: PartKind::Suspension,
                material: Material::SteelHss,
                mass_kg: Some(7.5),
                parent: Some("chassis".into()),
                break_group: None,
                supplier: supplier_gftd(),
                revision: Some("0.1.0".into()),
                source: Some(prov_part("strut")),
            },
        });
    }
    for (id, x, z) in [
        ("brake_fl", -track_f * 0.5, wheelbase * 0.5),
        ("brake_fr", track_f * 0.5, wheelbase * 0.5),
        ("brake_rl", -track_r * 0.5, -wheelbase * 0.5),
        ("brake_rr", track_r * 0.5, -wheelbase * 0.5),
    ] {
        entities.push(AnnotatedEntity {
            primitive: ScadPrim::Cylinder {
                h: 0.025,
                r1: 0.135,
                r2: 0.135,
            },
            transform: ScadTransform::default().translate(x, 0.30, z).rotate_xyzw(
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
            annotation: ScadAnnotation {
                part_id: id.into(),
                display_name: Some(format!("Brake disc {}", id.split('_').last().unwrap())),
                kind: PartKind::Brake,
                material: Material::SteelMild,
                mass_kg: Some(8.0),
                parent: Some("chassis".into()),
                break_group: None,
                supplier: Supplier {
                    name: "Akebono".into(),
                    cpe: String::new(),
                    mpn: "ABK-NA-235".into(),
                },
                revision: Some("0.1.0".into()),
                source: Some(prov_part("brake_disc")),
            },
        });
    }
    for (id, x, z) in [
        ("wheel_fl", -track_f * 0.5, wheelbase * 0.5),
        ("wheel_fr", track_f * 0.5, wheelbase * 0.5),
        ("wheel_rl", -track_r * 0.5, -wheelbase * 0.5),
        ("wheel_rr", track_r * 0.5, -wheelbase * 0.5),
    ] {
        entities.push(AnnotatedEntity {
            primitive: ScadPrim::Cylinder {
                h: 0.18,
                r1: 0.30,
                r2: 0.30,
            },
            transform: ScadTransform::default().translate(x, 0.30, z).rotate_xyzw(
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
            annotation: ScadAnnotation {
                part_id: id.into(),
                display_name: Some(format!("Wheel + tire {}", id)),
                kind: PartKind::Wheel,
                material: Material::Rubber,
                mass_kg: Some(15.0),
                parent: Some("chassis".into()),
                break_group: None,
                supplier: Supplier {
                    name: "Bridgestone".into(),
                    cpe: String::new(),
                    mpn: "ER300-185-60-R14".into(),
                },
                revision: Some("0.1.0".into()),
                source: Some(prov_part("wheel")),
            },
        });
    }
    for (id, x, name) in [
        ("seat_l", -0.30, "Seat (driver)"),
        ("seat_r", 0.30, "Seat (passenger)"),
    ] {
        entities.push(AnnotatedEntity {
            primitive: ScadPrim::Cube {
                size: [0.50, 0.95, 0.55],
            },
            transform: ScadTransform::default().translate(x, 0.65, -0.10),
            annotation: ScadAnnotation {
                part_id: id.into(),
                display_name: Some(name.into()),
                kind: PartKind::Interior,
                material: Material::Plastic,
                mass_kg: Some(18.0),
                parent: Some("chassis".into()),
                break_group: None,
                supplier: supplier_gftd(),
                revision: Some("0.1.0".into()),
                source: Some(prov_part("seat")),
            },
        });
    }
    entities.push(AnnotatedEntity {
        primitive: ScadPrim::Cube {
            size: [track_f - 0.1, 0.18, 0.30],
        },
        transform: ScadTransform::default().translate(0.0, belt_y + 0.05, 0.50),
        annotation: ScadAnnotation {
            part_id: "dashboard".into(),
            display_name: Some("Dashboard".into()),
            kind: PartKind::Interior,
            material: Material::Plastic,
            mass_kg: Some(9.0),
            parent: Some("chassis".into()),
            break_group: None,
            supplier: supplier_gftd(),
            revision: Some("0.1.0".into()),
            source: Some(prov_part("dashboard")),
        },
    });

    let mut hps: Vec<Hardpoint> = Vec::new();
    let hp = |id: &str, from: &str, to: &str, pos: [f32; 3], kind: HardpointKind| Hardpoint {
        id: id.into(),
        from_part: from.into(),
        to_part: to.into(),
        position: pos,
        kind,
    };
    hps.push(hp(
        "hp_header",
        "chassis",
        "windshield_header",
        [0.0, roof_y, 0.20],
        HardpointKind::Weld,
    ));
    hps.push(hp(
        "hp_hood",
        "chassis",
        "hood",
        [0.0, belt_y + 0.20, 1.55],
        HardpointKind::Hinge,
    ));
    hps.push(hp(
        "hp_trunk",
        "chassis",
        "trunk",
        [0.0, belt_y + 0.22, -0.95],
        HardpointKind::Hinge,
    ));
    hps.push(hp(
        "hp_door_l",
        "chassis",
        "door_l",
        [-track_f * 0.5 + 0.05, 0.50, 0.30],
        HardpointKind::Hinge,
    ));
    hps.push(hp(
        "hp_door_r",
        "chassis",
        "door_r",
        [track_f * 0.5 - 0.05, 0.50, 0.30],
        HardpointKind::Hinge,
    ));
    for fid in ["fender_fl", "fender_fr", "fender_rl", "fender_rr"] {
        hps.push(hp(
            &format!("hp_{fid}"),
            "chassis",
            fid,
            [0.0, belt_y - 0.05, 0.0],
            HardpointKind::Bolt,
        ));
    }
    hps.push(hp(
        "hp_windshield",
        "windshield_header",
        "windshield",
        [0.0, roof_y - 0.05, 0.40],
        HardpointKind::Adhesive,
    ));
    hps.push(hp(
        "hp_engine_l",
        "chassis",
        "engine",
        [-0.20, 0.45, 0.95],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_engine_r",
        "chassis",
        "engine",
        [0.20, 0.45, 0.95],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_trans",
        "engine",
        "transmission",
        [0.0, 0.45, 0.55],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_driveshaft_f",
        "transmission",
        "driveshaft",
        [0.0, 0.30, 0.20],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_driveshaft_r",
        "driveshaft",
        "diff",
        [0.0, 0.30, -0.95],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_radiator",
        "chassis",
        "radiator",
        [0.0, 0.40, 1.50],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_battery",
        "chassis",
        "battery",
        [0.40, 0.42, 1.30],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_fuel_tank",
        "chassis",
        "fuel_tank",
        [0.0, 0.35, -wheelbase * 0.5],
        HardpointKind::Bolt,
    ));
    for w in ["wheel_fl", "wheel_fr", "wheel_rl", "wheel_rr"] {
        let strut = match w {
            "wheel_fl" => "strut_fl",
            "wheel_fr" => "strut_fr",
            "wheel_rl" => "strut_rl",
            "wheel_rr" => "strut_rr",
            _ => unreachable!(),
        };
        let brake = match w {
            "wheel_fl" => "brake_fl",
            "wheel_fr" => "brake_fr",
            "wheel_rl" => "brake_rl",
            "wheel_rr" => "brake_rr",
            _ => unreachable!(),
        };
        hps.push(hp(
            &format!("hp_strut_{w}"),
            strut,
            w,
            [0.0, 0.30, 0.0],
            HardpointKind::Press,
        ));
        hps.push(hp(
            &format!("hp_strut_{strut}_chassis"),
            "chassis",
            strut,
            [0.0, 0.55, 0.0],
            HardpointKind::Bolt,
        ));
        hps.push(hp(
            &format!("hp_brake_{w}"),
            brake,
            w,
            [0.0, 0.30, 0.0],
            HardpointKind::Bolt,
        ));
    }
    hps.push(hp(
        "hp_seat_l",
        "chassis",
        "seat_l",
        [-0.30, 0.30, -0.10],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_seat_r",
        "chassis",
        "seat_r",
        [0.30, 0.30, -0.10],
        HardpointKind::Bolt,
    ));
    hps.push(hp(
        "hp_dashboard",
        "chassis",
        "dashboard",
        [0.0, belt_y, 0.55],
        HardpointKind::Bolt,
    ));

    from_annotated(
        "scad-roadster-na",
        "etzhayyim SCAD Roadster NA",
        "0.1.0",
        prov,
        &entities,
        hps,
    )
    .expect("assembly validates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roadster_has_expected_topology() {
        let asm = roadster_na();
        assert_eq!(asm.parts.len(), 33);
        assert!(asm.total_mass_kg() > 600.0);
        assert!(asm.total_mass_kg() < 800.0);
        // 4 wheels exist
        let wheels = asm
            .parts
            .iter()
            .filter(|p| matches!(p.kind, crate::part::PartKind::Wheel))
            .count();
        assert_eq!(wheels, 4);
    }
}
