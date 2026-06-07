//! Teleoperation safety envelope.
//!
//! Clean-room Rust port of the tazuna `teleop_safety.py` reasoner
//! (etzhayyim ADR-2606042100, gate G10 "soft-RT, not a safety system"): a
//! best-effort gate that converts a candidate command into a *safe* command
//! plus a [`SafeState`]. Priority (highest first):
//!
//! 1. **E-stop** latched → emit [`CommandKind::Estop`], zero motion.
//! 2. **Latency breach** (control round-trip over budget) → [`CommandKind::Halt`].
//! 3. **Deadman lapse** (enable button not held) → [`CommandKind::Halt`].
//! 4. Otherwise **Nominal** → pass the candidate through (still `dry_run` at R0).

use crate::command::{CommandKind, TeleopCommand};
use kami_input::gamepad::{GamepadState, Pad};

/// Safety state for one tick (drives metrics + HUD).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeState {
    Nominal,
    DeadmanLapse,
    LatencyBreach,
    Estopped,
    AutonomyFallback,
}

impl SafeState {
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        match self {
            SafeState::Nominal => 0,
            SafeState::DeadmanLapse => 1,
            SafeState::LatencyBreach => 2,
            SafeState::Estopped => 3,
            SafeState::AutonomyFallback => 4,
        }
    }
}

/// Button assignments + latency budget for the safety layer.
#[derive(Debug, Clone, Copy)]
pub struct SafetyConfig {
    /// Deadman: must be **held** to permit motion (default `L1`).
    pub deadman: Option<Pad>,
    /// E-stop: latches a stop on press (default `East` = B/○).
    pub estop: Option<Pad>,
    /// Resume: clears the e-stop latch on press (default `Start`).
    pub resume: Option<Pad>,
    /// Control round-trip latency budget, ms (default 250).
    pub latency_budget_ms: f32,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        SafetyConfig {
            deadman: Some(Pad::L1),
            estop: Some(Pad::East),
            resume: Some(Pad::Start),
            latency_budget_ms: 250.0,
        }
    }
}

/// Stateful safety envelope (holds the e-stop latch + last state).
#[derive(Debug, Clone)]
pub struct SafetyEnvelope {
    pub cfg: SafetyConfig,
    estopped: bool,
    last_state: SafeState,
}

impl SafetyEnvelope {
    pub fn new(cfg: SafetyConfig) -> Self {
        SafetyEnvelope { cfg, estopped: false, last_state: SafeState::Nominal }
    }

    pub fn state(&self) -> SafeState {
        self.last_state
    }

    pub fn is_estopped(&self) -> bool {
        self.estopped
    }

    /// Manually clear the e-stop latch (operator console).
    pub fn reset_estop(&mut self) {
        self.estopped = false;
    }

    /// Gate a candidate command. Returns the safe command and the resulting
    /// [`SafeState`]. `latency_ms` is the measured control round-trip for the
    /// tick. Edge-triggered buttons (`estop`/`resume`) rely on the caller
    /// having called [`GamepadState::begin_frame`] before applying this tick's
    /// input.
    pub fn gate(
        &mut self,
        gp: &GamepadState,
        latency_ms: f32,
        cmd: TeleopCommand,
    ) -> (TeleopCommand, SafeState) {
        let dof = cmd.joint_vel.len();

        // Latch / unlatch e-stop (edge-triggered).
        if let Some(e) = self.cfg.estop {
            if gp.just_pressed(e) {
                self.estopped = true;
            }
        }
        if let Some(r) = self.cfg.resume {
            if self.estopped && gp.just_pressed(r) {
                self.estopped = false;
            }
        }

        let state = if self.estopped {
            SafeState::Estopped
        } else if latency_ms > self.cfg.latency_budget_ms {
            SafeState::LatencyBreach
        } else if self.cfg.deadman.map(|d| !gp.pressed(d)).unwrap_or(false) {
            SafeState::DeadmanLapse
        } else {
            SafeState::Nominal
        };

        self.last_state = state;

        let out = match state {
            SafeState::Estopped => TeleopCommand::estop(dof),
            SafeState::LatencyBreach | SafeState::DeadmanLapse => TeleopCommand::halt(dof),
            SafeState::Nominal => cmd,
            // AutonomyFallback is produced by the handoff layer, not gate().
            SafeState::AutonomyFallback => {
                TeleopCommand::zeroed(CommandKind::Handback, dof)
            }
        };
        (out, state)
    }
}
