//! Window compositor — z-order management, focus, drag state.
//!
//! The compositor system runs each frame to:
//! 1. Sort windows by z_order
//! 2. Render desktop background (wallpaper / gradient)
//! 3. Render each window: shadow → background rect → title bar → content area
//! 4. Render overlays: taskbar, notifications, consent modals, launcher
//!
//! All rendering uses kami-ui-gpu (rounded rect, gradient, shadow) and
//! kami-text (SDF text for titles, labels).

/// Compositor state tracking z-order and drag operations.
pub struct CompositorState {
    /// Ordered list of window entity IDs, front-to-back.
    z_stack: Vec<u64>,
    /// Currently focused window (receives input).
    focused: Option<u64>,
    /// Active drag operation.
    drag: Option<DragState>,
    /// Desktop dimensions (updated on resize).
    pub desktop_width: f32,
    pub desktop_height: f32,
    /// Taskbar height (reserved at bottom).
    pub taskbar_height: f32,
}

/// Active window drag (title bar grab).
struct DragState {
    window_id: u64,
    offset_x: f32,
    offset_y: f32,
}

impl CompositorState {
    /// Create compositor with default 1920x1080 desktop.
    pub fn new() -> Self {
        Self {
            z_stack: Vec::new(),
            focused: None,
            drag: None,
            desktop_width: 1920.0,
            desktop_height: 1080.0,
            taskbar_height: 48.0,
        }
    }

    /// Bring a window to the front of the z-stack.
    pub fn bring_to_front(&mut self, window_id: u64) {
        self.z_stack.retain(|&id| id != window_id);
        self.z_stack.insert(0, window_id);
        self.focused = Some(window_id);
    }

    /// Remove a window from the z-stack.
    pub fn remove(&mut self, window_id: u64) {
        self.z_stack.retain(|&id| id != window_id);
        if self.focused == Some(window_id) {
            self.focused = self.z_stack.first().copied();
        }
    }

    /// Get the currently focused window.
    pub fn focused_window(&self) -> Option<u64> {
        self.focused
    }

    /// Get the z-stack (front-to-back order) for rendering.
    pub fn z_stack(&self) -> &[u64] {
        &self.z_stack
    }

    /// Start a drag operation.
    pub fn start_drag(&mut self, window_id: u64, offset_x: f32, offset_y: f32) {
        self.drag = Some(DragState {
            window_id,
            offset_x,
            offset_y,
        });
    }

    /// Update drag position. Returns (window_id, new_x, new_y) if dragging.
    pub fn update_drag(&self, mouse_x: f32, mouse_y: f32) -> Option<(u64, f32, f32)> {
        self.drag
            .as_ref()
            .map(|d| (d.window_id, mouse_x - d.offset_x, mouse_y - d.offset_y))
    }

    /// End the current drag operation.
    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    /// Usable desktop area (excludes taskbar).
    pub fn usable_height(&self) -> f32 {
        self.desktop_height - self.taskbar_height
    }
}

impl Default for CompositorState {
    fn default() -> Self {
        Self::new()
    }
}
