//! Camera-only autonomy: a car perceives a wall with a forward-facing depth
//! camera (no lidar) and routes around it via `step_multimodal`. Uses the
//! accumulating map (`dynamic_obstacles = false`), the right mode for a static
//! world seen through a limited-FOV sensor.

use glam::{Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::{Camera, CameraIntrinsics, DepthImage, LidarReturn};

const NO_LIDAR: [LidarReturn; 0] = [];

/// Wall occupies x∈[19,21], y∈[-3,3]; the camera-visible front face is x=19.
fn wall_points() -> Vec<Vec3> {
    let mut pts = Vec::new();
    let mut y = -3.0;
    while y <= 3.0 {
        let mut z = 0.3;
        while z <= 2.5 {
            pts.push(Vec3::new(19.0, y, z));
            z += 0.15;
        }
        y += 0.15;
    }
    pts
}

/// Clearance from point `p` to the wall box (0 if inside).
fn wall_clearance(p: Vec2) -> f32 {
    let dx = (19.0 - p.x).max(p.x - 21.0).max(0.0);
    let dy = (-3.0 - p.y).max(p.y - 3.0).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

fn forward_camera(pose: Pose2) -> Camera {
    let intr = CameraIntrinsics::from_hfov(160, 120, 90f32.to_radians());
    let mut cam = Camera::new("front", "/cam/front", intr);
    let eye = Vec3::new(pose.x, pose.y, 1.0);
    let fwd = pose.forward();
    let target = eye + Vec3::new(fwd.x, fwd.y, 0.0) * 10.0;
    cam.look_at(eye, target, Vec3::new(0.0, 0.0, 1.0));
    cam
}

#[test]
fn car_routes_around_a_camera_seen_wall() {
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(40.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());

    let mut cfg = AutopilotConfig::for_class(VehicleClass::Car);
    cfg.dynamic_obstacles = false; // static world, accumulate what the camera sees
    let mut ap = Autopilot::new(cfg, start);
    ap.set_goal(goal);

    let pts = wall_points();
    let mut min_clear = f32::INFINITY;
    let mut max_lateral = 0.0f32;
    let mut arrived = false;
    for _ in 0..1500 {
        let pose = car.pose();
        min_clear = min_clear.min(wall_clearance(pose.pos()));
        max_lateral = max_lateral.max(pose.y.abs());
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }
        let cam = forward_camera(pose);
        let depth: DepthImage = cam.render_points_to_depth_image(&pts);
        let cmd = ap.step_multimodal(pose, car.speed(), &NO_LIDAR, &[(&depth, &cam)], pose, dt);
        car.step(cmd, dt);
    }

    assert!(
        arrived,
        "car should reach the goal using only the depth camera"
    );
    assert!(
        min_clear > 0.3,
        "car clipped the wall (min clearance {min_clear:.2} m)"
    );
    assert!(
        max_lateral > 3.0,
        "car should detour around the wall, max |y| {max_lateral:.1}"
    );
}
