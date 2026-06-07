//! kami-teleop — remote-operation pipeline for the KAMI engine.
//!
//! Controller (PS5 / Switch / Xbox / Steam, via [`kami_input::gamepad`]) →
//! [`TeleopMapper`] → [`SafetyEnvelope`] (deadman / e-stop / latency, dry-run by
//! default) → joint-velocity integration + joint-space PD → Isaac-parity
//! `kami-genesis` simulation → [`TeleopMetrics`] analysis.
//!
//! This is the kami-engine **body** of the tazuna 手綱 remote-robotics actor
//! (etzhayyim ADR-2606042100): the clean-room teleoperation + analysis layer the
//! actor's `teleop_session` cell drives. Constitutional posture inherited:
//!
//! - **G7 outward-gated** — every command is `dry_run` at R0 (plan/replay/sim
//!   only; no live actuation). Live actuation is Council Lv6+ gated upstream.
//! - **G10 soft-RT, not a safety system** — deadman / e-stop / latency budget
//!   are *best-effort* ([`safety`]); hard real-time (IEC 61508) is out of scope.
//! - **clean-room Isaac surface** — drives only `isaacsim.core.api`-shaped calls
//!   (ADR-2605261800 §D10.1); no NVIDIA code linked.
//!
//! # Example
//!
//! ```no_run
//! use kami_teleop::{ArmTeleopSim, TeleopMapper};
//! use kami_input::gamepad::{ControllerProfile, GamepadState, Pad, Axis};
//!
//! # let urdf = "";
//! let mut sim = ArmTeleopSim::from_urdf(urdf, 1.0 / 240.0).unwrap();
//! let mapper = TeleopMapper::for_arm(sim.dof());
//! let mut gp = GamepadState::new(ControllerProfile::Ps5DualSense);
//!
//! for _ in 0..600 {
//!     gp.begin_frame();
//!     gp.set_button(Pad::L1, true, 1.0);     // hold deadman
//!     gp.set_stick(Axis::LeftX, 0.8);        // command base yaw
//!     let report = sim.drive(&gp, &mapper, /*latency_ms*/ 30.0, /*dt*/ 1.0 / 60.0);
//!     assert!(report.dry_run);
//! }
//! let a = sim.analysis();
//! println!("nominal={:.0}% mean_err={:.3}", a.pct_nominal * 100.0, a.mean_tracking_err);
//! ```

pub mod command;
pub mod mapping;
pub mod metrics;
pub mod safety;
pub mod sim;

pub use command::{CommandKind, DriveCommand, TeleopCommand};
pub use mapping::TeleopMapper;
pub use metrics::{TeleopAnalysis, TeleopMetrics};
pub use safety::{SafeState, SafetyConfig, SafetyEnvelope};
pub use sim::{ArmTeleopSim, TeleopError, TickReport};

/// etzhayyim ADR governing the tazuna remote-robotics teleop actor.
pub const ADR: &str = "ADR-2606042100";

#[cfg(test)]
mod tests {
    use super::*;
    use kami_input::gamepad::{Axis, ControllerProfile, GamepadState, Pad};

    const ARM6_URDF: &str = include_str!("../../fixtures/giemon_arm6/giemon_arm6.urdf");

    fn sim() -> ArmTeleopSim {
        ArmTeleopSim::from_urdf(ARM6_URDF, 1.0 / 240.0).expect("build arm6 sim")
    }

    #[test]
    fn loads_six_dof_arm() {
        let s = sim();
        assert_eq!(s.dof(), 6);
        assert!(s.end_effector_pose().is_some());
    }

    /// Drive the base yaw (gravity-neutral DOF 0) with `stick_x` for `n` ticks
    /// under a held deadman; return the final DOF-0 angle + the session analysis.
    fn drive_base(stick_x: f32, n: usize) -> (f32, TeleopAnalysis) {
        let mut s = sim();
        let mapper = TeleopMapper::for_arm(s.dof());
        let mut gp = GamepadState::new(ControllerProfile::Ps5DualSense);
        for _ in 0..n {
            gp.begin_frame();
            gp.set_button(Pad::L1, true, 1.0); // deadman held
            gp.set_stick(Axis::LeftX, stick_x);
            let r = s.drive(&gp, &mapper, 20.0, 1.0 / 60.0);
            assert_eq!(r.state, SafeState::Nominal);
            assert!(r.dry_run, "R0 must be dry-run");
        }
        (s.joint_positions()[0], s.analysis())
    }

    #[test]
    fn deadman_held_drives_base_joint_nominally() {
        let (q1, a) = drive_base(1.0, 200);
        // Substantial motion of the commanded joint.
        assert!(q1.abs() > 0.3, "base joint barely moved: {q1}");
        assert_eq!(a.ticks, 200);
        assert!((a.pct_nominal - 1.0).abs() < 1e-6);
        assert_eq!(a.estop_count, 0);
        assert!(a.min_limit_margin >= 0.0);
        // Velocity-teleop tracking error stays bounded by the lookahead window.
        assert!(a.mean_tracking_err < 0.6, "tracking error too large: {}", a.mean_tracking_err);
    }

    #[test]
    fn command_direction_controls_motion() {
        // Reversing the stick must reverse the joint's motion (controller is in
        // charge of direction, whatever the engine's absolute sign convention).
        let (q_plus, _) = drive_base(1.0, 120);
        let (q_minus, _) = drive_base(-1.0, 120);
        assert!(
            q_plus * q_minus < 0.0,
            "command sign did not reverse motion: +{q_plus} / -{q_minus}"
        );
    }

    #[test]
    fn deadman_lapse_zeros_motion() {
        let mut s = sim();
        let mapper = TeleopMapper::for_arm(s.dof());
        let mut gp = GamepadState::new(ControllerProfile::XboxSeries);
        let q0 = s.joint_positions()[0];

        for _ in 0..120 {
            gp.begin_frame();
            // deadman NOT held
            gp.set_stick(Axis::LeftX, 1.0);
            let r = s.drive(&gp, &mapper, 20.0, 1.0 / 60.0);
            assert_eq!(r.state, SafeState::DeadmanLapse);
        }
        let q1 = s.joint_positions()[0];
        assert!((q1 - q0).abs() < 1e-2, "moved despite deadman lapse: {q0} -> {q1}");
        assert!((s.analysis().pct_deadman_lapse - 1.0).abs() < 1e-6);
    }

    #[test]
    fn estop_latches_and_resumes() {
        let mut s = sim();
        let mapper = TeleopMapper::for_arm(s.dof());
        let mut gp = GamepadState::new(ControllerProfile::SwitchPro);

        // Tick 0..5 nominal (deadman held).
        for _ in 0..5 {
            gp.begin_frame();
            gp.set_button(Pad::L1, true, 1.0);
            let r = s.drive(&gp, &mapper, 20.0, 1.0 / 60.0);
            assert_eq!(r.state, SafeState::Nominal);
        }
        // Press e-stop (East) once → latches.
        gp.begin_frame();
        gp.set_button(Pad::L1, true, 1.0);
        gp.set_button(Pad::East, true, 1.0);
        let r = s.drive(&gp, &mapper, 20.0, 1.0 / 60.0);
        assert_eq!(r.state, SafeState::Estopped);
        assert!(s.safety().is_estopped());

        // Release East but stay latched.
        gp.begin_frame();
        gp.set_button(Pad::L1, true, 1.0);
        gp.set_button(Pad::East, false, 0.0);
        let r = s.drive(&gp, &mapper, 20.0, 1.0 / 60.0);
        assert_eq!(r.state, SafeState::Estopped);

        // Press resume (Start) → clears latch.
        gp.begin_frame();
        gp.set_button(Pad::L1, true, 1.0);
        gp.set_button(Pad::Start, true, 1.0);
        let r = s.drive(&gp, &mapper, 20.0, 1.0 / 60.0);
        assert_eq!(r.state, SafeState::Nominal);
        assert!(!s.safety().is_estopped());

        assert_eq!(s.analysis().estop_count, 1);
    }

    #[test]
    fn latency_breach_halts() {
        let mut s = sim();
        let mapper = TeleopMapper::for_arm(s.dof());
        let mut gp = GamepadState::new(ControllerProfile::SteamInput);
        gp.begin_frame();
        gp.set_button(Pad::L1, true, 1.0);
        gp.set_stick(Axis::LeftX, 1.0);
        // 500 ms > 250 ms budget → LatencyBreach.
        let r = s.drive(&gp, &mapper, 500.0, 1.0 / 60.0);
        assert_eq!(r.state, SafeState::LatencyBreach);
    }

    #[test]
    fn profiles_share_mapping() {
        // PS5 and Switch must map identical raw input to identical motion.
        let drive_one = |profile| {
            let mut s = sim();
            let mapper = TeleopMapper::for_arm(s.dof());
            let mut gp = GamepadState::new(profile);
            for _ in 0..30 {
                gp.begin_frame();
                gp.set_button(Pad::L1, true, 1.0);
                gp.set_stick(Axis::LeftX, 1.0);
                s.drive(&gp, &mapper, 10.0, 1.0 / 60.0);
            }
            s.joint_positions()[0]
        };
        let ps5 = drive_one(ControllerProfile::Ps5DualSense);
        let switch = drive_one(ControllerProfile::SwitchPro);
        assert!((ps5 - switch).abs() < 1e-5, "profiles diverged: {ps5} vs {switch}");
    }
}
