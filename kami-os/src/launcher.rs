//! App launcher — grid overlay showing installed Apps.
//!
//! Activated from taskbar button or keyboard shortcut (Super key).
//! Shows a searchable grid of app icons with names, sorted by recency.

use serde::{Deserialize, Serialize};

/// Launcher overlay state.
pub struct LauncherState {
    /// Whether the launcher overlay is visible.
    pub visible: bool,
    /// Search filter text.
    pub search_query: String,
    /// Installed app entries.
    pub apps: Vec<LauncherApp>,
    /// Currently selected index (keyboard navigation).
    pub selected_index: usize,
}

/// An app entry in the launcher grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherApp {
    /// App nanoid.
    pub app_id: String,
    /// Display name.
    pub name: String,
    /// Icon: emoji, initials, or URL.
    pub icon: String,
    /// App description.
    pub description: String,
    /// App vanity domain (e.g. "news.gftd.ai").
    pub domain: String,
    /// Whether currently running (has open window).
    pub running: bool,
}

impl LauncherState {
    /// Create empty launcher.
    pub fn new() -> Self {
        Self {
            visible: false,
            search_query: String::new(),
            apps: Vec::new(),
            selected_index: 0,
        }
    }

    /// Toggle launcher visibility, resetting search on open.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.search_query.clear();
            self.selected_index = 0;
        }
    }

    /// Filter apps by search query.
    pub fn filtered_apps(&self) -> Vec<&LauncherApp> {
        if self.search_query.is_empty() {
            return self.apps.iter().collect();
        }
        let q = self.search_query.to_lowercase();
        self.apps
            .iter()
            .filter(|app| {
                app.name.to_lowercase().contains(&q)
                    || app.description.to_lowercase().contains(&q)
                    || app.domain.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Move selection up in the grid.
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down in the grid.
    pub fn select_next(&mut self) {
        let max = self.filtered_apps().len().saturating_sub(1);
        if self.selected_index < max {
            self.selected_index += 1;
        }
    }
}

impl Default for LauncherState {
    fn default() -> Self {
        Self::new()
    }
}
