//! Defensive behaviour under degenerate inputs: goal-at-start, no goal, an
//! unreachable goal, and numerical finiteness. A mature autonomy layer must
//! degrade to a safe stop — never panic, NaN, or drive into an obstacle.

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

fn finite_pose(p: Pose2) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.yaw.is_finite()
}

#[test]
fn arrives_immediately_when_goal_is_the_start() {
    let start = Pose2::new(5.0, 5.0, 0.0);
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(start.pos());
    let cmd = ap.step(start, 0.0, &[], start, 1.0 / 30.0);
    assert_eq!(ap.state, DriveState::Arrived);
    assert!(cmd.brake > 0.0 && cmd.throttle == 0.0, "should hold a stop at the goal");
}

#[test]
fn no_goal_holds_a_safe_idle_stop() {
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    // No set_goal call.
    let cmd = ap.step(start, 3.0, &[], start, 1.0 / 30.0);
    assert_eq!(ap.state, DriveState::Idle);
    assert!(cmd.brake > 0.0 && cmd.throttle == 0.0);
    assert!(ap.telemetry().distance_to_goal.is_infinite());
}

#[test]
fn unreachable_goal_blocks_without_panic_or_collision() {
    let dt = 1.0 / 30.0;
    // A wall fully spanning the corridor; the goal sits behind it with no gap.
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(6.0, -40.0, -1.0),
        max: Vec3::new(8.0, 40.0, 3.0),
    });
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(Vec2::new(30.0, 0.0)); // behind the wall

    let mut rammed = false;
    for _ in 0..2000 {
        let p = car.pose();
        assert!(finite_pose(p) && car.speed().is_finite(), "values went non-finite: {p:?}");
        if p.x >= 6.0 {
            rammed = true; // crossed into the wall
        }
        if ap.state == DriveState::Arrived {
            panic!("should not arrive through an impassable wall");
        }
        let cmd = ap.step(p, car.speed(), &sweep(&scene, p), p, dt);
        car.step(cmd, dt);
    }
    assert!(!rammed, "must never drive into the impassable wall");
    // It settles to a safe non-moving stop in front of the wall.
    assert!(car.speed().abs() < 0.2 && car.pose().x < 6.0, "should hold short of the wall");
}

#[test]
fn poses_and_speeds_stay_finite_through_a_full_run() {
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

    for _ in 0..1500 {
        let p = car.pose();
        assert!(finite_pose(p) && car.speed().is_finite());
        let tm = ap.telemetry();
        assert!(tm.distance_to_goal.is_finite() && tm.cross_track_error.is_finite());
        if ap.state == DriveState::Arrived {
            break;
        }
        let cmd = ap.step(p, car.speed(), &sweep(&scene, p), p, dt);
        assert!(cmd.throttle.is_finite() && cmd.steer.is_finite() && cmd.brake.is_finite());
        car.step(cmd, dt);
    }
}
