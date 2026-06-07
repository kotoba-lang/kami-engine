# kami-teleop

Teleoperation pipeline for the KAMI engine — the kami-engine **body** of the
tazuna 手綱 remote-robotics actor (etzhayyim ADR-2606042100).

```
controller (PS5 / Switch / Xbox / Steam)        ← kami-input::gamepad
        │  detect profile · deadzone · poll
        ▼
TeleopMapper        sticks/triggers/buttons → joint velocities + gripper
        ▼
SafetyEnvelope      deadman · e-stop (latched) · latency budget · dry-run (G7)
        ▼
ArmTeleopSim        velocity-integrate → joint-space PD → set_joint_efforts → step
        │            (Isaac-parity isaacsim.core.api surface, kami-genesis)
        ▼
TeleopMetrics       tracking error · effort jerk · limit margin · latency · % per SafeState
```

## Controller support

`kami_input::gamepad::ControllerProfile` detects **PS5 DualSense**, **Switch
Pro**, **Xbox Series**, and **Steam** controllers (vendor id + product name),
sharing the W3C "standard gamepad" index layout the browser Gamepad API and
native `gilrs` normalize onto. Families differ in detection + on-screen glyphs
(South = ✕ on PS5, A on Xbox, B on Switch). `Deadzone` does radial
edge-rescaling; `GamepadState` is a poll-model snapshot with edge detection.

## Example

```rust
use kami_teleop::{ArmTeleopSim, TeleopMapper};
use kami_input::gamepad::{ControllerProfile, GamepadState, Pad, Axis};

let mut sim = ArmTeleopSim::from_urdf(urdf, 1.0 / 240.0)?;
let mapper = TeleopMapper::for_arm(sim.dof());
let mut gp = GamepadState::new(ControllerProfile::Ps5DualSense);

loop {
    gp.begin_frame();
    gp.set_button(Pad::L1, true, 1.0);   // hold deadman
    gp.set_stick(Axis::LeftX, 0.8);      // command base yaw
    let report = sim.drive(&gp, &mapper, /*latency_ms*/ 30.0, /*dt*/ 1.0 / 60.0);
    assert!(report.dry_run);             // R0: plan/replay/sim only (G7)
}
let a = sim.analysis();
println!("nominal {:.0}%  mean_err {:.3}  jerk {:.3}", a.pct_nominal * 100.0, a.mean_tracking_err, a.mean_jerk);
```

## Constitutional posture (inherited from tazuna ADR-2606042100)

- **G7 outward-gated** — every command is `dry_run` at R0 (no live actuation).
- **G10 soft-RT, not a safety system** — deadman / e-stop / latency are
  best-effort; IEC 61508 hard real-time is out of scope.
- **clean-room Isaac surface** — drives only `isaacsim.core.api`-shaped calls
  (ADR-2605261800 §D10.1); no NVIDIA code linked.

The browser front of the same pipeline (Gamepad API acquisition + tazuna
`teleopCommand` framing) lives in `@etzhayyim/kami-engine-sdk/teleoperation`.

`cargo test -p kami-teleop` · `cargo test -p kami-input`
