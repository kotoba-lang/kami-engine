//! Focus-aware input routing — delegates to kami-input FocusManager.
//!
//! Thin wrapper providing OS-specific focus target names while
//! reusing the generic FocusManager from kami-input.

pub use kami_input::{FocusManager, FocusTarget, PanelId};

/// OS-level input router wrapping kami-input::FocusManager with OS semantics.
pub struct InputRouterState {
    /// Generic focus manager (from kami-input SDK).
    pub focus: FocusManager,
}

impl InputRouterState {
    /// Create with no focus.
    pub fn new() -> Self {
        Self {
            focus: FocusManager::new(),
        }
    }

    /// Set the focused window.
    pub fn set_focus(&mut self, window_id: u64) {
        self.focus.set_focus(window_id);
    }

    /// Clear focus if the given window is currently focused.
    pub fn clear_focus_if(&mut self, window_id: u64) {
        self.focus.clear_focus(window_id);
    }

    /// Set consent modal blocking state.
    pub fn set_consent_modal(&mut self, active: bool) {
        if active {
            self.focus.push_modal(0); // sentinel modal ID for consent
        } else {
            self.focus.pop_modal();
        }
    }

    /// Set launcher overlay state.
    pub fn set_launcher(&mut self, active: bool) {
        self.focus.set_global_overlay(active);
    }

    /// Resolve where input should go (delegates to FocusManager).
    pub fn resolve_target(&self) -> FocusTarget {
        self.focus.resolve()
    }

    /// Get the currently focused window.
    pub fn focused(&self) -> Option<u64> {
        self.focus.focused_panel()
    }
}

impl Default for InputRouterState {
    fn default() -> Self {
        Self::new()
    }
}
