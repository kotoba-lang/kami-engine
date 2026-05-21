//! Taskbar system — bottom bar showing agent status, budget, clock, and window list.
//!
//! Rendered via kami-ui-gpu at the bottom of the desktop canvas.
//! Layout: [Launcher] | [Window List] | [Agent Status] [Budget] [Clock]

use serde::{Deserialize, Serialize};

/// Taskbar persistent state.
pub struct TaskbarState {
    /// Active agent summaries for the status area.
    pub agents: Vec<AgentStatusEntry>,
    /// Current GCC budget balance (formatted string).
    pub budget_display: String,
    /// Pending consent count (badge number).
    pub pending_consent_count: u32,
    /// Whether the launcher is open.
    pub launcher_open: bool,
}

/// Agent summary for taskbar display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusEntry {
    /// Agent DID.
    pub did: String,
    /// Short display name.
    pub name: String,
    /// Status indicator color: green=active, yellow=busy, red=error, gray=paused.
    pub status: AgentStatus,
}

/// Agent operational status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Active,
    Busy,
    Error,
    Paused,
    Dead,
}

impl TaskbarState {
    /// Create empty taskbar.
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            budget_display: "0 GCC".to_string(),
            pending_consent_count: 0,
            launcher_open: false,
        }
    }

    /// Update agent list from Magatama XRPC response.
    pub fn update_agents(&mut self, agents: Vec<AgentStatusEntry>) {
        self.agents = agents;
    }

    /// Update budget display.
    pub fn update_budget(&mut self, formatted: String) {
        self.budget_display = formatted;
    }

    /// Update pending consent count.
    pub fn update_consent_count(&mut self, count: u32) {
        self.pending_consent_count = count;
    }

    /// Toggle launcher visibility.
    pub fn toggle_launcher(&mut self) {
        self.launcher_open = !self.launcher_open;
    }
}

impl Default for TaskbarState {
    fn default() -> Self {
        Self::new()
    }
}
