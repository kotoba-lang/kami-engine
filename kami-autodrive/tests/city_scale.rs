//! Scale test: an autonomous car navigates a realistic multi-building street
//! grid (a synthetic city block, the stand-in for OSM/Shibuya building AABBs)
//! from one corner to the far corner, weaving through the streets. Exercises
//! perception + A* + control against a dense static obstacle field, not a toy
//! single obstacle.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::Telemetry;
use kami_autodrive::{
    Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass,
};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const MOUNT_Z: f32 = 1.0;
const BLDG_HALF: f32 = 4.0;

/// 3×3 grid of 8 m buildings on 16 m centres ⇒ 8 m streets between them.
fn buildings() -> Vec<Vec2> {
    let mut v = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            v.push(Vec2::new(15.0 + i as f32 * 16.0, 15.0 + j as f32 * 16.0));
        }
    }
    v
}

fn city_scene(bldgs: &[Vec2]) -> Scene {
    let mut s = Scene::new();
    for c in bldgs {
        s.add(Primitive::Aabb {
            min: Vec3::new(c.x - BLDG_HALF, c.y - BLDG_HALF, -1.0),
            max: Vec3::new(c.x + BLDG_HALF, c.y + BLDG_HALF, 4.0),
        });
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
        Vec3::new(pose.x, pose.y, MOUNT_Z),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}

/// Nearest clearance from `p` to any building (0 ⇒ inside one).
fn min_clearance(p: Vec2, bldgs: &[Vec2]) -> f32 {
    bldgs
        .iter()
        .map(|c| {
            let dx = (c.x - BLDG_HALF - p.x)
                .max(p.x - (c.x + BLDG_HALF))
                .max(0.0);
            let dy = (c.y - BLDG_HALF - p.y)
                .max(p.y - (c.y + BLDG_HALF))
                .max(0.0);
            (dx * dx + dy * dy).sqrt()
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn car_navigates_a_city_street_grid() {
    let dt = 1.0 / 30.0;
    let bldgs = buildings();
    let scene = city_scene(&bldgs);

    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(62.0, 62.0); // past the far corner of the grid
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let mut min_clear = f32::INFINITY;
    let mut arrived = false;
    for _ in 0..4000 {
        let pose = car.pose();
        min_clear = min_clear.min(min_clearance(pose.pos(), &bldgs));
        if ap.state == DriveState::Arrived {
            arrived = true;
            break;
        }
        let cmd = ap.step(pose, car.speed(), &sweep(&scene, pose), pose, dt);
        car.step(cmd, dt);
    }

    assert!(
        arrived,
        "car should weave through the streets to the far corner"
    );
    assert!(
        min_clear > 0.3,
        "car clipped a building (min clearance {min_clear:.2} m)"
    );
}

#[test]
fn telemetry_tracks_progress_through_the_grid() {
    let dt = 1.0 / 30.0;
    let bldgs = buildings();
    let scene = city_scene(&bldgs);
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(62.0, 62.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let mut first: Option<Telemetry> = None;
    let mut last = ap.telemetry();
    let mut max_xte = 0.0f32;
    let mut moved = false;
    for _ in 0..4000 {
        let pose = car.pose();
        if ap.state == DriveState::Arrived {
            break;
        }
        let cmd = ap.step(pose, car.speed(), &sweep(&scene, pose), pose, dt);
        car.step(cmd, dt);

        let tm = ap.telemetry();
        if first.is_none() {
            first = Some(tm);
        }
        last = tm;
        if car.speed() > 1.0 {
            moved = true;
            max_xte = max_xte.max(tm.cross_track_error);
        }
    }

    let first = first.unwrap();
    assert!(moved, "car should drive");
    assert!(first.distance_to_goal.is_finite() && first.distance_to_goal > 50.0);
    assert!(
        last.distance_to_goal < first.distance_to_goal - 30.0,
        "telemetry distance_to_goal should fall as it progresses ({:.1} → {:.1})",
        first.distance_to_goal,
        last.distance_to_goal
    );
    assert!(last.target_speed >= 0.0 && last.path_waypoints >= 1);
    // The tracker should keep the car reasonably close to its planned path.
    assert!(
        max_xte < 3.0,
        "cross-track error should stay bounded (max {max_xte:.2} m)"
    );
}
