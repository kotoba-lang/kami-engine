//! kami-input: Unified input system.
//!
//! Keyboard, mouse, touch, gamepad, and gesture detection.
//! Platform-agnostic API consumed by all kami-web entry points.

use glam::Vec2;

pub mod gamepad;
pub use gamepad::{Axis, ControllerProfile, Deadzone, GamepadState, Pad};

/// Input action (abstract, mapped from physical input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Movement
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    // Camera
    ZoomIn,
    ZoomOut,
    PanStart,
    PanEnd,
    PanMove,
    // Interaction
    Primary,
    Secondary,
    Cancel,
    Confirm,
    // Game
    Jump,
    Sprint,
    Interact,
    Attack,
    // System
    Pause,
    Reset,
    Menu,
    Fullscreen,
}

/// Input source device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Mouse,
    Touch,
    /// Pen tablet stylus (e.g. XP-Pen Deco, Wacom).
    Stylus,
    Gamepad(u8),
}

/// Stylus-specific data extracted from PointerEvent.
///
/// Supports XP-Pen Deco (8192/16384 pressure levels, ±60° tilt),
/// Wacom, Apple Pencil, and other W3C PointerEvent-compliant tablets.
/// Browser normalizes pressure to 0.0–1.0 regardless of hardware levels.
#[derive(Debug, Clone, Copy, Default)]
pub struct StylusData {
    /// Normalized pen pressure (0.0 = no contact, 1.0 = max device pressure).
    /// XP-Pen Deco: hardware 8192/16384 levels → browser normalizes to 0.0–1.0.
    pub pressure: f32,
    /// Tilt angle between Y–Z plane and stylus plane, in degrees (−90 to 90).
    /// XP-Pen Deco supports ±60°.
    pub tilt_x: f32,
    /// Tilt angle between X–Z plane and stylus plane, in degrees (−90 to 90).
    pub tilt_y: f32,
    /// Barrel rotation pressure (−1.0 to 1.0). 0 if unsupported.
    pub tangential_pressure: f32,
    /// Pen twist/rotation in degrees (0 to 359). 0 if unsupported.
    pub twist: u16,
}

/// Raw input event.
#[derive(Debug, Clone)]
pub enum InputEvent {
    KeyDown {
        code: String,
        device: Device,
    },
    KeyUp {
        code: String,
        device: Device,
    },
    PointerDown {
        x: f32,
        y: f32,
        button: u8,
        /// Stylus data when `device == Device::Stylus`. None for mouse.
        stylus: Option<StylusData>,
        device: Device,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: u8,
        stylus: Option<StylusData>,
        device: Device,
    },
    PointerMove {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        /// Stylus data when `device == Device::Stylus`. None for mouse.
        stylus: Option<StylusData>,
        device: Device,
    },
    Scroll {
        dx: f32,
        dy: f32,
        device: Device,
    },
    Touch {
        phase: TouchPhase,
        id: u32,
        x: f32,
        y: f32,
        device: Device,
    },
    GamepadAxis {
        pad: u8,
        axis: u8,
        value: f32,
    },
    GamepadButton {
        pad: u8,
        button: u8,
        pressed: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Start,
    Move,
    End,
    Cancel,
}

/// Gesture detector.
#[derive(Debug, Clone)]
pub struct GestureState {
    pub touches: Vec<TouchPoint>,
    pub pinch_distance: Option<f32>,
    pub pinch_delta: f32,    // positive = zoom in
    pub swipe: Option<Vec2>, // direction if swiped
    pub tap_count: u8,       // 1 = tap, 2 = double-tap
}

#[derive(Debug, Clone)]
pub struct TouchPoint {
    pub id: u32,
    pub start: Vec2,
    pub current: Vec2,
    pub phase: TouchPhase,
}

impl Default for GestureState {
    fn default() -> Self {
        Self {
            touches: Vec::new(),
            pinch_distance: None,
            pinch_delta: 0.0,
            swipe: None,
            tap_count: 0,
        }
    }
}

impl GestureState {
    /// Process a touch event and update gesture state.
    pub fn process(&mut self, event: &InputEvent) {
        match event {
            InputEvent::Touch {
                phase, id, x, y, ..
            } => {
                let pos = Vec2::new(*x, *y);
                match phase {
                    TouchPhase::Start => {
                        self.touches.push(TouchPoint {
                            id: *id,
                            start: pos,
                            current: pos,
                            phase: *phase,
                        });
                        self.tap_count = 0;
                    }
                    TouchPhase::Move => {
                        if let Some(t) = self.touches.iter_mut().find(|t| t.id == *id) {
                            t.current = pos;
                            t.phase = *phase;
                        }
                        self.detect_pinch();
                    }
                    TouchPhase::End | TouchPhase::Cancel => {
                        if let Some(t) = self.touches.iter().find(|t| t.id == *id) {
                            let dist = (t.current - t.start).length();
                            if dist < 10.0 {
                                self.tap_count += 1;
                            } else if dist > 30.0 {
                                self.swipe = Some((t.current - t.start).normalize());
                            }
                        }
                        self.touches.retain(|t| t.id != *id);
                        self.pinch_distance = None;
                    }
                }
            }
            _ => {}
        }
    }

    fn detect_pinch(&mut self) {
        if self.touches.len() == 2 {
            let d = (self.touches[0].current - self.touches[1].current).length();
            if let Some(prev) = self.pinch_distance {
                self.pinch_delta = d - prev;
            }
            self.pinch_distance = Some(d);
        }
    }
}

/// Key binding map: physical input → abstract action.
pub struct InputMap {
    pub bindings: Vec<(String, Action)>, // (key_code, action)
}

impl InputMap {
    /// Default WASD + arrow + gamepad mapping.
    pub fn default_fps() -> Self {
        Self {
            bindings: vec![
                ("KeyW".into(), Action::MoveUp),
                ("ArrowUp".into(), Action::MoveUp),
                ("KeyS".into(), Action::MoveDown),
                ("ArrowDown".into(), Action::MoveDown),
                ("KeyA".into(), Action::MoveLeft),
                ("ArrowLeft".into(), Action::MoveLeft),
                ("KeyD".into(), Action::MoveRight),
                ("ArrowRight".into(), Action::MoveRight),
                ("Space".into(), Action::Jump),
                ("ShiftLeft".into(), Action::Sprint),
                ("KeyE".into(), Action::Interact),
                ("Escape".into(), Action::Pause),
            ],
        }
    }

    /// Graph viewer mapping.
    pub fn default_graph() -> Self {
        Self {
            bindings: vec![
                ("KeyW".into(), Action::MoveUp),
                ("ArrowUp".into(), Action::MoveUp),
                ("KeyS".into(), Action::MoveDown),
                ("ArrowDown".into(), Action::MoveDown),
                ("KeyA".into(), Action::MoveLeft),
                ("ArrowLeft".into(), Action::MoveLeft),
                ("KeyD".into(), Action::MoveRight),
                ("ArrowRight".into(), Action::MoveRight),
                ("Equal".into(), Action::ZoomIn),
                ("NumpadAdd".into(), Action::ZoomIn),
                ("Minus".into(), Action::ZoomOut),
                ("NumpadSubtract".into(), Action::ZoomOut),
            ],
        }
    }

    pub fn resolve(&self, code: &str) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(k, _)| k == code)
            .map(|(_, a)| *a)
    }
}

// ── Focus Management ──────────────────────────
// Multi-panel/window focus routing for KAMI apps (OS, pptx, xlsx, maps).

/// Unique panel/window identifier for focus routing.
pub type PanelId = u64;

/// Focus target resolution priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// Route to the specified panel/window.
    Panel(PanelId),
    /// Route to a modal overlay (blocks panel input).
    Modal(PanelId),
    /// Route to a global overlay (e.g. launcher, notification).
    GlobalOverlay,
    /// No focus target (input discarded).
    None,
}

/// Focus manager for multi-panel KAMI apps.
///
/// Tracks which panel has focus, handles modal stacking,
/// and resolves input targets with correct priority.
pub struct FocusManager {
    /// Currently focused panel (receives keyboard/mouse).
    focused: Option<PanelId>,
    /// Modal stack (LIFO — topmost modal captures all input).
    modal_stack: Vec<PanelId>,
    /// Global overlay active (launcher, notification — captures all input).
    global_overlay: bool,
}

impl FocusManager {
    /// Create with no focus.
    pub fn new() -> Self {
        Self {
            focused: None,
            modal_stack: Vec::new(),
            global_overlay: false,
        }
    }

    /// Set the focused panel.
    pub fn set_focus(&mut self, panel: PanelId) {
        self.focused = Some(panel);
    }

    /// Clear focus from a specific panel.
    pub fn clear_focus(&mut self, panel: PanelId) {
        if self.focused == Some(panel) {
            self.focused = None;
        }
    }

    /// Push a modal onto the stack (captures input until popped).
    pub fn push_modal(&mut self, panel: PanelId) {
        self.modal_stack.push(panel);
    }

    /// Pop the topmost modal. Returns the popped panel ID.
    pub fn pop_modal(&mut self) -> Option<PanelId> {
        self.modal_stack.pop()
    }

    /// Set global overlay state (e.g. launcher open).
    pub fn set_global_overlay(&mut self, active: bool) {
        self.global_overlay = active;
    }

    /// Resolve where an input event should be dispatched.
    /// Priority: global overlay > modal stack top > focused panel > none.
    pub fn resolve(&self) -> FocusTarget {
        if self.global_overlay {
            return FocusTarget::GlobalOverlay;
        }
        if let Some(&modal) = self.modal_stack.last() {
            return FocusTarget::Modal(modal);
        }
        match self.focused {
            Some(id) => FocusTarget::Panel(id),
            None => FocusTarget::None,
        }
    }

    /// Get the currently focused panel (ignoring modals/overlays).
    pub fn focused_panel(&self) -> Option<PanelId> {
        self.focused
    }

    /// Check if any modal is active.
    pub fn has_modal(&self) -> bool {
        !self.modal_stack.is_empty()
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_map() {
        let map = InputMap::default_graph();
        assert_eq!(map.resolve("KeyW"), Some(Action::MoveUp));
        assert_eq!(map.resolve("Equal"), Some(Action::ZoomIn));
        assert_eq!(map.resolve("KeyX"), None);
    }

    #[test]
    fn test_focus_manager() {
        let mut fm = FocusManager::new();
        assert_eq!(fm.resolve(), FocusTarget::None);

        fm.set_focus(42);
        assert_eq!(fm.resolve(), FocusTarget::Panel(42));

        fm.push_modal(99);
        assert_eq!(fm.resolve(), FocusTarget::Modal(99));

        fm.set_global_overlay(true);
        assert_eq!(fm.resolve(), FocusTarget::GlobalOverlay);

        fm.set_global_overlay(false);
        assert_eq!(fm.resolve(), FocusTarget::Modal(99));

        fm.pop_modal();
        assert_eq!(fm.resolve(), FocusTarget::Panel(42));
    }

    #[test]
    fn test_gesture() {
        let mut g = GestureState::default();
        g.process(&InputEvent::Touch {
            phase: TouchPhase::Start,
            id: 0,
            x: 100.0,
            y: 100.0,
            device: Device::Touch,
        });
        g.process(&InputEvent::Touch {
            phase: TouchPhase::End,
            id: 0,
            x: 102.0,
            y: 101.0,
            device: Device::Touch,
        });
        assert_eq!(g.tap_count, 1);
    }
}
