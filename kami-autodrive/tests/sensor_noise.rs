//! Robustness: the autonomy must still navigate when the lidar is noisy (real
//! sensors are). Deterministic per-beam range noise is injected each tick; the
//! occupancy grid + configuration-space inflation should absorb the jitter and
//! the car still routes around a wall to its goal without clipping it.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const MOUNT_Z: f32 = 1.0;

/// Tiny deterministic LCG → uniform noise in [-amp, amp].
fn noise(state: &mut u64, amp: f32) -> f32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let u = ((*state >> 40) as f32) / ((1u64 << 24) as f32); // [0,1)
    (u * 2.0 - 1.0) * amp
}

fn noisy_sweep(scene: &Scene, pose: Pose2, rng: &mut u64, amp: f32) -> Vec<LidarReturn> {
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
    let mut returns = lidar.acquire_data(scene);
    for r in returns.iter_mut() {
        if r.range.is_finite() {
            let dn = noise(rng, amp);
            let scale = ((r.range + dn) / r.range).max(0.1);
            r.range += dn;
            r.point_sensor *= scale; // keep point consistent with the noisy range
        }
    }
    returns
}

#[test]
fn navigates_with_noisy_lidar() {
    let dt = 1.0 / 30.0;
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(18.0, -5.0, -1.0),
        max: Vec3::new(22.0, 5.0, 3.0),
    });
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(40.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let mut rng: u64 = 0x1234_5678_9abc_def0;
    let amp = 0.25; // ±25 cm range noise
    let mut collided = false;
    let mut arrived = false;
    for _ in 0..1500 {
        let pose = car.pose();
        // True (noise-free) collision check against the wall box.
        if pose.x > 17.0 && pose.x < 23.0 && pose.y.abs() < 5.0 {
            collided = true;
        }
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }
        let returns = noisy_sweep(&scene, pose, &mut rng, amp);
        let cmd = ap.step(pose, car.speed(), &returns, pose, dt);
        car.step(cmd, dt);
    }

    assert!(
        arrived,
        "car should still reach the goal under noisy sensing"
    );
    assert!(
        !collided,
        "car must not drive into the wall despite sensor noise"
    );
}
