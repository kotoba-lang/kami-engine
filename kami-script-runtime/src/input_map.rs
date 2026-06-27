//! input_map — device-neutral input mapping (ADR-0037 seam #3).
//!
//! The game only ever asks the abstract `kami:engine/input` surface for *named*
//! actions: `(axis "MoveX")`, `(key-down? "Fire")`. Each platform's raw devices
//! — touch sticks (iOS / Android), DualSense (PS5), Joy-Con / Pro (Switch), MFi
//! (iOS) — are translated into that surface **here**, in pure Rust, so the same
//! `.clj` runs on all of them unchanged. This module has no platform deps and is
//! fully unit-tested; it is shared by every non-keyboard host (Steps 3/4/5), not
//! iOS-specific.
//!
//! The two device shapes a non-keyboard platform needs:
//!   - [`VirtualStick`] — an on-screen thumbstick fed a raw touch point.
//!   - [`apply_dead_zone`] — radial dead zone + clamp for a physical analog stick.
//!
//! Both produce an `[x, y]` pair in `[-1, 1]` with **y up** (screen-y grows down,
//! so it is negated), ready to drop into two named axes via
//! [`crate::KamiScriptRuntime::feed_stick`].

/// A circular on-screen thumbstick. A touch within `radius` of `center` becomes
/// a clamped `[-1, 1]` axis pair; touches inside `dead_zone` (a fraction of the
/// radius) read as zero so a resting thumb does not drift the character.
#[derive(Debug, Clone, Copy)]
pub struct VirtualStick {
    /// Stick centre in the same screen-space coordinates as the touch points.
    pub center: [f32; 2],
    /// Travel radius in pixels; a touch this far out reads as full deflection.
    pub radius: f32,
    /// Dead-zone radius as a fraction of `radius` (e.g. `0.15`), in `[0, 1)`.
    pub dead_zone: f32,
}

impl VirtualStick {
    /// A stick centred at `center` with the given travel `radius` and a 15%
    /// dead zone — a sane default for a touch thumbstick.
    pub fn new(center: [f32; 2], radius: f32) -> Self {
        Self {
            center,
            radius,
            dead_zone: 0.15,
        }
    }

    /// Map an active touch point to `[x, y]` in `[-1, 1]`, **y up**.
    ///
    /// Returns `[0, 0]` inside the dead zone. Beyond `radius` the magnitude
    /// clamps to 1 (further travel doesn't over-drive the axis). Output past the
    /// dead zone is rescaled so the usable range starts cleanly at 0, giving
    /// smooth control right at the dead-zone edge instead of a jump.
    pub fn axes(&self, touch: [f32; 2]) -> [f32; 2] {
        let dx = touch[0] - self.center[0];
        let dy = touch[1] - self.center[1];
        let r = if self.radius > f32::EPSILON {
            self.radius
        } else {
            1.0
        };
        let mag = (dx * dx + dy * dy).sqrt();
        let dead = self.dead_zone.clamp(0.0, 0.999) * r;
        if mag <= dead {
            return [0.0, 0.0];
        }
        // Rescale (dead .. r) → (0 .. 1) along the touch direction, clamp to 1.
        let scaled = ((mag - dead) / (r - dead)).min(1.0);
        let inv = scaled / mag; // unit direction × scaled magnitude
        [dx * inv, -dy * inv] // negate y: screen-down → stick-up
    }
}

/// Radial dead zone + clamp for a physical analog stick (gamepad / MFi /
/// DualSense / Joy-Con). Input and output are `[-1, 1]` per axis with **y up**
/// already (gamepad APIs report up as positive); we only gate the dead zone and
/// clamp the magnitude to the unit circle. Inside `dead` (a fraction in `[0,1)`)
/// the result is `[0, 0]`; past it the value is rescaled so control starts at 0.
pub fn apply_dead_zone(x: f32, y: f32, dead: f32) -> [f32; 2] {
    let mag = (x * x + y * y).sqrt();
    let dead = dead.clamp(0.0, 0.999);
    if mag <= dead {
        return [0.0, 0.0];
    }
    let scaled = ((mag - dead) / (1.0 - dead)).min(1.0);
    let inv = scaled / mag;
    [x * inv, y * inv]
}

/// The press/release edges for one frame, as abstract action names (sorted for
/// stable iteration). Produced by [`ButtonEdges::update`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Edges {
    /// Actions whose button went down THIS frame (the `key-pressed?` edge).
    pub pressed: Vec<String>,
    /// Actions whose button went up this frame.
    pub released: Vec<String>,
}

/// Edge detector for buttons, keyed by abstract action name. Every non-keyboard
/// platform reports a *held* set each frame (DualSense cross, Joy-Con / Pro
/// buttons, MFi buttons, touch-tap zones); the guest, though, also wants the
/// down-edge for `(key-pressed? "Jump")`. This computes that edge host-side so
/// `key-down?` (level) and `key-pressed?` (edge) read identically on every
/// target. Pure state, no platform deps.
#[derive(Debug, Default)]
pub struct ButtonEdges {
    prev: std::collections::HashSet<String>,
}

impl ButtonEdges {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the actions whose buttons are held this frame; get back which are
    /// newly pressed (down this frame, not last) and newly released. Updates the
    /// internal previous-frame set.
    pub fn update(&mut self, held: &[&str]) -> Edges {
        let cur: std::collections::HashSet<String> = held.iter().map(|s| s.to_string()).collect();
        let mut pressed: Vec<String> = cur.difference(&self.prev).cloned().collect();
        let mut released: Vec<String> = self.prev.difference(&cur).cloned().collect();
        pressed.sort();
        released.sort();
        self.prev = cur;
        Edges { pressed, released }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn button_press_is_an_edge() {
        let mut e = ButtonEdges::new();
        // First frame held → pressed edge.
        assert_eq!(e.update(&["Jump"]).pressed, vec!["Jump".to_string()]);
        // Still held → no new press.
        assert_eq!(e.update(&["Jump"]), Edges::default());
        // Released → release edge, no press.
        assert_eq!(e.update(&[]).released, vec!["Jump".to_string()]);
    }

    #[test]
    fn button_edges_track_multiple_actions() {
        let mut e = ButtonEdges::new();
        let f0 = e.update(&["Fire"]);
        assert_eq!(f0.pressed, vec!["Fire".to_string()]);
        // Fire stays held, Jump newly pressed → only Jump is a new edge.
        let f1 = e.update(&["Fire", "Jump"]);
        assert_eq!(f1.pressed, vec!["Jump".to_string()]);
        assert!(f1.released.is_empty());
        // Drop Fire, keep Jump → Fire releases, no new press.
        let f2 = e.update(&["Jump"]);
        assert_eq!(f2.released, vec!["Fire".to_string()]);
        assert!(f2.pressed.is_empty());
    }

    #[test]
    fn stick_center_is_zero() {
        let s = VirtualStick::new([100.0, 100.0], 50.0);
        assert_eq!(s.axes([100.0, 100.0]), [0.0, 0.0]);
    }

    #[test]
    fn stick_dead_zone_reads_zero() {
        let s = VirtualStick::new([100.0, 100.0], 50.0); // dead zone = 7.5px
        assert_eq!(s.axes([104.0, 100.0]), [0.0, 0.0], "inside dead zone");
    }

    #[test]
    fn stick_full_right_is_plus_x() {
        let s = VirtualStick::new([100.0, 100.0], 50.0);
        let a = s.axes([150.0, 100.0]); // exactly radius to the right
        assert!(close(a[0], 1.0) && close(a[1], 0.0), "got {a:?}");
    }

    #[test]
    fn stick_up_is_plus_y() {
        // Touch ABOVE centre (smaller screen-y) must read as +y (stick up).
        let s = VirtualStick::new([100.0, 100.0], 50.0);
        let a = s.axes([100.0, 50.0]);
        assert!(close(a[0], 0.0) && close(a[1], 1.0), "got {a:?}");
    }

    #[test]
    fn stick_clamps_beyond_radius() {
        let s = VirtualStick::new([100.0, 100.0], 50.0);
        let a = s.axes([300.0, 100.0]); // way past radius
        assert!(close(a[0], 1.0), "magnitude clamps to 1, got {a:?}");
    }

    #[test]
    fn dead_zone_gates_small_input() {
        assert_eq!(apply_dead_zone(0.05, 0.0, 0.1), [0.0, 0.0]);
        let a = apply_dead_zone(1.0, 0.0, 0.1);
        assert!(close(a[0], 1.0) && close(a[1], 0.0), "got {a:?}");
    }

    #[test]
    fn dead_zone_clamps_to_unit_circle() {
        let a = apply_dead_zone(1.0, 1.0, 0.0); // magnitude √2 > 1
        let m = (a[0] * a[0] + a[1] * a[1]).sqrt();
        assert!(close(m, 1.0), "clamped to unit circle, got mag {m}");
    }
}
