//! KAMI Power — power analysis for digital designs.
//!
//! Provides UPF power intent parsing, static/dynamic power estimation, IR drop
//! grid analysis, and electromigration rule checking.

pub mod upf {
    //! Unified Power Format (UPF) power intent description and validation.

    use serde::{Deserialize, Serialize};

    /// A power domain grouping related design elements.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PowerDomain {
        pub name: String,
        pub supply_net: String,
        pub ground_net: String,
        pub elements: Vec<String>,
        pub default_retention: bool,
    }

    /// A power switch controlling domain supply.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PowerSwitch {
        pub name: String,
        pub domain: String,
        pub control_signal: String,
        pub ack_signal: String,
    }

    /// Isolation cell clamp behavior.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ClampValue {
        HighZ,
        Zero,
        One,
        Latch,
    }

    /// Location of an isolation cell relative to its domain.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum IsolationLocation {
        From,
        To,
    }

    /// An isolation cell at a domain boundary.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IsolationCell {
        pub name: String,
        pub domain: String,
        pub location: IsolationLocation,
        pub clamp_value: ClampValue,
    }

    /// A retention cell preserving state during power-down.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RetentionCell {
        pub name: String,
        pub domain: String,
        pub save_signal: String,
        pub restore_signal: String,
    }

    /// Direction of a level shifter.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ShifterDirection {
        LowToHigh,
        HighToLow,
    }

    /// A level shifter between two voltage domains.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LevelShifter {
        pub name: String,
        pub from_domain: String,
        pub to_domain: String,
        pub direction: ShifterDirection,
    }

    /// Complete UPF design description.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct UpfDesign {
        pub domains: Vec<PowerDomain>,
        pub switches: Vec<PowerSwitch>,
        pub isolations: Vec<IsolationCell>,
        pub retentions: Vec<RetentionCell>,
        pub level_shifters: Vec<LevelShifter>,
    }

    /// Parse a simplified UPF text format.
    ///
    /// Supported commands (one per line):
    /// - `create_power_domain <name> -supply <net> -ground <gnd> [-elements <e1,e2,...>] [-retention]`
    /// - `create_power_switch <name> -domain <dom> -control <sig> -ack <sig>`
    /// - `set_isolation <name> -domain <dom> -location <from|to> -clamp <highz|zero|one|latch>`
    /// - `set_retention <name> -domain <dom> -save <sig> -restore <sig>`
    /// - `set_level_shifter <name> -from <dom> -to <dom> -direction <low_to_high|high_to_low>`
    pub fn parse_upf(input: &str) -> UpfDesign {
        let mut design = UpfDesign::default();

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            match tokens[0] {
                "create_power_domain" => {
                    let name = tokens.get(1).unwrap_or(&"").to_string();
                    let supply = flag_value(&tokens, "-supply").unwrap_or_default();
                    let ground = flag_value(&tokens, "-ground").unwrap_or_default();
                    let elements: Vec<String> = flag_value(&tokens, "-elements")
                        .map(|s| s.split(',').map(|e| e.to_string()).collect())
                        .unwrap_or_default();
                    let retention = tokens.contains(&"-retention");
                    design.domains.push(PowerDomain {
                        name,
                        supply_net: supply,
                        ground_net: ground,
                        elements,
                        default_retention: retention,
                    });
                }
                "create_power_switch" => {
                    let name = tokens.get(1).unwrap_or(&"").to_string();
                    design.switches.push(PowerSwitch {
                        name,
                        domain: flag_value(&tokens, "-domain").unwrap_or_default(),
                        control_signal: flag_value(&tokens, "-control").unwrap_or_default(),
                        ack_signal: flag_value(&tokens, "-ack").unwrap_or_default(),
                    });
                }
                "set_isolation" => {
                    let name = tokens.get(1).unwrap_or(&"").to_string();
                    let loc = match flag_value(&tokens, "-location").as_deref() {
                        Some("to") => IsolationLocation::To,
                        _ => IsolationLocation::From,
                    };
                    let clamp = match flag_value(&tokens, "-clamp").as_deref() {
                        Some("zero") => ClampValue::Zero,
                        Some("one") => ClampValue::One,
                        Some("latch") => ClampValue::Latch,
                        _ => ClampValue::HighZ,
                    };
                    design.isolations.push(IsolationCell {
                        name,
                        domain: flag_value(&tokens, "-domain").unwrap_or_default(),
                        location: loc,
                        clamp_value: clamp,
                    });
                }
                "set_retention" => {
                    let name = tokens.get(1).unwrap_or(&"").to_string();
                    design.retentions.push(RetentionCell {
                        name,
                        domain: flag_value(&tokens, "-domain").unwrap_or_default(),
                        save_signal: flag_value(&tokens, "-save").unwrap_or_default(),
                        restore_signal: flag_value(&tokens, "-restore").unwrap_or_default(),
                    });
                }
                "set_level_shifter" => {
                    let name = tokens.get(1).unwrap_or(&"").to_string();
                    let dir = match flag_value(&tokens, "-direction").as_deref() {
                        Some("high_to_low") => ShifterDirection::HighToLow,
                        _ => ShifterDirection::LowToHigh,
                    };
                    design.level_shifters.push(LevelShifter {
                        name,
                        from_domain: flag_value(&tokens, "-from").unwrap_or_default(),
                        to_domain: flag_value(&tokens, "-to").unwrap_or_default(),
                        direction: dir,
                    });
                }
                _ => {
                    log::warn!("Unknown UPF command: {}", tokens[0]);
                }
            }
        }
        design
    }

    /// Extract the value following a flag token (e.g., `-supply VDD` → `"VDD"`).
    fn flag_value(tokens: &[&str], flag: &str) -> Option<String> {
        tokens
            .iter()
            .position(|t| *t == flag)
            .and_then(|i| tokens.get(i + 1))
            .map(|s| s.to_string())
    }

    impl UpfDesign {
        /// Validate the UPF design, returning a list of warning/error messages.
        pub fn validate(&self) -> Vec<String> {
            let mut errors = Vec::new();
            let domain_names: Vec<&str> =
                self.domains.iter().map(|d| d.name.as_str()).collect();

            // Every switch must reference an existing domain.
            for sw in &self.switches {
                if !domain_names.contains(&sw.domain.as_str()) {
                    errors.push(format!(
                        "Power switch '{}' references unknown domain '{}'",
                        sw.name, sw.domain
                    ));
                }
            }
            // Every isolation must reference an existing domain.
            for iso in &self.isolations {
                if !domain_names.contains(&iso.domain.as_str()) {
                    errors.push(format!(
                        "Isolation cell '{}' references unknown domain '{}'",
                        iso.name, iso.domain
                    ));
                }
            }
            // Every retention must reference an existing domain.
            for ret in &self.retentions {
                if !domain_names.contains(&ret.domain.as_str()) {
                    errors.push(format!(
                        "Retention cell '{}' references unknown domain '{}'",
                        ret.name, ret.domain
                    ));
                }
            }
            // Level shifters must reference existing domains.
            for ls in &self.level_shifters {
                if !domain_names.contains(&ls.from_domain.as_str()) {
                    errors.push(format!(
                        "Level shifter '{}' references unknown from_domain '{}'",
                        ls.name, ls.from_domain
                    ));
                }
                if !domain_names.contains(&ls.to_domain.as_str()) {
                    errors.push(format!(
                        "Level shifter '{}' references unknown to_domain '{}'",
                        ls.name, ls.to_domain
                    ));
                }
            }
            errors
        }
    }
}

pub mod estimation {
    //! Static and dynamic power estimation.

    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// Breakdown of power by component.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PowerBreakdown {
        pub leakage_uw: f64,
        pub internal_uw: f64,
        pub switching_uw: f64,
        pub total_uw: f64,
    }

    /// Full power estimate with per-domain and per-hierarchy breakdowns.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PowerEstimate {
        pub total_mw: f64,
        pub by_domain: HashMap<String, PowerBreakdown>,
        pub by_hierarchy: HashMap<String, f64>,
    }

    /// Estimate static (leakage) power in micro-watts.
    ///
    /// Leakage scales exponentially with technology node shrink and linearly
    /// with temperature above a reference (25 C). Approximation:
    /// `P_leak = cell_count * base_leak * voltage * temp_factor`
    pub fn estimate_static_power(
        cell_count: u32,
        tech_node_nm: u32,
        voltage: f64,
        temperature_c: f64,
    ) -> f64 {
        // Base leakage per cell (nW) decreases with larger nodes.
        let base_leak_nw: f64 = match tech_node_nm {
            0..=5 => 50.0,
            6..=7 => 30.0,
            8..=14 => 15.0,
            15..=28 => 8.0,
            29..=65 => 4.0,
            _ => 2.0,
        };
        // Temperature factor: doubles roughly every 10 C above 25 C.
        let temp_factor = 2.0_f64.powf((temperature_c - 25.0) / 10.0);
        // Convert nW → uW.
        cell_count as f64 * base_leak_nw * voltage * temp_factor / 1000.0
    }

    /// Estimate dynamic (switching + internal) power in micro-watts.
    ///
    /// `P_dynamic = alpha * C_load * V^2 * f`
    ///
    /// - `toggle_rate` (alpha) — fraction of cells switching per cycle.
    /// - `clock_freq_mhz` — clock frequency in MHz.
    /// - `load_cap_pf` — average load capacitance per cell in pF.
    pub fn estimate_dynamic_power(
        cell_count: u32,
        toggle_rate: f64,
        clock_freq_mhz: f64,
        voltage: f64,
        load_cap_pf: f64,
    ) -> f64 {
        // P = alpha * C * V^2 * f  (per cell, summed)
        // Units: pF * V^2 * MHz = pF * V^2 * 1e6 Hz = 1e-12 * 1e6 * W = 1e-6 W = uW
        cell_count as f64 * toggle_rate * load_cap_pf * voltage * voltage * clock_freq_mhz
    }
}

pub mod ir_drop {
    //! IR drop analysis on a resistive power grid.

    use serde::{Deserialize, Serialize};

    /// Resistive grid with computed node voltages.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IrDropGrid {
        pub rows: usize,
        pub cols: usize,
        pub node_voltages: Vec<Vec<f64>>,
        pub supply_voltage: f64,
    }

    /// IR drop analysis result.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IrDropResult {
        pub max_drop_mv: f64,
        pub avg_drop_mv: f64,
        pub worst_node: (usize, usize),
        pub hotspots: Vec<(usize, usize, f64)>,
    }

    /// Analyze IR drop on a uniform resistive grid.
    ///
    /// Models a simplified power grid where supply pads are at the perimeter
    /// and current is drawn at each internal node. Uses iterative relaxation
    /// (Gauss-Seidel) to solve the resistive network.
    ///
    /// - `supply_v` — nominal supply voltage (V).
    /// - `current_map` — current draw at each grid node (A).
    /// - `resistance_per_unit` — resistance between adjacent grid nodes (ohms).
    pub fn analyze_ir_drop(
        grid_rows: usize,
        grid_cols: usize,
        supply_v: f64,
        current_map: &[Vec<f64>],
        resistance_per_unit: f64,
    ) -> IrDropResult {
        // Initialize all nodes to supply voltage.
        let mut v = vec![vec![supply_v; grid_cols]; grid_rows];

        // Perimeter nodes are ideal supply pads (fixed at supply_v).
        let is_pad = |r: usize, c: usize| -> bool {
            r == 0 || r == grid_rows - 1 || c == 0 || c == grid_cols - 1
        };

        // Gauss-Seidel iteration.
        let max_iter = 500;
        for _ in 0..max_iter {
            let mut max_delta: f64 = 0.0;
            for r in 0..grid_rows {
                for c in 0..grid_cols {
                    if is_pad(r, c) {
                        continue;
                    }
                    // Average of neighbors minus IR drop from local current.
                    let mut sum = 0.0;
                    let mut count = 0u32;
                    if r > 0 {
                        sum += v[r - 1][c];
                        count += 1;
                    }
                    if r + 1 < grid_rows {
                        sum += v[r + 1][c];
                        count += 1;
                    }
                    if c > 0 {
                        sum += v[r][c - 1];
                        count += 1;
                    }
                    if c + 1 < grid_cols {
                        sum += v[r][c + 1];
                        count += 1;
                    }
                    let current =
                        current_map.get(r).and_then(|row| row.get(c)).copied().unwrap_or(0.0);
                    let new_v = sum / count as f64 - current * resistance_per_unit / count as f64;
                    let delta = (new_v - v[r][c]).abs();
                    if delta > max_delta {
                        max_delta = delta;
                    }
                    v[r][c] = new_v;
                }
            }
            if max_delta < 1e-9 {
                break;
            }
        }

        // Compute drops.
        let mut max_drop_mv: f64 = 0.0;
        let mut total_drop = 0.0;
        let mut count = 0u64;
        let mut worst = (0, 0);
        let mut hotspots: Vec<(usize, usize, f64)> = Vec::new();
        let hotspot_threshold_mv = (supply_v * 0.05) * 1000.0; // 5% of supply

        for r in 0..grid_rows {
            for c in 0..grid_cols {
                let drop_mv = (supply_v - v[r][c]) * 1000.0;
                total_drop += drop_mv;
                count += 1;
                if drop_mv > max_drop_mv {
                    max_drop_mv = drop_mv;
                    worst = (r, c);
                }
                if drop_mv > hotspot_threshold_mv {
                    hotspots.push((r, c, drop_mv));
                }
            }
        }

        IrDropResult {
            max_drop_mv,
            avg_drop_mv: if count > 0 { total_drop / count as f64 } else { 0.0 },
            worst_node: worst,
            hotspots,
        }
    }
}

pub mod em {
    //! Electromigration (EM) rule checking.

    use serde::{Deserialize, Serialize};

    /// EM design rule for a metal layer.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EmRule {
        pub layer: String,
        pub max_avg_current_ma_per_um: f64,
        pub max_peak_current_ma_per_um: f64,
        pub max_rms_current_ma_per_um: f64,
    }

    /// Information about a wire segment.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WireInfo {
        pub net_name: String,
        pub layer: String,
        pub width_um: f64,
        pub length_um: f64,
        pub avg_current_ma: f64,
    }

    /// An EM rule violation.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EmViolation {
        pub net_name: String,
        pub layer: String,
        pub wire_width_um: f64,
        pub current_ma: f64,
        pub limit_ma: f64,
        pub margin: f64,
    }

    /// Check EM rules against wire segments.
    ///
    /// For each wire, the average current density (mA/um) is compared against
    /// the layer's `max_avg_current_ma_per_um` limit. Violations are returned
    /// with the computed margin (negative = violation).
    pub fn check_em(wires: &[WireInfo], rules: &[EmRule]) -> Vec<EmViolation> {
        let mut violations = Vec::new();
        for w in wires {
            if let Some(rule) = rules.iter().find(|r| r.layer == w.layer) {
                let limit_ma = rule.max_avg_current_ma_per_um * w.width_um;
                if w.avg_current_ma > limit_ma {
                    let margin = (limit_ma - w.avg_current_ma) / limit_ma;
                    violations.push(EmViolation {
                        net_name: w.net_name.clone(),
                        layer: w.layer.clone(),
                        wire_width_um: w.width_um,
                        current_ma: w.avg_current_ma,
                        limit_ma,
                        margin,
                    });
                }
            }
        }
        violations
    }
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upf_parse_basic_domains() {
        let input = r#"
# Power intent
create_power_domain PD_TOP -supply VDD -ground VSS -elements core,mem
create_power_domain PD_IO -supply VDDIO -ground VSS
create_power_switch SW1 -domain PD_TOP -control pwr_en -ack pwr_ack
set_isolation ISO1 -domain PD_TOP -location from -clamp zero
set_retention RET1 -domain PD_TOP -save save_sig -restore restore_sig
set_level_shifter LS1 -from PD_TOP -to PD_IO -direction low_to_high
"#;
        let design = upf::parse_upf(input);
        assert_eq!(design.domains.len(), 2);
        assert_eq!(design.domains[0].name, "PD_TOP");
        assert_eq!(design.domains[0].supply_net, "VDD");
        assert_eq!(design.domains[0].elements, vec!["core", "mem"]);
        assert_eq!(design.switches.len(), 1);
        assert_eq!(design.isolations.len(), 1);
        assert_eq!(design.isolations[0].clamp_value, upf::ClampValue::Zero);
        assert_eq!(design.retentions.len(), 1);
        assert_eq!(design.level_shifters.len(), 1);
        assert_eq!(
            design.level_shifters[0].direction,
            upf::ShifterDirection::LowToHigh
        );
        // Validation should pass — all refs are valid.
        assert!(design.validate().is_empty());
    }

    #[test]
    fn upf_validate_unknown_domain() {
        let input = "create_power_switch SW1 -domain MISSING -control en -ack ack";
        let design = upf::parse_upf(input);
        let errors = design.validate();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("MISSING"));
    }

    #[test]
    fn dynamic_power_formula() {
        // P = alpha * C * V^2 * f (per cell, summed)
        // 1000 cells, alpha=0.1, C=0.01 pF, V=1.0, f=1000 MHz
        // P = 1000 * 0.1 * 0.01 * 1.0 * 1000 = 1000 uW = 1 mW
        let p = estimation::estimate_dynamic_power(1000, 0.1, 1000.0, 1.0, 0.01);
        assert!((p - 1000.0).abs() < 0.01);
    }

    #[test]
    fn ir_drop_uniform_grid() {
        // 5x5 grid, uniform current draw at center.
        let rows = 5;
        let cols = 5;
        let supply = 1.0;
        let r = 0.1; // ohms
        let mut current_map = vec![vec![0.0; cols]; rows];
        current_map[2][2] = 0.01; // 10 mA at center

        let result = ir_drop::analyze_ir_drop(rows, cols, supply, &current_map, r);
        // Center should have the highest drop.
        assert_eq!(result.worst_node, (2, 2));
        assert!(result.max_drop_mv > 0.0);
        assert!(result.avg_drop_mv >= 0.0);
    }

    #[test]
    fn em_violation_detection() {
        let rules = vec![em::EmRule {
            layer: "M1".into(),
            max_avg_current_ma_per_um: 1.0,
            max_peak_current_ma_per_um: 3.0,
            max_rms_current_ma_per_um: 2.0,
        }];
        let wires = vec![
            em::WireInfo {
                net_name: "net_ok".into(),
                layer: "M1".into(),
                width_um: 2.0,
                length_um: 100.0,
                avg_current_ma: 1.5, // limit = 1.0 * 2.0 = 2.0 → OK
            },
            em::WireInfo {
                net_name: "net_bad".into(),
                layer: "M1".into(),
                width_um: 0.5,
                length_um: 50.0,
                avg_current_ma: 1.0, // limit = 1.0 * 0.5 = 0.5 → violation
            },
        ];
        let violations = em::check_em(&wires, &rules);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].net_name, "net_bad");
        assert!(violations[0].margin < 0.0);
        assert!((violations[0].limit_ma - 0.5).abs() < 0.001);
    }

    #[test]
    fn power_breakdown_total() {
        let b = estimation::PowerBreakdown {
            leakage_uw: 100.0,
            internal_uw: 200.0,
            switching_uw: 300.0,
            total_uw: 600.0,
        };
        assert!((b.total_uw - (b.leakage_uw + b.internal_uw + b.switching_uw)).abs() < 0.01);
    }

    #[test]
    fn static_power_scales_with_temperature() {
        let p25 = estimation::estimate_static_power(1000, 7, 0.8, 25.0);
        let p85 = estimation::estimate_static_power(1000, 7, 0.8, 85.0);
        // At 85 C power should be significantly higher than at 25 C.
        assert!(p85 > p25 * 3.0);
    }
}
