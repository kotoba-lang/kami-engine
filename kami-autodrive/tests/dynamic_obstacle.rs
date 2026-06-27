//! Dynamic-obstacle avoidance: an autonomous car yields to a box that crosses
//! its path, then proceeds to the goal once it clears. Exercises the
//! fresh-each-tick occupancy map + reactive emergency stop (a persistent map
//! would smear the moving box into a permanent wall across the corridor and the
//! car would never recover).

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const MOUNT_Z: f32 = 1.0;
const BOX_HALF: f32 = 2.0;

fn sweep(scene: &Scene, pose: Pose2) -> Vec<LidarReturn> {
    let intr = LidarIntrinsics {
        hfov: std::f32::consts::TAU,
        vfov: 0.05,
        h_beams: 240,
        v_beams: 1,
        range_min: 0.2,
        range_max: 80.0,
    };
    let mut lidar = Lidar::new("ring", "/lidar", intr);
    let s2w = Affine3A::from_rotation_translation(
        Quat::from_rotation_z(pose.yaw),
        Vec3::new(pose.x, pose.y, MOUNT_Z),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}

/// Box centre at time `t`: crosses the corridor at x=22 from y=-14 upward at
/// 5 m/s and keeps going, clearing the path after a few seconds.
fn box_center(t: f32) -> Vec2 {
    Vec2::new(22.0, -14.0 + 5.0 * t)
}

fn box_scene(center: Vec2) -> Scene {
    let mut s = Scene::new();
    s.add(Primitive::Aabb {
        min: Vec3::new(center.x - BOX_HALF, center.y - BOX_HALF, -1.0),
        max: Vec3::new(center.x + BOX_HALF, center.y + BOX_HALF, 3.0),
    });
    s
}

/// Distance from point `p` to the (axis-aligned) box surface; 0 if inside.
fn point_box_clearance(p: Vec2, center: Vec2) -> f32 {
    let dx = (center.x - BOX_HALF - p.x)
        .max(p.x - (center.x + BOX_HALF))
        .max(0.0);
    let dy = (center.y - BOX_HALF - p.y)
        .max(p.y - (center.y + BOX_HALF))
        .max(0.0);
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn car_yields_to_a_crossing_box_then_arrives() {
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(40.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let mut min_clear = f32::INFINITY;
    let mut peaked = false; // reached cruise before the encounter
    let mut min_speed_mid = f32::INFINITY; // slowest speed mid-corridor after cruising
    let mut arrived = false;
    for step in 0..1500 {
        let t = step as f32 * dt;
        let bc = box_center(t);
        let pose = car.pose();
        min_clear = min_clear.min(point_box_clearance(pose.pos(), bc));
        if car.speed() > 4.5 {
            peaked = true;
        }
        // Once it has cruised, record its slowest speed while still mid-corridor
        // (away from both the start and the goal-approach deceleration).
        if peaked && pose.x > 4.0 && pose.pos().distance(goal) > 6.0 {
            min_speed_mid = min_speed_mid.min(car.speed());
        }
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }
        let scene = box_scene(bc);
        let returns = sweep(&scene, pose);
        let cmd = ap.step(pose, car.speed(), &returns, pose, dt);
        car.step(cmd, dt);
    }

    // Arrival proves the fresh-each-tick map let the box clear (an accumulating
    // map would smear the box's track into a permanent wall and deadlock).
    assert!(arrived, "car should reach the goal after the box clears");
    assert!(
        min_clear > 0.3,
        "car clipped the crossing box (min clearance {min_clear:.2} m)"
    );
    assert!(
        min_speed_mid < 3.5,
        "car should slow for the crossing box (dynamic reaction); min mid-speed {min_speed_mid:.1}"
    );
}
