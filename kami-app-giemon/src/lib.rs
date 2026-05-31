//! kami-app-giemon — Giemon robot kit viewer (giemon.etzhayyim.com)
//!
//! Procedural ArmCrawler model:
//!   - Rubber-track crawler chassis (2 tracks + 8 drive wheels)
//!   - 6-DOF arm (J1–J6 links) in display pose
//!   - Gripper (2 fingers)
//!
//! Orbit camera + pick-to-highlight. No external assets.

use glam::{Mat4, Quat, Vec3};
#[cfg(target_family = "wasm")]
use kami_app::{CameraMode, InputMode, KamiApp};
use kami_pipelines::{unit_box, unit_cylinder};
use kami_render::RenderContext;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

// ── colours (linear sRGB) ──────────────────────────────────────────────────
const ALUMINIUM:  [f32; 3] = [0.76, 0.78, 0.80];
const STEEL:      [f32; 3] = [0.55, 0.58, 0.62];
const RUBBER:     [f32; 3] = [0.12, 0.12, 0.13];
const ARM_BODY:   [f32; 3] = [0.96, 0.55, 0.13]; // orange
const SERVO:      [f32; 3] = [0.22, 0.25, 0.28];
const GRIPPER:    [f32; 3] = [0.30, 0.72, 0.55]; // teal

// ── JS entry ──────────────────────────────────────────────────────────────
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.02, 0.18, 0.0),
            distance: 0.75,
            yaw: 0.55,
            pitch: 0.30,
        })
        .with_input(InputMode::OrbitMouse);

    let sky  = kami_pipelines::SkyAdapter::new(app.render_context());
    let cad  = kami_pipelines::CadSceneAdapter::new(app.render_context());
    build_armcrawler(app.render_context(), &cad);
    log::info!("[giemon] batches={}", cad.batch_count());

    let handle = cad.clone();
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            if let Some(p) = handle.pick_from_camera_if_clicked(camera) {
                handle.set_highlighted_by_id(&p.feature_id);
                log::info!("[giemon] pick id={} dist={:.3}", p.feature_id, p.distance);
            }
        });

    log::info!("[giemon] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── physics arm (kami-genesis 3-D spatial solver + contact) ─────────────────
// giemon.htm "physics-arm": a real 6-DOF manipulator loaded from URDF
// (`assets/giemon_arm6.urdf`) and driven by the kami-genesis 3-D
// reduced-coordinate spatial solver (Featherstone RNEA bias + CRBA mass matrix
// + LDLᵀ, semi-implicit Euler — the algorithm class PhysX's Articulation uses)
// with the rigid contact/collision solver against a ground plane. Clean-room:
// no NVIDIA / PhysX / Isaac code (ADR-2605261800 N1..N9).
//
// Controls: 1–6 select a joint, J / L torque it (− / +), key-up → 0.

const ARM6_URDF: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon_arm6/giemon_arm6.urdf");
const GROUND: [f32; 3] = [0.42, 0.46, 0.40]; // muted olive ground
const ARM_DT: f32 = 1.0 / 240.0;

/// Parse the URDF and build the 3-D articulation config.
fn giemon_arm6_config() -> kami_genesis::Articulation3dConfig {
    let sys = kami_articulated::parse_urdf(ARM6_URDF).expect("giemon_arm6.urdf parses");
    kami_genesis::Articulation3dConfig::from_articulated_system(
        &sys,
        Vec3::new(0.0, 0.0, -9.81),
        ARM_DT,
    )
}

/// Segment vector (body frame) from body `i` origin to its child's origin —
/// the rendered "link". Leaf body gets a short tool tip.
fn link_segment(cfg: &kami_genesis::Articulation3dConfig, i: usize) -> Vec3 {
    cfg.bodies
        .iter()
        .find(|b| b.parent == i as isize)
        .map(|c| c.r_tree)
        .unwrap_or(Vec3::new(0.0, 0.0, 0.05))
}

/// Local box transform for a link segment `seg` (body frame): a beam from the
/// origin to `seg`, local +Z = length.
fn segment_box(seg: Vec3, thick: f32) -> Mat4 {
    let len = seg.length().max(1.0e-4);
    let dir = seg / len;
    Mat4::from_scale_rotation_translation(
        Vec3::new(thick, thick, len),
        Quat::from_rotation_arc(Vec3::Z, dir),
        seg * 0.5,
    )
}

#[cfg(target_family = "wasm")]
thread_local! {
    static JOINT_TORQUE: std::cell::Cell<f32> = std::cell::Cell::new(0.0);
    static SELECTED_JOINT: std::cell::Cell<usize> = std::cell::Cell::new(1);
}

/// JS hook: torque (N·m) applied to the currently-selected joint. J → −T,
/// L → +T, key-up → 0.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = giemonSetJointTorque)]
pub fn giemon_set_joint_torque(torque: f32) {
    JOINT_TORQUE.with(|t| t.set(torque));
}

/// JS hook: select which joint (1-based, matching the URDF j1..j6) the torque
/// drives. Out-of-range values are clamped.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = giemonSelectJoint)]
pub fn giemon_select_joint(one_based: u32) {
    SELECTED_JOINT.with(|s| s.set((one_based.max(1)) as usize - 1));
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_sim_v1(canvas_id: &str) -> Result<(), JsValue> {
    use kami_genesis::{Articulation3dState, Collider, ContactParams, ContactWorld};

    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 0.30, 0.0),
            distance: 1.05,
            yaw: 0.7,
            pitch: 0.22,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let (bp, bn, bi) = unit_box();

    let cfg = giemon_arm6_config();
    let nb = cfg.n_bodies();
    log::info!("[giemon-sim] 6-DOF arm: bodies={nb} ndof={}", cfg.ndof);

    // The kami-genesis solver works in z-up; our renderer is y-up. Rotate the
    // whole scene −90° about X so the arm's +z stands up as render +y.
    let to_render = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    // Static ground plane (render y = 0) + base plate.
    push_box(ctx, &cad, "ground", "Ground", GROUND,
        Vec3::new(2.0, 0.01, 2.0), Vec3::new(0.0, -0.005, 0.0), Quat::IDENTITY);
    push_box(ctx, &cad, "base_plate", "Base plate", ALUMINIUM,
        Vec3::new(0.18, 0.02, 0.18), Vec3::new(0.0, 0.01, 0.0), Quat::IDENTITY);

    // Per-body geometry + animated batch indices.
    let segs: Vec<Vec3> = (0..nb).map(|i| link_segment(&cfg, i)).collect();
    let thicks: Vec<f32> = (0..nb)
        .map(|i| 0.040 - 0.004 * i as f32) // taper 0.040 → 0.016
        .map(|t| t.max(0.014))
        .collect();
    let link_local: Vec<Mat4> = (0..nb).map(|i| segment_box(segs[i], thicks[i])).collect();

    let link_start = cad.batch_count();
    let fk0 = cfg.fk_world(&vec![0.0; cfg.ndof]);
    for i in 0..nb {
        cad.push_triangles(ctx, format!("link_{i}"), format!("Link {i}"),
            &bp, &bn, &bi, ARM_BODY, to_render * fk0[i] * link_local[i]);
    }
    let joint_start = cad.batch_count();
    for i in 0..nb {
        let color = if cfg.bodies[i].movable() { SERVO } else { ALUMINIUM };
        cad.push_triangles(ctx, format!("joint_{i}"), format!("Joint {i}"),
            &bp, &bn, &bi, color,
            to_render * fk0[i] * Mat4::from_scale(Vec3::splat(thicks[i] * 1.3)));
    }

    // Contact: a sphere at each link's distal end vs the ground plane (z = 0).
    let colliders: Vec<(usize, Collider)> = (1..nb)
        .map(|i| (i, Collider::Sphere { center: segs[i], radius: thicks[i] * 0.6 }))
        .collect();
    let contacts = ContactWorld::new(colliders, ContactParams { ground_z: 0.0, friction: 0.9, ..Default::default() });

    // Start folded so gravity + contact are visible; J/L lift it back.
    let mut state = Articulation3dState::zeros(cfg.ndof);
    if cfg.ndof >= 3 {
        state.q[1] = 0.6;
        state.q[2] = 1.0;
    }
    if cfg.ndof >= 5 {
        state.q[4] = 0.5;
    }

    let render = cad.clone();
    let pick = cad.clone();
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            let torque = JOINT_TORQUE.with(|t| t.get());
            let sel = SELECTED_JOINT.with(|s| s.get()).min(cfg.ndof.saturating_sub(1));
            let mut tau = vec![0.0_f32; cfg.ndof];
            if cfg.ndof > 0 {
                tau[sel] = torque;
            }
            for _ in 0..4 {
                contacts.step(&cfg, &mut state, &tau);
            }
            let fk = cfg.fk_world(&state.q);
            for i in 0..nb {
                render.replace_batch_world(link_start + i, &bp, &bn, &bi, ARM_BODY,
                    to_render * fk[i] * link_local[i]);
                let color = if cfg.bodies[i].movable() { SERVO } else { ALUMINIUM };
                render.replace_batch_world(joint_start + i, &bp, &bn, &bi, color,
                    to_render * fk[i] * Mat4::from_scale(Vec3::splat(thicks[i] * 1.3)));
            }
            if let Some(p) = pick.pick_from_camera_if_clicked(camera) {
                pick.set_highlighted_by_id(&p.feature_id);
            }
        });

    log::info!("[giemon-sim] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── model builder ─────────────────────────────────────────────────────────

pub fn build_armcrawler(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    crawler_chassis(ctx, cad);
    arm_assembly(ctx, cad);
}

fn push_box(
    ctx: &RenderContext,
    cad: &kami_pipelines::CadSceneAdapter,
    id: &str,
    name: &str,
    color: [f32; 3],
    scale: Vec3,
    translate: Vec3,
    rotate: Quat,
) {
    let (pp, pn, pi) = unit_box();
    cad.push_triangles(
        ctx, id, name, &pp, &pn, &pi, color,
        Mat4::from_scale_rotation_translation(scale, rotate, translate),
    );
}

fn push_cyl(
    ctx: &RenderContext,
    cad: &kami_pipelines::CadSceneAdapter,
    id: &str,
    name: &str,
    color: [f32; 3],
    radius: f32,
    height: f32,
    translate: Vec3,
    rotate: Quat,
) {
    let (pp, pn, pi) = unit_cylinder(24);
    cad.push_triangles(
        ctx, id, name, &pp, &pn, &pi, color,
        Mat4::from_scale_rotation_translation(
            Vec3::new(radius, height, radius),
            rotate,
            translate,
        ),
    );
}

// ── chassis ───────────────────────────────────────────────────────────────

fn crawler_chassis(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    // Main body plate (220 × 60 × 160 mm)
    push_box(ctx, cad, "chassis", "Chassis", ALUMINIUM,
        Vec3::new(0.220, 0.060, 0.160),
        Vec3::new(0.0, 0.030, 0.0),
        Quat::IDENTITY,
    );

    // Tracks (left z=-0.105, right z=+0.105)
    for (id, name, z) in [
        ("track_l", "Left track",  -0.105_f32),
        ("track_r", "Right track",  0.105_f32),
    ] {
        push_box(ctx, cad, id, name, RUBBER,
            Vec3::new(0.240, 0.048, 0.040),
            Vec3::new(0.0, 0.024, z),
            Quat::IDENTITY,
        );
    }

    // Drive wheels (Ø35 × 38 mm depth, 4 per side)
    let wheel_xs = [-0.090_f32, -0.030, 0.030, 0.090];
    let rot90x = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for (side_i, &z) in [-0.105_f32, 0.105_f32].iter().enumerate() {
        for (wi, &wx) in wheel_xs.iter().enumerate() {
            let id = format!("wheel_{}_{}", if side_i == 0 { "l" } else { "r" }, wi);
            let name = format!("Drive wheel {}{}",
                if side_i == 0 { "L" } else { "R" }, wi + 1);
            push_cyl(ctx, cad, &id, &name, STEEL,
                0.0175, 0.038,
                Vec3::new(wx, 0.020, z),
                rot90x,
            );
        }
    }

    // Battery box (under chassis, 100 × 28 × 60 mm)
    push_box(ctx, cad, "battery", "Battery (18650 × 4)", [0.20, 0.20, 0.22],
        Vec3::new(0.100, 0.028, 0.060),
        Vec3::new(0.0, 0.0, 0.0),  // flush with chassis bottom
        Quat::IDENTITY,
    );
}

// ── arm ───────────────────────────────────────────────────────────────────
// Arm root mounted at (0.04, 0.060, 0.0) — top-front of chassis.
// Display pose: slight forward lean, partially extended.

fn arm_assembly(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    let root = Vec3::new(0.04, 0.060, 0.0);

    // J1 base cylinder (waist rotation axis, vertical)
    push_cyl(ctx, cad, "j1", "J1 — Waist", SERVO,
        0.022, 0.040,
        root + Vec3::new(0.0, 0.020, 0.0),
        Quat::IDENTITY,
    );

    // L1 — shoulder link (vertical, 90 mm)
    let l1_top = root + Vec3::new(0.0, 0.040 + 0.090, 0.0);
    push_box(ctx, cad, "l1", "Link 1 (shoulder)", ARM_BODY,
        Vec3::new(0.030, 0.090, 0.030),
        root + Vec3::new(0.0, 0.040 + 0.045, 0.0),
        Quat::IDENTITY,
    );

    // J2 servo box at l1_top
    push_box(ctx, cad, "j2", "J2 — Shoulder pitch", SERVO,
        Vec3::new(0.040, 0.032, 0.040),
        l1_top + Vec3::new(0.0, 0.016, 0.0),
        Quat::IDENTITY,
    );

    // L2 — upper arm, angled 30° forward (100 mm)
    let ang2 = 30_f32.to_radians();
    let l2_dir = Vec3::new(ang2.sin(), ang2.cos(), 0.0);
    let l2_rot = Quat::from_rotation_z(-ang2);
    let l2_mid = l1_top + Vec3::new(0.0, 0.032, 0.0) + l2_dir * 0.050;
    push_box(ctx, cad, "l2", "Link 2 (upper arm)", ARM_BODY,
        Vec3::new(0.028, 0.100, 0.028),
        l2_mid,
        l2_rot,
    );

    // J3 elbow
    let l2_end = l1_top + Vec3::new(0.0, 0.032, 0.0) + l2_dir * 0.100;
    push_box(ctx, cad, "j3", "J3 — Elbow", SERVO,
        Vec3::new(0.036, 0.030, 0.036),
        l2_end + Vec3::new(0.0, 0.015, 0.0),
        Quat::IDENTITY,
    );

    // L3 — forearm, angled 20° more forward (80 mm)
    let ang3 = (30_f32 + 20.0).to_radians();
    let l3_dir = Vec3::new(ang3.sin(), ang3.cos(), 0.0);
    let l3_rot = Quat::from_rotation_z(-ang3);
    let l3_start = l2_end + Vec3::new(0.0, 0.030, 0.0);
    let l3_mid = l3_start + l3_dir * 0.040;
    push_box(ctx, cad, "l3", "Link 3 (forearm)", ARM_BODY,
        Vec3::new(0.024, 0.080, 0.024),
        l3_mid,
        l3_rot,
    );

    // J4 wrist rotation
    let l3_end = l3_start + l3_dir * 0.080;
    push_cyl(ctx, cad, "j4", "J4 — Forearm rotation", SERVO,
        0.016, 0.030,
        l3_end,
        Quat::from_rotation_z(-ang3),
    );

    // L4 + J5 (wrist pitch, 60 mm)
    let l4_start = l3_end + l3_dir * 0.015;
    let l4_mid = l4_start + l3_dir * 0.030;
    push_box(ctx, cad, "l4", "Link 4 (wrist)", ARM_BODY,
        Vec3::new(0.020, 0.060, 0.020),
        l4_mid,
        l3_rot,
    );

    // J5 wrist
    let l4_end = l4_start + l3_dir * 0.060;
    push_cyl(ctx, cad, "j5", "J5 — Wrist pitch", SERVO,
        0.014, 0.024,
        l4_end,
        Quat::from_rotation_z(-ang3 + std::f32::consts::FRAC_PI_2),
    );

    // J6 + gripper
    let grip_base = l4_end + l3_dir * 0.020;
    push_cyl(ctx, cad, "j6", "J6 — Wrist rotation", SERVO,
        0.012, 0.020,
        grip_base,
        Quat::from_rotation_z(-ang3),
    );

    // Gripper fingers (2)
    let fwd = l3_dir * 0.018;
    for (gid, gname, offset_z) in [
        ("grip_l", "Gripper finger L", -0.012_f32),
        ("grip_r", "Gripper finger R",  0.012_f32),
    ] {
        push_box(ctx, cad, gid, gname, GRIPPER,
            Vec3::new(0.008, 0.032, 0.008),
            grip_base + fwd + Vec3::new(0.0, 0.0, offset_z),
            l3_rot,
        );
    }

    // ArmCrawlerHAT PCB (on top of chassis rear)
    push_box(ctx, cad, "hat_pcb", "ArmCrawlerHAT PCB", [0.12, 0.38, 0.18],
        Vec3::new(0.065, 0.002, 0.056),
        Vec3::new(-0.06, 0.061, 0.0),
        Quat::IDENTITY,
    );

    // RPi 5 (under HAT)
    push_box(ctx, cad, "rpi5", "Raspberry Pi 5", [0.10, 0.32, 0.10],
        Vec3::new(0.085, 0.016, 0.056),
        Vec3::new(-0.06, 0.052, 0.0),
        Quat::IDENTITY,
    );
}

// ── hitogata colours ──────────────────────────────────────────────────────
const SHELL:  [f32; 3] = [0.88, 0.90, 0.93]; // white-grey structural panel
const ACCENT: [f32; 3] = [0.18, 0.55, 0.88]; // blue accent / visor

// ── caterpillar colours ───────────────────────────────────────────────────
const ARMOR:  [f32; 3] = [0.22, 0.25, 0.29]; // dark armour plate
const SENSOR: [f32; 3] = [0.12, 0.68, 0.82]; // cyan LiDAR / camera

// ── Humanoid JS entry ─────────────────────────────────────────────────────
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_hitogata_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon-hitogata")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 0.14, 0.0),
            distance: 0.62,
            yaw: 0.45,
            pitch: 0.10,
        })
        .with_input(InputMode::OrbitMouse);

    let sky = kami_pipelines::SkyAdapter::new(app.render_context());
    let cad = kami_pipelines::CadSceneAdapter::new(app.render_context());
    build_hitogata(app.render_context(), &cad);
    log::info!("[giemon-hitogata] batches={}", cad.batch_count());

    let handle = cad.clone();
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            if let Some(p) = handle.pick_from_camera_if_clicked(camera) {
                handle.set_highlighted_by_id(&p.feature_id);
                log::info!("[giemon-hitogata] pick={} d={:.3}", p.feature_id, p.distance);
            }
        });

    log::info!("[giemon-hitogata] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── Caterpillar JS entry ──────────────────────────────────────────────────
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_caterpillar_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon-caterpillar")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 0.06, 0.0),
            distance: 0.88,
            yaw: 0.55,
            pitch: 0.28,
        })
        .with_input(InputMode::OrbitMouse);

    let sky = kami_pipelines::SkyAdapter::new(app.render_context());
    let cad = kami_pipelines::CadSceneAdapter::new(app.render_context());
    build_caterpillar(app.render_context(), &cad);
    log::info!("[giemon-caterpillar] batches={}", cad.batch_count());

    let handle = cad.clone();
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            if let Some(p) = handle.pick_from_camera_if_clicked(camera) {
                handle.set_highlighted_by_id(&p.feature_id);
                log::info!("[giemon-caterpillar] pick={} d={:.3}", p.feature_id, p.distance);
            }
        });

    log::info!("[giemon-caterpillar] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── Humanoid model ────────────────────────────────────────────────────────
// Giemon Bipede — 17-DOF biped, ~285mm tall, standing display pose.

pub fn build_hitogata(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    hitogata_legs(ctx, cad);
    hitogata_torso(ctx, cad);
    hitogata_arms(ctx, cad);
}

fn hitogata_legs(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    let rot90x = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);

    for (leg_x, s, cap) in [(-0.028_f32, "l", "L"), (0.028_f32, "r", "R")] {
        // Foot (60×15×35mm, shifted 5mm forward)
        push_box(ctx, cad, &format!("foot_{s}"), &format!("Foot {cap}"), SHELL,
            Vec3::new(0.060, 0.015, 0.035),
            Vec3::new(leg_x, 0.0075, 0.005),
            Quat::IDENTITY,
        );
        // Ankle servo (y=0.015)
        push_cyl(ctx, cad, &format!("ankle_{s}"), &format!("Ankle {cap}"), SERVO,
            0.008, 0.014, Vec3::new(leg_x, 0.015, 0.0), rot90x);
        // Shin (16×72×16mm, y 0.015–0.087)
        push_box(ctx, cad, &format!("shin_{s}"), &format!("Shin {cap}"), SHELL,
            Vec3::new(0.016, 0.072, 0.016),
            Vec3::new(leg_x, 0.051, 0.0),
            Quat::IDENTITY,
        );
        // Knee servo (y=0.087)
        push_cyl(ctx, cad, &format!("knee_{s}"), &format!("Knee {cap}"), SERVO,
            0.010, 0.016, Vec3::new(leg_x, 0.087, 0.0), rot90x);
        // Thigh (18×74×18mm, y 0.087–0.161)
        push_box(ctx, cad, &format!("thigh_{s}"), &format!("Thigh {cap}"), SHELL,
            Vec3::new(0.018, 0.074, 0.018),
            Vec3::new(leg_x, 0.124, 0.0),
            Quat::IDENTITY,
        );
        // Hip servo (y=0.161)
        push_cyl(ctx, cad, &format!("hip_{s}"), &format!("Hip {cap}"), SERVO,
            0.011, 0.018, Vec3::new(leg_x, 0.161, 0.0), rot90x);
    }
}

fn hitogata_torso(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    // Pelvis bridge (80×20×38mm)
    push_box(ctx, cad, "pelvis", "Pelvis", SHELL,
        Vec3::new(0.080, 0.020, 0.038),
        Vec3::new(0.0, 0.171, 0.0),
        Quat::IDENTITY,
    );
    // Torso shell (58×82×34mm, y 0.181–0.263)
    push_box(ctx, cad, "torso", "Torso", SHELL,
        Vec3::new(0.058, 0.082, 0.034),
        Vec3::new(0.0, 0.222, 0.0),
        Quat::IDENTITY,
    );
    // Blue chest accent panel
    push_box(ctx, cad, "chest_panel", "Chest Panel", ACCENT,
        Vec3::new(0.034, 0.040, 0.003),
        Vec3::new(0.0, 0.218, 0.019),
        Quat::IDENTITY,
    );
    // Neck servo (y=0.263)
    push_cyl(ctx, cad, "neck", "Neck", SERVO,
        0.009, 0.010, Vec3::new(0.0, 0.263, 0.0), Quat::IDENTITY);
    // Head (40×38×35mm)
    push_box(ctx, cad, "head", "Head", SHELL,
        Vec3::new(0.040, 0.038, 0.035),
        Vec3::new(0.0, 0.282, 0.0),
        Quat::IDENTITY,
    );
    // Blue camera visor strip
    push_box(ctx, cad, "visor", "Camera Visor", ACCENT,
        Vec3::new(0.032, 0.006, 0.003),
        Vec3::new(0.0, 0.286, 0.019),
        Quat::IDENTITY,
    );
}

fn hitogata_arms(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    let rot90x = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let arm_ang = 20_f32.to_radians(); // arms angled 20° outward from vertical

    for (sx, s, cap) in [(-1.0_f32, "l", "L"), (1.0_f32, "r", "R")] {
        let shoulder = Vec3::new(sx * 0.040, 0.255, 0.0);
        // arm direction: mostly -Y, slight ±X spread
        let arm_dir  = Vec3::new(sx * arm_ang.sin(), -arm_ang.cos(), 0.0);
        let arm_rot  = Quat::from_rotation_z(sx * arm_ang);

        // Shoulder servo
        push_cyl(ctx, cad, &format!("shoulder_{s}"), &format!("Shoulder {cap}"), SERVO,
            0.010, 0.014, shoulder, rot90x);
        // Upper arm (15×62×15mm)
        push_box(ctx, cad, &format!("upper_arm_{s}"), &format!("Upper Arm {cap}"), SHELL,
            Vec3::new(0.015, 0.062, 0.015),
            shoulder + arm_dir * 0.031,
            arm_rot,
        );
        // Elbow servo
        let elbow = shoulder + arm_dir * 0.062;
        push_cyl(ctx, cad, &format!("elbow_{s}"), &format!("Elbow {cap}"), SERVO,
            0.009, 0.012, elbow, rot90x);
        // Forearm (13×55×13mm)
        push_box(ctx, cad, &format!("forearm_{s}"), &format!("Forearm {cap}"), SHELL,
            Vec3::new(0.013, 0.055, 0.013),
            elbow + arm_dir * 0.028,
            arm_rot,
        );
        // Wrist servo + hand
        let wrist = elbow + arm_dir * 0.055;
        push_cyl(ctx, cad, &format!("wrist_{s}"), &format!("Wrist {cap}"), SERVO,
            0.008, 0.010, wrist, Quat::IDENTITY);
        push_box(ctx, cad, &format!("hand_{s}"), &format!("Hand {cap}"), GRIPPER,
            Vec3::new(0.014, 0.020, 0.009),
            wrist + arm_dir * 0.012,
            arm_rot,
        );
    }
}

// ── Caterpillar model ─────────────────────────────────────────────────────
// Giemon Caterpillar — heavy dual-track UGV, 380×300mm footprint.
// 6 drive wheels per side, 360° LiDAR + stereo camera. No manipulator arm.

pub fn build_caterpillar(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    caterpillar_body(ctx, cad);
    caterpillar_sensors(ctx, cad);
}

fn caterpillar_body(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    let rot90x = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);

    // Armoured body (380×80×200mm)
    push_box(ctx, cad, "cat_body", "Chassis Armour", ARMOR,
        Vec3::new(0.380, 0.080, 0.200),
        Vec3::new(0.0, 0.040, 0.0),
        Quat::IDENTITY,
    );

    // Rubber tracks (L/R, 400×60mm × 65mm wide each)
    for (id, name, tz) in [
        ("cat_track_l", "Left Track",  -0.132_f32),
        ("cat_track_r", "Right Track",  0.132_f32),
    ] {
        push_box(ctx, cad, id, name, RUBBER,
            Vec3::new(0.400, 0.060, 0.065),
            Vec3::new(0.0, 0.030, tz),
            Quat::IDENTITY,
        );
    }

    // Drive wheels — 6 per side (Ø 42mm, 52mm deep)
    let wheel_xs = [-0.150_f32, -0.090, -0.030, 0.030, 0.090, 0.150];
    for (si, &tz) in [-0.132_f32, 0.132_f32].iter().enumerate() {
        let s = if si == 0 { "l" } else { "r" };
        for (wi, &wx) in wheel_xs.iter().enumerate() {
            push_cyl(ctx, cad,
                &format!("cat_wh_{s}{wi}"),
                &format!("Drive Wheel {}{}", s.to_uppercase(), wi + 1),
                STEEL, 0.021, 0.052,
                Vec3::new(wx, 0.020, tz),
                rot90x,
            );
        }
    }

    // Electronics bay — front top (120×40×160mm, PCB green)
    push_box(ctx, cad, "cat_elec", "Electronics Bay", [0.10, 0.35, 0.15],
        Vec3::new(0.120, 0.040, 0.160),
        Vec3::new(0.100, 0.100, 0.0),
        Quat::IDENTITY,
    );

    // Battery pack — rear (140×35×160mm)
    push_box(ctx, cad, "cat_batt", "Battery Pack (18650 × 8)", [0.20, 0.20, 0.22],
        Vec3::new(0.140, 0.035, 0.160),
        Vec3::new(-0.110, 0.097, 0.0),
        Quat::IDENTITY,
    );

    // RPi 5 Compute board
    push_box(ctx, cad, "cat_rpi", "Raspberry Pi 5", [0.10, 0.32, 0.10],
        Vec3::new(0.085, 0.016, 0.056),
        Vec3::new(-0.120, 0.099, 0.0),
        Quat::IDENTITY,
    );
}

fn caterpillar_sensors(ctx: &RenderContext, cad: &kami_pipelines::CadSceneAdapter) {
    // Top sensor platform (aluminium, 220×10×160mm)
    push_box(ctx, cad, "cat_top", "Sensor Platform", ALUMINIUM,
        Vec3::new(0.220, 0.010, 0.160),
        Vec3::new(-0.010, 0.085, 0.0),
        Quat::IDENTITY,
    );

    // 360° LiDAR dome (Ø60mm, 28mm tall cylinder)
    push_cyl(ctx, cad, "lidar", "LiDAR 360°", SENSOR,
        0.030, 0.028, Vec3::new(-0.010, 0.099, 0.0), Quat::IDENTITY);

    // IMU + GPS puck (above LiDAR)
    push_cyl(ctx, cad, "imu_gps", "IMU + GPS", ALUMINIUM,
        0.015, 0.008, Vec3::new(-0.010, 0.113, 0.0), Quat::IDENTITY);

    // Camera mast pole (12×65×12mm)
    push_box(ctx, cad, "cam_mast", "Camera Mast", ALUMINIUM,
        Vec3::new(0.012, 0.065, 0.012),
        Vec3::new(0.085, 0.118, 0.0),
        Quat::IDENTITY,
    );

    // Stereo camera head (32×22×24mm)
    push_box(ctx, cad, "cam_head", "Stereo Camera", SENSOR,
        Vec3::new(0.032, 0.022, 0.024),
        Vec3::new(0.085, 0.161, 0.005),
        Quat::IDENTITY,
    );
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use kami_app::CameraMode;

    #[test]
    fn camera_mode_compiles() {
        let _ = CameraMode::Orbit {
            target: Vec3::ZERO,
            distance: 0.75,
            yaw: 0.55,
            pitch: 0.30,
        };
    }

    #[test]
    fn urdf_loads_as_6dof_arm() {
        // The committed URDF parses into a 7-body (base + 6 links), 6-DOF arm.
        let cfg = giemon_arm6_config();
        assert_eq!(cfg.ndof, 6, "expected 6 DOF");
        assert_eq!(cfg.n_bodies(), 7, "base + 6 links");
        assert_eq!(cfg.bodies[0].joint_type, kami_genesis::JointType3d::Fixed, "base fixed");
        assert!(cfg.bodies[1..].iter().all(|b| b.movable()), "links movable");
    }

    #[test]
    fn arm_steps_under_gravity_and_stays_finite() {
        // The 3-D solver advances the URDF arm under gravity without blow-up,
        // and the arm actually moves (droops) from a non-equilibrium pose.
        let cfg = giemon_arm6_config();
        let mut st = kami_genesis::Articulation3dState::zeros(cfg.ndof);
        st.q[1] = 0.6;
        st.q[2] = 1.0;
        let q_start = st.q.clone();
        let tau = vec![0.0_f32; cfg.ndof];
        for _ in 0..240 {
            cfg.step(&mut st, &tau);
        }
        assert!(st.q.iter().all(|x| x.is_finite()));
        assert!(st.qdot.iter().all(|x| x.is_finite()));
        assert!(st.q != q_start, "arm should move under gravity");
    }

    #[test]
    fn link_segments_are_well_formed() {
        // Every rendered link segment has positive length; the box transform
        // reproduces that length along its local +Z.
        let cfg = giemon_arm6_config();
        for i in 0..cfg.n_bodies() {
            let seg = link_segment(&cfg, i);
            assert!(seg.length() > 1.0e-3, "body {i} segment too short");
            let w = segment_box(seg, 0.03);
            let scale = w.to_scale_rotation_translation().0;
            assert!((scale.z - seg.length()).abs() < 1.0e-4, "body {i} length");
        }
    }
}
