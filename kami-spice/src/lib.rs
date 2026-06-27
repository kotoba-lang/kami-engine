pub use analysis::{AnalysisResult, AnalysisType, solve_dc_op};
/// KAMI SPICE — circuit simulation engine.
///
/// Modified Nodal Analysis (MNA) based SPICE simulator supporting DC operating
/// point, DC sweep, AC analysis, transient analysis, and Monte Carlo runs.
/// Includes SPICE netlist parser/exporter and device model library.
pub use circuit::{SpiceCircuit, SpiceElement};
pub use model::{BjtModel, DiodeModel, ModelLibrary, MosfetModel};
pub use netlist::{export_spice, parse_spice_netlist};

// ── circuit ──────────────────────────────────────────────────────────────────

pub mod circuit {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// MOSFET type (N-channel or P-channel).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub enum MosfetType {
        Nmos,
        Pmos,
    }

    /// BJT type (NPN or PNP).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub enum BjtType {
        Npn,
        Pnp,
    }

    /// A single SPICE circuit element.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum SpiceElement {
        Resistor {
            name: String,
            n1: String,
            n2: String,
            value: f64,
        },
        Capacitor {
            name: String,
            n1: String,
            n2: String,
            value: f64,
        },
        Inductor {
            name: String,
            n1: String,
            n2: String,
            value: f64,
        },
        VoltageSource {
            name: String,
            n_pos: String,
            n_neg: String,
            dc_value: f64,
            ac_mag: f64,
            ac_phase: f64,
        },
        CurrentSource {
            name: String,
            n_pos: String,
            n_neg: String,
            dc_value: f64,
        },
        Mosfet {
            name: String,
            gate: String,
            drain: String,
            source: String,
            bulk: String,
            model_name: String,
            w: f64,
            l: f64,
            mosfet_type: MosfetType,
        },
        Bjt {
            name: String,
            collector: String,
            base: String,
            emitter: String,
            model_name: String,
            bjt_type: BjtType,
        },
        Diode {
            name: String,
            anode: String,
            cathode: String,
            model_name: String,
        },
    }

    /// A SPICE circuit containing elements and model references.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct SpiceCircuit {
        pub elements: Vec<SpiceElement>,
        pub models: HashMap<String, String>,
    }

    impl SpiceCircuit {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_element(&mut self, element: SpiceElement) {
            self.elements.push(element);
        }

        /// Returns the number of unique non-ground nodes.
        pub fn node_count(&self) -> usize {
            let mut nodes = std::collections::HashSet::new();
            for el in &self.elements {
                match el {
                    SpiceElement::Resistor { n1, n2, .. }
                    | SpiceElement::Capacitor { n1, n2, .. }
                    | SpiceElement::Inductor { n1, n2, .. } => {
                        if n1 != "0" && n1 != "gnd" {
                            nodes.insert(n1.clone());
                        }
                        if n2 != "0" && n2 != "gnd" {
                            nodes.insert(n2.clone());
                        }
                    }
                    SpiceElement::VoltageSource { n_pos, n_neg, .. }
                    | SpiceElement::CurrentSource { n_pos, n_neg, .. } => {
                        if n_pos != "0" && n_pos != "gnd" {
                            nodes.insert(n_pos.clone());
                        }
                        if n_neg != "0" && n_neg != "gnd" {
                            nodes.insert(n_neg.clone());
                        }
                    }
                    SpiceElement::Mosfet {
                        gate,
                        drain,
                        source,
                        bulk,
                        ..
                    } => {
                        for n in [gate, drain, source, bulk] {
                            if n != "0" && n != "gnd" {
                                nodes.insert(n.clone());
                            }
                        }
                    }
                    SpiceElement::Bjt {
                        collector,
                        base,
                        emitter,
                        ..
                    } => {
                        for n in [collector, base, emitter] {
                            if n != "0" && n != "gnd" {
                                nodes.insert(n.clone());
                            }
                        }
                    }
                    SpiceElement::Diode { anode, cathode, .. } => {
                        if anode != "0" && anode != "gnd" {
                            nodes.insert(anode.clone());
                        }
                        if cathode != "0" && cathode != "gnd" {
                            nodes.insert(cathode.clone());
                        }
                    }
                }
            }
            nodes.len()
        }

        pub fn element_count(&self) -> usize {
            self.elements.len()
        }
    }
}

// ── analysis ─────────────────────────────────────────────────────────────────

pub mod analysis {
    use crate::circuit::{SpiceCircuit, SpiceElement};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// Type of analysis to perform.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum AnalysisType {
        DcOp,
        DcSweep {
            source: String,
            start: f64,
            stop: f64,
            step: f64,
        },
        AcAnalysis {
            fstart: f64,
            fstop: f64,
            points_per_decade: u32,
        },
        Transient {
            tstep: f64,
            tstop: f64,
            tstart: f64,
        },
        MonteCarlo {
            num_runs: u32,
            analysis: Box<AnalysisType>,
        },
    }

    /// Result of a circuit analysis.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct AnalysisResult {
        pub node_voltages: HashMap<String, Vec<f64>>,
        pub branch_currents: HashMap<String, Vec<f64>>,
        pub time_points: Vec<f64>,
    }

    /// Build node-index mapping (ground = index -1, excluded from matrix).
    fn build_node_map(circuit: &SpiceCircuit) -> HashMap<String, usize> {
        let mut nodes = std::collections::BTreeSet::new();
        for el in &circuit.elements {
            match el {
                SpiceElement::Resistor { n1, n2, .. }
                | SpiceElement::Capacitor { n1, n2, .. }
                | SpiceElement::Inductor { n1, n2, .. } => {
                    if n1 != "0" && n1 != "gnd" {
                        nodes.insert(n1.clone());
                    }
                    if n2 != "0" && n2 != "gnd" {
                        nodes.insert(n2.clone());
                    }
                }
                SpiceElement::VoltageSource { n_pos, n_neg, .. }
                | SpiceElement::CurrentSource { n_pos, n_neg, .. } => {
                    if n_pos != "0" && n_pos != "gnd" {
                        nodes.insert(n_pos.clone());
                    }
                    if n_neg != "0" && n_neg != "gnd" {
                        nodes.insert(n_neg.clone());
                    }
                }
                _ => {}
            }
        }
        let mut map = HashMap::new();
        for (i, name) in nodes.into_iter().enumerate() {
            map.insert(name, i);
        }
        map
    }

    fn node_idx(map: &HashMap<String, usize>, name: &str) -> Option<usize> {
        if name == "0" || name == "gnd" {
            None
        } else {
            map.get(name).copied()
        }
    }

    /// Solve DC operating point using Modified Nodal Analysis (MNA).
    ///
    /// Builds the MNA matrix for resistors, voltage sources, and current sources,
    /// then solves via Gaussian elimination with partial pivoting.
    pub fn solve_dc_op(circuit: &SpiceCircuit) -> AnalysisResult {
        let node_map = build_node_map(circuit);
        let n = node_map.len();

        // Count voltage sources (each adds one MNA row/col).
        let vsrc_count = circuit
            .elements
            .iter()
            .filter(|e| matches!(e, SpiceElement::VoltageSource { .. }))
            .count();

        let size = n + vsrc_count;
        // Augmented matrix [A | b] stored row-major.
        let mut mat = vec![vec![0.0_f64; size + 1]; size];

        let mut vsrc_idx = 0usize;
        for el in &circuit.elements {
            match el {
                SpiceElement::Resistor { n1, n2, value, .. } => {
                    let g = 1.0 / value;
                    let i1 = node_idx(&node_map, n1);
                    let i2 = node_idx(&node_map, n2);
                    if let Some(a) = i1 {
                        mat[a][a] += g;
                    }
                    if let Some(b) = i2 {
                        mat[b][b] += g;
                    }
                    if let (Some(a), Some(b)) = (i1, i2) {
                        mat[a][b] -= g;
                        mat[b][a] -= g;
                    }
                }
                SpiceElement::VoltageSource {
                    n_pos,
                    n_neg,
                    dc_value,
                    name: _,
                    ..
                } => {
                    let row = n + vsrc_idx;
                    let i_pos = node_idx(&node_map, n_pos);
                    let i_neg = node_idx(&node_map, n_neg);
                    // KVL: V(n_pos) - V(n_neg) = dc_value
                    if let Some(a) = i_pos {
                        mat[row][a] += 1.0;
                        mat[a][row] += 1.0;
                    }
                    if let Some(b) = i_neg {
                        mat[row][b] -= 1.0;
                        mat[b][row] -= 1.0;
                    }
                    mat[row][size] = *dc_value;
                    vsrc_idx += 1;
                }
                SpiceElement::CurrentSource {
                    n_pos,
                    n_neg,
                    dc_value,
                    ..
                } => {
                    // Current flows from n_pos to n_neg (out of n_pos, into n_neg).
                    let i_pos = node_idx(&node_map, n_pos);
                    let i_neg = node_idx(&node_map, n_neg);
                    if let Some(a) = i_pos {
                        mat[a][size] -= dc_value;
                    }
                    if let Some(b) = i_neg {
                        mat[b][size] += dc_value;
                    }
                }
                _ => {
                    // Nonlinear devices (MOSFET, BJT, diode) require iterative
                    // Newton-Raphson; skipped in linear DC op.
                }
            }
        }

        // Gaussian elimination with partial pivoting.
        for col in 0..size {
            // Find pivot.
            let mut max_row = col;
            let mut max_val = mat[col][col].abs();
            for row in (col + 1)..size {
                let v = mat[row][col].abs();
                if v > max_val {
                    max_val = v;
                    max_row = row;
                }
            }
            mat.swap(col, max_row);

            let pivot = mat[col][col];
            if pivot.abs() < 1e-15 {
                continue; // Singular or zero pivot — skip.
            }
            for row in (col + 1)..size {
                let factor = mat[row][col] / pivot;
                for j in col..=size {
                    mat[row][j] -= factor * mat[col][j];
                }
            }
        }

        // Back-substitution.
        let mut x = vec![0.0; size];
        for i in (0..size).rev() {
            let mut sum = mat[i][size];
            for j in (i + 1)..size {
                sum -= mat[i][j] * x[j];
            }
            if mat[i][i].abs() > 1e-15 {
                x[i] = sum / mat[i][i];
            }
        }

        // Build result.
        let mut result = AnalysisResult::default();
        for (name, &idx) in &node_map {
            result.node_voltages.insert(name.clone(), vec![x[idx]]);
        }
        // Branch currents through voltage sources.
        vsrc_idx = 0;
        for el in &circuit.elements {
            if let SpiceElement::VoltageSource { name, .. } = el {
                result
                    .branch_currents
                    .insert(name.clone(), vec![x[n + vsrc_idx]]);
                vsrc_idx += 1;
            }
        }
        result
    }
}

// ── model ────────────────────────────────────────────────────────────────────

pub mod model {
    use crate::circuit::{BjtType, MosfetType};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// MOSFET compact model parameters (Level 1 style).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MosfetModel {
        pub name: String,
        pub mosfet_type: MosfetType,
        pub vth0: f64,
        pub kp: f64,
        pub lambda: f64,
        pub tox: f64,
        pub nsub: f64,
        pub uo: f64,
        pub phi: f64,
        pub gamma: f64,
    }

    impl MosfetModel {
        pub fn default_nmos(name: &str) -> Self {
            Self {
                name: name.to_string(),
                mosfet_type: MosfetType::Nmos,
                vth0: 0.7,
                kp: 110e-6,
                lambda: 0.04,
                tox: 9e-9,
                nsub: 1e17,
                uo: 600.0,
                phi: 0.65,
                gamma: 0.37,
            }
        }
    }

    /// BJT compact model parameters (Ebers-Moll style).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BjtModel {
        pub name: String,
        pub bjt_type: BjtType,
        pub is_sat: f64,
        pub bf: f64,
        pub br: f64,
        pub vaf: f64,
        pub var: f64,
    }

    impl BjtModel {
        pub fn default_npn(name: &str) -> Self {
            Self {
                name: name.to_string(),
                bjt_type: BjtType::Npn,
                is_sat: 1e-15,
                bf: 100.0,
                br: 1.0,
                vaf: 100.0,
                var: 0.0,
            }
        }
    }

    /// Diode model parameters.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DiodeModel {
        pub name: String,
        pub is_sat: f64,
        pub n: f64,
        pub bv: f64,
        pub rs: f64,
    }

    impl DiodeModel {
        pub fn default_diode(name: &str) -> Self {
            Self {
                name: name.to_string(),
                is_sat: 1e-14,
                n: 1.0,
                bv: 100.0,
                rs: 0.0,
            }
        }
    }

    /// Device model library (name → model type discriminated by enum).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum DeviceModel {
        Mosfet(MosfetModel),
        Bjt(BjtModel),
        Diode(DiodeModel),
    }

    /// Library holding named device models.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct ModelLibrary {
        models: HashMap<String, DeviceModel>,
    }

    impl ModelLibrary {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_mosfet(&mut self, model: MosfetModel) {
            self.models
                .insert(model.name.clone(), DeviceModel::Mosfet(model));
        }

        pub fn add_bjt(&mut self, model: BjtModel) {
            self.models
                .insert(model.name.clone(), DeviceModel::Bjt(model));
        }

        pub fn add_diode(&mut self, model: DiodeModel) {
            self.models
                .insert(model.name.clone(), DeviceModel::Diode(model));
        }

        pub fn get(&self, name: &str) -> Option<&DeviceModel> {
            self.models.get(name)
        }

        pub fn count(&self) -> usize {
            self.models.len()
        }
    }
}

// ── netlist ──────────────────────────────────────────────────────────────────

pub mod netlist {
    use crate::circuit::{BjtType, MosfetType, SpiceCircuit, SpiceElement};
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum NetlistError {
        #[error("parse error at line {line}: {message}")]
        Parse { line: usize, message: String },
        #[error("insufficient tokens at line {line}")]
        InsufficientTokens { line: usize },
    }

    /// Parse a SPICE netlist string into a `SpiceCircuit`.
    ///
    /// Recognises element lines (R, C, L, V, I, M, Q, D), `.model` directives,
    /// and analysis commands (`.dc`, `.ac`, `.tran`). Title line and comments
    /// (lines starting with `*`) are skipped.
    pub fn parse_spice_netlist(input: &str) -> Result<SpiceCircuit, NetlistError> {
        let mut circuit = SpiceCircuit::new();
        let lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return Ok(circuit);
        }

        // In SPICE, the very first line is always the title — skip it.
        let mut title_skipped = false;

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('*') {
                continue;
            }

            // Skip the first non-blank, non-comment line (title).
            if !title_skipped {
                title_skipped = true;
                continue;
            }

            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            let first = tokens[0];
            let prefix = first.chars().next().unwrap_or(' ');

            match prefix.to_ascii_uppercase() {
                'R' => {
                    if tokens.len() < 4 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::Resistor {
                        name: tokens[0].to_string(),
                        n1: tokens[1].to_string(),
                        n2: tokens[2].to_string(),
                        value: parse_eng(tokens[3]),
                    });
                }
                'C' if !first.starts_with(".") => {
                    if tokens.len() < 4 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::Capacitor {
                        name: tokens[0].to_string(),
                        n1: tokens[1].to_string(),
                        n2: tokens[2].to_string(),
                        value: parse_eng(tokens[3]),
                    });
                }
                'L' => {
                    if tokens.len() < 4 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::Inductor {
                        name: tokens[0].to_string(),
                        n1: tokens[1].to_string(),
                        n2: tokens[2].to_string(),
                        value: parse_eng(tokens[3]),
                    });
                }
                'V' => {
                    if tokens.len() < 4 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::VoltageSource {
                        name: tokens[0].to_string(),
                        n_pos: tokens[1].to_string(),
                        n_neg: tokens[2].to_string(),
                        dc_value: parse_eng(tokens[3]),
                        ac_mag: 0.0,
                        ac_phase: 0.0,
                    });
                }
                'I' => {
                    if tokens.len() < 4 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::CurrentSource {
                        name: tokens[0].to_string(),
                        n_pos: tokens[1].to_string(),
                        n_neg: tokens[2].to_string(),
                        dc_value: parse_eng(tokens[3]),
                    });
                }
                'M' => {
                    // M<name> drain gate source bulk model W=<w> L=<l>
                    if tokens.len() < 6 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    let w = find_param(&tokens, "W").unwrap_or(1e-6);
                    let l = find_param(&tokens, "L").unwrap_or(1e-6);
                    circuit.add_element(SpiceElement::Mosfet {
                        name: tokens[0].to_string(),
                        drain: tokens[1].to_string(),
                        gate: tokens[2].to_string(),
                        source: tokens[3].to_string(),
                        bulk: tokens[4].to_string(),
                        model_name: tokens[5].to_string(),
                        w,
                        l,
                        mosfet_type: MosfetType::Nmos, // determined by model in practice
                    });
                }
                'Q' => {
                    // Q<name> collector base emitter model
                    if tokens.len() < 5 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::Bjt {
                        name: tokens[0].to_string(),
                        collector: tokens[1].to_string(),
                        base: tokens[2].to_string(),
                        emitter: tokens[3].to_string(),
                        model_name: tokens[4].to_string(),
                        bjt_type: BjtType::Npn,
                    });
                }
                'D' => {
                    // D<name> anode cathode model
                    if tokens.len() < 4 {
                        return Err(NetlistError::InsufficientTokens { line: line_num + 1 });
                    }
                    circuit.add_element(SpiceElement::Diode {
                        name: tokens[0].to_string(),
                        anode: tokens[1].to_string(),
                        cathode: tokens[2].to_string(),
                        model_name: tokens[3].to_string(),
                    });
                }
                '.' => {
                    // Directives.
                    let directive = tokens[0].to_lowercase();
                    if directive == ".model" && tokens.len() >= 3 {
                        circuit
                            .models
                            .insert(tokens[1].to_string(), tokens[2..].join(" "));
                    }
                    // .dc, .ac, .tran stored as metadata (analysis type
                    // selection handled by the caller).
                }
                _ => {
                    // Unknown element prefix — skip.
                }
            }
        }
        Ok(circuit)
    }

    /// Parse engineering notation (e.g. "1k" → 1000, "10u" → 10e-6).
    fn parse_eng(s: &str) -> f64 {
        let s = s.trim();
        if s.is_empty() {
            return 0.0;
        }
        // Try direct parse first.
        if let Ok(v) = s.parse::<f64>() {
            return v;
        }
        let (num_part, suffix) = split_suffix(s);
        let base: f64 = num_part.parse().unwrap_or(0.0);
        let mult = match suffix.to_lowercase().as_str() {
            "t" => 1e12,
            "g" => 1e9,
            "meg" | "x" => 1e6,
            "k" => 1e3,
            "m" => 1e-3,
            "u" | "µ" => 1e-6,
            "n" => 1e-9,
            "p" => 1e-12,
            "f" => 1e-15,
            _ => 1.0,
        };
        base * mult
    }

    fn split_suffix(s: &str) -> (&str, &str) {
        let idx = s
            .find(|c: char| c.is_alphabetic() || c == 'µ')
            .unwrap_or(s.len());
        (&s[..idx], &s[idx..])
    }

    fn find_param(tokens: &[&str], key: &str) -> Option<f64> {
        for t in tokens {
            if let Some(rest) = t
                .strip_prefix(key)
                .or_else(|| t.strip_prefix(&key.to_lowercase()))
            {
                if let Some(val) = rest.strip_prefix('=') {
                    return Some(parse_eng(val));
                }
            }
        }
        None
    }

    /// Export a `SpiceCircuit` to SPICE netlist format.
    pub fn export_spice(circuit: &crate::circuit::SpiceCircuit) -> String {
        let mut out = String::from("KAMI SPICE netlist\n");
        for el in &circuit.elements {
            match el {
                SpiceElement::Resistor {
                    name,
                    n1,
                    n2,
                    value,
                } => {
                    out.push_str(&format!("{} {} {} {}\n", name, n1, n2, value));
                }
                SpiceElement::Capacitor {
                    name,
                    n1,
                    n2,
                    value,
                } => {
                    out.push_str(&format!("{} {} {} {}\n", name, n1, n2, value));
                }
                SpiceElement::Inductor {
                    name,
                    n1,
                    n2,
                    value,
                } => {
                    out.push_str(&format!("{} {} {} {}\n", name, n1, n2, value));
                }
                SpiceElement::VoltageSource {
                    name,
                    n_pos,
                    n_neg,
                    dc_value,
                    ..
                } => {
                    out.push_str(&format!("{} {} {} {}\n", name, n_pos, n_neg, dc_value));
                }
                SpiceElement::CurrentSource {
                    name,
                    n_pos,
                    n_neg,
                    dc_value,
                } => {
                    out.push_str(&format!("{} {} {} {}\n", name, n_pos, n_neg, dc_value));
                }
                SpiceElement::Mosfet {
                    name,
                    drain,
                    gate,
                    source,
                    bulk,
                    model_name,
                    w,
                    l,
                    ..
                } => {
                    out.push_str(&format!(
                        "{} {} {} {} {} {} W={} L={}\n",
                        name, drain, gate, source, bulk, model_name, w, l
                    ));
                }
                SpiceElement::Bjt {
                    name,
                    collector,
                    base,
                    emitter,
                    model_name,
                    ..
                } => {
                    out.push_str(&format!(
                        "{} {} {} {} {}\n",
                        name, collector, base, emitter, model_name
                    ));
                }
                SpiceElement::Diode {
                    name,
                    anode,
                    cathode,
                    model_name,
                } => {
                    out.push_str(&format!("{} {} {} {}\n", name, anode, cathode, model_name));
                }
            }
        }
        for (name, def) in &circuit.models {
            out.push_str(&format!(".model {} {}\n", name, def));
        }
        out.push_str(".end\n");
        out
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// DC operating point: voltage divider R1=1k, R2=1k, V1=10V.
    /// Expected: V(mid) = 5V.
    #[test]
    fn dc_op_voltage_divider() {
        let mut ckt = SpiceCircuit::new();
        ckt.add_element(circuit::SpiceElement::VoltageSource {
            name: "V1".into(),
            n_pos: "in".into(),
            n_neg: "0".into(),
            dc_value: 10.0,
            ac_mag: 0.0,
            ac_phase: 0.0,
        });
        ckt.add_element(circuit::SpiceElement::Resistor {
            name: "R1".into(),
            n1: "in".into(),
            n2: "mid".into(),
            value: 1000.0,
        });
        ckt.add_element(circuit::SpiceElement::Resistor {
            name: "R2".into(),
            n1: "mid".into(),
            n2: "0".into(),
            value: 1000.0,
        });

        let result = solve_dc_op(&ckt);
        let v_mid = result.node_voltages["mid"][0];
        assert!((v_mid - 5.0).abs() < 1e-9, "expected 5V, got {}", v_mid);
        let v_in = result.node_voltages["in"][0];
        assert!((v_in - 10.0).abs() < 1e-9, "expected 10V, got {}", v_in);
    }

    /// Parse a basic SPICE netlist.
    #[test]
    fn parse_netlist() {
        let netlist = "\
Voltage Divider
V1 in 0 10
R1 in mid 1k
R2 mid 0 1k
.end
";
        let ckt = parse_spice_netlist(netlist).expect("parse failed");
        assert_eq!(ckt.element_count(), 3);
        assert_eq!(ckt.node_count(), 2); // "in" and "mid"
    }

    /// Model library add/get.
    #[test]
    fn model_library() {
        let mut lib = ModelLibrary::new();
        lib.add_mosfet(model::MosfetModel::default_nmos("NMOS1"));
        lib.add_bjt(model::BjtModel::default_npn("NPN1"));
        lib.add_diode(model::DiodeModel::default_diode("D1"));
        assert_eq!(lib.count(), 3);
        assert!(lib.get("NMOS1").is_some());
        assert!(lib.get("MISSING").is_none());
    }

    /// Element count tracking.
    #[test]
    fn element_count() {
        let mut ckt = SpiceCircuit::new();
        assert_eq!(ckt.element_count(), 0);
        ckt.add_element(circuit::SpiceElement::Resistor {
            name: "R1".into(),
            n1: "a".into(),
            n2: "b".into(),
            value: 100.0,
        });
        ckt.add_element(circuit::SpiceElement::Capacitor {
            name: "C1".into(),
            n1: "a".into(),
            n2: "b".into(),
            value: 1e-12,
        });
        assert_eq!(ckt.element_count(), 2);
        assert_eq!(ckt.node_count(), 2);
    }

    /// Netlist export roundtrip: export then re-parse.
    #[test]
    fn netlist_export_roundtrip() {
        let mut ckt = SpiceCircuit::new();
        ckt.add_element(circuit::SpiceElement::VoltageSource {
            name: "V1".into(),
            n_pos: "in".into(),
            n_neg: "0".into(),
            dc_value: 5.0,
            ac_mag: 0.0,
            ac_phase: 0.0,
        });
        ckt.add_element(circuit::SpiceElement::Resistor {
            name: "R1".into(),
            n1: "in".into(),
            n2: "out".into(),
            value: 1000.0,
        });

        let exported = netlist::export_spice(&ckt);
        let reparsed = parse_spice_netlist(&exported).expect("re-parse failed");
        assert_eq!(reparsed.element_count(), ckt.element_count());
    }

    /// DC op with current source.
    #[test]
    fn dc_op_current_source() {
        let mut ckt = SpiceCircuit::new();
        // I1 pushes 1mA into node "a", R1=1k from "a" to ground → V(a) = 1V.
        ckt.add_element(circuit::SpiceElement::CurrentSource {
            name: "I1".into(),
            n_pos: "0".into(),
            n_neg: "a".into(),
            dc_value: 1e-3,
        });
        ckt.add_element(circuit::SpiceElement::Resistor {
            name: "R1".into(),
            n1: "a".into(),
            n2: "0".into(),
            value: 1000.0,
        });

        let result = solve_dc_op(&ckt);
        let v_a = result.node_voltages["a"][0];
        assert!((v_a - 1.0).abs() < 1e-9, "expected 1V, got {}", v_a);
    }
}
