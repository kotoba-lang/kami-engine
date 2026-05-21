//! kami-eng-web: WASM entry point for KAMI Engineering SDK.
//!
//! Exposes EDA/CAD/CAM/RTL/CAE functionality to the browser via wasm-bindgen.
//! Each function is callable from JavaScript/TypeScript.

use wasm_bindgen::prelude::*;

// ── EDA: Schematic ──

#[wasm_bindgen]
pub fn eda_create_schematic() -> String {
    let sch = kami_eda::schematic::Schematic::new();
    serde_json::json!({
        "name": "Main",
        "symbols": sch.symbols.len(),
        "wires": sch.wires.len(),
    }).to_string()
}

#[wasm_bindgen]
pub fn eda_run_erc(schematic_json: &str) -> String {
    // Placeholder: parse schematic JSON, run ERC, return violations
    let _ = schematic_json;
    serde_json::json!({ "violations": [], "error_count": 0, "warning_count": 0 }).to_string()
}

#[wasm_bindgen]
pub fn eda_export_spice(schematic_json: &str) -> String {
    let _ = schematic_json;
    "* KAMI Engineering SDK SPICE export\n.end\n".to_string()
}

#[wasm_bindgen]
pub fn eda_run_drc(pcb_json: &str) -> String {
    let _ = pcb_json;
    serde_json::json!({ "violations": [], "error_count": 0, "warning_count": 0 }).to_string()
}

#[wasm_bindgen]
pub fn eda_export_gerber(pcb_json: &str) -> String {
    let _ = pcb_json;
    let apertures = vec![
        (10, kami_eng_io::gerber::Aperture::Circle { diameter: 0.2 }),
    ];
    kami_eng_io::gerber::generate(&apertures, &[])
}

// ── CAD: Modeling ──

#[wasm_bindgen]
pub fn cad_create_box(width: f64, height: f64, depth: f64) -> String {
    let min = glam::DVec3::ZERO;
    let max = glam::DVec3::new(width, height, depth);
    let (solid, edges, vertices) = kami_cad::brep::make_box(1, min, max);
    serde_json::json!({
        "faces": solid.face_count(),
        "edges": solid.edge_count(),
        "vertices": solid.vertex_count(&edges),
        "volume": solid.volume(&edges, &vertices),
        "surface_area": solid.surface_area(&edges, &vertices),
    }).to_string()
}

#[wasm_bindgen]
pub fn cad_tessellate_box(width: f64, height: f64, depth: f64, _tolerance: f64) -> Vec<f32> {
    let min = glam::DVec3::ZERO;
    let max = glam::DVec3::new(width, height, depth);
    let (solid, edges, vertices) = kami_cad::brep::make_box(1, min, max);
    let (positions, _indices) = kami_cad::tessellate::tessellate_solid(&solid, &edges, &vertices);
    positions.iter().flat_map(|v| [v.x as f32, v.y as f32, v.z as f32]).collect()
}

#[wasm_bindgen]
pub fn cad_export_step(name: &str) -> String {
    let mut out = kami_eng_io::step::generate_header(&format!("{}.step", name), "KAMI User");
    out.push_str(kami_eng_io::step::generate_footer());
    out
}

// ── CAM: Toolpath ──

#[wasm_bindgen]
pub fn cam_generate_gcode(
    width: f64,
    height: f64,
    depth: f64,
    tool_diameter: f64,
    feed_rate: f64,
    spindle_rpm: f64,
) -> String {
    use kami_cam::gcode;
    use kami_cam::tool::*;
    use kami_cam::toolpath::*;
    use kami_cam::stock::*;

    let mut lib = ToolLibrary::new();
    let tool = Tool {
        id: 1,
        name: "End Mill".to_string(),
        tool_type: ToolType::EndMill,
        diameter: tool_diameter,
        flute_length: depth * 2.0,
        overall_length: depth * 4.0,
        flute_count: 4,
        corner_radius: 0.0,
        material: ToolMaterial::Carbide,
        coating: Some("TiAlN".to_string()),
    };
    let tool_id = tool.id;
    lib.add(tool);

    let stock = Stock::new(
        StockShape::Block {
            width,
            height,
            depth: depth * 1.5,
        },
        CamMaterial::aluminum_6061(),
    );

    let ops = vec![CamOperation::Pocket {
        tool_id,
        depth,
        stepover: tool_diameter * 0.4,
        strategy: PocketStrategy::Zigzag,
        feed_rate,
        spindle_rpm,
        pocket_min: glam::DVec3::new(5.0, 5.0, 0.0),
        pocket_max: glam::DVec3::new(width - 5.0, height - 5.0, 0.0),
    }];

    let mut job = CamJob::new(stock, lib);
    for op in &ops {
        job.add_operation(op.clone());
    }
    let segments = job.generate_toolpath();

    let config = gcode::GcodeConfig {
        machine_type: gcode::MachineType::Mill3Axis,
        post_processor: gcode::PostProcessor::Fanuc,
        units: gcode::GcodeUnits::Millimeters,
        safe_height: 25.0,
        coordinate_system: gcode::CoordinateSystem::G54,
        program_number: 1,
        coolant: true,
    };

    gcode::generate_gcode(&segments, &config)
}

// ── RTL: Simulation ──

#[wasm_bindgen]
pub fn rtl_parse_verilog(source: &str) -> String {
    match kami_rtl::hdl::parse_verilog(source) {
        Ok((name, ports)) => serde_json::json!({
            "name": name,
            "port_count": ports.len(),
        }).to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

#[wasm_bindgen]
pub fn rtl_simulate(top_module: &str, duration: u64) -> String {
    let _ = top_module;
    let mut sim = kami_rtl::simulator::RtlSimulator::new();
    sim.register_signal("clk", 1);
    sim.register_signal("data", 8);

    // Generate clock: toggle every 5 time units
    for t in (0..duration).step_by(5) {
        let val = if (t / 5) % 2 == 0 {
            kami_rtl::simulator::LogicValue::One
        } else {
            kami_rtl::simulator::LogicValue::Zero
        };
        sim.schedule_event(t, "clk", vec![val]);
    }

    sim.run(duration);

    let clk_history = sim.get_signal_history("clk");
    serde_json::json!({
        "time": sim.time,
        "clk_transitions": clk_history.map_or(0, |h| h.len()),
    }).to_string()
}

#[wasm_bindgen]
pub fn rtl_export_vcd(duration: u64) -> String {
    let signals = vec![
        kami_rtl::waveform::WaveformSignal {
            name: "clk".to_string(),
            width: 1,
            transitions: (0..duration)
                .step_by(5)
                .map(|t| (t, if (t / 5) % 2 == 0 { "1".to_string() } else { "0".to_string() }))
                .collect(),
            display_format: kami_rtl::waveform::DisplayFormat::Binary,
            color: "#00ff00".to_string(),
        },
    ];
    kami_rtl::waveform::export_vcd(&signals)
}

// ── CAE: Analysis ──

#[wasm_bindgen]
pub fn cae_generate_box_mesh(width: f64, height: f64, depth: f64, divisions: u32) -> String {
    let mesh = kami_cae::mesh::generate_box_mesh(width, height, depth, divisions);
    let stats = mesh.mesh_stats();
    serde_json::json!({
        "node_count": stats.node_count,
        "element_count": stats.element_count,
        "min_quality": stats.min_quality,
        "avg_quality": stats.avg_quality,
    }).to_string()
}

#[wasm_bindgen]
pub fn cae_material_presets() -> String {
    let lib = kami_cae::material::MaterialLibrary::with_presets();
    let names: Vec<&str> = lib.materials.iter().map(|m| m.name.as_str()).collect();
    serde_json::json!({ "materials": names }).to_string()
}

// ── Shared: Engineering Core ──

#[wasm_bindgen]
pub fn eng_snap_to_grid(x: f64, y: f64, grid_spacing: f64) -> Vec<f64> {
    let grid = kami_eng_core::grid::GridConfig::new(grid_spacing);
    let snapped = kami_eng_core::snap::snap_to_grid(glam::DVec2::new(x, y), &grid);
    vec![snapped.x, snapped.y]
}

#[wasm_bindgen]
pub fn eng_measure_distance(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> f64 {
    let a = glam::DVec3::new(x1, y1, z1);
    let b = glam::DVec3::new(x2, y2, z2);
    kami_eng_core::measurement::distance_point_point(a, b).value
}

#[wasm_bindgen]
pub fn eng_color_map_sample(color_map: &str, t: f32) -> Vec<f32> {
    let cm = match color_map {
        "jet" => kami_eng_render::ColorMap::Jet,
        "viridis" => kami_eng_render::ColorMap::Viridis,
        "coolwarm" => kami_eng_render::ColorMap::Coolwarm,
        "grayscale" => kami_eng_render::ColorMap::Grayscale,
        _ => kami_eng_render::ColorMap::Rainbow,
    };
    cm.sample(t).to_vec()
}

// ── SPICE: Circuit Simulation ──

/// Parse a SPICE netlist string, solve DC operating point via MNA, and return
/// JSON with node voltages and branch currents.
#[wasm_bindgen]
pub fn spice_solve_dc_op(netlist: &str) -> String {
    match kami_spice::parse_spice_netlist(netlist) {
        Ok(circuit) => {
            let result = kami_spice::solve_dc_op(&circuit);
            serde_json::json!({
                "node_voltages": result.node_voltages,
                "branch_currents": result.branch_currents,
                "element_count": circuit.element_count(),
                "node_count": circuit.node_count(),
            }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

// ── PDK: Process Design Kit ──

/// Helper to convert a node name string (e.g. "N7", "N5") to TechNode enum.
fn parse_tech_node(node: &str) -> Option<kami_pdk::TechNode> {
    match node.to_uppercase().as_str() {
        "N180" => Some(kami_pdk::TechNode::N180),
        "N130" => Some(kami_pdk::TechNode::N130),
        "N90"  => Some(kami_pdk::TechNode::N90),
        "N65"  => Some(kami_pdk::TechNode::N65),
        "N45"  => Some(kami_pdk::TechNode::N45),
        "N28"  => Some(kami_pdk::TechNode::N28),
        "N22"  => Some(kami_pdk::TechNode::N22),
        "N16"  => Some(kami_pdk::TechNode::N16),
        "N14"  => Some(kami_pdk::TechNode::N14),
        "N10"  => Some(kami_pdk::TechNode::N10),
        "N7"   => Some(kami_pdk::TechNode::N7),
        "N5"   => Some(kami_pdk::TechNode::N5),
        "N3"   => Some(kami_pdk::TechNode::N3),
        "N2"   => Some(kami_pdk::TechNode::N2),
        _      => None,
    }
}

/// Return TechFile JSON for a given technology node (e.g. "N7").
#[wasm_bindgen]
pub fn pdk_tech_info(node: &str) -> String {
    match parse_tech_node(node) {
        Some(tech) => {
            let tf = kami_pdk::TechFile::for_node(tech);
            serde_json::json!({
                "node": format!("{:?}", tf.node),
                "feature_nm": tf.node.feature_nm(),
                "num_metal_layers": tf.num_metal_layers,
                "grid_unit_nm": tf.grid_unit_nm,
                "min_width": tf.min_width,
                "min_spacing": tf.min_spacing,
                "layer_count": tf.layer_map.len(),
            }).to_string()
        }
        None => serde_json::json!({ "error": format!("Unknown tech node: {}", node) }).to_string(),
    }
}

/// Return a generic standard cell library JSON for a given technology node.
#[wasm_bindgen]
pub fn pdk_stdcell_library(node: &str) -> String {
    match parse_tech_node(node) {
        Some(tech) => {
            let lib = kami_pdk::stdcell::create_generic_lib(tech);
            let cells: Vec<serde_json::Value> = lib.cells.iter().map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "function": format!("{:?}", c.function),
                    "drive_strength": c.drive_strength,
                    "area": c.area,
                    "input_pins": c.input_pins,
                    "output_pins": c.output_pins,
                })
            }).collect();
            serde_json::json!({
                "library_name": lib.name,
                "tech_node": format!("{:?}", lib.tech_node),
                "cell_count": lib.cells.len(),
                "cells": cells,
            }).to_string()
        }
        None => serde_json::json!({ "error": format!("Unknown tech node: {}", node) }).to_string(),
    }
}

/// Compile a memory macro (SRAM) and return estimated area/timing JSON.
#[wasm_bindgen]
pub fn pdk_compile_memory(words: u32, bits: u32, tech: &str) -> String {
    match parse_tech_node(tech) {
        Some(node) => {
            let spec = kami_pdk::memory::MemorySpec {
                mem_type: kami_pdk::memory::MemoryType::Sram,
                words,
                bits,
                mux: 4,
                banks: 1,
            };
            let result = kami_pdk::memory::compile_memory(&spec, node);
            serde_json::json!({
                "name": result.name,
                "area_um2": result.area_um2,
                "read_time_ns": result.read_time_ns,
                "write_time_ns": result.write_time_ns,
                "leakage_uw": result.leakage_uw,
                "pins": result.pins,
            }).to_string()
        }
        None => serde_json::json!({ "error": format!("Unknown tech node: {}", tech) }).to_string(),
    }
}

// ── PnR: Place and Route ──

/// Auto-floorplan blocks parsed from JSON. Input: JSON array of
/// `{ "name", "block_type", "width", "height" }`. Returns floorplan JSON.
#[wasm_bindgen]
pub fn pnr_auto_floorplan(blocks_json: &str) -> String {
    let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(blocks_json);
    match parsed {
        Ok(arr) => {
            let blocks: Vec<kami_pnr::FloorplanBlock> = arr.iter().map(|v| {
                let w = v["width"].as_f64().unwrap_or(100.0);
                let h = v["height"].as_f64().unwrap_or(100.0);
                kami_pnr::FloorplanBlock {
                    name: v["name"].as_str().unwrap_or("block").to_string(),
                    block_type: kami_pnr::BlockType::StdCellRegion,
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                    fixed: false,
                }
            }).collect();
            let total_area: f64 = blocks.iter().map(|b| b.width * b.height).sum();
            let die_side = (total_area * 1.5).sqrt();
            let fp = kami_pnr::floorplan::auto_floorplan(blocks, die_side, die_side);
            let block_info: Vec<serde_json::Value> = fp.blocks.iter().map(|b| {
                serde_json::json!({
                    "name": b.name,
                    "x": b.x,
                    "y": b.y,
                    "width": b.width,
                    "height": b.height,
                })
            }).collect();
            serde_json::json!({
                "die_width": fp.die_width,
                "die_height": fp.die_height,
                "utilization": fp.utilization(),
                "block_count": fp.blocks.len(),
                "blocks": block_info,
            }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

/// Export a simple test GDSII binary (single boundary element).
#[wasm_bindgen]
pub fn pnr_export_gdsii() -> Vec<u8> {
    let structure = kami_pnr::GdsiiStructure {
        name: "KAMI_TEST".to_string(),
        elements: vec![
            kami_pnr::GdsiiElement::Boundary {
                layer: 1,
                datatype: 0,
                xy: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000), (0, 0)],
            },
        ],
    };
    kami_pnr::gdsii::export_gdsii(&[structure])
}

// ── DFT: Design for Test ──

/// Insert scan chains: distribute `ff_count` flip-flops across `chain_count` chains.
/// Returns JSON with chain stats.
#[wasm_bindgen]
pub fn dft_insert_scan(ff_count: u32, chain_count: u32) -> String {
    let ffs: Vec<String> = (0..ff_count).map(|i| format!("ff_{}", i)).collect();
    let config = kami_dft::ScanChainConfig {
        chain_count: chain_count as usize,
        max_length: 1000,
        clock_name: "clk".to_string(),
        scan_enable: "scan_en".to_string(),
        scan_in_prefix: "SI_".to_string(),
        scan_out_prefix: "SO_".to_string(),
    };
    let chains = kami_dft::scan::insert_scan_chains(ffs, &config);
    let stats = kami_dft::ScanChain::scan_chain_stats(&chains);
    serde_json::json!({
        "num_chains": stats.num_chains,
        "total_ffs": stats.total_ffs,
        "max_chain_length": stats.max_length,
        "min_chain_length": stats.min_length,
        "chains": chains.iter().map(|c| serde_json::json!({
            "id": c.id,
            "length": c.length,
            "scan_in": c.scan_in_port,
            "scan_out": c.scan_out_port,
        })).collect::<Vec<_>>(),
    }).to_string()
}

/// Generate ATPG test patterns for `fault_count` stuck-at faults across `gate_count` gates.
#[wasm_bindgen]
pub fn dft_generate_atpg(fault_count: u32, gate_count: u32) -> String {
    let faults: Vec<kami_dft::atpg::Fault> = (0..fault_count).map(|i| {
        kami_dft::atpg::Fault {
            net_name: format!("net_{}", i),
            fault_type: if i % 2 == 0 {
                kami_dft::atpg::FaultType::StuckAt0
            } else {
                kami_dft::atpg::FaultType::StuckAt1
            },
            detected: false,
        }
    }).collect();
    let result = kami_dft::atpg::generate_patterns(faults, gate_count);
    serde_json::json!({
        "pattern_count": result.patterns.len(),
        "fault_coverage": result.fault_coverage,
        "detected_faults": result.detected_faults,
        "total_faults": result.total_faults,
        "aborted_faults": result.aborted_faults,
    }).to_string()
}

// ── Verify: Formal Verification & Coverage ──

/// Equivalence check between golden and revised gate-level netlists (JSON format).
/// Input JSON: `{ "gates": [["out","AND",["a","b"]], ...] }`
#[wasm_bindgen]
pub fn verify_check_equivalence(golden_json: &str, revised_json: &str) -> String {
    fn parse_gates(json_str: &str) -> Result<Vec<(String, String, Vec<String>)>, String> {
        let val: serde_json::Value = serde_json::from_str(json_str).map_err(|e| e.to_string())?;
        let arr = val["gates"].as_array().ok_or("missing 'gates' array")?;
        let mut gates = Vec::new();
        for g in arr {
            let g_arr = g.as_array().ok_or("gate must be array")?;
            let out = g_arr
                .first()
                .and_then(|v| v.as_str())
                .ok_or("gate[0] must be output name string")?
                .to_string();
            let gate_type = g_arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or("gate[1] must be gate type string")?
                .to_string();
            let inputs = g_arr
                .get(2)
                .and_then(|v| v.as_array())
                .ok_or("gate[2] must be input array")?
                .iter()
                .map(|x| {
                    x.as_str()
                        .ok_or("gate input must be string")
                        .map(|s| s.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            gates.push((out, gate_type, inputs));
        }
        Ok(gates)
    }

    match (parse_gates(golden_json), parse_gates(revised_json)) {
        (Ok(golden), Ok(revised)) => {
            let result = kami_verify::equivalence::check_equivalence(&golden, &revised);
            serde_json::json!({
                "status": format!("{:?}", result.status),
                "checked_points": result.checked_points,
                "mismatch_count": result.mismatches.len(),
                "time_ms": result.time_ms,
            })
            .to_string()
        }
        (Err(e), _) | (_, Err(e)) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Generate a coverage report given total coverage points and hit count.
#[wasm_bindgen]
pub fn verify_coverage_report(total: u32, hit: u32) -> String {
    let points: Vec<kami_verify::coverage::CoveragePoint> = (0..total).map(|i| {
        kami_verify::coverage::CoveragePoint {
            name: format!("cp_{}", i),
            cov_type: kami_verify::coverage::CoverageType::Line,
            hit: i < hit,
            hit_count: if i < hit { 1 } else { 0 },
        }
    }).collect();
    let report = kami_verify::coverage::CoverageReport {
        groups: vec![kami_verify::coverage::CoverageGroup {
            name: "default".to_string(),
            points,
            cross_bins: vec![],
        }],
    };
    serde_json::json!({
        "total_coverage_pct": report.total_coverage(),
        "total_points": total,
        "hit_points": hit,
        "uncovered_count": report.uncovered_points().len(),
    }).to_string()
}

// ── Power: Power Analysis ──

/// Estimate dynamic power given cell count, toggle rate, frequency, and voltage.
/// Returns JSON with power in microwatts and milliwatts.
#[wasm_bindgen]
pub fn power_estimate_dynamic(cells: u32, toggle: f64, freq_mhz: f64, voltage: f64) -> String {
    let load_cap_pf = 0.01; // typical average load capacitance
    let dynamic_uw = kami_power::estimation::estimate_dynamic_power(
        cells, toggle, freq_mhz, voltage, load_cap_pf,
    );
    serde_json::json!({
        "dynamic_power_uw": dynamic_uw,
        "dynamic_power_mw": dynamic_uw / 1000.0,
        "cells": cells,
        "toggle_rate": toggle,
        "freq_mhz": freq_mhz,
        "voltage": voltage,
    }).to_string()
}

/// Analyze IR drop on a uniform grid. Returns JSON with max/avg drop and hotspots.
#[wasm_bindgen]
pub fn power_analyze_ir_drop(rows: u32, cols: u32, supply_v: f64) -> String {
    let r = rows as usize;
    let c = cols as usize;
    // Uniform current draw at interior nodes (10 mA each).
    let mut current_map = vec![vec![0.0_f64; c]; r];
    for row in 1..r.saturating_sub(1) {
        for col in 1..c.saturating_sub(1) {
            current_map[row][col] = 0.01;
        }
    }
    let result = kami_power::ir_drop::analyze_ir_drop(r, c, supply_v, &current_map, 0.1);
    serde_json::json!({
        "max_drop_mv": result.max_drop_mv,
        "avg_drop_mv": result.avg_drop_mv,
        "worst_node": { "row": result.worst_node.0, "col": result.worst_node.1 },
        "hotspot_count": result.hotspots.len(),
        "grid_rows": rows,
        "grid_cols": cols,
        "supply_v": supply_v,
    }).to_string()
}

// ── SI: Signal Integrity ──

/// Calculate characteristic impedance Z0 for a microstrip transmission line.
#[wasm_bindgen]
pub fn si_calculate_z0(width_mm: f64, height_mm: f64, er: f64) -> String {
    let tline = kami_si::TLineType::Microstrip {
        width: width_mm,
        height: height_mm,
        er,
    };
    let params = kami_si::transmission_line::calculate_z0(&tline, 10.0);
    serde_json::json!({
        "z0_ohm": params.z0_ohm,
        "delay_ps_per_mm": params.delay_ps_per_mm,
        "loss_db_per_mm": params.loss_db_per_mm,
        "length_mm": params.length_mm,
        "type": "microstrip",
        "width_mm": width_mm,
        "height_mm": height_mm,
        "er": er,
    }).to_string()
}

/// Generate eye diagram data for a serial link at given data rate and amplitude.
#[wasm_bindgen]
pub fn si_eye_diagram(rate_gbps: f64, amplitude_mv: f64) -> String {
    let data = kami_si::eye_diagram::generate_eye_data(
        rate_gbps,
        amplitude_mv,
        /* rise_time_ps */ 30.0,
        /* noise_rms_mv */ amplitude_mv * 0.03,
        /* jitter_rms_ps */ 5.0,
        /* num_bits */ 128,
    );
    serde_json::json!({
        "eye_height_mv": data.metrics.eye_height_mv,
        "eye_width_ps": data.metrics.eye_width_ps,
        "jitter_rms_ps": data.metrics.jitter_rms_ps,
        "jitter_pp_ps": data.metrics.jitter_pp_ps,
        "ber_estimate": data.metrics.ber_estimate,
        "sample_count": data.samples.len(),
        "rate_gbps": rate_gbps,
        "amplitude_mv": amplitude_mv,
    }).to_string()
}

// ── PKG: Packaging ──

/// Estimate IC package body size and thermal properties.
/// `pkg_type`: "QFP", "BGA", "CSP", "WLCSP".
#[wasm_bindgen]
pub fn pkg_estimate(pkg_type: &str, die_w: f64, die_h: f64) -> String {
    let pt = match pkg_type.to_uppercase().as_str() {
        "QFP" => kami_pkg::PackageType::QFP { pin_count: 144, pitch_mm: 0.5 },
        "BGA" => kami_pkg::PackageType::BGA { rows: 20, cols: 20, pitch_mm: 0.8 },
        "CSP" => kami_pkg::PackageType::CSP { rows: 10, cols: 10, pitch_mm: 0.5 },
        "WLCSP" => kami_pkg::PackageType::WLCSP { bump_rows: 8, bump_cols: 8, bump_pitch_um: 400.0 },
        _ => return serde_json::json!({ "error": format!("Unknown package type: {}", pkg_type) }).to_string(),
    };
    let pkg = kami_pkg::package::estimate_package(pt, (die_w, die_h));
    serde_json::json!({
        "name": pkg.name,
        "body_size_mm": { "x": pkg.body_size_mm.0, "y": pkg.body_size_mm.1, "z": pkg.body_size_mm.2 },
        "die_size_mm": { "x": pkg.die_size_mm.0, "y": pkg.die_size_mm.1 },
        "pin_count": pkg.pin_count,
        "thermal_resistance_jc": pkg.thermal_resistance_jc,
        "thermal_resistance_ja": pkg.thermal_resistance_ja,
    }).to_string()
}

/// Calculate junction and case temperatures for a given power and ambient temp.
#[wasm_bindgen]
pub fn pkg_thermal(power_w: f64, ambient_c: f64) -> String {
    let spec = kami_pkg::ThermalSpec {
        power_w,
        ambient_c,
        theta_jc: 5.0,
        theta_ca: 20.0,
        airflow_m_per_s: None,
    };
    let result = kami_pkg::thermal::calculate_thermal(&spec);
    serde_json::json!({
        "junction_temp_c": result.junction_temp_c,
        "case_temp_c": result.case_temp_c,
        "power_w": result.power_w,
        "ambient_c": result.ambient_c,
        "theta_ja": result.theta_ja,
    }).to_string()
}

// ── Yield: Yield & Reliability ──

/// Run Monte Carlo simulation with a Gaussian distribution around a nominal value.
/// Returns JSON with statistical summary.
#[wasm_bindgen]
pub fn yield_monte_carlo(nominal: f64, sigma: f64, runs: u32) -> String {
    let config = kami_yield::MonteCarloConfig {
        num_runs: runs,
        seed: 42,
        parameters: vec![kami_yield::McParameter {
            name: "param".to_string(),
            nominal,
            distribution: kami_yield::Distribution::Gaussian { sigma },
        }],
    };
    let results = kami_yield::monte_carlo::run_monte_carlo(
        &config,
        |inputs| inputs[0],
        nominal - 3.0 * sigma,
        nominal + 3.0 * sigma,
    );
    let output = results.last().unwrap();
    serde_json::json!({
        "parameter_name": output.parameter_name,
        "mean": output.mean,
        "std_dev": output.std_dev,
        "min": output.min,
        "max": output.max,
        "yield_pass": output.yield_pass,
        "num_runs": runs,
    }).to_string()
}

/// Return the 5 standard PVT corners (TT/FF/SS/FS/SF) as JSON.
#[wasm_bindgen]
pub fn yield_standard_corners() -> String {
    let corners = kami_yield::corner::standard_corners();
    let arr: Vec<serde_json::Value> = corners.iter().map(|c| {
        serde_json::json!({
            "name": c.name,
            "process": format!("{:?}", c.process),
            "voltage": c.voltage,
            "temperature_c": c.temperature_c,
        })
    }).collect();
    serde_json::json!({ "corners": arr, "count": corners.len() }).to_string()
}

// ── IP: IP Management ──

/// Generate a Network-on-Chip design. Topology: "mesh", "ring", "crossbar".
#[wasm_bindgen]
pub fn ip_generate_noc(topology: &str, rows: u32, cols: u32) -> String {
    let topo = match topology.to_lowercase().as_str() {
        "mesh" => kami_ip::NocTopology::Mesh { rows, cols },
        "ring" => kami_ip::NocTopology::Ring { nodes: rows * cols },
        "crossbar" => kami_ip::NocTopology::Crossbar { ports: rows * cols },
        _ => return serde_json::json!({ "error": format!("Unknown topology: {}", topology) }).to_string(),
    };
    let config = kami_ip::NocConfig {
        topology: topo,
        data_width: 64,
        flit_size: 128,
        routing: kami_ip::noc::RoutingAlgorithm::XY,
    };
    let design = kami_ip::noc::generate_noc(&config);
    serde_json::json!({
        "router_count": design.routers.len(),
        "link_count": design.links.len(),
        "total_area_um2": design.total_area_um2,
        "estimated_latency_cycles": design.estimated_latency_cycles,
        "topology": topology,
    }).to_string()
}

/// Analyze clock domain crossings from a JSON signal list.
/// Input: `[{ "name", "source_clock", "dest_clock", "width", "has_synchronizer" }]`
#[wasm_bindgen]
pub fn ip_analyze_cdc(signals_json: &str) -> String {
    let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(signals_json);
    match parsed {
        Ok(arr) => {
            let signals: Vec<kami_ip::cdc::CdcSignal> = arr.iter().map(|v| {
                kami_ip::cdc::CdcSignal {
                    name: v["name"].as_str().unwrap_or("sig").to_string(),
                    source_clock: v["source_clock"].as_str().unwrap_or("clk_a").to_string(),
                    dest_clock: v["dest_clock"].as_str().unwrap_or("clk_b").to_string(),
                    width: v["width"].as_u64().unwrap_or(1) as u32,
                    has_synchronizer: v["has_synchronizer"].as_bool().unwrap_or(false),
                    synchronizer: None,
                }
            }).collect();
            // Collect unique clock domains from signals.
            let mut clock_set = std::collections::HashSet::new();
            for s in &signals {
                clock_set.insert(s.source_clock.clone());
                clock_set.insert(s.dest_clock.clone());
            }
            let clocks: Vec<kami_ip::cdc::ClockDomain> = clock_set.into_iter().map(|name| {
                kami_ip::cdc::ClockDomain { name, freq_mhz: 100.0 }
            }).collect();
            let report = kami_ip::cdc::analyze_cdc(&signals, &clocks);
            serde_json::json!({
                "crossing_count": report.crossings.len(),
                "violation_count": report.violations.len(),
                "violations": report.violations.iter().map(|v| serde_json::json!({
                    "signal": v.signal,
                    "issue": format!("{:?}", v.issue),
                })).collect::<Vec<_>>(),
            }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

// ── Flow: P10 Integration ──

/// Run minimal end-to-end implementation flow and return signoff report JSON.
/// Input JSON fields are optional:
/// `rtl_source`, `top_module_hint`, `die_width_um`, `die_height_um`,
/// `clock_freq_mhz`, `supply_v`, `cell_count_estimate`.
#[wasm_bindgen]
pub fn flow_run_signoff(input_json: &str) -> String {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FlowRunSignoffThresholds {
        max_ir_drop_mv: Option<f64>,
        min_dft_atpg_coverage: Option<f64>,
        si_z0_min_ohm: Option<f64>,
        si_z0_max_ohm: Option<f64>,
        min_yield_pass_ratio: Option<f64>,
        min_setup_slack_ps: Option<f64>,
        min_hold_slack_ps: Option<f64>,
        max_drc_violations: Option<usize>,
        max_lvs_mismatches: Option<usize>,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FlowRunSignoffDrcLvs {
        run_drc: Option<bool>,
        run_lvs: Option<bool>,
        drc_rule_deck: Option<String>,
        lvs_rule_deck: Option<String>,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FlowRunSignoffInput {
        rtl_source: Option<String>,
        top_module_hint: Option<String>,
        die_width_um: Option<f64>,
        die_height_um: Option<f64>,
        clock_freq_mhz: Option<f64>,
        supply_v: Option<f64>,
        cell_count_estimate: Option<u32>,
        policy_version: Option<String>,
        policy_profile: Option<String>,
        thresholds: Option<FlowRunSignoffThresholds>,
        drc_lvs: Option<FlowRunSignoffDrcLvs>,
    }

    let mut input = kami_flow::FlowInput::default();
    if !input_json.trim().is_empty() {
        let parsed: Result<FlowRunSignoffInput, _> = serde_json::from_str(input_json);
        match parsed {
            Ok(v) => {
                if let Some(x) = v.rtl_source { input.rtl_source = x; }
                if let Some(x) = v.top_module_hint { input.top_module_hint = Some(x); }
                if let Some(x) = v.die_width_um { input.die_width_um = x; }
                if let Some(x) = v.die_height_um { input.die_height_um = x; }
                if let Some(x) = v.clock_freq_mhz { input.clock_freq_mhz = x; }
                if let Some(x) = v.supply_v { input.supply_v = x; }
                if let Some(x) = v.cell_count_estimate { input.cell_count_estimate = x; }
                if let Some(x) = v.policy_version { input.policy_version = x; }
                if let Some(x) = v.policy_profile { input.policy_profile = x; }
                if let Some(t) = v.thresholds {
                    if let Some(x) = t.max_ir_drop_mv { input.thresholds.max_ir_drop_mv = x; }
                    if let Some(x) = t.min_dft_atpg_coverage { input.thresholds.min_dft_atpg_coverage = x; }
                    if let Some(x) = t.si_z0_min_ohm { input.thresholds.si_z0_min_ohm = x; }
                    if let Some(x) = t.si_z0_max_ohm { input.thresholds.si_z0_max_ohm = x; }
                    if let Some(x) = t.min_yield_pass_ratio { input.thresholds.min_yield_pass_ratio = x; }
                    if let Some(x) = t.min_setup_slack_ps { input.thresholds.min_setup_slack_ps = x; }
                    if let Some(x) = t.min_hold_slack_ps { input.thresholds.min_hold_slack_ps = x; }
                    if let Some(x) = t.max_drc_violations { input.thresholds.max_drc_violations = x; }
                    if let Some(x) = t.max_lvs_mismatches { input.thresholds.max_lvs_mismatches = x; }
                }
                if let Some(c) = v.drc_lvs {
                    if let Some(x) = c.run_drc { input.drc_lvs.run_drc = x; }
                    if let Some(x) = c.run_lvs { input.drc_lvs.run_lvs = x; }
                    if let Some(x) = c.drc_rule_deck { input.drc_lvs.drc_rule_deck = x; }
                    if let Some(x) = c.lvs_rule_deck { input.drc_lvs.lvs_rule_deck = x; }
                }
            }
            Err(e) => return serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    match kami_flow::run_minimal_flow(&input) {
        Ok(report) => serde_json::to_string(&report)
            .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

// ── Version ──

#[wasm_bindgen]
pub fn eng_sdk_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
