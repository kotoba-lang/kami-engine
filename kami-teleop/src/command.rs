//! Controller-agnostic, body-agnostic teleoperation command.
//!
//! The command vocabulary mirrors the tazuna `teleopCommand` lexicon
//! (etzhayyim ADR-2606042100): the [`CommandKind`] enum is the on-chain-anchored
//! command kind, and `dry_run` is the G7 outward gate (R0 = `true`: plan/replay
//! only, never live actuation).

/// Command kind — mirrors tazuna `teleopCommand.kind`
/// (`move` / `manipulate` / `halt` / `estop` / `handback`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    /// Base / locomotion motion.
    Move,
    /// Articulated manipulation (arm joints / gripper).
    Manipulate,
    /// Controlled stop (hold pose, zero velocity).
    Halt,
    /// Emergency stop (latched; requires explicit resume).
    Estop,
    /// Hand control back to supervised autonomy.
    Handback,
}

/// Mobile-base drive command. Field shape mirrors `kami_autodrive::Command`
/// (throttle/brake/steer) so a teleop base command can drop into the same plant.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DriveCommand {
    /// Accelerator, 0..=1.
    pub throttle: f32,
    /// Foot brake, 0..=1.
    pub brake: f32,
    /// Steering, -1..=1 (positive = left/CCW).
    pub steer: f32,
}

/// A single normalized teleoperation command for one tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TeleopCommand {
    pub kind: CommandKind,
    /// Per-DOF joint **velocity** target (rad/s for revolute, m/s for prismatic).
    pub joint_vel: Vec<f32>,
    /// Gripper rate, -1 (close) ..= +1 (open).
    pub gripper: f32,
    /// Mobile-base command (for `Move` on a wheeled/tracked body).
    pub base: DriveCommand,
    /// G7 outward gate — `true` = plan/replay only (no live actuation). R0 = true.
    pub dry_run: bool,
}

impl TeleopCommand {
    /// A zeroed command with the given kind and DOF.
    pub fn zeroed(kind: CommandKind, dof: usize) -> Self {
        TeleopCommand {
            kind,
            joint_vel: vec![0.0; dof],
            gripper: 0.0,
            base: DriveCommand::default(),
            dry_run: true,
        }
    }

    /// Controlled stop (hold pose).
    pub fn halt(dof: usize) -> Self {
        Self::zeroed(CommandKind::Halt, dof)
    }

    /// Latched emergency stop.
    pub fn estop(dof: usize) -> Self {
        Self::zeroed(CommandKind::Estop, dof)
    }

    /// True if no joint, gripper, or base motion is commanded.
    pub fn is_zero(&self) -> bool {
        const EPS: f32 = 1e-4;
        self.gripper.abs() < EPS
            && self.base.throttle.abs() < EPS
            && self.base.brake.abs() < EPS
            && self.base.steer.abs() < EPS
            && self.joint_vel.iter().all(|v| v.abs() < EPS)
    }
}
