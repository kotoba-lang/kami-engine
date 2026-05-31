//! End-to-end closed-loop autonomy tests on the kinematic bicycle plant.
//!
//! Each test wires a simulated 2-D lidar (`kami-sensor-sim`) around a
//! `BicycleModel` and runs the `Autopilot` until arrival, proving the full
//! perception -> planning -> control loop reaches a goal and avoids obstacles.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass};
use kami_sensor_sim::{Lidar, LidarIntrinsics, Primitive, Scene};

/// A planar 360° ring lidar mounted at `MOUNT_Z`.
const MOUNT_Z: f32 = 1.0;

fn ring_intrinsics() -> LidarIntrinsics {
    LidarIntrinsics {
        hfov: std::f32::consts::TAU,
        vfov: 0.05, // ~3°, effectively a 2-D ring
        h_beams: 240,
        v_beams: 1,
        range_min: 0.2,
        range_max: 80.0,
    }
}

/// World→sensor transform for a planar pose (z-up, yaw about +z).
fn sensor_view(pose: Pose2) -> Affine3A {
    let s2w = Affine3A::from_rotation_translation(
        Quat::from_rotation_z(pose.yaw),
        Vec3::new(pose.x, pose.y, MOUNT_Z),
    );
    s2w.inverse()
}

fn sweep(scene: &Scene, pose: Pose2) -> Vec<kami_sensor_sim::LidarReturn> {
    let mut lidar = Lidar::new("ring", "/lidar", ring_intrinsics());
    lidar.view = sensor_view(pose);
    lidar.acquire_data(scene)
}

/// Drive `class` from origin to `goal` through `scene`, returning the final
/// pose, whether it arrived, and whether it ever collided.
fn run(
    class: VehicleClass,
    goal: Vec2,
    scene: &Scene,
    obstacles: &[(Vec2, f32)],
    max_steps: usize,
) -> (Pose2, bool, bool) {
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let mut plant = BicycleModel::new(start, class.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(class), start);
    ap.set_goal(goal);

    let mut collided = false;
    for _ in 0..max_steps {
        let pose = plant.pose();
        // Collision check against the true obstacle geometry.
        for &(c, r) in obstacles {
            if pose.pos().distance(c) < r {
                collided = true;
            }
        }
        if ap.state == DriveState::Arrived {
            return (pose, true, collided);
        }
        let returns = sweep(scene, pose);
        let cmd = ap.step(pose, plant.speed(), &returns, pose, dt);
        plant.step(cmd, dt);
    }
    (plant.pose(), ap.state == DriveState::Arrived, collided)
}

#[test]
fn reaches_goal_on_open_ground() {
    let scene = Scene::new();
    let goal = Vec2::new(40.0, 0.0);
    let (pose, arrived, collided) = run(VehicleClass::Car, goal, &scene, &[], 600);
    assert!(arrived, "should arrive on open ground, ended at {:?}", pose);
    assert!(!collided);
    assert!(pose.pos().distance(goal) < 2.0);
}

#[test]
fn routes_around_a_blocking_obstacle() {
    // A wall-like box straddling the straight-line path at x≈20.
    let obstacle_center = Vec2::new(20.0, 0.0);
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(18.0, -5.0, -1.0),
        max: Vec3::new(22.0, 5.0, 3.0),
    });
    let goal = Vec2::new(40.0, 0.0);
    // Conservative collision radius around the box for the assertion.
    let (pose, arrived, collided) =
        run(VehicleClass::Car, goal, &scene, &[(obstacle_center, 5.0)], 1200);

    assert!(arrived, "should route around and arrive, ended at {:?}", pose);
    assert!(!collided, "must not drive through the obstacle");
}

#[test]
fn emergency_stops_for_a_sudden_wall() {
    // A wall close ahead, no room modelled to plan around within the cone:
    // the reactive layer must brake to a stop rather than ram it.
    let mut scene = Scene::new();
    scene.add(Primitive::Aabb {
        min: Vec3::new(6.0, -20.0, -1.0),
        max: Vec3::new(8.0, 20.0, 3.0),
    });
    let dt = 1.0 / 30.0;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let class = VehicleClass::Car;
    let mut plant = BicycleModel::new(start, class.limits());
    // Force the planner to keep aiming straight into the wall by goal-behind-wall
    // while the wall fully spans the corridor (no lateral gap in the cone).
    let mut ap = Autopilot::new(AutopilotConfig::for_class(class), start);
    ap.set_goal(Vec2::new(30.0, 0.0));

    let mut min_dist_to_wall = f32::INFINITY;
    let mut stopped_short = false;
    for _ in 0..400 {
        let pose = plant.pose();
        let dist = 7.0 - pose.x; // distance to wall front face at x=6..8
        min_dist_to_wall = min_dist_to_wall.min(dist.abs());
        if pose.x >= 6.0 {
            panic!("rammed the wall at x={}", pose.x);
        }
        let returns = sweep(&scene, pose);
        let cmd = ap.step(pose, plant.speed(), &returns, pose, dt);
        plant.step(cmd, dt);
        if plant.speed() < 0.05 && pose.x > 1.0 {
            stopped_short = true;
        }
    }
    assert!(stopped_short, "should brake to a stop before the wall");
    assert!(min_dist_to_wall > 0.5, "stopped too close / clipped the wall");
}

#[test]
fn ship_with_wide_turns_still_arrives() {
    // Ships have a 30 m wheelbase: verify the same loop handles a large
    // turning radius and still converges to an offset goal.
    let scene = Scene::new();
    let goal = Vec2::new(60.0, 25.0);
    let (pose, arrived, _) = run(VehicleClass::Ship, goal, &scene, &[], 2000);
    assert!(arrived, "ship should arrive, ended at {:?}", pose);
}

#[test]
fn lidar_ingest_marks_occupancy() {
    use kami_autodrive::OccupancyGrid;
    let mut scene = Scene::new();
    scene.add(Primitive::Sphere { center: Vec3::new(10.0, 0.0, 1.0), radius: 1.5 });
    let pose = Pose2::new(0.0, 0.0, 0.0);
    let returns = sweep(&scene, pose);

    let mut grid = OccupancyGrid::centered(Vec2::ZERO, 30.0, 0.5);
    grid.ingest_lidar(&returns, pose, (-1.0, 1.5));

    // The sphere front face (~x=8.5) should be marked occupied.
    let (cx, cy) = grid.world_to_cell(Vec2::new(8.5, 0.0)).unwrap();
    let mut any = false;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let x = (cx as i32 + dx) as usize;
            let y = (cy as i32 + dy) as usize;
            if grid.is_occupied(x, y) {
                any = true;
            }
        }
    }
    assert!(any, "lidar hit on the sphere should mark occupancy near x=8.5");
}
