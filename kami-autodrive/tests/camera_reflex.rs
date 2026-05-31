//! Camera-only reactive reflex: a depth camera (no lidar) now drives the
//! emergency stop, closing the "no camera-only reflex" gap. A close wall seen
//! only by the camera triggers a brake on the same tick — before any replan.

use glam::{Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, DriveState, Pose2, VehicleClass,
};
use kami_sensor_sim::{Camera, CameraIntrinsics, LidarReturn};

const NO_LIDAR: [LidarReturn; 0] = [];

fn forward_camera(pose: Pose2) -> Camera {
    let intr = CameraIntrinsics::from_hfov(160, 120, 90f32.to_radians());
    let mut cam = Camera::new("front", "/cam", intr);
    let eye = Vec3::new(pose.x, pose.y, 1.0);
    let fwd = pose.forward();
    cam.look_at(eye, eye + Vec3::new(fwd.x, fwd.y, 0.0) * 10.0, Vec3::new(0.0, 0.0, 1.0));
    cam
}

fn wall_points(x: f32) -> Vec<Vec3> {
    let mut pts = Vec::new();
    let mut y = -4.0;
    while y <= 4.0 {
        let mut z = 0.3;
        while z <= 2.5 {
            pts.push(Vec3::new(x, y, z));
            z += 0.2;
        }
        y += 0.2;
    }
    pts
}

#[test]
fn camera_only_reflex_brakes_for_a_close_wall() {
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(Vec2::new(40.0, 0.0));

    // The car is moving at 10 m/s; a wall 5 m ahead is well inside the braking
    // distance. Seen ONLY by the camera (empty lidar).
    let cam = forward_camera(start);
    let depth = cam.render_points_to_depth_image(&wall_points(5.0));
    let cmd = ap.step_multimodal(start, 10.0, &NO_LIDAR, &[(&depth, &cam)], start, dt);

    assert!(cmd.brake > 0.5, "camera reflex should emergency-brake (brake={})", cmd.brake);
    assert!(cmd.throttle == 0.0, "should not be accelerating into the wall");
    assert_eq!(ap.state, DriveState::Blocked);
}

#[test]
fn no_phantom_brake_without_an_obstacle() {
    // Same speed, same position, but nothing ahead (empty lidar, no camera) —
    // the car must NOT brake. Confirms the brake above came from the camera.
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(Vec2::new(40.0, 0.0));
    let cmd = ap.step(start, 10.0, &NO_LIDAR, start, dt);
    assert!(cmd.brake < 0.5, "open road must not emergency-brake (brake={})", cmd.brake);
}
