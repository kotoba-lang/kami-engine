//! Gamepad teleoperation input layer — PS5 / Switch / Xbox / Steam.
//!
//! kami-input historically carried only raw `InputEvent::Gamepad{Axis,Button}`
//! (platform `u8` indices). This module adds the controller-agnostic semantic
//! layer needed for **teleoperation**: a stable [`Pad`]/[`Axis`] vocabulary, a
//! per-family [`ControllerProfile`] (raw-index → semantic mapping + on-screen
//! glyphs + vendor detection), radial [`Deadzone`] conditioning, and a
//! poll-model [`GamepadState`] that consumers sample each frame.
//!
//! The raw index tables follow the **W3C "standard gamepad" mapping**, which the
//! browser Gamepad API normalizes PS5 / Xbox / Switch / Steam controllers onto;
//! the same tables are a sane default for native `gilrs`/SDL standard mappings.
//! Profiles therefore share the index mapping and differ in **detection**,
//! **glyphs**, and trigger semantics — exactly the bits a teleop HUD needs.
//!
//! Consumed by the `kami-teleop` crate (controller → safety-gated robot command
//! → Isaac-parity sim) and by the kami-engine-sdk browser teleop module.

use crate::InputEvent;
use glam::Vec2;

/// Semantic, controller-agnostic gamepad button.
///
/// Face buttons are named by **position** (South/East/West/North), not letter,
/// because the physical label differs per family (South = ✕ on PS5, A on Xbox,
/// B on a Switch Pro). Use [`ControllerProfile::glyph`] for the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pad {
    South,
    East,
    West,
    North,
    L1,
    R1,
    L2,
    R2,
    LStick,
    RStick,
    Select,
    Start,
    Guide,
    Share,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl Pad {
    /// All buttons, in [`Pad::index`] order.
    pub const ALL: [Pad; 18] = [
        Pad::South,
        Pad::East,
        Pad::West,
        Pad::North,
        Pad::L1,
        Pad::R1,
        Pad::L2,
        Pad::R2,
        Pad::LStick,
        Pad::RStick,
        Pad::Select,
        Pad::Start,
        Pad::Guide,
        Pad::Share,
        Pad::DpadUp,
        Pad::DpadDown,
        Pad::DpadLeft,
        Pad::DpadRight,
    ];

    /// Dense index into a `[_; 18]` state array.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Semantic analog axis. `L2`/`R2` are the analog trigger pulls (0..=1); in the
/// W3C standard mapping these arrive as `buttons[6]/[7].value`, so a
/// [`GamepadState`] stores them alongside the stick axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    L2,
    R2,
}

/// Controller family. Selects vendor detection + on-screen glyphs; the raw
/// index mapping is the shared W3C-standard table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerProfile {
    /// W3C "standard gamepad" / unknown family (generic A/B/X/Y glyphs).
    Standard,
    /// Sony DualSense (PS5). Vendor `054c`.
    Ps5DualSense,
    /// Nintendo Switch Pro Controller. Vendor `057e`.
    SwitchPro,
    /// Microsoft Xbox (One / Series). Vendor `045e`.
    XboxSeries,
    /// Valve Steam Controller / Steam Deck / Steam Input. Vendor `28de`.
    SteamInput,
}

impl ControllerProfile {
    /// Detect the family from a Gamepad API `id` string (vendor/product hints).
    ///
    /// Works on both the human-readable browser id (e.g.
    /// `"DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c ...)"`)
    /// and bare `"vvvv:pppp"` USB ids.
    pub fn detect(id: &str) -> ControllerProfile {
        let l = id.to_ascii_lowercase();
        let has = |needle: &str| l.contains(needle);
        // Vendor id (hex) takes precedence, then human-readable product names.
        if has("054c") || has("dualsense") || has("dualshock") || has("playstation") {
            ControllerProfile::Ps5DualSense
        } else if has("057e") || has("switch") || has("joy-con") || has("joycon") || has("nintendo")
        {
            ControllerProfile::SwitchPro
        } else if has("28de") || has("steam") || has("valve") {
            ControllerProfile::SteamInput
        } else if has("045e") || has("xbox") || has("xinput") || has("microsoft") {
            ControllerProfile::XboxSeries
        } else {
            ControllerProfile::Standard
        }
    }

    /// Raw button index (W3C standard mapping) → semantic [`Pad`].
    pub fn button(self, raw: u8) -> Option<Pad> {
        Some(match raw {
            0 => Pad::South,
            1 => Pad::East,
            2 => Pad::West,
            3 => Pad::North,
            4 => Pad::L1,
            5 => Pad::R1,
            6 => Pad::L2,
            7 => Pad::R2,
            8 => Pad::Select,
            9 => Pad::Start,
            10 => Pad::LStick,
            11 => Pad::RStick,
            12 => Pad::DpadUp,
            13 => Pad::DpadDown,
            14 => Pad::DpadLeft,
            15 => Pad::DpadRight,
            16 => Pad::Guide,
            17 => Pad::Share,
            _ => return None,
        })
    }

    /// Raw axis index (W3C standard mapping) → stick [`Axis`].
    pub fn stick_axis(self, raw: u8) -> Option<Axis> {
        Some(match raw {
            0 => Axis::LeftX,
            1 => Axis::LeftY,
            2 => Axis::RightX,
            3 => Axis::RightY,
            _ => return None,
        })
    }

    /// Human-readable family name (for a teleop HUD).
    pub fn label(self) -> &'static str {
        match self {
            ControllerProfile::Standard => "Standard Gamepad",
            ControllerProfile::Ps5DualSense => "PlayStation 5 DualSense",
            ControllerProfile::SwitchPro => "Nintendo Switch Pro",
            ControllerProfile::XboxSeries => "Xbox Series",
            ControllerProfile::SteamInput => "Steam Controller / Deck",
        }
    }

    /// On-screen glyph for a button under this family.
    pub fn glyph(self, b: Pad) -> &'static str {
        use ControllerProfile::*;
        use Pad::*;
        match (self, b) {
            // Face buttons differ per family (position → label).
            (Ps5DualSense, South) => "✕",
            (Ps5DualSense, East) => "○",
            (Ps5DualSense, West) => "□",
            (Ps5DualSense, North) => "△",
            (SwitchPro, South) => "B",
            (SwitchPro, East) => "A",
            (SwitchPro, West) => "Y",
            (SwitchPro, North) => "X",
            (XboxSeries, South) | (SteamInput, South) | (Standard, South) => "A",
            (XboxSeries, East) | (SteamInput, East) | (Standard, East) => "B",
            (XboxSeries, West) | (SteamInput, West) | (Standard, West) => "X",
            (XboxSeries, North) | (SteamInput, North) | (Standard, North) => "Y",
            // Shoulders / triggers.
            (Ps5DualSense, L1) => "L1",
            (Ps5DualSense, R1) => "R1",
            (Ps5DualSense, L2) => "L2",
            (Ps5DualSense, R2) => "R2",
            (SwitchPro, L1) => "L",
            (SwitchPro, R1) => "R",
            (SwitchPro, L2) => "ZL",
            (SwitchPro, R2) => "ZR",
            (_, L1) => "LB",
            (_, R1) => "RB",
            (_, L2) => "LT",
            (_, R2) => "RT",
            // Sticks.
            (_, LStick) => "L3",
            (_, RStick) => "R3",
            // System buttons.
            (Ps5DualSense, Select) => "Create",
            (Ps5DualSense, Start) => "Options",
            (Ps5DualSense, Guide) => "PS",
            (Ps5DualSense, Share) => "Mute",
            (SwitchPro, Select) => "−",
            (SwitchPro, Start) => "+",
            (SwitchPro, Guide) => "Home",
            (SwitchPro, Share) => "Capture",
            (_, Select) => "View",
            (_, Start) => "Menu",
            (_, Guide) => "Guide",
            (_, Share) => "Share",
            // D-pad.
            (_, DpadUp) => "↑",
            (_, DpadDown) => "↓",
            (_, DpadLeft) => "←",
            (_, DpadRight) => "→",
        }
    }
}

/// Radial deadzone with edge-rescaling (scaled radial deadzone — no axial bias).
#[derive(Debug, Clone, Copy)]
pub struct Deadzone {
    /// Inner radius below which input reads zero (0..1).
    pub inner: f32,
    /// Outer radius at/above which input saturates to 1 (0..1, > `inner`).
    pub outer: f32,
}

impl Default for Deadzone {
    fn default() -> Self {
        // Sensible stick defaults; DualSense/Xbox sticks rest within ~0.08.
        Deadzone { inner: 0.12, outer: 0.95 }
    }
}

impl Deadzone {
    pub fn new(inner: f32, outer: f32) -> Self {
        Deadzone { inner, outer }
    }

    /// Apply to a 2-axis stick vector, preserving direction (radial), so a
    /// diagonal does not clip to a square. Output magnitude is in 0..=1.
    pub fn radial(&self, v: Vec2) -> Vec2 {
        let m = v.length();
        if m <= self.inner {
            return Vec2::ZERO;
        }
        let span = (self.outer - self.inner).max(1e-6);
        let scaled = ((m - self.inner) / span).clamp(0.0, 1.0);
        v * (scaled / m)
    }

    /// Apply to a single analog channel (trigger / 1-D axis). Sign-preserving.
    pub fn scalar(&self, x: f32) -> f32 {
        let s = x.signum();
        let m = x.abs();
        if m <= self.inner {
            return 0.0;
        }
        let span = (self.outer - self.inner).max(1e-6);
        s * ((m - self.inner) / span).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Button {
    down: bool,
    prev: bool,
    value: f32,
}

/// Poll-model gamepad snapshot for one controller.
///
/// Fed either from raw [`InputEvent`]s ([`GamepadState::apply_event`]) or by a
/// host that already has a normalized snapshot (the browser `Gamepad` object →
/// [`GamepadState::set_button`] / [`GamepadState::set_stick`]). Consumers call
/// [`GamepadState::begin_frame`] once per tick, then sample.
#[derive(Debug, Clone)]
pub struct GamepadState {
    pub profile: ControllerProfile,
    pub connected: bool,
    buttons: [Button; 18],
    /// LeftX, LeftY, RightX, RightY.
    axes: [f32; 4],
}

impl GamepadState {
    pub fn new(profile: ControllerProfile) -> Self {
        GamepadState {
            profile,
            connected: false,
            buttons: [Button::default(); 18],
            axes: [0.0; 4],
        }
    }

    /// Latch edge-detection state. Call once at the start of each frame, before
    /// applying that frame's events / snapshot, so [`Self::just_pressed`] is
    /// well-defined.
    pub fn begin_frame(&mut self) {
        for b in &mut self.buttons {
            b.prev = b.down;
        }
    }

    /// Apply a raw kami-input gamepad event using the active profile.
    pub fn apply_event(&mut self, ev: &InputEvent) {
        match *ev {
            InputEvent::GamepadButton { button, pressed, .. } => {
                if let Some(p) = self.profile.button(button) {
                    let b = &mut self.buttons[p.index()];
                    b.down = pressed;
                    b.value = if pressed { 1.0 } else { 0.0 };
                }
                self.connected = true;
            }
            InputEvent::GamepadAxis { axis, value, .. } => {
                if let Some(a) = self.profile.stick_axis(axis) {
                    self.set_stick(a, value);
                }
                self.connected = true;
            }
            _ => {}
        }
    }

    /// Set a button directly (digital + optional analog `value`, e.g. trigger).
    pub fn set_button(&mut self, b: Pad, down: bool, value: f32) {
        let s = &mut self.buttons[b.index()];
        s.down = down;
        s.value = value;
    }

    /// Set a stick / trigger axis directly. `L2`/`R2` write the matching
    /// trigger button's analog value (and digital `down` past 0.5).
    pub fn set_stick(&mut self, a: Axis, value: f32) {
        match a {
            Axis::LeftX => self.axes[0] = value,
            Axis::LeftY => self.axes[1] = value,
            Axis::RightX => self.axes[2] = value,
            Axis::RightY => self.axes[3] = value,
            Axis::L2 => self.set_button(Pad::L2, value > 0.5, value),
            Axis::R2 => self.set_button(Pad::R2, value > 0.5, value),
        }
    }

    pub fn pressed(&self, b: Pad) -> bool {
        self.buttons[b.index()].down
    }

    /// True only on the frame the button transitioned up → down.
    pub fn just_pressed(&self, b: Pad) -> bool {
        let s = self.buttons[b.index()];
        s.down && !s.prev
    }

    /// Analog value for a button (1.0 for plain digital, pull for triggers).
    pub fn value(&self, b: Pad) -> f32 {
        self.buttons[b.index()].value
    }

    /// Raw (pre-deadzone) value for a semantic axis.
    pub fn axis(&self, a: Axis) -> f32 {
        match a {
            Axis::LeftX => self.axes[0],
            Axis::LeftY => self.axes[1],
            Axis::RightX => self.axes[2],
            Axis::RightY => self.axes[3],
            Axis::L2 => self.value(Pad::L2),
            Axis::R2 => self.value(Pad::R2),
        }
    }

    /// Raw left/right stick vectors (Gamepad-API convention: +Y is **down**).
    pub fn left_stick(&self) -> Vec2 {
        Vec2::new(self.axes[0], self.axes[1])
    }

    pub fn right_stick(&self) -> Vec2 {
        Vec2::new(self.axes[2], self.axes[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Device;

    #[test]
    fn detect_families() {
        assert_eq!(
            ControllerProfile::detect("DualSense Wireless Controller (Vendor: 054c)"),
            ControllerProfile::Ps5DualSense
        );
        assert_eq!(
            ControllerProfile::detect("Pro Controller (Vendor: 057e Product: 2009)"),
            ControllerProfile::SwitchPro
        );
        assert_eq!(
            ControllerProfile::detect("Xbox Wireless Controller (045e)"),
            ControllerProfile::XboxSeries
        );
        assert_eq!(
            ControllerProfile::detect("28de:1142 Steam Controller"),
            ControllerProfile::SteamInput
        );
        assert_eq!(ControllerProfile::detect("Generic USB Joypad"), ControllerProfile::Standard);
    }

    #[test]
    fn glyphs_swap_per_family() {
        // The "South" position is ✕ on PS5, A on Xbox, B on Switch.
        assert_eq!(ControllerProfile::Ps5DualSense.glyph(Pad::South), "✕");
        assert_eq!(ControllerProfile::XboxSeries.glyph(Pad::South), "A");
        assert_eq!(ControllerProfile::SwitchPro.glyph(Pad::South), "B");
        assert_eq!(ControllerProfile::SwitchPro.glyph(Pad::R2), "ZR");
    }

    #[test]
    fn deadzone_radial_kills_drift_and_preserves_diagonal() {
        let dz = Deadzone::new(0.1, 1.0);
        assert_eq!(dz.radial(Vec2::new(0.05, 0.05)), Vec2::ZERO);
        let out = dz.radial(Vec2::new(0.8, 0.0));
        assert!((out.x - 0.7777).abs() < 1e-3, "{out:?}");
        // Diagonal keeps direction (does not clip to a square).
        let d = dz.radial(Vec2::new(0.7, 0.7));
        assert!((d.x - d.y).abs() < 1e-5);
    }

    #[test]
    fn poll_state_from_raw_events_with_profile() {
        let mut gp = GamepadState::new(ControllerProfile::Ps5DualSense);
        gp.begin_frame();
        gp.apply_event(&InputEvent::GamepadButton { pad: 0, button: 0, pressed: true });
        gp.apply_event(&InputEvent::GamepadAxis { pad: 0, axis: 0, value: 0.9 });
        assert!(gp.connected);
        assert!(gp.pressed(Pad::South));
        assert!(gp.just_pressed(Pad::South));
        assert!((gp.axis(Axis::LeftX) - 0.9).abs() < 1e-6);

        // Next frame, still held → no longer "just pressed".
        gp.begin_frame();
        gp.apply_event(&InputEvent::GamepadButton { pad: 0, button: 0, pressed: true });
        assert!(gp.pressed(Pad::South));
        assert!(!gp.just_pressed(Pad::South));

        // Unused fields are tolerated (Device import keeps parity with lib API).
        let _ = Device::Gamepad(0);
    }
}
