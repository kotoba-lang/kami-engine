//! Determinism guard: identical inputs must produce a bit-identical trajectory
//! every run. The perception/planning/control stack uses no HashMaps, clocks,
//! or RNG, so two runs of the same scenario must match exactly. This pins that
//! guarantee against future nondeterminism creep (e.g. swapping a Vec for a
//! HashMap in the occupancy grid or planner).

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

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
        Vec3::new(pose.x, pose.y, 1.0),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}

/// Run the wall-routing scenario, returning the full per-tick trajectory as
/// raw bits so comparison is exact (NaN-free, no float fuzz).
fn run() -> Vec<(u32, u32, u32, u32)> {
    let dt = 1.0 / 30.0;
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(18.0, -5.0, -1.0),
        max: Vec3::new(22.0, 5.0, 3.0),
    });
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(Vec2::new(40.0, 0.0));

    let mut trace = Vec::new();
    for _ in 0..1200 {
        let p = car.pose();
        trace.push((p.x.to_bits(), p.y.to_bits(), p.yaw.to_bits(), car.speed().to_bits()));
        if ap.state == DriveState::Arrived {
            break;
        }
        let cmd = ap.step(p, car.speed(), &sweep(&scene, p), p, dt);
        car.step(cmd, dt);
    }
    trace
}

#[test]
fn identical_runs_are_bit_identical() {
    let a = run();
    let b = run();
    assert_eq!(a.len(), b.len(), "trajectory length differs between runs");
    assert!(a.len() > 100, "scenario should produce a real trajectory");
    assert_eq!(a, b, "two identical runs diverged — nondeterminism crept in");
}
