//! High-fidelity car test: the same `Autopilot` drives a `kami-vehicle`
//! BeamNG-grade soft-body sedan to a waypoint.
//!
//! Run with: `cargo test -p kami-autodrive --features soft-body-car`
#![cfg(feature = "soft-body-car")]

use kami_autodrive::vehicle_adapter::SoftBodyCar;
use kami_autodrive::{Autopilot, AutopilotConfig, DriveState, Plant, VehicleClass};
use kami_sensor_sim::LidarReturn;
use kami_vehicle::models::garage::VehicleKind;

#[test]
fn soft_body_sedan_drives_to_waypoint() {
    let mut car = SoftBodyCar::new(VehicleKind::Sedan, 0.0);
    let start = car.pose();

    // Goal 18 m straight ahead in the car's initial heading.
    let fwd = start.forward();
    let goal = start.pos() + fwd * 18.0;

    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let dt = 1.0 / 60.0;
    let empty: [LidarReturn; 0] = []; // open ground, no obstacles
    let mut moved = 0.0f32;
    for _ in 0..900 {
        let pose = car.pose();
        moved = pose.pos().distance(start.pos());
        if ap.state == DriveState::Arrived {
            break;
        }
        let cmd = ap.step(pose, car.speed(), &empty, pose, dt);
        car.step(cmd, dt);
    }

    let final_pose = car.pose();
    let dist_to_goal = final_pose.pos().distance(goal);
    // The soft-body car must actuate and make real forward progress toward the
    // goal — proving Command flows into the powertrain and pose feeds back.
    assert!(
        moved > 4.0,
        "soft-body car should travel several metres (moved {:.2} m)",
        moved
    );
    assert!(
        dist_to_goal < start.pos().distance(goal),
        "should end closer to the goal than it started (d0={:.1}, d1={:.1})",
        start.pos().distance(goal),
        dist_to_goal
    );
}
