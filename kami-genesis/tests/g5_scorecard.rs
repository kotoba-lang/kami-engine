//! G5 quantitative quality-gate harness (ADR-2605261600 §G5).
//!
//! Goal: score a kami-genesis rollout against an external reference trajectory
//! and gate at >= 0.75. The reference is a CSV of `step,x,x_dot,theta,theta_dot`
//! rows. Two provenances are supported, preferring NVIDIA when present:
//!
//!   - `reference_freefall_isaac.csv`   — NVIDIA Isaac Sim ground truth,
//!     captured ONCE on the isolated trial machine (G5 procedure). NOT in the
//!     repo by default; only the metrics CSV is imported back, never the
//!     Omniverse/Isaac binaries (ADR-2605261800 §2(b) N1..N9 NEVER).
//!   - `reference_freefall_analytic.csv` — analytic stand-in committed now so
//!     the scoring code is exercised and a deterministic regression baseline
//!     is pinned. Clearly labelled as NOT NVIDIA.
//!
//! This is clean-room validation: we never run NVIDIA here — we score against
//! data it produced (or, until that lands, against the analytic baseline).
//! When the Isaac CSV is dropped in, this same harness scores against it with
//! zero code change.

use kami_genesis::{CartpoleConfig, CartpoleState};

const ISAAC_CSV: &str = "../fixtures/cartpole/reference_freefall_isaac.csv";
const ANALYTIC_CSV: &str = "../fixtures/cartpole/reference_freefall_analytic.csv";

/// One reference row: cartpole state at a step.
#[derive(Debug, Clone, Copy)]
struct Row {
    step: usize,
    state: [f32; 4],
}

/// Parse the G5 CSV schema (`step,x,x_dot,theta,theta_dot`), skipping `#`
/// comments and the header line.
fn parse_csv(text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("step") {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() != 5 {
            continue;
        }
        let f = |s: &str| s.trim().parse::<f32>();
        if let (Ok(step), Ok(x), Ok(xd), Ok(th), Ok(thd)) = (
            cols[0].trim().parse::<usize>(),
            f(cols[1]),
            f(cols[2]),
            f(cols[3]),
            f(cols[4]),
        ) {
            rows.push(Row {
                step,
                state: [x, xd, th, thd],
            });
        }
    }
    rows
}

/// Load whichever reference is available, preferring the NVIDIA Isaac CSV.
/// Returns `(rows, is_isaac)`.
fn load_reference() -> Option<(Vec<Row>, bool)> {
    if let Ok(t) = std::fs::read_to_string(ISAAC_CSV) {
        let rows = parse_csv(&t);
        if !rows.is_empty() {
            return Some((rows, true));
        }
    }
    if let Ok(t) = std::fs::read_to_string(ANALYTIC_CSV) {
        let rows = parse_csv(&t);
        if !rows.is_empty() {
            return Some((rows, false));
        }
    }
    None
}

/// Run the kami-genesis cartpole free-fall to match the reference setup
/// (action=0, theta0=0.1, cfg=default), producing one state per step.
fn kami_rollout(steps: usize) -> Vec<[f32; 4]> {
    let cfg = CartpoleConfig::default();
    let mut s = CartpoleState {
        theta: 0.1,
        ..Default::default()
    };
    let mut traj = Vec::with_capacity(steps);
    for _ in 0..steps {
        s.step(0.0, &cfg);
        traj.push([s.x, s.x_dot, s.theta, s.theta_dot]);
    }
    traj
}

/// G5 score in [0, 1]: `1 / (1 + rmse/rms_ref)`, RMSE over the pole DOFs
/// (theta, theta_dot) normalised by the reference RMS scale. 1.0 = perfect;
/// the §G5 gate requires >= 0.75.
fn g5_score(reference: &[Row], kami: &[[f32; 4]]) -> f32 {
    let n = reference.len().min(kami.len());
    if n == 0 {
        return 0.0;
    }
    let mut sq_err = 0.0f32;
    let mut sq_ref = 0.0f32;
    for r in reference.iter().take(n) {
        let k = kami[r.step.min(kami.len() - 1)];
        for idx in [2usize, 3] {
            let e = k[idx] - r.state[idx];
            sq_err += e * e;
            sq_ref += r.state[idx] * r.state[idx];
        }
    }
    let rmse = (sq_err / (2 * n) as f32).sqrt();
    let rms_ref = (sq_ref / (2 * n) as f32).sqrt().max(1e-6);
    1.0 / (1.0 + rmse / rms_ref)
}

#[test]
fn g5_csv_schema_parses() {
    let (rows, is_isaac) = load_reference().expect("a reference CSV must exist");
    assert!(rows.len() >= 30, "reference too short: {} rows", rows.len());
    assert_eq!(rows[0].step, 0);
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r.step, i, "non-contiguous step at row {i}");
        assert!(r.state.iter().all(|v| v.is_finite()));
    }
    eprintln!(
        "[G5] reference = {} ({} rows)",
        if is_isaac {
            "ISAAC (NVIDIA ground truth)"
        } else {
            "analytic stand-in"
        },
        rows.len()
    );
}

#[test]
fn g5_score_passes_gate_against_reference() {
    let (reference, is_isaac) = load_reference().expect("a reference CSV must exist");
    let kami = kami_rollout(reference.len());
    let score = g5_score(&reference, &kami);
    eprintln!(
        "[G5] score={score:.4} vs {} reference (gate >= 0.75)",
        if is_isaac { "ISAAC" } else { "analytic" }
    );
    // Against the analytic stand-in the rollout is the SAME closed-form
    // integrator → score ≈ 1.0 (pins determinism). Against a future Isaac CSV
    // the gate is the real >= 0.75 acceptance threshold.
    assert!(score >= 0.75, "G5 score below gate: {score:.4}");
}

#[test]
fn g5_score_is_monotonic_in_error() {
    let (reference, _) = load_reference().expect("reference CSV");
    let exact = kami_rollout(reference.len());
    let perturbed: Vec<[f32; 4]> = exact
        .iter()
        .map(|s| [s[0], s[1], s[2] + 0.2, s[3] + 0.2])
        .collect();
    let s_exact = g5_score(&reference, &exact);
    let s_pert = g5_score(&reference, &perturbed);
    assert!(
        s_exact > s_pert,
        "perturbed not worse: exact={s_exact} pert={s_pert}"
    );
    for s in [s_exact, s_pert] {
        assert!((0.0..=1.0).contains(&s), "score out of [0,1]: {s}");
    }
}
