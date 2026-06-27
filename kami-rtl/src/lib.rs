/// KAMI RTL — HDL editor, Verilog/VHDL parsing, event-driven simulation,
/// waveform viewer, and synthesis netlist.

/// HDL modeling: Verilog/VHDL AST, port/signal types, basic parsing.
pub mod hdl {
    use serde::{Deserialize, Serialize};

    /// Bit-range for port/signal width (e.g. `[7:0]` → `BitRange { msb: 7, lsb: 0 }`).
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct BitRange {
        pub msb: u32,
        pub lsb: u32,
    }

    impl BitRange {
        pub fn width(&self) -> u32 {
            self.msb - self.lsb + 1
        }

        pub fn single() -> Self {
            Self { msb: 0, lsb: 0 }
        }
    }

    /// Port direction.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum PortDirection {
        Input,
        Output,
        Inout,
    }

    /// Signal type (net vs variable).
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum SignalType {
        Wire,
        Reg,
        Logic,
    }

    /// A port declaration within a module.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct RtlPort {
        pub name: String,
        pub direction: PortDirection,
        pub width: BitRange,
        pub signal_type: SignalType,
    }

    /// A module instantiation inside another module.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RtlInstance {
        pub module_name: String,
        pub instance_name: String,
        pub port_connections: Vec<(String, String)>,
    }

    /// Clock edge specification.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum ClockEdge {
        Posedge,
        Negedge,
    }

    /// Reset specification for sequential blocks.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ResetSpec {
        pub signal: String,
        pub active_low: bool,
    }

    /// Always block type — combinational or sequential.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum AlwaysBlock {
        Combinational {
            sensitivity: Vec<String>,
            body: Vec<RtlStatement>,
        },
        Sequential {
            clock: String,
            clock_edge: ClockEdge,
            reset: Option<ResetSpec>,
            body: Vec<RtlStatement>,
        },
    }

    /// RTL statement inside procedural blocks.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum RtlStatement {
        Assign {
            target: String,
            value: RtlExpr,
        },
        If {
            condition: RtlExpr,
            then_body: Vec<RtlStatement>,
            else_body: Vec<RtlStatement>,
        },
        Case {
            selector: RtlExpr,
            arms: Vec<(RtlExpr, Vec<RtlStatement>)>,
            default: Option<Vec<RtlStatement>>,
        },
        ForLoop {
            var: String,
            start: RtlExpr,
            end: RtlExpr,
            step: RtlExpr,
            body: Vec<RtlStatement>,
        },
    }

    /// RTL expression tree.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum RtlExpr {
        /// Integer/binary literal.
        Literal(u64),
        /// Named signal reference.
        Signal(String),
        /// Binary operation.
        BinaryOp {
            op: BinaryOperator,
            lhs: Box<RtlExpr>,
            rhs: Box<RtlExpr>,
        },
        /// Unary operation.
        UnaryOp {
            op: UnaryOperator,
            operand: Box<RtlExpr>,
        },
        /// Concatenation `{a, b, c}`.
        Concat(Vec<RtlExpr>),
        /// Bit/part select `signal[msb:lsb]`.
        Select {
            signal: Box<RtlExpr>,
            range: BitRange,
        },
        /// Ternary `cond ? true_val : false_val`.
        Ternary {
            condition: Box<RtlExpr>,
            true_val: Box<RtlExpr>,
            false_val: Box<RtlExpr>,
        },
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub enum BinaryOperator {
        And,
        Or,
        Xor,
        Add,
        Sub,
        Mul,
        Shl,
        Shr,
        Eq,
        Neq,
        Lt,
        Gt,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub enum UnaryOperator {
        Not,
        Negate,
        ReductionAnd,
        ReductionOr,
    }

    /// Continuous assignment (`assign lhs = rhs;`).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContinuousAssign {
        pub target: String,
        pub value: RtlExpr,
    }

    /// Top-level RTL module representation.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RtlModule {
        pub name: String,
        pub ports: Vec<RtlPort>,
        pub parameters: Vec<(String, u64)>,
        pub instances: Vec<RtlInstance>,
        pub always_blocks: Vec<AlwaysBlock>,
        pub assigns: Vec<ContinuousAssign>,
    }

    /// Parse a basic Verilog module declaration, extracting module name and port
    /// list from `module <name>(<port1>, <port2>, ...); ... endmodule`.
    ///
    /// Returns `(module_name, port_names)`.  This is intentionally minimal — a
    /// full parser would use a proper grammar, but this handles the most common
    /// Verilog-2001 module header pattern.
    pub fn parse_verilog(src: &str) -> Result<(String, Vec<String>), String> {
        // Find "module" keyword.
        let module_start = src
            .find("module ")
            .ok_or_else(|| "missing 'module' keyword".to_string())?;
        let after_module = &src[module_start + 7..];

        // Extract module name — the next non-whitespace token before '(' or ';'.
        let name_end = after_module
            .find(|c: char| c == '(' || c == ';' || c.is_whitespace())
            .unwrap_or(after_module.len());
        let module_name = after_module[..name_end].trim().to_string();
        if module_name.is_empty() {
            return Err("empty module name".to_string());
        }

        // Extract port list between parentheses.
        let rest = &after_module[name_end..];
        let open_paren = rest.find('(');
        let close_paren = rest.find(')');
        let ports = match (open_paren, close_paren) {
            (Some(o), Some(c)) if o < c => {
                let port_str = &rest[o + 1..c];
                port_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        };

        // Verify endmodule exists.
        if !src.contains("endmodule") {
            return Err("missing 'endmodule'".to_string());
        }

        Ok((module_name, ports))
    }
}

/// Event-driven RTL simulator with delta-cycle support.
pub mod simulator {
    use serde::{Deserialize, Serialize};
    use std::collections::{BinaryHeap, HashMap};
    use std::fmt;

    /// Four-valued logic (IEEE 1164).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum LogicValue {
        Zero,
        One,
        X,
        Z,
    }

    impl fmt::Display for LogicValue {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                LogicValue::Zero => write!(f, "0"),
                LogicValue::One => write!(f, "1"),
                LogicValue::X => write!(f, "x"),
                LogicValue::Z => write!(f, "z"),
            }
        }
    }

    impl LogicValue {
        /// Bitwise AND for four-valued logic.
        pub fn and(self, other: Self) -> Self {
            match (self, other) {
                (LogicValue::Zero, _) | (_, LogicValue::Zero) => LogicValue::Zero,
                (LogicValue::One, LogicValue::One) => LogicValue::One,
                _ => LogicValue::X,
            }
        }

        /// Bitwise OR for four-valued logic.
        pub fn or(self, other: Self) -> Self {
            match (self, other) {
                (LogicValue::One, _) | (_, LogicValue::One) => LogicValue::One,
                (LogicValue::Zero, LogicValue::Zero) => LogicValue::Zero,
                _ => LogicValue::X,
            }
        }

        /// Bitwise NOT for four-valued logic.
        pub fn not(self) -> Self {
            match self {
                LogicValue::Zero => LogicValue::One,
                LogicValue::One => LogicValue::Zero,
                _ => LogicValue::X,
            }
        }
    }

    /// State of a multi-bit signal.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SignalState {
        pub width: u32,
        pub values: Vec<LogicValue>,
    }

    impl SignalState {
        pub fn new(width: u32) -> Self {
            Self {
                width,
                values: vec![LogicValue::X; width as usize],
            }
        }

        pub fn from_values(values: Vec<LogicValue>) -> Self {
            let width = values.len() as u32;
            Self { width, values }
        }
    }

    /// A scheduled simulation event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SimEvent {
        pub time: u64,
        pub signal: String,
        pub value: Vec<LogicValue>,
    }

    impl PartialEq for SimEvent {
        fn eq(&self, other: &Self) -> bool {
            self.time == other.time
        }
    }
    impl Eq for SimEvent {}

    impl PartialOrd for SimEvent {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for SimEvent {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // Reverse for min-heap behavior with BinaryHeap.
            other.time.cmp(&self.time)
        }
    }

    /// Record of a signal value change for history/waveform.
    #[derive(Debug, Clone)]
    pub struct SignalChange {
        pub time: u64,
        pub value: Vec<LogicValue>,
    }

    /// Event-driven RTL simulator.
    ///
    /// Processes events in time order. Within the same time step, iterates
    /// through all pending events (delta cycle) before advancing.
    pub struct RtlSimulator {
        pub time: u64,
        event_queue: BinaryHeap<SimEvent>,
        signals: HashMap<String, SignalState>,
        history: HashMap<String, Vec<SignalChange>>,
    }

    impl RtlSimulator {
        pub fn new() -> Self {
            Self {
                time: 0,
                event_queue: BinaryHeap::new(),
                signals: HashMap::new(),
                history: HashMap::new(),
            }
        }

        /// Register a signal with a given bit width.  Initialised to X.
        pub fn register_signal(&mut self, name: &str, width: u32) {
            self.signals
                .insert(name.to_string(), SignalState::new(width));
            self.history.insert(name.to_string(), Vec::new());
        }

        /// Set input value at the current simulation time.
        pub fn set_input(&mut self, signal: &str, value: Vec<LogicValue>) {
            self.schedule_event(self.time, signal, value);
        }

        /// Schedule an event for a future (or current) time.
        pub fn schedule_event(&mut self, time: u64, signal: &str, value: Vec<LogicValue>) {
            self.event_queue.push(SimEvent {
                time,
                signal: signal.to_string(),
                value,
            });
        }

        /// Run the simulator for `duration` time units, processing events in
        /// time order.  Within the same time step all pending events are
        /// processed (delta cycle).
        pub fn run(&mut self, duration: u64) {
            let end_time = self.time + duration;

            while let Some(event) = self.event_queue.peek() {
                if event.time > end_time {
                    break;
                }

                let event = self.event_queue.pop().unwrap();
                self.time = event.time;

                // Apply state change.
                if let Some(state) = self.signals.get_mut(&event.signal) {
                    let new_len = event.value.len().min(state.width as usize);
                    state.values[..new_len].copy_from_slice(&event.value[..new_len]);
                }

                // Record in history.
                if let Some(hist) = self.history.get_mut(&event.signal) {
                    hist.push(SignalChange {
                        time: event.time,
                        value: event.value,
                    });
                }
            }

            self.time = end_time;
        }

        /// Current value of a signal.
        pub fn get_signal(&self, name: &str) -> Option<&SignalState> {
            self.signals.get(name)
        }

        /// Full transition history for a signal.
        pub fn get_signal_history(&self, name: &str) -> Option<&Vec<SignalChange>> {
            self.history.get(name)
        }
    }
}

/// Waveform view and IEEE 1364 VCD export.
pub mod waveform {
    use serde::{Deserialize, Serialize};

    /// Display format for a waveform signal.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum DisplayFormat {
        Binary,
        Hex,
        Decimal,
        Analog,
    }

    /// Waveform signal with recorded transitions.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WaveformSignal {
        pub name: String,
        pub width: u32,
        /// `(time, value_string)` pairs.
        pub transitions: Vec<(u64, String)>,
        pub display_format: DisplayFormat,
        pub color: String,
    }

    /// Waveform viewer state.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WaveformView {
        pub signals: Vec<WaveformSignal>,
        pub time_range: (u64, u64),
        pub cursor_time: u64,
        pub zoom: f64,
    }

    impl WaveformView {
        pub fn new() -> Self {
            Self {
                signals: Vec::new(),
                time_range: (0, 0),
                cursor_time: 0,
                zoom: 1.0,
            }
        }

        /// Add a signal to the view and update the time range.
        pub fn add_signal(&mut self, signal: WaveformSignal) {
            if let Some(&(t, _)) = signal.transitions.last() {
                if t > self.time_range.1 {
                    self.time_range.1 = t;
                }
            }
            self.signals.push(signal);
        }
    }

    /// Export waveform signals to IEEE 1364 Value Change Dump (VCD) format.
    ///
    /// Produces a valid VCD file with:
    /// - `$date` / `$version` / `$timescale` header
    /// - `$scope` / `$var` / `$upscope` variable definitions
    /// - `$dumpvars` initial values
    /// - `#<time>` + value-change lines
    pub fn export_vcd(signals: &[WaveformSignal]) -> String {
        let mut out = String::new();

        // --- Header ---
        out.push_str("$date\n  Generated by KAMI RTL\n$end\n");
        out.push_str("$version\n  KAMI RTL 0.1.0\n$end\n");
        out.push_str("$timescale\n  1ns\n$end\n");

        // --- Variable definitions ---
        out.push_str("$scope module top $end\n");
        for (i, sig) in signals.iter().enumerate() {
            let id_char = vcd_id(i);
            out.push_str(&format!(
                "$var wire {} {} {} $end\n",
                sig.width, id_char, sig.name
            ));
        }
        out.push_str("$upscope $end\n");
        out.push_str("$enddefinitions $end\n");

        // --- Initial values ---
        out.push_str("$dumpvars\n");
        for (i, sig) in signals.iter().enumerate() {
            let id_char = vcd_id(i);
            if let Some((_, val)) = sig.transitions.first() {
                format_vcd_value(&mut out, sig.width, val, &id_char);
            } else {
                // Default X.
                if sig.width == 1 {
                    out.push_str(&format!("x{}\n", id_char));
                } else {
                    out.push_str(&format!("bx {}\n", id_char));
                }
            }
        }
        out.push_str("$end\n");

        // --- Collect and sort all transitions by time ---
        let mut all_changes: Vec<(u64, usize, &str)> = Vec::new();
        for (i, sig) in signals.iter().enumerate() {
            for (time, val) in &sig.transitions {
                all_changes.push((*time, i, val));
            }
        }
        all_changes.sort_by_key(|&(t, idx, _)| (t, idx));

        // --- Value changes ---
        let mut current_time: Option<u64> = None;
        for (time, idx, val) in &all_changes {
            if current_time != Some(*time) {
                out.push_str(&format!("#{}\n", time));
                current_time = Some(*time);
            }
            let id_char = vcd_id(*idx);
            format_vcd_value(&mut out, signals[*idx].width, val, &id_char);
        }

        out
    }

    /// Generate a VCD identifier from index (!, ", #, …).
    fn vcd_id(index: usize) -> String {
        // VCD identifiers are printable ASCII 33–126.
        let c = (33 + (index % 94)) as u8 as char;
        c.to_string()
    }

    /// Write a single VCD value change line.
    fn format_vcd_value(out: &mut String, width: u32, val: &str, id: &str) {
        if width == 1 {
            // Scalar: `<value><id>`.
            let v = val.chars().next().unwrap_or('x');
            out.push_str(&format!("{}{}\n", v, id));
        } else {
            // Vector: `b<bits> <id>`.
            out.push_str(&format!("b{} {}\n", val, id));
        }
    }
}

/// Gate-level synthesis: netlist, gate types, statistics, optimisation.
pub mod synthesis {
    use serde::{Deserialize, Serialize};

    /// Fundamental gate types for technology mapping.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum GateType {
        And,
        Or,
        Not,
        Nand,
        Nor,
        Xor,
        Xnor,
        Buf,
        Mux,
        Dff,
    }

    /// A gate with typed inputs and a single output net.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Gate {
        pub gate_type: GateType,
        pub inputs: Vec<String>,
        pub output: String,
    }

    /// Gate-level netlist — collection of gates plus primary I/O.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GateNetlist {
        pub gates: Vec<Gate>,
        pub primary_inputs: Vec<String>,
        pub primary_outputs: Vec<String>,
    }

    /// Summary statistics for a netlist.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GateStats {
        pub gate_count: usize,
        pub lut_count: usize,
        pub ff_count: usize,
        pub estimated_max_freq_mhz: f64,
    }

    impl GateNetlist {
        pub fn new() -> Self {
            Self {
                gates: Vec::new(),
                primary_inputs: Vec::new(),
                primary_outputs: Vec::new(),
            }
        }

        /// Compute summary statistics.
        ///
        /// - `gate_count`: total gates (excluding DFF).
        /// - `lut_count`: combinational gates (rough LUT estimate, 1 gate ≈ 1 LUT).
        /// - `ff_count`: number of DFF gates.
        /// - `estimated_max_freq_mhz`: naive estimate based on combinational depth
        ///   (assumes 1 ns gate delay, longest chain = critical path).
        pub fn stats(&self) -> GateStats {
            let ff_count = self
                .gates
                .iter()
                .filter(|g| g.gate_type == GateType::Dff)
                .count();
            let comb_count = self.gates.len() - ff_count;

            // Rough critical-path estimate: count max fan-in chain depth.
            let depth = self.estimate_depth();
            let delay_ns = depth.max(1) as f64; // 1 ns per gate
            let max_freq = 1000.0 / delay_ns; // MHz

            GateStats {
                gate_count: self.gates.len(),
                lut_count: comb_count,
                ff_count,
                estimated_max_freq_mhz: max_freq,
            }
        }

        /// Estimate combinational depth by traversing fan-in chains.
        fn estimate_depth(&self) -> usize {
            use std::collections::HashMap;
            // Build output→gate index.
            let mut output_map: HashMap<&str, usize> = HashMap::new();
            for (i, gate) in self.gates.iter().enumerate() {
                output_map.insert(&gate.output, i);
            }

            let mut memo: HashMap<usize, usize> = HashMap::new();

            fn depth_of(
                idx: usize,
                gates: &[Gate],
                output_map: &HashMap<&str, usize>,
                memo: &mut HashMap<usize, usize>,
            ) -> usize {
                if let Some(&d) = memo.get(&idx) {
                    return d;
                }
                if gates[idx].gate_type == GateType::Dff {
                    memo.insert(idx, 0);
                    return 0;
                }
                let mut max_input_depth = 0usize;
                for inp in &gates[idx].inputs {
                    if let Some(&pred) = output_map.get(inp.as_str()) {
                        let d = depth_of(pred, gates, output_map, memo);
                        max_input_depth = max_input_depth.max(d);
                    }
                }
                let d = max_input_depth + 1;
                memo.insert(idx, d);
                d
            }

            let mut max_depth = 0usize;
            for i in 0..self.gates.len() {
                let d = depth_of(i, &self.gates, &output_map, &mut memo);
                max_depth = max_depth.max(d);
            }
            max_depth
        }

        /// Basic optimisation pass: constant folding.
        ///
        /// Removes gates whose inputs are all constant (tied to primary inputs
        /// named `1'b0` or `1'b1`) and propagates the resulting constant output.
        pub fn optimize(&mut self) {
            let mut constants: std::collections::HashMap<String, bool> =
                std::collections::HashMap::new();
            constants.insert("1'b0".to_string(), false);
            constants.insert("1'b1".to_string(), true);

            let mut changed = true;
            while changed {
                changed = false;
                let mut to_remove: Vec<usize> = Vec::new();

                for (i, gate) in self.gates.iter().enumerate() {
                    let all_const = gate.inputs.iter().all(|inp| constants.contains_key(inp));
                    if !all_const {
                        continue;
                    }

                    let input_vals: Vec<bool> =
                        gate.inputs.iter().map(|inp| constants[inp]).collect();

                    let result = match gate.gate_type {
                        GateType::And => input_vals.iter().all(|&v| v),
                        GateType::Or => input_vals.iter().any(|&v| v),
                        GateType::Not => !input_vals[0],
                        GateType::Nand => !input_vals.iter().all(|&v| v),
                        GateType::Nor => !input_vals.iter().any(|&v| v),
                        GateType::Xor => input_vals.iter().fold(false, |acc, &v| acc ^ v),
                        GateType::Xnor => !input_vals.iter().fold(false, |acc, &v| acc ^ v),
                        GateType::Buf => input_vals[0],
                        GateType::Mux => {
                            // inputs: [sel, a, b] → sel ? b : a
                            if input_vals.len() >= 3 {
                                if input_vals[0] {
                                    input_vals[2]
                                } else {
                                    input_vals[1]
                                }
                            } else {
                                continue;
                            }
                        }
                        GateType::Dff => continue, // sequential — skip
                    };

                    constants.insert(gate.output.clone(), result);
                    to_remove.push(i);
                    changed = true;
                }

                // Remove folded gates in reverse order to keep indices valid.
                to_remove.sort_unstable();
                for &i in to_remove.iter().rev() {
                    self.gates.remove(i);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_verilog_module() {
        let src = r#"
module counter(clk, rst, out);
  // body elided
endmodule
"#;
        let (name, ports) = hdl::parse_verilog(src).expect("parse should succeed");
        assert_eq!(name, "counter");
        assert_eq!(ports, vec!["clk", "rst", "out"]);
    }

    #[test]
    fn test_simulator_event_processing() {
        let mut sim = simulator::RtlSimulator::new();
        sim.register_signal("clk", 1);
        sim.register_signal("data", 1);

        // Schedule clock toggle at t=5 and data change at t=10.
        sim.schedule_event(5, "clk", vec![simulator::LogicValue::One]);
        sim.schedule_event(10, "data", vec![simulator::LogicValue::One]);
        sim.schedule_event(15, "clk", vec![simulator::LogicValue::Zero]);

        sim.run(20);

        assert_eq!(sim.time, 20);
        let clk = sim.get_signal("clk").unwrap();
        assert_eq!(clk.values[0], simulator::LogicValue::Zero);
        let data = sim.get_signal("data").unwrap();
        assert_eq!(data.values[0], simulator::LogicValue::One);

        let clk_hist = sim.get_signal_history("clk").unwrap();
        assert_eq!(clk_hist.len(), 2); // Two transitions: 0→1, 1→0
        assert_eq!(clk_hist[0].time, 5);
        assert_eq!(clk_hist[1].time, 15);
    }

    #[test]
    fn test_vcd_export_format() {
        let signals = vec![waveform::WaveformSignal {
            name: "clk".to_string(),
            width: 1,
            transitions: vec![
                (0, "0".to_string()),
                (5, "1".to_string()),
                (10, "0".to_string()),
            ],
            display_format: waveform::DisplayFormat::Binary,
            color: "#00ff00".to_string(),
        }];

        let vcd = waveform::export_vcd(&signals);

        // VCD must contain mandatory sections.
        assert!(vcd.contains("$date"));
        assert!(vcd.contains("$timescale"));
        assert!(vcd.contains("$var wire 1"));
        assert!(vcd.contains("$enddefinitions $end"));
        assert!(vcd.contains("$dumpvars"));
        // Must contain time markers.
        assert!(vcd.contains("#0"));
        assert!(vcd.contains("#5"));
        assert!(vcd.contains("#10"));
    }

    #[test]
    fn test_logic_value_display() {
        assert_eq!(format!("{}", simulator::LogicValue::Zero), "0");
        assert_eq!(format!("{}", simulator::LogicValue::One), "1");
        assert_eq!(format!("{}", simulator::LogicValue::X), "x");
        assert_eq!(format!("{}", simulator::LogicValue::Z), "z");

        // Four-valued logic operations.
        assert_eq!(
            simulator::LogicValue::One.and(simulator::LogicValue::Zero),
            simulator::LogicValue::Zero
        );
        assert_eq!(
            simulator::LogicValue::One.or(simulator::LogicValue::Zero),
            simulator::LogicValue::One
        );
        assert_eq!(
            simulator::LogicValue::One.not(),
            simulator::LogicValue::Zero
        );
        assert_eq!(
            simulator::LogicValue::X.and(simulator::LogicValue::One),
            simulator::LogicValue::X
        );
    }

    #[test]
    fn test_gate_netlist_stats_and_optimize() {
        let mut netlist = synthesis::GateNetlist {
            gates: vec![
                synthesis::Gate {
                    gate_type: synthesis::GateType::And,
                    inputs: vec!["1'b1".to_string(), "1'b1".to_string()],
                    output: "n1".to_string(),
                },
                synthesis::Gate {
                    gate_type: synthesis::GateType::Or,
                    inputs: vec!["n1".to_string(), "a".to_string()],
                    output: "n2".to_string(),
                },
                synthesis::Gate {
                    gate_type: synthesis::GateType::Dff,
                    inputs: vec!["n2".to_string()],
                    output: "q".to_string(),
                },
            ],
            primary_inputs: vec!["a".to_string(), "1'b0".to_string(), "1'b1".to_string()],
            primary_outputs: vec!["q".to_string()],
        };

        let stats = netlist.stats();
        assert_eq!(stats.gate_count, 3);
        assert_eq!(stats.ff_count, 1);
        assert_eq!(stats.lut_count, 2); // 3 total - 1 DFF

        // Optimize should fold the constant AND(1,1)→1, removing that gate.
        netlist.optimize();
        // The AND gate is folded (both inputs constant). The OR gate remains
        // because "a" is not constant. DFF also remains.
        assert_eq!(netlist.gates.len(), 2);
        // Verify the AND gate was removed (only OR and DFF remain).
        let gate_types: Vec<_> = netlist.gates.iter().map(|g| g.gate_type).collect();
        assert!(!gate_types.contains(&synthesis::GateType::And));
        assert!(gate_types.contains(&synthesis::GateType::Or));
        assert!(gate_types.contains(&synthesis::GateType::Dff));
    }
}
