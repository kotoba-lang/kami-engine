//! Multi-agent autonomy: several agents share a world, each sensing the others
//! as obstacles. Priority/index right-of-way decides who yields.
//!
//! Honest scope: this is decentralised yielding, not cooperative negotiation.
//! It handles a moving crossing and a stopped/parked agent. The hard case — two
//! agents head-on in the *same* lane — still needs active lane discipline /
//! negotiation (and reverse for a cornered bicycle) and is out of scope here.

use glam::Vec2;
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, Fleet, FleetAgent, Pose2, VehicleClass,
};

fn car_agent(start: Pose2, goal: Vec2, priority: u32) -> FleetAgent {
    let plant = Box::new(BicycleModel::new(start, VehicleClass::Car.limits()));
    let ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    // Sensing/collision radius kept below the car's footprint_radius (1.3) so
    // the autopilot's C-space inflation leaves a positive passing margin.
    FleetAgent::new(plant, ap, goal, 1.0, priority)
}

fn run(fleet: &mut Fleet, max_steps: usize) -> f32 {
    let dt = 1.0 / 30.0;
    let mut min_sep = f32::INFINITY;
    for _ in 0..max_steps {
        min_sep = min_sep.min(fleet.min_separation());
        if fleet.all_arrived() {
            break;
        }
        fleet.step(dt);
    }
    min_sep
}

#[test]
fn perpendicular_crossing_stays_collision_free() {
    // A crosses west→east, B south→north, paths intersecting near (20, 0). B
    // (index 1) yields to A (index 0): two moving agents coordinate at a
    // shared point without colliding.
    let a = car_agent(Pose2::new(0.0, 0.0, 0.0), Vec2::new(40.0, 0.0), 0);
    let b = car_agent(
        Pose2::new(20.0, -22.0, std::f32::consts::FRAC_PI_2),
        Vec2::new(20.0, 22.0),
        0,
    );
    let mut fleet = Fleet::new(vec![a, b]);
    let min_sep = run(&mut fleet, 3000);

    assert!(
        fleet.all_arrived(),
        "both crossing agents should reach their goals"
    );
    assert!(
        min_sep > 0.0,
        "crossing agents collided (min separation {min_sep:.2} m)"
    );
}

#[test]
fn head_on_lane_discipline_passes_without_collision() {
    // Two agents swap positions along the same line (a head-on conflict that
    // previously collided). Lane discipline makes both bias right, so they pass
    // right-to-right and both reach their goals.
    let a = car_agent(Pose2::new(0.0, 0.0, 0.0), Vec2::new(40.0, 0.0), 0);
    let b = car_agent(
        Pose2::new(40.0, 0.0, std::f32::consts::PI),
        Vec2::new(0.0, 0.0),
        0,
    );
    let mut fleet = Fleet::new(vec![a, b]);
    let min_sep = run(&mut fleet, 3000);

    assert!(
        fleet.all_arrived(),
        "both head-on agents should pass and arrive"
    );
    assert!(
        min_sep > 0.0,
        "lane discipline should avoid the collision (min sep {min_sep:.2} m)"
    );
}

#[test]
fn four_way_intersection_resolves_collision_free() {
    use std::f32::consts::{FRAC_PI_2, PI};
    // Four cars converge on the origin from N/S/E/W, each crossing to the far
    // side — head-on pairs AND perpendicular crossings at one point. Universal
    // collision avoidance (everyone keeps clear) + lane discipline lets all
    // four clear without overlap.
    let agents = vec![
        car_agent(Pose2::new(-30.0, 0.0, 0.0), Vec2::new(30.0, 0.0), 0),
        car_agent(Pose2::new(0.0, -30.0, FRAC_PI_2), Vec2::new(0.0, 30.0), 1),
        car_agent(Pose2::new(30.0, 0.0, PI), Vec2::new(-30.0, 0.0), 2),
        car_agent(Pose2::new(0.0, 30.0, -FRAC_PI_2), Vec2::new(0.0, -30.0), 3),
    ];
    let mut fleet = Fleet::new(agents);
    let min_sep = run(&mut fleet, 4000);

    assert!(
        fleet.all_arrived(),
        "all four should clear the intersection"
    );
    assert!(
        min_sep > 0.0,
        "4-way must stay collision-free (min sep {min_sep:.2} m)"
    );
}

#[test]
fn overtakes_a_parked_agent_on_the_path() {
    // B is parked squarely on A's straight line and has the right of way; the
    // moving A (lower priority) must route around it.
    let b = car_agent(Pose2::new(20.0, 0.0, 0.0), Vec2::new(20.0, 0.0), 1); // parked, right of way
    let a = car_agent(Pose2::new(0.0, 0.0, 0.0), Vec2::new(40.0, 0.0), 0); // yields to B
    let mut fleet = Fleet::new(vec![b, a]);
    let min_sep = run(&mut fleet, 2000);

    assert!(
        fleet.all_arrived(),
        "the moving agent should route past the parked one"
    );
    assert!(
        min_sep > 0.0,
        "agents overlapped (min separation {min_sep:.2} m)"
    );
    // Proof it detoured rather than stopping short: the mover passed the parked
    // agent's x with a lateral offset.
    let mover = fleet.agents[1].pose();
    assert!(
        mover.x > 30.0,
        "mover should have passed the parked agent (x={:.1})",
        mover.x
    );
}
