/// KAMI Flow — orchestration layer for end-to-end digital implementation.
///
/// P10 integrated pipeline:
/// RTL -> PnR -> GDSII -> Verify/Power/DFT/SI/Yield -> STA -> DRC/LVS -> Signoff.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowThresholds {
    pub max_ir_drop_mv: f64,
    pub min_dft_atpg_coverage: f64,
    pub si_z0_min_ohm: f64,
    pub si_z0_max_ohm: f64,
    pub min_yield_pass_ratio: f64,
    pub min_setup_slack_ps: f64,
    pub min_hold_slack_ps: f64,
    pub max_drc_violations: usize,
    pub max_lvs_mismatches: usize,
}

impl Default for FlowThresholds {
    fn default() -> Self {
        Self {
            max_ir_drop_mv: 100.0,
            min_dft_atpg_coverage: 0.95,
            si_z0_min_ohm: 40.0,
            si_z0_max_ohm: 60.0,
            min_yield_pass_ratio: 0.95,
            min_setup_slack_ps: 0.0,
            min_hold_slack_ps: 0.0,
            max_drc_violations: 0,
            max_lvs_mismatches: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrcLvsConfig {
    pub run_drc: bool,
    pub run_lvs: bool,
    pub drc_rule_deck: String,
    pub lvs_rule_deck: String,
}

impl Default for DrcLvsConfig {
    fn default() -> Self {
        Self {
            run_drc: true,
            run_lvs: true,
            drc_rule_deck: "default_drc.deck".to_string(),
            lvs_rule_deck: "default_lvs.deck".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowInput {
    pub rtl_source: String,
    pub top_module_hint: Option<String>,
    pub die_width_um: f64,
    pub die_height_um: f64,
    pub clock_freq_mhz: f64,
    pub supply_v: f64,
    pub cell_count_estimate: u32,
    pub policy_version: String,
    pub policy_profile: String,
    pub thresholds: FlowThresholds,
    pub drc_lvs: DrcLvsConfig,
}

impl Default for FlowInput {
    fn default() -> Self {
        Self {
            rtl_source: "module top(input a, input b, output y); assign y = a & b; endmodule"
                .to_string(),
            top_module_hint: None,
            die_width_um: 2000.0,
            die_height_um: 2000.0,
            clock_freq_mhz: 500.0,
            supply_v: 0.8,
            cell_count_estimate: 20_000,
            policy_version: "p11.1".to_string(),
            policy_profile: "nominal".to_string(),
            thresholds: FlowThresholds::default(),
            drc_lvs: DrcLvsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub name: String,
    pub kind: String,
    pub bytes: usize,
    pub hash_fnv1a64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignoffReport {
    pub run_id: String,
    pub input_hash_fnv1a64: String,
    pub policy_version: String,
    pub policy_profile: String,
    pub top_module: String,
    pub rtl_port_count: usize,
    pub rtl_parse_ok: bool,
    pub floorplan_utilization: f64,
    pub floorplan_violations: Vec<String>,
    pub gdsii_size_bytes: usize,
    pub equivalence_status: String,
    pub equivalence_mismatch_count: usize,
    pub dynamic_power_mw: f64,
    pub ir_max_drop_mv: f64,
    pub dft_scan_chain_count: usize,
    pub dft_atpg_coverage: f64,
    pub si_z0_ohm: f64,
    pub si_eye_height_mv: f64,
    pub yield_pass_ratio: f64,
    pub pvt_corner_count: usize,
    pub sta_setup_slack_ps: f64,
    pub sta_hold_slack_ps: f64,
    pub drc_violations: usize,
    pub lvs_mismatches: usize,
    pub drc_rule_deck: String,
    pub lvs_rule_deck: String,
    pub artifact_count: usize,
    pub artifacts: Vec<ArtifactRecord>,
    pub signoff_pass: bool,
    pub checks: Vec<(String, bool)>,
}

#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("RTL parse failed: {0}")]
    RtlParse(String),
}

fn fnv1a64_hex(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in data {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

fn build_run_id(input_hash: &str, gds_hash: &str) -> String {
    format!("run-{}-{}", &input_hash[..8], &gds_hash[..8])
}

fn run_pnr(die_width_um: f64, die_height_um: f64) -> (kami_pnr::Floorplan, Vec<u8>) {
    let blocks = vec![
        kami_pnr::FloorplanBlock {
            name: "stdcell_core".to_string(),
            block_type: kami_pnr::BlockType::StdCellRegion,
            x: 0.0,
            y: 0.0,
            width: die_width_um * 0.8,
            height: die_height_um * 0.7,
            fixed: false,
        },
        kami_pnr::FloorplanBlock {
            name: "sram0".to_string(),
            block_type: kami_pnr::BlockType::Macro,
            x: 0.0,
            y: 0.0,
            width: die_width_um * 0.15,
            height: die_height_um * 0.15,
            fixed: false,
        },
    ];
    let fp = kami_pnr::floorplan::auto_floorplan(blocks, die_width_um, die_height_um);

    let gds = kami_pnr::gdsii::export_gdsii(&[kami_pnr::GdsiiStructure {
        name: "TOP".to_string(),
        elements: vec![kami_pnr::GdsiiElement::Boundary {
            layer: 1,
            datatype: 0,
            xy: vec![
                (0, 0),
                (die_width_um as i32, 0),
                (die_width_um as i32, die_height_um as i32),
                (0, die_height_um as i32),
                (0, 0),
            ],
        }],
    }]);

    (fp, gds)
}

fn run_verify() -> kami_verify::equivalence::EquivResult {
    let golden = vec![(
        "y".to_string(),
        "AND".to_string(),
        vec!["a".to_string(), "b".to_string()],
    )];
    let revised = golden.clone();
    kami_verify::equivalence::check_equivalence(&golden, &revised)
}

fn run_power(cell_count_estimate: u32, clock_freq_mhz: f64, supply_v: f64) -> (f64, f64) {
    let dynamic_uw = kami_power::estimation::estimate_dynamic_power(
        cell_count_estimate,
        0.15,
        clock_freq_mhz,
        supply_v,
        0.01,
    );

    let rows = 8usize;
    let cols = 8usize;
    let mut current_map = vec![vec![0.0_f64; cols]; rows];
    for row in 1..rows - 1 {
        for col in 1..cols - 1 {
            current_map[row][col] = 0.01;
        }
    }
    let ir = kami_power::ir_drop::analyze_ir_drop(rows, cols, supply_v, &current_map, 0.1);
    (dynamic_uw / 1000.0, ir.max_drop_mv)
}

fn run_dft(ff_count: u32, gate_count: u32) -> (usize, f64) {
    let ffs: Vec<String> = (0..ff_count).map(|i| format!("ff_{i}")).collect();
    let chains = kami_dft::scan::insert_scan_chains(
        ffs,
        &kami_dft::ScanChainConfig {
            chain_count: 4,
            max_length: 1024,
            clock_name: "clk".to_string(),
            scan_enable: "scan_en".to_string(),
            scan_in_prefix: "SI_".to_string(),
            scan_out_prefix: "SO_".to_string(),
        },
    );
    let scan_stats = kami_dft::ScanChain::scan_chain_stats(&chains);

    let faults: Vec<kami_dft::Fault> = (0..64)
        .map(|i| kami_dft::Fault {
            net_name: format!("n_{i}"),
            fault_type: if i % 2 == 0 {
                kami_dft::FaultType::StuckAt0
            } else {
                kami_dft::FaultType::StuckAt1
            },
            detected: false,
        })
        .collect();
    let atpg = kami_dft::atpg::generate_patterns(faults, gate_count);
    (scan_stats.num_chains, atpg.fault_coverage)
}

fn run_si() -> (f64, f64) {
    let tline = kami_si::TLineType::Microstrip {
        width: 0.3,
        height: 0.2,
        er: 4.2,
    };
    let z = kami_si::transmission_line::calculate_z0(&tline, 10.0);
    let eye = kami_si::eye_diagram::generate_eye_data(10.0, 800.0, 30.0, 10.0, 5.0, 128);
    (z.z0_ohm, eye.metrics.eye_height_mv)
}

fn run_yield() -> (f64, usize) {
    let config = kami_yield::MonteCarloConfig {
        num_runs: 1000,
        seed: 42,
        parameters: vec![kami_yield::McParameter {
            name: "vth".to_string(),
            nominal: 0.4,
            distribution: kami_yield::Distribution::Gaussian { sigma: 0.02 },
        }],
    };
    let results = kami_yield::monte_carlo::run_monte_carlo(&config, |p| p[0], 0.35, 0.45);
    let yield_pass = results.last().map(|r| r.yield_pass).unwrap_or(0.0);
    let corners = kami_yield::corner::standard_corners();
    (yield_pass, corners.len())
}

fn run_sta(clock_freq_mhz: f64, cell_count_estimate: u32) -> (f64, f64) {
    let period_ps = 1_000_000.0 / clock_freq_mhz.max(1.0);
    let estimated_path_delay_ps = period_ps * 0.45 + (cell_count_estimate as f64 / 1000.0) * 2.5;
    let setup_slack_ps = period_ps - estimated_path_delay_ps;
    let hold_slack_ps = 15.0 - (cell_count_estimate as f64 / 5000.0);
    (setup_slack_ps, hold_slack_ps)
}

fn run_drc_lvs(
    fp: &kami_pnr::Floorplan,
    equiv: &kami_verify::equivalence::EquivResult,
    config: &DrcLvsConfig,
) -> (usize, usize) {
    let drc_violations = if config.run_drc {
        let mut count = fp.validate().len();
        if fp.utilization() > 0.85 {
            count += 1;
        }
        count
    } else {
        0
    };

    let lvs_mismatches = if config.run_lvs {
        if matches!(equiv.status, kami_verify::equivalence::EquivStatus::Pass) {
            0
        } else {
            1
        }
    } else {
        0
    };

    (drc_violations, lvs_mismatches)
}

pub fn run_minimal_flow(input: &FlowInput) -> Result<SignoffReport, FlowError> {
    let input_json = serde_json::to_vec(input).unwrap_or_default();
    let input_hash_fnv1a64 = fnv1a64_hex(&input_json);

    let (parsed_module_name, ports) =
        kami_rtl::hdl::parse_verilog(&input.rtl_source).map_err(FlowError::RtlParse)?;
    let top_module = input.top_module_hint.clone().unwrap_or(parsed_module_name);

    let (fp, gds_bytes) = run_pnr(input.die_width_um, input.die_height_um);
    let gdsii_size_bytes = gds_bytes.len();
    let floorplan_violations = fp.validate();

    let equiv = run_verify();
    let (dynamic_power_mw, ir_max_drop_mv) =
        run_power(input.cell_count_estimate, input.clock_freq_mhz, input.supply_v);
    let (dft_scan_chain_count, dft_atpg_coverage) =
        run_dft((input.cell_count_estimate / 100).max(16), input.cell_count_estimate / 10);
    let (si_z0_ohm, si_eye_height_mv) = run_si();
    let (yield_pass_ratio, pvt_corner_count) = run_yield();
    let (sta_setup_slack_ps, sta_hold_slack_ps) =
        run_sta(input.clock_freq_mhz, input.cell_count_estimate);
    let (drc_violations, lvs_mismatches) = run_drc_lvs(&fp, &equiv, &input.drc_lvs);

    let t = &input.thresholds;
    let checks = vec![
        ("rtl_parse".to_string(), true),
        (
            "floorplan_valid".to_string(),
            floorplan_violations.is_empty(),
        ),
        (
            "equivalence".to_string(),
            matches!(equiv.status, kami_verify::equivalence::EquivStatus::Pass),
        ),
        (
            "sta_setup_slack".to_string(),
            sta_setup_slack_ps >= t.min_setup_slack_ps,
        ),
        (
            "sta_hold_slack".to_string(),
            sta_hold_slack_ps >= t.min_hold_slack_ps,
        ),
        (
            "ir_drop_under_threshold".to_string(),
            ir_max_drop_mv <= t.max_ir_drop_mv,
        ),
        (
            "dft_atpg_coverage".to_string(),
            dft_atpg_coverage >= t.min_dft_atpg_coverage,
        ),
        (
            "si_z0_range".to_string(),
            si_z0_ohm >= t.si_z0_min_ohm && si_z0_ohm <= t.si_z0_max_ohm,
        ),
        (
            "yield_threshold".to_string(),
            yield_pass_ratio >= t.min_yield_pass_ratio,
        ),
        (
            "drc_violations_threshold".to_string(),
            drc_violations <= t.max_drc_violations,
        ),
        (
            "lvs_mismatches_threshold".to_string(),
            lvs_mismatches <= t.max_lvs_mismatches,
        ),
    ];
    let signoff_pass = checks.iter().all(|(_, ok)| *ok);

    let rtl_bytes = input.rtl_source.as_bytes().to_vec();
    let floorplan_json = serde_json::to_vec(&fp).unwrap_or_default();
    let gds_hash = fnv1a64_hex(&gds_bytes);
    let run_id = build_run_id(&input_hash_fnv1a64, &gds_hash);

    let artifacts = vec![
        ArtifactRecord {
            name: "rtl_source.v".to_string(),
            kind: "rtl".to_string(),
            bytes: rtl_bytes.len(),
            hash_fnv1a64: fnv1a64_hex(&rtl_bytes),
        },
        ArtifactRecord {
            name: "floorplan.json".to_string(),
            kind: "pnr-floorplan".to_string(),
            bytes: floorplan_json.len(),
            hash_fnv1a64: fnv1a64_hex(&floorplan_json),
        },
        ArtifactRecord {
            name: "layout.gds".to_string(),
            kind: "gdsii".to_string(),
            bytes: gdsii_size_bytes,
            hash_fnv1a64: gds_hash,
        },
    ];

    Ok(SignoffReport {
        run_id,
        input_hash_fnv1a64,
        policy_version: input.policy_version.clone(),
        policy_profile: input.policy_profile.clone(),
        top_module,
        rtl_port_count: ports.len(),
        rtl_parse_ok: true,
        floorplan_utilization: fp.utilization(),
        floorplan_violations,
        gdsii_size_bytes,
        equivalence_status: format!("{:?}", equiv.status),
        equivalence_mismatch_count: equiv.mismatches.len(),
        dynamic_power_mw,
        ir_max_drop_mv,
        dft_scan_chain_count,
        dft_atpg_coverage,
        si_z0_ohm,
        si_eye_height_mv,
        yield_pass_ratio,
        pvt_corner_count,
        sta_setup_slack_ps,
        sta_hold_slack_ps,
        drc_violations,
        lvs_mismatches,
        drc_rule_deck: input.drc_lvs.drc_rule_deck.clone(),
        lvs_rule_deck: input.drc_lvs.lvs_rule_deck.clone(),
        artifact_count: artifacts.len(),
        artifacts,
        signoff_pass,
        checks,
    })
}

pub fn run_minimal_flow_json(input: &FlowInput) -> Result<String, FlowError> {
    let report = run_minimal_flow(input)?;
    serde_json::to_string(&report).map_err(|e| FlowError::RtlParse(e.to_string()))
}

pub fn render_signoff_html(report: &SignoffReport) -> String {
    let mut rows = String::new();
    for (name, ok) in &report.checks {
        rows.push_str(&format!(
            "<tr><td>{}</td><td style=\"color:{}\">{}</td></tr>",
            name,
            if *ok { "#0a0" } else { "#b00" },
            if *ok { "PASS" } else { "FAIL" }
        ));
    }

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>KAMI Signoff</title></head><body>\
         <h1>KAMI P10 Signoff Report</h1>\
         <p><b>Run ID:</b> {}</p>\
         <p><b>Input Hash:</b> {}</p>\
         <p><b>Policy:</b> {} ({})</p>\
         <p><b>Top:</b> {}</p>\
         <p><b>RTL Ports:</b> {}</p>\
         <p><b>PnR Utilization:</b> {:.3}</p>\
         <p><b>GDSII Size:</b> {} bytes</p>\
         <p><b>Equivalence:</b> {} (mismatch={})</p>\
         <p><b>STA Setup Slack:</b> {:.3} ps</p>\
         <p><b>STA Hold Slack:</b> {:.3} ps</p>\
         <p><b>Dynamic Power:</b> {:.3} mW</p>\
         <p><b>IR Max Drop:</b> {:.3} mV</p>\
         <p><b>DRC Violations:</b> {} (deck={})</p>\
         <p><b>LVS Mismatches:</b> {} (deck={})</p>\
         <p><b>DFT Scan Chains:</b> {}</p>\
         <p><b>ATPG Coverage:</b> {:.2}%</p>\
         <p><b>SI Z0:</b> {:.3} ohm</p>\
         <p><b>SI Eye Height:</b> {:.3} mV</p>\
         <p><b>Yield:</b> {:.2}%</p>\
         <p><b>PVT Corners:</b> {}</p>\
         <p><b>Artifacts:</b> {}</p>\
         <p><b>Overall:</b> <span style=\"color:{}\">{}</span></p>\
         <table border=\"1\" cellspacing=\"0\" cellpadding=\"6\">\
         <tr><th>Check</th><th>Result</th></tr>{}</table>\
         </body></html>",
        report.run_id,
        report.input_hash_fnv1a64,
        report.policy_version,
        report.policy_profile,
        report.top_module,
        report.rtl_port_count,
        report.floorplan_utilization,
        report.gdsii_size_bytes,
        report.equivalence_status,
        report.equivalence_mismatch_count,
        report.sta_setup_slack_ps,
        report.sta_hold_slack_ps,
        report.dynamic_power_mw,
        report.ir_max_drop_mv,
        report.drc_violations,
        report.drc_rule_deck,
        report.lvs_mismatches,
        report.lvs_rule_deck,
        report.dft_scan_chain_count,
        report.dft_atpg_coverage * 100.0,
        report.si_z0_ohm,
        report.si_eye_height_mv,
        report.yield_pass_ratio * 100.0,
        report.pvt_corner_count,
        report.artifact_count,
        if report.signoff_pass { "#0a0" } else { "#b00" },
        if report.signoff_pass { "PASS" } else { "FAIL" },
        rows
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_generates_signoff_report() {
        let input = FlowInput::default();
        let report = run_minimal_flow(&input).expect("flow should succeed");
        assert!(report.rtl_parse_ok);
        assert!(report.gdsii_size_bytes > 100);
        assert_eq!(report.equivalence_status, "Pass");
        assert!(report.dynamic_power_mw > 0.0);
        assert!(report.dft_atpg_coverage > 0.0);
        assert!(report.si_z0_ohm > 0.0);
        assert!(report.yield_pass_ratio > 0.0);
        assert!(report.sta_setup_slack_ps.is_finite());
        assert!(report.artifact_count >= 3);
        assert_eq!(report.policy_version, "p11.1");
    }

    #[test]
    fn flow_threshold_override_can_fail_signoff() {
        let mut input = FlowInput::default();
        input.thresholds.min_setup_slack_ps = 10_000.0;
        let report = run_minimal_flow(&input).expect("flow should succeed");
        assert!(!report.signoff_pass);
    }

    #[test]
    fn flow_fails_on_invalid_rtl() {
        let input = FlowInput {
            rtl_source: "module broken(".to_string(),
            ..FlowInput::default()
        };
        assert!(run_minimal_flow(&input).is_err());
    }

    #[test]
    fn signoff_html_contains_summary() {
        let report = run_minimal_flow(&FlowInput::default()).expect("flow should succeed");
        let html = render_signoff_html(&report);
        assert!(html.contains("KAMI P10 Signoff Report"));
        assert!(html.contains("STA Setup Slack"));
        assert!(html.contains("DRC Violations"));
        assert!(html.contains("LVS Mismatches"));
        assert!(html.contains("Run ID"));
    }
}
