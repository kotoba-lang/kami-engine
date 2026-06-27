//! Heterogeneous fleet: a car (bicycle), a drone (multirotor), and a ship
//! (hydrodynamic) share one world and reach their goals without colliding —
//! each a different `Plant`, all driven by the same Autopilot and coordinated
//! by the Fleet's priority right-of-way. Demonstrates the Plant abstraction
//! holding across vehicle classes inside multi-agent.

use glam::Vec2;
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, Fleet, FleetAgent, Multirotor, Plant, Pose2,
    ShipHydro, VehicleClass,
};

fn agent(
    plant: Box<dyn Plant>,
    class: VehicleClass,
    start: Pose2,
    goal: Vec2,
    radius: f32,
    priority: u32,
) -> FleetAgent {
    let ap = Autopilot::new(AutopilotConfig::for_class(class), start);
    FleetAgent::new(plant, ap, goal, radius, priority)
}

#[test]
fn car_drone_ship_share_a_world_collision_free() {
    use std::f32::consts::FRAC_PI_2;

    // Car (right of way) drives west→east through the crossing at (30, 0).
    let car_start = Pose2::new(0.0, 0.0, 0.0);
    let car = agent(
        Box::new(BicycleModel::new(car_start, VehicleClass::Car.limits())),
        VehicleClass::Car,
        car_start,
        Vec2::new(60.0, 0.0),
        1.0,
        2, // highest priority
    );

    // Drone crosses south→north through the same point; yields to the car.
    let drone_start = Pose2::new(30.0, -28.0, FRAC_PI_2);
    let drone = agent(
        Box::new(Multirotor::new(drone_start, VehicleClass::Drone.limits())),
        VehicleClass::Drone,
        drone_start,
        Vec2::new(30.0, 28.0),
        0.5,
        1,
    );

    // Ship cruises a parallel lane to the north — slow + large, coexisting.
    let ship_start = Pose2::new(0.0, 40.0, 0.0);
    let ship = agent(
        Box::new(ShipHydro::new(ship_start, VehicleClass::Ship.limits())),
        VehicleClass::Ship,
        ship_start,
        Vec2::new(60.0, 40.0),
        4.0,
        0, // lowest priority
    );

    let mut fleet = Fleet::new(vec![car, drone, ship]);

    let dt = 1.0 / 30.0;
    let mut min_sep = f32::INFINITY;
    let mut steps = 0;
    for _ in 0..6000 {
        min_sep = min_sep.min(fleet.min_separation());
        if fleet.all_arrived() {
            break;
        }
        fleet.step(dt);
        steps += 1;
    }

    assert!(
        fleet.all_arrived(),
        "all three classes should reach their goals (took {steps} steps; car {:?}, drone {:?}, ship {:?})",
        fleet.agents[0].pose(),
        fleet.agents[1].pose(),
        fleet.agents[2].pose(),
    );
    assert!(
        min_sep > 0.0,
        "mixed fleet collided (min separation {min_sep:.2} m)"
    );
}
