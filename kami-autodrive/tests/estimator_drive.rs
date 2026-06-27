//! End-to-end localisation robustness: the car drives autonomously on its
//! *estimated* pose (IMU dead-reckoning between sparse absolute fixes), not the
//! ground truth, and still routes around a wall to the goal. Closes the loop
//! between the StateEstimator and the Autopilot under realistic localisation
//! dropout.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, StateEstimator,
    VehicleClass,
};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const MOUNT_Z: f32 = 1.0;

fn noise(state: &mut u64, amp: f32) -> f32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let u = ((*state >> 40) as f32) / ((1u64 << 24) as f32);
    (u * 2.0 - 1.0) * amp
}

/// Lidar physically swept from the car's TRUE pose.
fn sweep(scene: &Scene, true_pose: Pose2) -> Vec<LidarReturn> {
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
        Quat::from_rotation_z(true_pose.yaw),
        Vec3::new(true_pose.x, true_pose.y, MOUNT_Z),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}

#[test]
fn drives_on_estimated_pose_through_fix_dropout() {
    let dt = 1.0 / 50.0;
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(18.0, -5.0, -1.0),
        max: Vec3::new(22.0, 5.0, 3.0),
    });

    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(40.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut est = StateEstimator::new(start);
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let mut rng: u64 = 0x0bad_c0de_1337_4242;
    let mut prev_speed = car.speed();
    let mut prev_yaw = car.pose().yaw;
    let mut collided = false;
    let mut max_est_err = 0.0f32;
    let mut arrived = false;

    for step in 0..2000 {
        let truth = car.pose();
        if truth.x > 18.0 && truth.x < 22.0 && truth.y.abs() < 5.0 {
            collided = true; // inside the actual wall box
        }
        // Autopilot's notion of arrival is on the estimate; cross-check truth.
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }

        // IMU sample synthesised from true motion (bias + jitter).
        let true_accel = (car.speed() - prev_speed) / dt;
        let true_yaw_rate = (truth.yaw - prev_yaw) / dt;
        prev_speed = car.speed();
        prev_yaw = truth.yaw;
        est.predict(
            true_accel + 0.05 + noise(&mut rng, 0.12),
            true_yaw_rate + 0.008 + noise(&mut rng, 0.006),
            dt,
        );
        // Absolute fix every 12 ticks (~0.24 s).
        if step % 12 == 11 {
            est.correct(truth, 0.8);
            est.correct_speed(car.speed(), 0.8);
        }
        max_est_err = max_est_err.max(est.pose().pos().distance(truth.pos()));

        // Drive on the ESTIMATE: pose + sensor pose both come from `est`.
        let ep = est.pose();
        let cmd = ap.step(ep, est.speed(), &sweep(&scene, truth), ep, dt);
        car.step(cmd, dt);
    }

    let truth = car.pose();
    assert!(arrived, "should reach the goal driving on the estimate");
    assert!(
        truth.pos().distance(goal) < 3.0,
        "true pose should end near goal ({truth:?})"
    );
    assert!(
        !collided,
        "must not hit the wall despite navigating on a noisy estimate"
    );
    // Sanity: the estimate stayed reasonable throughout (else success was luck).
    assert!(
        max_est_err < 2.0,
        "estimate drifted too far ({max_est_err:.2} m)"
    );
}
