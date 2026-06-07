//! Controller → command mapping.
//!
//! Maps a polled [`GamepadState`] (any of the PS5/Switch/Xbox/Steam profiles —
//! they share the W3C-standard index layout) into a [`TeleopCommand`].
//!
//! Arm button allocation (6-DOF reference, leaves room for the safety layer):
//!
//! | Input              | Effect                          |
//! |--------------------|---------------------------------|
//! | Left stick X / Y   | joint 0 (base yaw) / joint 1    |
//! | Right stick Y / X  | joint 2 / joint 3               |
//! | R2 − L2 (triggers) | joint 4 (analog ±)              |
//! | North − South      | joint 5 (±)                     |
//! | D-pad ↑ − ↓        | gripper open / close            |
//!
//! Reserved for [`crate::safety`]: `L1` (deadman, hold), `East` (e-stop),
//! `Start` (resume). No overlap with the motion bindings above.

use crate::command::{CommandKind, DriveCommand, TeleopCommand};
use kami_input::gamepad::{Axis, Deadzone, GamepadState, Pad};

/// Profile-agnostic controller→command mapper.
#[derive(Debug, Clone)]
pub struct TeleopMapper {
    /// Number of actuated DOF on the target body.
    pub dof: usize,
    /// Joint velocity (rad/s or m/s) at full stick deflection.
    pub speed: f32,
    /// Gripper rate scale.
    pub gripper_speed: f32,
    /// Stick/trigger deadzone conditioning.
    pub deadzone: Deadzone,
    /// G7 dry-run flag stamped onto produced commands (R0 = true).
    pub dry_run: bool,
}

impl TeleopMapper {
    /// Arm mapper with sensible defaults (1.5 rad/s, default deadzone, dry-run).
    pub fn for_arm(dof: usize) -> Self {
        TeleopMapper {
            dof,
            speed: 1.5,
            gripper_speed: 1.0,
            deadzone: Deadzone::default(),
            dry_run: true,
        }
    }

    fn jv(&self, jv: &mut [f32], i: usize, v: f32) {
        if i < jv.len() {
            jv[i] = v * self.speed;
        }
    }

    /// Map controller state → an articulated-arm command (joint velocities +
    /// gripper). `kind` is `Manipulate` when anything is commanded, else `Move`.
    pub fn map_arm(&self, gp: &GamepadState) -> TeleopCommand {
        let ls = self.deadzone.radial(gp.left_stick());
        let rs = self.deadzone.radial(gp.right_stick());
        let mut jv = vec![0.0_f32; self.dof];

        // Sticks → first four joints. Gamepad +Y is *down*, so negate for "up = +".
        self.jv(&mut jv, 0, ls.x);
        self.jv(&mut jv, 1, -ls.y);
        self.jv(&mut jv, 2, -rs.y);
        self.jv(&mut jv, 3, rs.x);

        // Triggers → joint 4 (analog ±).
        let trig = self.deadzone.scalar(gp.axis(Axis::R2)) - self.deadzone.scalar(gp.axis(Axis::L2));
        self.jv(&mut jv, 4, trig);

        // North/South face buttons → joint 5 (±).
        let face = btn(gp, Pad::North) - btn(gp, Pad::South);
        self.jv(&mut jv, 5, face);

        // D-pad up/down → gripper rate.
        let gripper = (btn(gp, Pad::DpadUp) - btn(gp, Pad::DpadDown)) * self.gripper_speed;

        let any = gripper.abs() > 1e-4 || jv.iter().any(|v| v.abs() > 1e-4);
        TeleopCommand {
            kind: if any { CommandKind::Manipulate } else { CommandKind::Move },
            joint_vel: jv,
            gripper,
            base: DriveCommand::default(),
            dry_run: self.dry_run,
        }
    }

    /// Map controller state → a mobile-base drive command (triggers =
    /// throttle/brake, left stick X = steer). Mirrors `kami_autodrive::Command`.
    pub fn map_base(&self, gp: &GamepadState) -> TeleopCommand {
        let base = DriveCommand {
            throttle: self.deadzone.scalar(gp.axis(Axis::R2)),
            brake: self.deadzone.scalar(gp.axis(Axis::L2)),
            steer: self.deadzone.radial(gp.left_stick()).x,
        };
        let any = base.throttle.abs() > 1e-4 || base.brake.abs() > 1e-4 || base.steer.abs() > 1e-4;
        TeleopCommand {
            kind: if any { CommandKind::Move } else { CommandKind::Halt },
            joint_vel: vec![0.0; self.dof],
            gripper: 0.0,
            base,
            dry_run: self.dry_run,
        }
    }
}

#[inline]
fn btn(gp: &GamepadState, p: Pad) -> f32 {
    if gp.pressed(p) { 1.0 } else { 0.0 }
}
