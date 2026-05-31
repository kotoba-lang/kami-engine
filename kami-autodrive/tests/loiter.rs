//! Fixed-wing loiter: a non-stopping aircraft can't capture a point inside its
//! turn radius (previously it spiralled into a wide, wrongly-centred orbit and
//! drifted away). It now flies to the waypoint and loiters over it — reaching
//! "on station" and holding there indefinitely.

use glam::Vec2;
use kami_autodrive::{
    Autopilot, AutopilotConfig, DriveState, FixedWing, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::LidarReturn;

const NO_LIDAR: [LidarReturn; 0] = [];

#[test]
fn fixed_wing_loiters_over_a_distant_waypoint() {
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(2500.0, 2000.0); // km-scale, realistic for a fixed-wing
    let mut plane = FixedWing::new(start, 500.0, VehicleClass::Aircraft.limits());
    let stall = plane.stall_speed();
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Aircraft), start);
    let loiter_r = ap.cfg.loiter_radius.unwrap();
    ap.set_goal(goal);

    // Ingress until established on station.
    let mut arrived = false;
    let mut min_airspeed = f32::INFINITY;
    for _ in 0..6000 {
        min_airspeed = min_airspeed.min(plane.airspeed);
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }
        // Loitering passes through DriveState::Loitering on the way in.
        let pose = plane.pose();
        let cmd = ap.step(pose, plane.speed(), &NO_LIDAR, pose, dt);
        plane.step(cmd, dt);
    }
    assert!(arrived, "aircraft should reach the loiter station");
    assert!(min_airspeed > stall, "must stay above stall ({stall:.0} m/s), dipped to {min_airspeed:.0}");

    // Hold station: keep flying and confirm it ORBITS the goal (stays within a
    // couple of turn radii) rather than drifting off to infinity.
    let mut max_dist = 0.0f32;
    for _ in 0..1200 {
        let pose = plane.pose();
        max_dist = max_dist.max(pose.pos().distance(goal));
        let cmd = ap.step(pose, plane.speed(), &NO_LIDAR, pose, dt);
        plane.step(cmd, dt);
    }
    assert!(
        max_dist < 3.0 * loiter_r,
        "should hold station near the waypoint (max dist {max_dist:.0} m vs loiter r {loiter_r:.0})"
    );
}

#[test]
fn stopping_vehicles_do_not_loiter() {
    // A car has no loiter radius — it captures and stops at the point.
    let cfg = AutopilotConfig::for_class(VehicleClass::Car);
    assert!(cfg.loiter_radius.is_none());
    let cfg_ship = AutopilotConfig::for_class(VehicleClass::Ship);
    assert!(cfg_ship.loiter_radius.is_none());
    let cfg_air = AutopilotConfig::for_class(VehicleClass::Aircraft);
    assert!(cfg_air.loiter_radius.is_some(), "aircraft loiters");
}
