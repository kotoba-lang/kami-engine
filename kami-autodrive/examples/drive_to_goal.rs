//! Headless demo: a car autonomously routes around a wall to a goal.
//!
//! ```sh
//! cargo run -p kami-autodrive --example drive_to_goal
//! ```

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const MOUNT_Z: f32 = 1.0;

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

fn main() {
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(18.0, -5.0, -1.0),
        max: Vec3::new(22.0, 5.0, 3.0),
    });

    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(40.0, 0.0);
    let mut plant = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    println!("# t  x  y  yaw  speed  state");
    for step in 0..1200 {
        let pose = plant.pose();
        if ap.state == DriveState::Arrived {
            println!("ARRIVED at step {step}: ({:.1}, {:.1})", pose.x, pose.y);
            break;
        }
        let returns = sweep(&scene, pose);
        let cmd = ap.step(pose, plant.speed(), &returns, pose, dt);
        plant.step(cmd, dt);
        if step % 30 == 0 {
            println!(
                "{:5.1} {:6.2} {:6.2} {:6.2} {:5.2} {:?}",
                step as f32 * dt,
                pose.x,
                pose.y,
                pose.yaw,
                plant.speed(),
                ap.state
            );
        }
    }
}
