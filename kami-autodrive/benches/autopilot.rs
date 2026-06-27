//! Throughput benchmarks for the autonomy hot paths — guards against
//! performance regressions in `Autopilot::step` and the perception/planner.
//!
//! Run: `cargo bench -p kami-autodrive`

use criterion::{Criterion, criterion_group, criterion_main};
use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{Autopilot, AutopilotConfig, BicycleModel, Plant, Pose2, VehicleClass};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};
use std::hint::black_box;

fn city_scene() -> Scene {
    let mut s = Scene::new();
    for i in 0..3 {
        for j in 0..3 {
            let cx = 15.0 + i as f32 * 16.0;
            let cy = 15.0 + j as f32 * 16.0;
            s.add(Primitive::Aabb {
                min: Vec3::new(cx - 4.0, cy - 4.0, -1.0),
                max: Vec3::new(cx + 4.0, cy + 4.0, 4.0),
            });
        }
    }
    s
}

fn sweep(scene: &Scene, pose: Pose2) -> Vec<LidarReturn> {
    let intr = LidarIntrinsics {
        hfov: std::f32::consts::TAU,
        vfov: 0.05,
        h_beams: 360,
        v_beams: 1,
        range_min: 0.2,
        range_max: 120.0,
    };
    let mut lidar = Lidar::new("ring", "/lidar", intr);
    let s2w = Affine3A::from_rotation_translation(
        Quat::from_rotation_z(pose.yaw),
        Vec3::new(pose.x, pose.y, 1.0),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}

fn bench(c: &mut Criterion) {
    let scene = city_scene();
    let start = Pose2::new(0.0, 0.0, 0.0);

    // A representative pose mid-grid, with a real sweep cached.
    let pose = Pose2::new(7.0, 7.0, 0.5);
    let returns = sweep(&scene, pose);

    c.bench_function("autopilot_step_city", |b| {
        // Fresh autopilot per iteration keeps the planner state comparable.
        b.iter(|| {
            let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
            ap.set_goal(Vec2::new(62.0, 62.0));
            let cmd = ap.step(
                black_box(pose),
                black_box(5.0),
                black_box(&returns),
                black_box(pose),
                1.0 / 30.0,
            );
            black_box(cmd)
        })
    });

    c.bench_function("lidar_sweep_city", |b| {
        b.iter(|| black_box(sweep(black_box(&scene), black_box(pose))))
    });

    c.bench_function("bicycle_step", |b| {
        let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
        let cmd = kami_autodrive::Command {
            throttle: 0.5,
            steer: 0.1,
            ..Default::default()
        };
        b.iter(|| {
            car.step(black_box(cmd), 1.0 / 30.0);
            black_box(car.pose())
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
