//! Window ECS component and management.
//!
//! Each window is an hecs entity with `WindowComponent` + `WindowRect`.
//! The compositor queries these to render z-ordered rounded rectangles
//! via kami-ui-gpu, with content rendered per `WindowContent` variant.

use serde::{Deserialize, Serialize};

/// Window lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowState {
    /// Normal resizable window.
    Normal,
    /// Minimized to taskbar.
    Minimized,
    /// Maximized (fills desktop area above taskbar).
    Maximized,
}

/// What the window renders inside its content area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WindowContent {
    /// Overlay an iframe for a Svelte appview.
    IFrame { url: String },
    /// Render a KAMI sub-scene (3D, graph, VRM).
    Kami { scene_json: String },
    /// Embedded XRPC terminal.
    Terminal,
    /// R2/IPFS file browser.
    FileExplorer,
    /// Agent chat conversation.
    AgentChat { convo_id: String },
}

/// Core window metadata (hecs component).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowComponent {
    /// App nanoid that owns this window.
    pub app_id: String,
    /// Window title text.
    pub title: String,
    /// Current lifecycle state.
    pub state: WindowState,
    /// Content type and data.
    pub content: WindowContent,
    /// Z-order assigned by compositor.
    pub z_order: u32,
}

/// Window position and size (hecs component, separate for ECS queries).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Configuration for opening a new window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub app_id: String,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub content: WindowContent,
}

impl WindowComponent {
    /// Create a WindowComponent from a WindowConfig.
    pub fn from_config(config: &WindowConfig) -> Self {
        Self {
            app_id: config.app_id.clone(),
            title: config.title.clone(),
            state: WindowState::Normal,
            content: config.content.clone(),
            z_order: 0,
        }
    }
}

impl WindowRect {
    /// Check if a point (px, py) is inside this rect.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    /// Title bar hit area (top 32px of window).
    pub fn title_bar_contains(&self, px: f32, py: f32) -> bool {
        const TITLE_BAR_HEIGHT: f32 = 32.0;
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + TITLE_BAR_HEIGHT
    }
}
