use glam::DVec3;

use crate::gcode::{GcodeConfig, generate_gcode};
use crate::stock::{CamMaterial, Stock, StockShape};
use crate::tool::{Tool, ToolLibrary, ToolMaterial, ToolType};
use crate::toolpath::{CamJob, CamOperation, PocketStrategy};

fn sample_endmill() -> Tool {
    Tool {
        id: 1,
        name: "6mm 2-flute carbide".into(),
        tool_type: ToolType::EndMill,
        diameter: 6.0,
        flute_length: 20.0,
        overall_length: 50.0,
        flute_count: 2,
        corner_radius: 0.0,
        material: ToolMaterial::Carbide,
        coating: Some("TiAlN".into()),
    }
}

// -----------------------------------------------------------------------
// 1. Tool library CRUD
// -----------------------------------------------------------------------
#[test]
fn tool_library_crud() {
    let mut lib = ToolLibrary::new();
    assert!(lib.is_empty());

    // Add
    let t1 = sample_endmill();
    assert!(lib.add(t1.clone()).is_none());
    assert_eq!(lib.len(), 1);

    // Get
    let fetched = lib.get(1).expect("tool 1 should exist");
    assert_eq!(fetched.name, "6mm 2-flute carbide");
    assert_eq!(fetched.tool_type, ToolType::EndMill);

    // Replace
    let mut t1_v2 = sample_endmill();
    t1_v2.name = "6mm 3-flute carbide".into();
    t1_v2.flute_count = 3;
    let old = lib.add(t1_v2).expect("should return previous");
    assert_eq!(old.flute_count, 2);
    assert_eq!(lib.get(1).unwrap().flute_count, 3);

    // Add second tool
    let t2 = Tool {
        id: 2,
        name: "10mm ball nose".into(),
        tool_type: ToolType::BallNose,
        diameter: 10.0,
        flute_length: 25.0,
        overall_length: 75.0,
        flute_count: 2,
        corner_radius: 5.0,
        material: ToolMaterial::HSS,
        coating: None,
    };
    lib.add(t2);
    assert_eq!(lib.len(), 2);

    // List sorted by id
    let list = lib.list();
    assert_eq!(list[0].id, 1);
    assert_eq!(list[1].id, 2);

    // Remove
    let removed = lib.remove(1).expect("should remove tool 1");
    assert_eq!(removed.id, 1);
    assert_eq!(lib.len(), 1);
    assert!(lib.get(1).is_none());
}

// -----------------------------------------------------------------------
// 2. G-code header and footer validity
// -----------------------------------------------------------------------
#[test]
fn gcode_header_footer_valid() {
    let segments = vec![
        crate::toolpath::ToolpathSegment {
            segment_type: crate::toolpath::SegmentType::Rapid,
            start: DVec3::ZERO,
            end: DVec3::new(10.0, 0.0, 5.0),
            feed_rate: 0.0,
            center: None,
            tool_id: 1,
        },
        crate::toolpath::ToolpathSegment {
            segment_type: crate::toolpath::SegmentType::Linear,
            start: DVec3::new(10.0, 0.0, 5.0),
            end: DVec3::new(10.0, 0.0, -2.0),
            feed_rate: 500.0,
            center: None,
            tool_id: 1,
        },
    ];

    let config = GcodeConfig::default();
    let gcode = generate_gcode(&segments, &config);

    // Header checks
    assert!(gcode.starts_with('%'));
    assert!(gcode.contains("O0001"));
    assert!(gcode.contains("G21"));
    assert!(gcode.contains("G90"));
    assert!(gcode.contains("G54"));

    // Tool change
    assert!(gcode.contains("T01 M06"));
    // Spindle
    assert!(gcode.contains("M03"));
    // Coolant
    assert!(gcode.contains("M08"));

    // Motion
    assert!(gcode.contains("G00"));
    assert!(gcode.contains("G01"));
    assert!(gcode.contains("F500.0"));

    // Footer
    assert!(gcode.contains("M05"));
    assert!(gcode.contains("M09"));
    assert!(gcode.contains("M30"));
    assert!(gcode.trim_end().ends_with('%'));
}

// -----------------------------------------------------------------------
// 3. G-code arc output (G02/G03)
// -----------------------------------------------------------------------
#[test]
fn gcode_arc_output() {
    let segments = vec![
        crate::toolpath::ToolpathSegment {
            segment_type: crate::toolpath::SegmentType::ArcCW,
            start: DVec3::new(10.0, 0.0, -1.0),
            end: DVec3::new(0.0, 10.0, -1.0),
            feed_rate: 300.0,
            center: Some(DVec3::new(0.0, 0.0, -1.0)),
            tool_id: 1,
        },
        crate::toolpath::ToolpathSegment {
            segment_type: crate::toolpath::SegmentType::ArcCCW,
            start: DVec3::new(0.0, 10.0, -1.0),
            end: DVec3::new(10.0, 0.0, -1.0),
            feed_rate: 300.0,
            center: Some(DVec3::new(0.0, 0.0, -1.0)),
            tool_id: 1,
        },
    ];

    let config = GcodeConfig::default();
    let gcode = generate_gcode(&segments, &config);

    assert!(gcode.contains("G02"));
    assert!(gcode.contains("G03"));
    // I/J values: center - start
    assert!(gcode.contains("I-10.0000"));
    assert!(gcode.contains("J0.0000"));
}

// -----------------------------------------------------------------------
// 4. Pocket toolpath generates zigzag segments
// -----------------------------------------------------------------------
#[test]
fn pocket_toolpath_generates_segments() {
    let mut lib = ToolLibrary::new();
    lib.add(sample_endmill());

    let stock = Stock::new(
        StockShape::Block {
            width: 100.0,
            height: 100.0,
            depth: 20.0,
        },
        CamMaterial::aluminum_6061(),
    );

    let mut job = CamJob::new(stock, lib);
    job.add_operation(CamOperation::Pocket {
        tool_id: 1,
        depth: 3.0,
        stepover: 3.0,
        strategy: PocketStrategy::Zigzag,
        feed_rate: 800.0,
        spindle_rpm: 12000.0,
        pocket_min: DVec3::new(10.0, 10.0, 0.0),
        pocket_max: DVec3::new(50.0, 50.0, 0.0),
    });

    let segments = job.generate_toolpath();

    // Should produce a non-trivial number of segments
    assert!(
        segments.len() > 5,
        "expected many segments, got {}",
        segments.len()
    );

    // Should contain both rapids and linear cuts
    let has_rapid = segments
        .iter()
        .any(|s| s.segment_type == crate::toolpath::SegmentType::Rapid);
    let has_linear = segments
        .iter()
        .any(|s| s.segment_type == crate::toolpath::SegmentType::Linear);
    assert!(has_rapid, "pocket should have rapid moves");
    assert!(has_linear, "pocket should have linear cutting moves");

    // All segments should reference tool 1
    assert!(segments.iter().all(|s| s.tool_id == 1));

    // Linear segments should have the correct feed rate
    for s in &segments {
        if s.segment_type == crate::toolpath::SegmentType::Linear {
            assert!((s.feed_rate - 800.0).abs() < 1e-6);
        }
    }

    // Verify G-code round-trip: segments produce valid G-code
    let gcode = generate_gcode(&segments, &GcodeConfig::default());
    assert!(gcode.contains("G01"));
    assert!(gcode.contains("M30"));
}

// -----------------------------------------------------------------------
// 5. Material presets
// -----------------------------------------------------------------------
#[test]
fn material_presets() {
    let al = CamMaterial::aluminum_6061();
    assert!(al.density > 2.0 && al.density < 3.0);
    assert!(al.hardness > 50.0);

    let ti = CamMaterial::titanium_ti6al4v();
    assert!(ti.density > 4.0);
    assert!(ti.hardness > 300.0);

    let wood = CamMaterial::wood_oak();
    assert!(wood.density < 1.0);
}
