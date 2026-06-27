//! KAMI Verify — formal verification and coverage analysis for digital designs.
//!
//! Provides equivalence checking, model checking (BFS reachability), SVA-style
//! assertion evaluation, and multi-dimensional coverage collection with cross-bin
//! support.

pub mod equivalence {
    //! Combinational equivalence checking between golden and revised gate-level
    //! netlists. Uses exhaustive input-vector comparison for small circuits
    //! (< 20 inputs) and is structured for future BDD-based expansion.

    use serde::{Deserialize, Serialize};
    use std::collections::{BTreeMap, BTreeSet};

    /// Status of an equivalence check.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum EquivStatus {
        Pass,
        Fail,
        Inconclusive,
    }

    /// A single mismatch record between golden and revised outputs.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Mismatch {
        pub output_name: String,
        pub golden_value: String,
        pub revised_value: String,
        pub input_vector: Vec<bool>,
    }

    /// Result of an equivalence check.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EquivResult {
        pub status: EquivStatus,
        pub mismatches: Vec<Mismatch>,
        pub checked_points: u32,
        pub time_ms: u64,
    }

    /// Evaluate a single gate given current signal values.
    /// Gate types: AND, OR, NOT, XOR, NAND, NOR, BUF.
    fn eval_gate(gate_type: &str, inputs: &[bool]) -> bool {
        match gate_type {
            "AND" => inputs.iter().all(|&b| b),
            "OR" => inputs.iter().any(|&b| b),
            "NOT" => !inputs.first().copied().unwrap_or(false),
            "XOR" => inputs.iter().fold(false, |acc, &b| acc ^ b),
            "NAND" => !inputs.iter().all(|&b| b),
            "NOR" => !inputs.iter().any(|&b| b),
            "BUF" => inputs.first().copied().unwrap_or(false),
            _ => false,
        }
    }

    /// Simulate a gate-level netlist for one input vector.
    /// Each gate: `(output_name, gate_type, input_names)`.
    /// Returns map of signal name → value.
    fn simulate(
        gates: &[(String, String, Vec<String>)],
        primary_inputs: &[String],
        input_vector: &[bool],
    ) -> BTreeMap<String, bool> {
        let mut signals: BTreeMap<String, bool> = BTreeMap::new();
        for (name, val) in primary_inputs.iter().zip(input_vector.iter()) {
            signals.insert(name.clone(), *val);
        }
        // Iterative propagation (topological-order assumed or converge).
        for _ in 0..gates.len() + 1 {
            for (out, gate_type, ins) in gates {
                if let Some(vals) = ins
                    .iter()
                    .map(|i| signals.get(i).copied())
                    .collect::<Option<Vec<_>>>()
                {
                    signals.insert(out.clone(), eval_gate(gate_type, &vals));
                }
            }
        }
        signals
    }

    /// Collect primary input names (names referenced as gate inputs but never
    /// produced as gate outputs).
    fn primary_inputs(gates: &[(String, String, Vec<String>)]) -> Vec<String> {
        let outputs: BTreeSet<&str> = gates.iter().map(|(o, _, _)| o.as_str()).collect();
        let mut inputs: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for (_, _, ins) in gates {
            for i in ins {
                if !outputs.contains(i.as_str()) && seen.insert(i.clone()) {
                    inputs.push(i.clone());
                }
            }
        }
        inputs
    }

    /// Collect primary output names (gate outputs never consumed by another gate).
    fn primary_outputs(gates: &[(String, String, Vec<String>)]) -> Vec<String> {
        let consumed: BTreeSet<&str> = gates
            .iter()
            .flat_map(|(_, _, ins)| ins.iter().map(|s| s.as_str()))
            .collect();
        gates
            .iter()
            .filter(|(o, _, _)| !consumed.contains(o.as_str()))
            .map(|(o, _, _)| o.clone())
            .collect()
    }

    /// Exhaustive equivalence check for small combinational circuits (< 20 inputs).
    /// For larger circuits the result is `Inconclusive`.
    pub fn check_equivalence(
        golden_gates: &[(String, String, Vec<String>)],
        revised_gates: &[(String, String, Vec<String>)],
    ) -> EquivResult {
        #[cfg(not(target_arch = "wasm32"))]
        let start = std::time::Instant::now();

        let pi_golden = primary_inputs(golden_gates);
        let pi_revised = primary_inputs(revised_gates);
        let pi: Vec<String> = {
            let mut s: Vec<String> = pi_golden.clone();
            for p in &pi_revised {
                if !s.contains(p) {
                    s.push(p.clone());
                }
            }
            s
        };

        if pi.len() > 20 {
            #[cfg(not(target_arch = "wasm32"))]
            let time_ms = start.elapsed().as_millis() as u64;
            #[cfg(target_arch = "wasm32")]
            let time_ms = 0;
            return EquivResult {
                status: EquivStatus::Inconclusive,
                mismatches: Vec::new(),
                checked_points: 0,
                time_ms,
            };
        }

        let po_golden = primary_outputs(golden_gates);
        let po_revised = primary_outputs(revised_gates);
        let outputs: Vec<String> = {
            let mut s = po_golden.clone();
            for p in &po_revised {
                if !s.contains(p) {
                    s.push(p.clone());
                }
            }
            s
        };

        let total = 1u64 << pi.len();
        let mut mismatches = Vec::new();

        for idx in 0..total {
            let vec: Vec<bool> = (0..pi.len()).map(|b| (idx >> b) & 1 == 1).collect();
            let g = simulate(golden_gates, &pi, &vec);
            let r = simulate(revised_gates, &pi, &vec);
            for out in &outputs {
                let gv = g.get(out).copied().unwrap_or(false);
                let rv = r.get(out).copied().unwrap_or(false);
                if gv != rv {
                    mismatches.push(Mismatch {
                        output_name: out.clone(),
                        golden_value: gv.to_string(),
                        revised_value: rv.to_string(),
                        input_vector: vec.clone(),
                    });
                }
            }
        }

        let status = if mismatches.is_empty() {
            EquivStatus::Pass
        } else {
            EquivStatus::Fail
        };

        #[cfg(not(target_arch = "wasm32"))]
        let time_ms = start.elapsed().as_millis() as u64;
        #[cfg(target_arch = "wasm32")]
        let time_ms = 0;

        EquivResult {
            status,
            mismatches,
            checked_points: total as u32,
            time_ms,
        }
    }
}

pub mod model_check {
    //! Bounded model checking via BFS reachability.

    use serde::{Deserialize, Serialize};
    use std::collections::{HashMap, HashSet, VecDeque};

    /// Type of temporal property.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PropertyType {
        Safety,
        Liveness,
        Reachability,
    }

    /// A property to verify.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Property {
        pub name: String,
        pub property_type: PropertyType,
        pub expression: String,
    }

    /// Verification status.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum CheckStatus {
        Verified,
        Violated,
        Timeout,
    }

    /// Result of model checking a single property.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ModelCheckResult {
        pub property_name: String,
        pub status: CheckStatus,
        pub counterexample: Option<Vec<HashMap<String, String>>>,
        pub depth: u32,
    }

    /// BFS-based model checking.
    ///
    /// `states` — total number of states (0..states-1).
    /// `transitions` — directed edges (from, to).
    /// `properties` — properties to check.
    ///
    /// For `Reachability` properties the `expression` is parsed as a target
    /// state id (decimal). The checker reports `Verified` if reachable from
    /// state 0, `Violated` otherwise.
    ///
    /// For `Safety` properties the `expression` names a "bad" state. `Verified`
    /// means the bad state is unreachable; `Violated` if reachable.
    ///
    /// `Liveness` is approximated: `Verified` when the target state is
    /// reachable (optimistic), `Timeout` otherwise.
    pub fn check_properties(
        states: u32,
        transitions: &[(u32, u32)],
        properties: &[Property],
    ) -> Vec<ModelCheckResult> {
        // Build adjacency list.
        let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(from, to) in transitions {
            adj.entry(from).or_default().push(to);
        }

        // BFS from state 0, record parent chain.
        let mut visited: HashSet<u32> = HashSet::new();
        let mut parent: HashMap<u32, u32> = HashMap::new();
        let mut depth_map: HashMap<u32, u32> = HashMap::new();
        let mut queue: VecDeque<u32> = VecDeque::new();

        if states > 0 {
            visited.insert(0);
            depth_map.insert(0, 0);
            queue.push_back(0);
        }

        while let Some(s) = queue.pop_front() {
            let d = depth_map[&s];
            if let Some(nexts) = adj.get(&s) {
                for &n in nexts {
                    if visited.insert(n) {
                        parent.insert(n, s);
                        depth_map.insert(n, d + 1);
                        queue.push_back(n);
                    }
                }
            }
        }

        let build_trace = |target: u32| -> Vec<HashMap<String, String>> {
            let mut path = vec![target];
            let mut cur = target;
            while let Some(&p) = parent.get(&cur) {
                path.push(p);
                cur = p;
            }
            path.reverse();
            path.iter()
                .map(|s| {
                    let mut m = HashMap::new();
                    m.insert("state".to_string(), s.to_string());
                    m
                })
                .collect()
        };

        properties
            .iter()
            .map(|prop| {
                let target: u32 = prop.expression.trim().parse().unwrap_or(u32::MAX);
                let reachable = visited.contains(&target);
                let depth = depth_map.get(&target).copied().unwrap_or(0);

                match prop.property_type {
                    PropertyType::Reachability => ModelCheckResult {
                        property_name: prop.name.clone(),
                        status: if reachable {
                            CheckStatus::Verified
                        } else {
                            CheckStatus::Violated
                        },
                        counterexample: if reachable {
                            Some(build_trace(target))
                        } else {
                            None
                        },
                        depth,
                    },
                    PropertyType::Safety => {
                        // "bad state" must be unreachable for safety.
                        ModelCheckResult {
                            property_name: prop.name.clone(),
                            status: if reachable {
                                CheckStatus::Violated
                            } else {
                                CheckStatus::Verified
                            },
                            counterexample: if reachable {
                                Some(build_trace(target))
                            } else {
                                None
                            },
                            depth,
                        }
                    }
                    PropertyType::Liveness => ModelCheckResult {
                        property_name: prop.name.clone(),
                        status: if reachable {
                            CheckStatus::Verified
                        } else {
                            CheckStatus::Timeout
                        },
                        counterexample: if reachable {
                            Some(build_trace(target))
                        } else {
                            None
                        },
                        depth,
                    },
                }
            })
            .collect()
    }
}

pub mod assertion {
    //! SVA-style assertion evaluation over signal traces.

    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// Assertion type following SystemVerilog Assertions.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum AssertionType {
        Assert,
        Assume,
        Cover,
        Restrict,
    }

    /// Severity level when an assertion fires.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Severity {
        Fatal,
        Error,
        Warning,
        Info,
    }

    /// An SVA-style assertion definition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SvaAssertion {
        pub name: String,
        pub assertion_type: AssertionType,
        pub expression: String,
        pub clock: String,
        pub severity: Severity,
    }

    /// Evaluation status of an assertion.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum AssertionStatus {
        Pass,
        Fail,
        Vacuous,
    }

    /// Result of evaluating a single assertion.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SvaResult {
        pub assertion_name: String,
        pub status: AssertionStatus,
        pub hit_count: u64,
        pub fail_count: u64,
        pub first_fail_time: Option<u64>,
    }

    /// Evaluate assertions against a signal trace.
    ///
    /// `signal_trace` is a vector of time-steps, each mapping signal names to
    /// string values. The assertion `expression` is evaluated as a simple
    /// signal-name lookup: the assertion passes at a given time-step when the
    /// named signal equals `"1"` or `"true"`.
    pub fn evaluate_assertions_simple(
        assertions: &[SvaAssertion],
        signal_trace: &[HashMap<String, String>],
    ) -> Vec<SvaResult> {
        assertions
            .iter()
            .map(|a| {
                let mut hit_count: u64 = 0;
                let mut fail_count: u64 = 0;
                let mut first_fail_time: Option<u64> = None;
                let mut clock_active_count: u64 = 0;

                for (t, signals) in signal_trace.iter().enumerate() {
                    // Only evaluate on active clock edges.
                    let clock_val = signals
                        .get(&a.clock)
                        .map(|v| v == "1" || v == "true")
                        .unwrap_or(true);
                    if !clock_val {
                        continue;
                    }
                    clock_active_count += 1;

                    let expr_val = signals
                        .get(&a.expression)
                        .map(|v| v == "1" || v == "true")
                        .unwrap_or(false);

                    if expr_val {
                        hit_count += 1;
                    } else {
                        fail_count += 1;
                        if first_fail_time.is_none() {
                            first_fail_time = Some(t as u64);
                        }
                    }
                }

                let status = if clock_active_count == 0 {
                    AssertionStatus::Vacuous
                } else if fail_count == 0 {
                    AssertionStatus::Pass
                } else {
                    AssertionStatus::Fail
                };

                SvaResult {
                    assertion_name: a.name.clone(),
                    status,
                    hit_count,
                    fail_count,
                    first_fail_time,
                }
            })
            .collect()
    }
}

pub mod coverage {
    //! Coverage collection and reporting (line, toggle, branch, condition, FSM,
    //! functional) with cross-bin support.

    use serde::{Deserialize, Serialize};

    /// Type of coverage metric.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum CoverageType {
        Line,
        Toggle,
        Branch,
        Condition,
        Fsm,
        Functional,
    }

    /// A single coverage point.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CoveragePoint {
        pub name: String,
        pub cov_type: CoverageType,
        pub hit: bool,
        pub hit_count: u64,
    }

    /// A cross-coverage bin.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CrossBin {
        pub name: String,
        pub dimensions: Vec<String>,
        pub hit: bool,
    }

    /// A group of related coverage points.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CoverageGroup {
        pub name: String,
        pub points: Vec<CoveragePoint>,
        pub cross_bins: Vec<CrossBin>,
    }

    /// Aggregate coverage report.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CoverageReport {
        pub groups: Vec<CoverageGroup>,
    }

    impl CoverageReport {
        /// Overall coverage percentage (0.0–100.0) across all points and cross
        /// bins.
        pub fn total_coverage(&self) -> f64 {
            let mut total = 0u64;
            let mut hit = 0u64;
            for g in &self.groups {
                for p in &g.points {
                    total += 1;
                    if p.hit {
                        hit += 1;
                    }
                }
                for cb in &g.cross_bins {
                    total += 1;
                    if cb.hit {
                        hit += 1;
                    }
                }
            }
            if total == 0 {
                100.0
            } else {
                (hit as f64 / total as f64) * 100.0
            }
        }

        /// Coverage percentage for a specific type (points only, cross bins
        /// excluded).
        pub fn coverage_by_type(&self, cov_type: CoverageType) -> f64 {
            let mut total = 0u64;
            let mut hit = 0u64;
            for g in &self.groups {
                for p in &g.points {
                    if p.cov_type == cov_type {
                        total += 1;
                        if p.hit {
                            hit += 1;
                        }
                    }
                }
            }
            if total == 0 {
                100.0
            } else {
                (hit as f64 / total as f64) * 100.0
            }
        }

        /// Return references to all uncovered points.
        pub fn uncovered_points(&self) -> Vec<&CoveragePoint> {
            self.groups
                .iter()
                .flat_map(|g| g.points.iter())
                .filter(|p| !p.hit)
                .collect()
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn equivalence_identical_circuits_pass() {
        let gates = vec![
            ("n1".into(), "AND".into(), vec!["a".into(), "b".into()]),
            ("out".into(), "OR".into(), vec!["n1".into(), "c".into()]),
        ];
        let result = equivalence::check_equivalence(&gates, &gates);
        assert_eq!(result.status, equivalence::EquivStatus::Pass);
        assert!(result.mismatches.is_empty());
        assert_eq!(result.checked_points, 8); // 3 inputs → 2^3
    }

    #[test]
    fn equivalence_different_circuits_fail() {
        let golden = vec![("out".into(), "AND".into(), vec!["a".into(), "b".into()])];
        let revised = vec![("out".into(), "OR".into(), vec!["a".into(), "b".into()])];
        let result = equivalence::check_equivalence(&golden, &revised);
        assert_eq!(result.status, equivalence::EquivStatus::Fail);
        assert!(!result.mismatches.is_empty());
    }

    #[test]
    fn reachability_finds_reachable_state() {
        // 0 → 1 → 2 → 3
        let transitions = vec![(0, 1), (1, 2), (2, 3)];
        let props = vec![model_check::Property {
            name: "reach_3".into(),
            property_type: model_check::PropertyType::Reachability,
            expression: "3".into(),
        }];
        let results = model_check::check_properties(4, &transitions, &props);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, model_check::CheckStatus::Verified);
        assert!(results[0].counterexample.is_some());
        assert_eq!(results[0].depth, 3);
    }

    #[test]
    fn safety_unreachable_bad_state() {
        // 0 → 1, state 2 is disconnected (bad state)
        let transitions = vec![(0, 1)];
        let props = vec![model_check::Property {
            name: "safe".into(),
            property_type: model_check::PropertyType::Safety,
            expression: "2".into(),
        }];
        let results = model_check::check_properties(3, &transitions, &props);
        assert_eq!(results[0].status, model_check::CheckStatus::Verified);
        assert!(results[0].counterexample.is_none());
    }

    #[test]
    fn coverage_calculation_correct() {
        let report = coverage::CoverageReport {
            groups: vec![coverage::CoverageGroup {
                name: "grp".into(),
                points: vec![
                    coverage::CoveragePoint {
                        name: "p1".into(),
                        cov_type: coverage::CoverageType::Line,
                        hit: true,
                        hit_count: 5,
                    },
                    coverage::CoveragePoint {
                        name: "p2".into(),
                        cov_type: coverage::CoverageType::Line,
                        hit: false,
                        hit_count: 0,
                    },
                    coverage::CoveragePoint {
                        name: "p3".into(),
                        cov_type: coverage::CoverageType::Toggle,
                        hit: true,
                        hit_count: 3,
                    },
                ],
                cross_bins: vec![coverage::CrossBin {
                    name: "cross1".into(),
                    dimensions: vec!["a".into(), "b".into()],
                    hit: true,
                }],
            }],
        };
        // 3 points + 1 cross bin = 4, 3 hit → 75%
        assert!((report.total_coverage() - 75.0).abs() < 0.01);
        // Line: 1 of 2 → 50%
        assert!((report.coverage_by_type(coverage::CoverageType::Line) - 50.0).abs() < 0.01);
        // Toggle: 1 of 1 → 100%
        assert!((report.coverage_by_type(coverage::CoverageType::Toggle) - 100.0).abs() < 0.01);
        let uncov = report.uncovered_points();
        assert_eq!(uncov.len(), 1);
        assert_eq!(uncov[0].name, "p2");
    }

    #[test]
    fn assertion_pass_and_fail() {
        let assertions = vec![
            assertion::SvaAssertion {
                name: "always_high".into(),
                assertion_type: assertion::AssertionType::Assert,
                expression: "sig_a".into(),
                clock: "clk".into(),
                severity: assertion::Severity::Error,
            },
            assertion::SvaAssertion {
                name: "sometimes_low".into(),
                assertion_type: assertion::AssertionType::Assert,
                expression: "sig_b".into(),
                clock: "clk".into(),
                severity: assertion::Severity::Warning,
            },
        ];
        let trace: Vec<HashMap<String, String>> = vec![
            [("clk", "1"), ("sig_a", "1"), ("sig_b", "1")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            [("clk", "1"), ("sig_a", "1"), ("sig_b", "0")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ];
        let results = assertion::evaluate_assertions_simple(&assertions, &trace);
        assert_eq!(results[0].status, assertion::AssertionStatus::Pass);
        assert_eq!(results[0].hit_count, 2);
        assert_eq!(results[1].status, assertion::AssertionStatus::Fail);
        assert_eq!(results[1].fail_count, 1);
        assert_eq!(results[1].first_fail_time, Some(1));
    }

    #[test]
    fn cross_coverage_tracking() {
        let report = coverage::CoverageReport {
            groups: vec![coverage::CoverageGroup {
                name: "cross_grp".into(),
                points: vec![],
                cross_bins: vec![
                    coverage::CrossBin {
                        name: "bin_0_0".into(),
                        dimensions: vec!["x".into(), "y".into()],
                        hit: true,
                    },
                    coverage::CrossBin {
                        name: "bin_0_1".into(),
                        dimensions: vec!["x".into(), "y".into()],
                        hit: false,
                    },
                ],
            }],
        };
        assert!((report.total_coverage() - 50.0).abs() < 0.01);
    }
}
