//! Render an autonomous run to an SVG for visual inspection / sharing.
//!
//! ```sh
//! cargo run -p kami-autodrive --example render_run > run.svg && open run.svg
//! ```
//!
//! Drives the city street-grid scenario and emits buildings, the final planned
//! path, the actual trajectory, and start/goal markers as SVG to stdout.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_autodrive::{Autopilot, AutopilotConfig, BicycleModel, DriveState, Plant, Pose2, VehicleClass};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const BLDG_HALF: f32 = 4.0;

fn buildings() -> Vec<Vec2> {
    let mut v = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            v.push(Vec2::new(15.0 + i as f32 * 16.0, 15.0 + j as f32 * 16.0));
        }
    }
    v
}

fn scene(bldgs: &[Vec2]) -> Scene {
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
        Vec3::new(pose.x, pose.y, 1.0),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}

const SCALE: f32 = 9.0; // px per metre
const PAD: f32 = 6.0; // world-metre padding around the scene
const SPAN: f32 = 62.0; // scene extent (0..62 plus padding)

// World → SVG pixel (y flipped so +y points up on screen).
fn sx(x: f32) -> f32 {
    (x + PAD) * SCALE
}
fn sy(y: f32) -> f32 {
    (SPAN + PAD - y) * SCALE
}

fn main() {
    let dt = 1.0 / 30.0;
    let bldgs = buildings();
    let sc = scene(&bldgs);
    let start = Pose2::new(0.0, 0.0, 0.0);
    let goal = Vec2::new(62.0, 62.0);
    let mut car = BicycleModel::new(start, VehicleClass::Car.limits());
    let mut ap = Autopilot::new(AutopilotConfig::for_class(VehicleClass::Car), start);
    ap.set_goal(goal);

    let mut traj: Vec<Vec2> = Vec::new();
    let mut final_path: Vec<Vec2> = Vec::new();
    for _ in 0..4000 {
        let pose = car.pose();
        traj.push(pose.pos());
        final_path = ap.path().to_vec();
        if ap.state == DriveState::Arrived {
            break;
        }
        let cmd = ap.step(pose, car.speed(), &sweep(&sc, pose), pose, dt);
        car.step(cmd, dt);
    }

    let w = (SPAN + 2.0 * PAD) * SCALE;
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w:.0}' height='{w:.0}' \
         viewBox='0 0 {w:.0} {w:.0}'>\n"
    ));
    out.push_str(&format!("<rect width='{w:.0}' height='{w:.0}' fill='#f0ead6'/>\n"));

    // Buildings.
    for c in &bldgs {
        out.push_str(&format!(
            "<rect x='{:.1}' y='{:.1}' width='{:.1}' height='{:.1}' fill='#9aa6b2' \
             stroke='#5b6b7b' stroke-width='1'/>\n",
            sx(c.x - BLDG_HALF),
            sy(c.y + BLDG_HALF),
            2.0 * BLDG_HALF * SCALE,
            2.0 * BLDG_HALF * SCALE,
        ));
    }

    // Final planned path (dashed).
    if final_path.len() >= 2 {
        let pts: String = final_path
            .iter()
            .map(|p| format!("{:.1},{:.1}", sx(p.x), sy(p.y)))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "<polyline points='{pts}' fill='none' stroke='#e0a020' stroke-width='2' \
             stroke-dasharray='6 4'/>\n"
        ));
    }

    // Actual trajectory.
    let tpts: String = traj
        .iter()
        .map(|p| format!("{:.1},{:.1}", sx(p.x), sy(p.y)))
        .collect::<Vec<_>>()
        .join(" ");
    out.push_str(&format!(
        "<polyline points='{tpts}' fill='none' stroke='#2a7de1' stroke-width='2.5'/>\n"
    ));

    // Start (green) + goal (red).
    out.push_str(&format!(
        "<circle cx='{:.1}' cy='{:.1}' r='5' fill='#3aa655'/>\n",
        sx(start.x),
        sy(start.y)
    ));
    out.push_str(&format!(
        "<circle cx='{:.1}' cy='{:.1}' r='5' fill='#d23a3a'/>\n",
        sx(goal.x),
        sy(goal.y)
    ));
    out.push_str("</svg>\n");

    print!("{out}");
    eprintln!(
        "[render_run] state={:?} steps={} trajectory_pts={}",
        ap.state,
        traj.len(),
        traj.len()
    );
}
