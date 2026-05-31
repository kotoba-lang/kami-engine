# kami-autodrive

Vehicle-class-agnostic **autonomy (guidance / navigation / control)** layer for
kami-engine. It is the wiring that closes the loop over the existing simulation
primitives — the same role an AV stack plays on top of NVIDIA Isaac Sim /
DRIVE Sim, but Apache/MIT, CPU-runnable, and free of any Omniverse /
commercial-GPU coupling (nv-compat target `isaacsim`).

```
lidar / camera ─▶ perception (occupancy grid) ─▶ planner (A*) ─▶
pure-pursuit + PID control ─▶ Command ─▶ plant ─▶ (new pose) ─▶ …
```

The kami pieces already existed in isolation — `kami-vehicle` (BeamNG-grade
soft-body car), `kami-sensor-sim` (lidar / camera / IMU, Isaac-Sim-API
compatible), `kami-pathfind` (A* grid). `kami-autodrive` connects them into one
closed perception → planning → control loop. See ADR-2606010600.

## Modules

| Module | Role |
|---|---|
| `types` | `Pose2` (z-up ROS REP-105 planar frame), `Command` (throttle/brake/steer/handbrake/reverse) |
| `classes` | `VehicleClass {Car, Ship, Drone, Aircraft}` → `VehicleLimits` (speed/accel/steer/turn-radius/footprint) |
| `perception` | `OccupancyGrid`: ingest lidar sweeps **and** depth-camera images → C-space-inflated costmap; `forward_clearance` for reactive braking |
| `planner` | A* over the inflated grid (via `kami-pathfind`) → line-of-sight-simplified world polyline |
| `control` | `PurePursuit` (wheelbase-decoupled via `turn_radius_ref`) + `SpeedController` (PID) + curvature speed limit |
| `plant` | `Plant` trait (the GNC↔body seam) + kinematic `BicycleModel` (with reverse) |
| `dynamics` | High-fidelity non-car plants: `ShipHydro`, `FixedWing`, `Multirotor` |
| `autopilot` | `Autopilot` + `DriveState` machine + `Telemetry` |
| `estimator` | `StateEstimator`: IMU/odometry dead-reckoning + complementary-filter correction toward sparse absolute fixes |
| `fleet` | `Fleet` / `FleetAgent`: N agents, each sensing the others, with priority right-of-way |
| `vehicle_adapter` | `SoftBodyCar` — drive a real `kami_vehicle` sedan (feature `soft-body-car`) |

## Per-class physics fidelity

| Class | Plant | Physics |
|---|---|---|
| **Car** | `kami-vehicle` soft-body (`soft-body-car`) / `BicycleModel` | Pacejka tire + full powertrain, or kinematic bicycle |
| **Ship** | `ShipHydro` | Fossen 3-DOF hydrodynamics (surge/sway/yaw, added mass, quadratic damping, speed-dependent rudder, turn-induced sway) |
| **Drone** | `Multirotor` | Rotor thrust-vector tilt + aero drag + yaw + sideslip damping (can hover) |
| **Aircraft** | `FixedWing` | Lift/drag, ISA air density, C_Lmax stall, coordinated bank-to-turn (cruise-altitude, cannot hover) |

All four share one GNC loop; each `Plant` is swappable with **zero change** to
perception/planning/control.

## Usage

```rust
use kami_autodrive::{Autopilot, AutopilotConfig, BicycleModel, Plant, Pose2, VehicleClass};
use glam::Vec2;

let start = Pose2::new(0.0, 0.0, 0.0);
let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
ap.set_goal(Vec2::new(40.0, 0.0));

let dt = 1.0 / 30.0;
loop {
    let pose = car.pose();
    let lidar = /* kami_sensor_sim ring sweep at `pose` */;
    let cmd = ap.step(pose, car.speed(), &lidar, pose, dt);
    car.step(cmd, dt);
    // ap.telemetry() → { state, distance_to_goal, cross_track_error, target_speed, … }
    if ap.state == kami_autodrive::DriveState::Arrived { break; }
}
```

Multi-modal (`step_multimodal`) fuses lidar **and** depth cameras into the same
grid each tick. `Fleet` drives many agents that sense each other.

## Capabilities

- 4 vehicle classes, one GNC loop, real per-class physics plants
- Lidar **and** depth-camera perception (multi-modal fusion)
- Static + **dynamic** obstacles (fresh-each-tick map + path-blocked replan)
- Reactive emergency braking + goal-approach deceleration + latched arrival
- Multi-agent right-of-way yielding (`Fleet`), including **heterogeneous** fleets (car + drone + ship sharing a world)
- Telemetry (state / distance-to-goal / cross-track error / target speed)
- Dead-reckoning state estimation that survives absolute-fix dropout
- Verified on a dense city street-grid, under noisy lidar, and with lidar+camera fusion

## Honest limitations (future work)

- Non-car plants are 3-DOF reduced-order, **not** 6-DOF CFD.
- Fixed-wing holds cruise altitude and **cannot hover** (it overflies a goal).
- Multi-agent is decentralised yielding, **not** cooperative negotiation; two
  agents head-on in the *same* lane can still deadlock.
- Reverse K-turn recovery exists but is **opt-in / off by default**
  (`stuck_limit = 0`); it backs out safely and gives up bounded, but does not
  yet reliably escape arbitrary tight corners.
- No camera-only reactive reflex (emergency braking uses the lidar cone).

## Governance

Per ADR-2606010600. **Simulation / design substrate only** — any real-world
deployment routes through the `wadachi` actor (ADR-2605242000): SAE L4 ceiling,
Transparent-Force gated, post-Council ratification.

## Tests

```sh
cargo test -p kami-autodrive                       # default (kinematic + dynamics + fleet + city + noise)
cargo test -p kami-autodrive --features soft-body-car   # + real soft-body sedan
cargo run  -p kami-autodrive --example drive_to_goal    # headless wall-avoidance demo
```
