//! Teleoperation analysis metrics.
//!
//! Accumulated over a sim session and summarized into a [`TeleopAnalysis`].
//! Tracks the things that matter for assessing whether a remote-operation task
//! is feasible and safe: joint tracking error, command smoothness (effort
//! jerk), proximity to joint limits, control latency, and the fraction of time
//! spent in each [`SafeState`].

use crate::safety::SafeState;

/// Rolling teleoperation metrics for one sim session.
#[derive(Debug, Clone)]
pub struct TeleopMetrics {
    pub ticks: u64,
    sum_tracking_err: f64,
    pub max_tracking_err: f32,
    sum_jerk: f64,
    /// Minimum observed distance to any joint limit (rad/m). Smaller = closer.
    pub min_limit_margin: f32,
    pub estop_count: u64,
    state_ticks: [u64; SafeState::COUNT],
    sum_latency: f64,
    pub max_latency_ms: f32,
    prev_efforts: Vec<f32>,
    prev_state: Option<SafeState>,
}

impl Default for TeleopMetrics {
    fn default() -> Self {
        TeleopMetrics::new()
    }
}

impl TeleopMetrics {
    pub fn new() -> Self {
        TeleopMetrics {
            ticks: 0,
            sum_tracking_err: 0.0,
            max_tracking_err: 0.0,
            sum_jerk: 0.0,
            min_limit_margin: f32::INFINITY,
            estop_count: 0,
            state_ticks: [0; SafeState::COUNT],
            sum_latency: 0.0,
            max_latency_ms: 0.0,
            prev_efforts: Vec::new(),
            prev_state: None,
        }
    }

    /// Record one control tick.
    pub(crate) fn record(
        &mut self,
        state: SafeState,
        tracking_err: f32,
        efforts: &[f32],
        limit_margin: f32,
        latency_ms: f32,
    ) {
        self.ticks += 1;
        self.sum_tracking_err += tracking_err as f64;
        self.max_tracking_err = self.max_tracking_err.max(tracking_err);
        self.min_limit_margin = self.min_limit_margin.min(limit_margin);
        self.sum_latency += latency_ms as f64;
        self.max_latency_ms = self.max_latency_ms.max(latency_ms);
        self.state_ticks[state.index()] += 1;

        // Count e-stop *entries* (transition into Estopped), not held ticks.
        if state == SafeState::Estopped && self.prev_state != Some(SafeState::Estopped) {
            self.estop_count += 1;
        }
        self.prev_state = Some(state);

        // Effort jerk = mean |Δ effort| vs previous tick (command smoothness).
        if self.prev_efforts.len() == efforts.len() && !efforts.is_empty() {
            let jerk: f32 = efforts
                .iter()
                .zip(&self.prev_efforts)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / efforts.len() as f32;
            self.sum_jerk += jerk as f64;
        }
        self.prev_efforts = efforts.to_vec();
    }

    fn pct(&self, s: SafeState) -> f32 {
        if self.ticks == 0 {
            0.0
        } else {
            self.state_ticks[s.index()] as f32 / self.ticks as f32
        }
    }

    /// Summarize the session.
    pub fn analysis(&self) -> TeleopAnalysis {
        let n = self.ticks.max(1) as f64;
        TeleopAnalysis {
            ticks: self.ticks,
            mean_tracking_err: (self.sum_tracking_err / n) as f32,
            max_tracking_err: self.max_tracking_err,
            mean_jerk: (self.sum_jerk / n) as f32,
            min_limit_margin: if self.min_limit_margin.is_finite() {
                self.min_limit_margin
            } else {
                0.0
            },
            mean_latency_ms: (self.sum_latency / n) as f32,
            max_latency_ms: self.max_latency_ms,
            estop_count: self.estop_count,
            pct_nominal: self.pct(SafeState::Nominal),
            pct_deadman_lapse: self.pct(SafeState::DeadmanLapse),
            pct_latency_breach: self.pct(SafeState::LatencyBreach),
            pct_estopped: self.pct(SafeState::Estopped),
            pct_autonomy_fallback: self.pct(SafeState::AutonomyFallback),
        }
    }
}

/// Summarized teleoperation analysis for a session.
#[derive(Debug, Clone, PartialEq)]
pub struct TeleopAnalysis {
    pub ticks: u64,
    /// Mean per-DOF joint position tracking error (rad/m).
    pub mean_tracking_err: f32,
    pub max_tracking_err: f32,
    /// Mean inter-tick effort change (command smoothness; lower = smoother).
    pub mean_jerk: f32,
    /// Closest approach to any joint limit over the session.
    pub min_limit_margin: f32,
    pub mean_latency_ms: f32,
    pub max_latency_ms: f32,
    pub estop_count: u64,
    pub pct_nominal: f32,
    pub pct_deadman_lapse: f32,
    pub pct_latency_breach: f32,
    pub pct_estopped: f32,
    pub pct_autonomy_fallback: f32,
}
