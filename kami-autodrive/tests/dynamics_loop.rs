//! Closed-loop tests for the high-fidelity non-car plants: the same
//! `Autopilot` drives a hydrodynamic ship, an aerodynamic fixed-wing, and a
//! rotor-thrust multirotor to their goals, and we assert physically meaningful
//! behaviour (turn-induced sway, above-stall flight, body tilt).

use glam::Vec2;
use kami_autodrive::{
    Autopilot, AutopilotConfig, DriveState, FixedWing, Multirotor, Plant, Pose2, ShipHydro,
    VehicleClass,
};
use kami_sensor_sim::LidarReturn;

const NO_OBSTACLES: [LidarReturn; 0] = [];

/// Drive a `Plant` to `goal` over open ground/water/air with `class` limits.
/// Returns (min distance to goal achieved, arrived flag, steps taken).
fn drive<P: Plant>(
    plant: &mut P,
    class: VehicleClass,
    goal: Vec2,
    dt: f32,
    max_steps: usize,
    mut on_step: impl FnMut(&P),
) -> (f32, bool, usize) {
    let start = plant.pose();
    let mut ap = Autopilot::new(AutopilotConfig::for_class(class), start);
    ap.set_goal(goal);
    let mut min_d = f32::INFINITY;
    for step in 0..max_steps {
        let pose = plant.pose();
        min_d = min_d.min(pose.pos().distance(goal));
        if ap.state == DriveState::Arrived {
            return (min_d, true, step);
        }
        let cmd = ap.step(pose, plant.speed(), &NO_OBSTACLES, pose, dt);
        plant.step(cmd, dt);
        on_step(plant);
    }
    (min_d, ap.state == DriveState::Arrived, max_steps)
}

#[test]
fn ship_hydrodynamics_turns_and_arrives() {
    let dt = 1.0 / 20.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut ship = ShipHydro::new(start, VehicleClass::Ship.limits());

    // Goal off to port forces a real turn (and thus sway coupling).
    let goal = Vec2::new(80.0, 40.0);
    let mut max_sway = 0.0f32;
    let (min_d, arrived, _steps) = drive(&mut ship, VehicleClass::Ship, goal, dt, 4000, |s| {
        max_sway = max_sway.max(s.v.abs())
    });

    assert!(arrived, "ship should arrive (closest {:.1} m)", min_d);
    // The surge↔yaw Coriolis coupling must induce measurable outward sway in
    // the turn — proof the hydrodynamic model (not a kinematic stand-in) runs.
    assert!(
        max_sway > 0.05,
        "turn should induce hydrodynamic sway (max |v| = {:.3} m/s)",
        max_sway
    );
}

#[test]
fn fixed_wing_flies_above_stall_to_goal() {
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut plane = FixedWing::new(start, 500.0, VehicleClass::Aircraft.limits());
    let stall = plane.stall_speed();

    // A long leg with a gentle lateral offset: the aircraft banks to turn onto
    // it. A fixed-wing cannot hit a point after a sharp turn, so the leg is
    // shallow enough to track and we count a fly-through.
    let goal = Vec2::new(600.0, 60.0);
    let mut min_airspeed = f32::INFINITY;
    let (min_d, arrived, _steps) = drive(&mut plane, VehicleClass::Aircraft, goal, dt, 4000, |p| {
        min_airspeed = min_airspeed.min(p.airspeed)
    });

    // Fixed-wing can't hover — count a fly-through within tolerance as success.
    assert!(
        arrived || min_d < 25.0,
        "aircraft should overfly the goal (closest {:.1} m)",
        min_d
    );
    assert!(
        min_airspeed > 0.9 * stall,
        "must stay above stall ({:.1} m/s); dipped to {:.1}",
        stall,
        min_airspeed
    );
}

#[test]
fn multirotor_tilts_to_translate_and_hovers_at_goal() {
    let dt = 1.0 / 50.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut drone = Multirotor::new(start, VehicleClass::Drone.limits());

    let goal = Vec2::new(30.0, 18.0);
    let mut max_tilt = 0.0f32;
    let (min_d, arrived, _steps) = drive(&mut drone, VehicleClass::Drone, goal, dt, 4000, |d| {
        max_tilt = max_tilt.max(d.tilt.abs())
    });

    assert!(
        arrived,
        "drone should reach and hold the goal (closest {:.1} m)",
        min_d
    );
    // Translation must come from real thrust-vector tilt.
    assert!(
        max_tilt > 0.05,
        "drone should tilt to translate (max tilt = {:.3} rad)",
        max_tilt
    );
    // It can decelerate to a near-hover at the goal (unlike the fixed-wing).
    assert!(
        drone.speed() < 3.0,
        "should be near hover at arrival (v = {:.2})",
        drone.speed()
    );
}
