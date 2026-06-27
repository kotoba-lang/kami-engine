//! Sensor fusion: lidar and a forward depth camera each see a *different* wall;
//! only fusing both (`step_multimodal`) lets the car avoid both and reach the
//! goal. Lidar alone would miss the camera-only wall and vice-versa.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::{
    Camera, CameraIntrinsics, DepthImage, Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene,
};

const MOUNT_Z: f32 = 1.0;

// Wall 1 (lidar-only) reaches up to y=3; wall 2 (camera-only) reaches HIGHER to
// y=6. Both have their gap above. A car that only saw wall 1 would climb just
// over y≈4 and then plough straight into wall 2 — so reaching the goal requires
// the camera to reveal wall 2 in time to climb higher. Each wall is
// `(x0, x1, y_top)`, spanning down to y=-8.
const W1: (f32, f32, f32) = (13.0, 17.0, 3.0);
const W2: (f32, f32, f32) = (25.0, 29.0, 6.0);
const Y_BOTTOM: f32 = -8.0;

fn lidar_scene() -> Scene {
    let mut s = Scene::new();
    s.add(Primitive::Aabb {
        min: Vec3::new(W1.0, Y_BOTTOM, -1.0),
        max: Vec3::new(W1.1, W1.2, 3.0),
    });
    s
}

fn camera_wall_points() -> Vec<Vec3> {
    let mut pts = Vec::new();
    let mut y = Y_BOTTOM;
    while y <= W2.2 {
        let mut z = 0.3;
        while z <= 2.5 {
            pts.push(Vec3::new(W2.0, y, z)); // front face of wall 2
            z += 0.2;
        }
        y += 0.2;
    }
    pts
}

fn lidar_sweep(scene: &Scene, pose: Pose2) -> Vec<LidarReturn> {
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

fn forward_camera(pose: Pose2) -> Camera {
    let intr = CameraIntrinsics::from_hfov(160, 120, 100f32.to_radians());
    let mut cam = Camera::new("front", "/cam", intr);
    let eye = Vec3::new(pose.x, pose.y, 1.0);
    let fwd = pose.forward();
    cam.look_at(
        eye,
        eye + Vec3::new(fwd.x, fwd.y, 0.0) * 10.0,
        Vec3::new(0.0, 0.0, 1.0),
    );
    cam
}

fn inside(p: Vec2, wall: (f32, f32, f32)) -> bool {
    p.x > wall.0 - 0.3 && p.x < wall.1 + 0.3 && p.y > Y_BOTTOM - 0.3 && p.y < wall.2 + 0.3
}

#[test]
fn lidar_and_camera_fuse_to_avoid_two_walls() {
    let dt = 1.0 / 30.0;
    let lscene = lidar_scene();
    let cam_pts = camera_wall_points();

    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(40.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut cfg = AutopilotConfig::for_class(VehicleClass::Car);
    cfg.dynamic_obstacles = false; // static world + limited-FOV camera ⇒ accumulate
    let mut ap = Autopilot::new(cfg, start);
    ap.set_goal(goal);

    let mut hit_w1 = false;
    let mut hit_w2 = false;
    let mut arrived = false;
    for _ in 0..2000 {
        let pose = car.pose();
        hit_w1 |= inside(pose.pos(), W1);
        hit_w2 |= inside(pose.pos(), W2);
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }
        let lidar = lidar_sweep(&lscene, pose);
        let cam = forward_camera(pose);
        let depth: DepthImage = cam.render_points_to_depth_image(&cam_pts);
        let cmd = ap.step_multimodal(pose, car.speed(), &lidar, &[(&depth, &cam)], pose, dt);
        car.step(cmd, dt);
    }

    assert!(arrived, "car should reach the goal by fusing both sensors");
    assert!(!hit_w1, "must avoid the lidar-only wall");
    assert!(!hit_w2, "must avoid the camera-only wall");
}
