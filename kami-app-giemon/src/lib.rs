//! kami-app-giemon — Giemon robot kit viewer (giemon.etzhayyim.com)
//!
//! Procedural ArmCrawler model:
//!   - Rubber-track crawler chassis (2 tracks + 8 drive wheels)
//!   - 6-DOF arm (J1–J6 links) in display pose
//!   - Gripper (2 fingers)
//!
//! Orbit camera + pick-to-highlight. No external assets.

use glam::{Mat4, Quat, Vec3};

pub mod mold_field;
pub use mold_field::MoldField;
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

// ── kabitori (黴取り / mold-removal) probe ──────────────────────────────────
// A slender cleaning manipulator for removing EXISTING mold from confined
// surfaces (A/C drain pans + blower housings, building gaps, HVAC ducts).
// Mixed prismatic-feed + revolute-segment articulation loaded from URDF and
// driven by the same kami-genesis 3-D spatial solver + contact solver as the
// arm above. The single contact ground plane stands in for the mold-laden
// surface; the brush head carries a capsule "bristle cross" whose endpoints
// scrub that plane under Coulomb friction. Clean-room (ADR-2605261800 N1..N9).
//
// Honest scope: rigid-body dynamics + ground-plane contact only. The mold
// biofilm is NOT an erodible material (no FEM/MPM solver in R1.1), and the
// duct/gap is a single plane (no wall colliders yet). What this validates:
// reach, contact-force regulation, and brush scrub shear on the target plane.

const KABITORI_URDF: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon_kabitori/giemon_kabitori.urdf");
const KABITORI_DT: f32 = 1.0 / 240.0;
/// Target mold surface = the contact ground plane, this far below the fixed
/// base (metres). The probe feeds in and droops its brush down onto it.
const KABITORI_SURFACE_Z: f32 = -0.22;

/// Parse the kabitori URDF and build the 3-D articulation config.
pub fn giemon_kabitori_config() -> kami_genesis::Articulation3dConfig {
    let sys =
        kami_articulated::parse_urdf(KABITORI_URDF).expect("giemon_kabitori.urdf parses");
    kami_genesis::Articulation3dConfig::from_articulated_system(
        &sys,
        Vec3::new(0.0, 0.0, -9.81),
        KABITORI_DT,
    )
}

/// Brush + distal-probe colliders (body-frame), vs the mold-surface plane.
/// The brush head is a capsule "cross" (±y and ±z bars) so a bristle endpoint
/// is near the surface at any spin angle; the distal segment gets a body
/// capsule so the probe shaft cannot pass through the surface either.
pub fn kabitori_colliders(
    cfg: &kami_genesis::Articulation3dConfig,
) -> Vec<(usize, kami_genesis::Collider)> {
    use kami_genesis::Collider;
    let mut c = Vec::new();
    if let Some(b) = cfg.body_index("link_seg2") {
        c.push((b, Collider::Capsule {
            a: Vec3::new(0.0, 0.0, 0.0),
            b: Vec3::new(0.08, 0.0, 0.0),
            radius: 0.012,
        }));
    }
    if let Some(b) = cfg.body_index("link_brush") {
        // Bristle cross through the head at body-x = 0.02.
        c.push((b, Collider::Capsule {
            a: Vec3::new(0.02, -0.025, 0.0),
            b: Vec3::new(0.02, 0.025, 0.0),
            radius: 0.012,
        }));
        c.push((b, Collider::Capsule {
            a: Vec3::new(0.02, 0.0, -0.025),
            b: Vec3::new(0.02, 0.0, 0.025),
            radius: 0.012,
        }));
    }
    c
}

/// Brush bristle-cross endpoints (body frame) — used both as colliders and as
/// the scrub footprint sample points.
const KABITORI_BRUSH_TIPS: [Vec3; 4] = [
    Vec3::new(0.02, -0.025, 0.0),
    Vec3::new(0.02, 0.025, 0.0),
    Vec3::new(0.02, 0.0, -0.025),
    Vec3::new(0.02, 0.0, 0.025),
];
const KABITORI_BRUSH_RADIUS: f32 = 0.012;
/// Mold removed per metre of tangential brush slip while pressed (coverage/m).
const KABITORI_SCRUB_RATE: f32 = 6.0;
/// Brush footprint radius on the surface (m).
const KABITORI_BRUSH_FOOTPRINT: f32 = 0.03;

/// Advance the kabitori sim one contact step, then erode the mold field where
/// the brush both presses the surface and slides over it. Returns the coverage
/// removed this step. Erosion ∝ tangential slip speed × dt (a pressure proxy is
/// folded into `KABITORI_SCRUB_RATE`); zero when the brush is not in contact.
pub fn kabitori_scrub_step(
    cfg: &kami_genesis::Articulation3dConfig,
    cw: &kami_genesis::ContactWorld,
    st: &mut kami_genesis::Articulation3dState,
    tau: &[f32],
    mold: &mut MoldField,
) -> f32 {
    cw.step(cfg, st, tau);
    let bi = match cfg.body_index("link_brush") {
        Some(b) => b,
        None => return 0.0,
    };
    let (r, p0) = cfg.link_world(&st.q)[bi];
    // Lowest bristle-tip endpoint ≈ the surface contact point.
    let mut cp = p0;
    let mut zmin = f32::INFINITY;
    for tip in KABITORI_BRUSH_TIPS {
        let w = p0 + r * tip;
        if w.z < zmin {
            zmin = w.z;
            cp = w;
        }
    }
    // Pressing the surface? (within a thin band above the plane.)
    let surface = cw.params.ground_z;
    if zmin > surface + KABITORI_BRUSH_RADIUS + 0.012 {
        return 0.0;
    }
    // Tangential slip speed of the contact point (horizontal components).
    let pj = cfg.point_jacobian(bi, cp, &st.q);
    let mut v = Vec3::ZERO;
    for d in 0..cfg.ndof {
        v += st.qdot[d] * Vec3::from(pj[d]);
    }
    let v_tangent = (v.x * v.x + v.y * v.y).sqrt();
    let intensity = KABITORI_SCRUB_RATE * v_tangent * cfg.dt;
    mold.scrub(cp.x, cp.y, KABITORI_BRUSH_FOOTPRINT, intensity)
}

const SURFACE: [f32; 3] = [0.30, 0.34, 0.30]; // mold-laden target surface (dark)
const MOLD: [f32; 3] = [0.34, 0.42, 0.28]; // mold patch tint (olive-green)
const BRUSH: [f32; 3] = [0.93, 0.84, 0.30]; // brush head (bristle yellow)
const PROBE: [f32; 3] = [0.62, 0.66, 0.70]; // probe shaft (steel)

/// Physics-driven kabitori (mold-removal) probe demo. The probe feeds into a
/// gap, droops its brush onto the mold surface (contact ground plane), and
/// scrubs autonomously (continuous brush spin + yaw sweep) — all advanced by
/// the kami-genesis 3-D solver + contact solver. Clean-room (ADR-2605261800).
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_kabitori_sim_v1(canvas_id: &str) -> Result<(), JsValue> {
    use glam::Vec2;
    use kami_genesis::{Articulation3dState, ContactParams, ContactWorld, Obstacle};

    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon-kabitori")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.34, -0.10, 0.0),
            distance: 1.05,
            yaw: 0.6,
            pitch: 0.18,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let (bp, bn, bi) = unit_box();

    let cfg = giemon_kabitori_config();
    let nb = cfg.n_bodies();
    log::info!("[kabitori-sim] probe: bodies={nb} ndof={}", cfg.ndof);

    // kami-genesis is z-up; the renderer is y-up → rotate −90° about X.
    // (world (x,y,z) → render (x, z, −y)), so the surface at world z=−0.22
    // renders at y=−0.22.
    let to_render = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let surf_y = KABITORI_SURFACE_Z;

    // Mold-laden target surface (the contact plane) + two gap walls (real
    // physics obstacles, §1) flanking the work zone + the base mount.
    push_box(ctx, &cad, "surface", "Mold surface", SURFACE,
        Vec3::new(1.4, 0.01, 1.0), Vec3::new(0.45, surf_y - 0.005, 0.0), Quat::IDENTITY);
    push_box(ctx, &cad, "gap_wall_l", "Gap wall L", SURFACE,
        Vec3::new(1.4, 0.30, 0.02), Vec3::new(0.45, surf_y + 0.15, -0.18), Quat::IDENTITY);
    push_box(ctx, &cad, "gap_wall_r", "Gap wall R", SURFACE,
        Vec3::new(1.4, 0.30, 0.02), Vec3::new(0.45, surf_y + 0.15, 0.18), Quat::IDENTITY);
    push_box(ctx, &cad, "base_mount", "Base mount", ALUMINIUM,
        Vec3::new(0.10, 0.06, 0.10), Vec3::new(0.0, 0.03, 0.0), Quat::IDENTITY);

    // Live mold field (§2): a grid of cells on the surface, world (x, y),
    // coloured by remaining coverage. Erodes where the brush scrubs.
    let mold_origin = Vec2::new(0.18, -0.15);
    let mold_cell = 0.03_f32;
    let (mold_nx, mold_ny) = (16_usize, 10_usize);
    let mut mold = MoldField::new(mold_origin, mold_cell, mold_nx, mold_ny, 1.0);
    let mold_start = cad.batch_count();
    let cell_local = Mat4::from_scale(Vec3::new(mold_cell * 0.92, 0.004, mold_cell * 0.92));
    for iy in 0..mold_ny {
        for ix in 0..mold_nx {
            let wx = mold_origin.x + (ix as f32 + 0.5) * mold_cell;
            let wy = mold_origin.y + (iy as f32 + 0.5) * mold_cell;
            // world (wx, wy, surf) → render (wx, surf, −wy)
            let m = Mat4::from_translation(Vec3::new(wx, surf_y + 0.003, -wy)) * cell_local;
            cad.push_triangles(ctx, format!("mold_{ix}_{iy}"), format!("Mold {ix},{iy}"),
                &bp, &bn, &bi, MOLD, m);
        }
    }

    // Per-body probe geometry (thin shaft, fatter brush head at the leaf).
    let segs: Vec<Vec3> = (0..nb).map(|i| link_segment(&cfg, i)).collect();
    let thicks: Vec<f32> = (0..nb)
        .map(|i| if i + 1 == nb { 0.030 } else { (0.026 - 0.002 * i as f32).max(0.012) })
        .collect();
    let link_local: Vec<Mat4> = (0..nb).map(|i| segment_box(segs[i], thicks[i])).collect();

    let link_start = cad.batch_count();
    let fk0 = cfg.fk_world(&vec![0.0; cfg.ndof]);
    for i in 0..nb {
        let color = if i + 1 == nb { BRUSH } else { PROBE };
        cad.push_triangles(ctx, format!("kab_link_{i}"), format!("Probe link {i}"),
            &bp, &bn, &bi, color, to_render * fk0[i] * link_local[i]);
    }

    let contacts = ContactWorld::new(
        kabitori_colliders(&cfg),
        ContactParams { ground_z: KABITORI_SURFACE_Z, friction: 0.9, ..Default::default() },
    )
    .with_obstacles(vec![
        Obstacle::Plane { normal: Vec3::new(0.0, 1.0, 0.0), offset: -0.18 },
        Obstacle::Plane { normal: Vec3::new(0.0, -1.0, 0.0), offset: -0.18 },
    ]);

    // Start straight out, above the surface; the controller drives it in.
    let mut state = Articulation3dState::zeros(cfg.ndof);
    state.q[2] = 0.2;
    let mut frame: u64 = 0;

    let render = cad.clone();
    let pick = cad.clone();
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            // Autonomous scrub program: feed in, dip pitch, curl segments to
            // hold the brush on the surface, spin the brush, sweep the yaw.
            let t = frame as f32 / 240.0;
            let mut tau = vec![0.0_f32; cfg.ndof];
            tau[0] = if state.q[0] < 0.28 { 8.0 } else { 0.0 }; // feed in, then hold
            tau[1] = 1.2 * (0.7 * t).sin();                     // yaw sweep (scrub L↔R)
            tau[2] = 4.0;                                       // pitch press-down
            tau[3] = 2.0;                                       // seg1 curl
            tau[4] = 1.2;                                       // seg2 curl
            tau[5] = 4.0;                                       // brush spin (constant)
            for _ in 0..4 {
                // Step physics AND erode the mold where the brush scrubs (§2).
                kabitori_scrub_step(&cfg, &contacts, &mut state, &tau, &mut mold);
            }
            frame += 1;
            let fk = cfg.fk_world(&state.q);
            for i in 0..nb {
                let color = if i + 1 == nb { BRUSH } else { PROBE };
                render.replace_batch_world(link_start + i, &bp, &bn, &bi, color,
                    to_render * fk[i] * link_local[i]);
            }
            // Recolour mold cells by remaining coverage (green mold → clean).
            for iy in 0..mold_ny {
                for ix in 0..mold_nx {
                    let cov = mold.coverage[iy * mold_nx + ix];
                    let color = [
                        SURFACE[0] + (MOLD[0] - SURFACE[0]) * cov,
                        SURFACE[1] + (MOLD[1] - SURFACE[1]) * cov,
                        SURFACE[2] + (MOLD[2] - SURFACE[2]) * cov,
                    ];
                    let wx = mold_origin.x + (ix as f32 + 0.5) * mold_cell;
                    let wy = mold_origin.y + (iy as f32 + 0.5) * mold_cell;
                    let m = Mat4::from_translation(Vec3::new(wx, surf_y + 0.003, -wy)) * cell_local;
                    render.replace_batch_world(mold_start + iy * mold_nx + ix,
                        &bp, &bn, &bi, color, m);
                }
            }
            if let Some(p) = pick.pick_from_camera_if_clicked(camera) {
                pick.set_highlighted_by_id(&p.feature_id);
            }
        });

    log::info!("[kabitori-sim] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── otete (御手) — 6-axis arm + gripper, 7-DOF physics sim ───────────────────
// The Giemon Otete kit (open-robo BOM) as a fixed-base 7-DOF articulation
// (6 revolute arm joints + 1 prismatic gripper) on the kami-genesis 3-D spatial
// solver + ground contact. Shares the J/L + 1–6 keyboard controls with arm6.

const OTETE_URDF: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon_otete/giemon_otete.urdf");

/// Parse the otete URDF → 3-D articulation config (7 DOF).
pub fn giemon_otete_config() -> kami_genesis::Articulation3dConfig {
    let sys = kami_articulated::parse_urdf(OTETE_URDF).expect("giemon_otete.urdf parses");
    kami_genesis::Articulation3dConfig::from_articulated_system(
        &sys,
        Vec3::new(0.0, 0.0, -9.81),
        ARM_DT,
    )
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_otete_sim_v1(canvas_id: &str) -> Result<(), JsValue> {
    use kami_genesis::{Articulation3dState, Collider, ContactParams, ContactWorld};

    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon-otete")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 0.32, 0.0),
            distance: 1.15,
            yaw: 0.7,
            pitch: 0.22,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let (bp, bn, bi) = unit_box();

    let cfg = giemon_otete_config();
    let nb = cfg.n_bodies();
    log::info!("[otete-sim] arm: bodies={nb} ndof={}", cfg.ndof);

    let to_render = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    push_box(ctx, &cad, "ground", "Ground", GROUND,
        Vec3::new(2.0, 0.01, 2.0), Vec3::new(0.0, -0.005, 0.0), Quat::IDENTITY);
    push_box(ctx, &cad, "base_plate", "Base plate", ALUMINIUM,
        Vec3::new(0.20, 0.02, 0.20), Vec3::new(0.0, 0.01, 0.0), Quat::IDENTITY);

    let segs: Vec<Vec3> = (0..nb).map(|i| link_segment(&cfg, i)).collect();
    let thicks: Vec<f32> = (0..nb).map(|i| (0.044 - 0.004 * i as f32).max(0.014)).collect();
    let link_local: Vec<Mat4> = (0..nb).map(|i| segment_box(segs[i], thicks[i])).collect();

    let link_start = cad.batch_count();
    let fk0 = cfg.fk_world(&vec![0.0; cfg.ndof]);
    for i in 0..nb {
        cad.push_triangles(ctx, format!("link_{i}"), format!("Link {i}"),
            &bp, &bn, &bi, SHELL, to_render * fk0[i] * link_local[i]);
    }
    let joint_start = cad.batch_count();
    for i in 0..nb {
        let color = if cfg.bodies[i].movable() { SERVO } else { ACCENT };
        cad.push_triangles(ctx, format!("joint_{i}"), format!("Joint {i}"),
            &bp, &bn, &bi, color,
            to_render * fk0[i] * Mat4::from_scale(Vec3::splat(thicks[i] * 1.3)));
    }

    let colliders: Vec<(usize, Collider)> = (1..nb)
        .map(|i| (i, Collider::Sphere { center: segs[i], radius: thicks[i] * 0.6 }))
        .collect();
    let contacts = ContactWorld::new(colliders, ContactParams { ground_z: 0.0, friction: 0.9, ..Default::default() });

    let mut state = Articulation3dState::zeros(cfg.ndof);
    if cfg.ndof >= 3 { state.q[1] = 0.5; state.q[2] = 0.9; }

    let render = cad.clone();
    let pick = cad.clone();
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            let torque = JOINT_TORQUE.with(|t| t.get());
            let sel = SELECTED_JOINT.with(|s| s.get()).min(cfg.ndof.saturating_sub(1));
            let mut tau = vec![0.0_f32; cfg.ndof];
            if cfg.ndof > 0 { tau[sel] = torque; }
            for _ in 0..4 {
                contacts.step(&cfg, &mut state, &tau);
            }
            let fk = cfg.fk_world(&state.q);
            for i in 0..nb {
                render.replace_batch_world(link_start + i, &bp, &bn, &bi, SHELL,
                    to_render * fk[i] * link_local[i]);
                let color = if cfg.bodies[i].movable() { SERVO } else { ACCENT };
                render.replace_batch_world(joint_start + i, &bp, &bn, &bi, color,
                    to_render * fk[i] * Mat4::from_scale(Vec3::splat(thicks[i] * 1.3)));
            }
            if let Some(p) = pick.pick_from_camera_if_clicked(camera) {
                pick.set_highlighted_by_id(&p.feature_id);
            }
        });

    log::info!("[otete-sim] backend={:?}", app.backend());
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

    // ── kabitori (mold-removal) probe ───────────────────────────────────────

    /// Lowest collider-centre world z over the kabitori brush/probe colliders
    /// (used to detect surface tunnelling).
    fn kabitori_min_collider_z(
        cfg: &kami_genesis::Articulation3dConfig,
        st: &kami_genesis::Articulation3dState,
    ) -> f32 {
        use kami_genesis::Collider;
        let lw = cfg.link_world(&st.q);
        let mut zmin = f32::INFINITY;
        for (body, col) in kabitori_colliders(cfg) {
            let (r, p0) = lw[body];
            let pts = match col {
                Collider::Sphere { center, .. } => vec![center],
                Collider::Capsule { a, b, .. } => vec![a, b],
            };
            for c in pts {
                zmin = zmin.min((p0 + r * c).z);
            }
        }
        zmin
    }

    #[test]
    fn kabitori_urdf_parses_mixed_topology() {
        use kami_genesis::JointType3d;
        let cfg = giemon_kabitori_config();
        assert_eq!(cfg.ndof, 6, "1 prismatic feed + 5 revolute");
        assert_eq!(cfg.n_bodies(), 7, "base + 6 links");
        assert_eq!(cfg.bodies[0].joint_type, JointType3d::Fixed, "base fixed");
        let prismatic = cfg.bodies.iter()
            .filter(|b| b.joint_type == JointType3d::Prismatic).count();
        let revolute = cfg.bodies.iter()
            .filter(|b| b.joint_type == JointType3d::Revolute).count();
        assert_eq!(prismatic, 1, "feed is the only prismatic DOF");
        assert_eq!(revolute, 5, "yaw, pitch, seg1, seg2, brush");
        assert!(cfg.body_index("link_brush").is_some(), "brush head exists");
        assert!(!kabitori_colliders(&cfg).is_empty(), "brush/probe colliders exist");
    }

    #[test]
    fn kabitori_feed_inserts_probe_into_gap() {
        // The prismatic feed advances the carriage (and the whole probe root)
        // into the gap along +x. Measured at the carriage, which the feed
        // drives directly — the distal links additionally droop under gravity,
        // so the feed DOF and the carriage x are the clean insertion signal.
        use kami_genesis::Articulation3dState;
        let cfg = giemon_kabitori_config();
        let mut st = Articulation3dState::zeros(cfg.ndof);
        let ci = cfg.body_index("link_carriage").expect("carriage body");
        let x0 = cfg.link_world(&st.q)[ci].1.x;
        let mut tau = vec![0.0_f32; cfg.ndof];
        tau[0] = 10.0; // feed forward
        for _ in 0..1500 {
            cfg.step(&mut st, &tau);
        }
        assert!(st.q.iter().all(|x| x.is_finite()), "feed state finite");
        assert!(st.q[0] > 0.10, "feed DOF should advance into the gap: q_feed={}", st.q[0]);
        let x1 = cfg.link_world(&st.q)[ci].1.x;
        assert!(x1 - x0 > 0.10, "carriage should translate into the gap: dx={}", x1 - x0);
    }

    #[test]
    fn kabitori_probe_reaches_and_contacts_surface() {
        // Feed in + dip pitch + curl segments + oscillate-spin the brush: the
        // brush/probe must reach the mold surface (contact) without tunnelling.
        use kami_genesis::{Articulation3dState, ContactParams, ContactWorld};
        let cfg = giemon_kabitori_config();
        let cw = ContactWorld::new(
            kabitori_colliders(&cfg),
            ContactParams { ground_z: KABITORI_SURFACE_Z, friction: 0.9, ..Default::default() },
        );
        let mut st = Articulation3dState::zeros(cfg.ndof);
        st.q[2] = 0.3; // small initial pitch dip
        let mut saw_contact = false;
        for k in 0..2500 {
            let mut tau = vec![0.0_f32; cfg.ndof];
            tau[0] = 6.0; // feed forward
            tau[2] = 4.0; // pitch dip (down)
            tau[3] = 2.0; // seg1 curl
            tau[4] = 1.2; // seg2 curl
            tau[5] = if (k / 120) % 2 == 0 { 3.0 } else { -3.0 }; // brush spin
            cw.step(&cfg, &mut st, &tau);
            saw_contact |= cw.contact_count(&cfg, &st.q) > 0;
        }
        assert!(st.q.iter().all(|x| x.is_finite()), "q finite");
        assert!(st.qdot.iter().all(|x| x.is_finite()), "qdot finite");
        assert!(saw_contact, "brush/probe never reached the mold surface");
        let zmin = kabitori_min_collider_z(&cfg, &st);
        assert!(zmin >= KABITORI_SURFACE_Z - 0.03, "tunnelled through surface: zmin={zmin}");
    }

    #[test]
    fn kabitori_contact_is_stable_once_settled() {
        // Passivity at the contact: let the probe fall from a non-penetrating
        // pose and settle on the surface under zero actuation, then verify that
        // over the following phase the total mechanical energy does not grow
        // (no Baumgarte limit-cycle pumping) and the pose stays finite.
        use kami_genesis::{Articulation3dState, ContactParams, ContactWorld};
        let cfg = giemon_kabitori_config();
        let cw = ContactWorld::new(
            kabitori_colliders(&cfg),
            ContactParams { ground_z: KABITORI_SURFACE_Z, friction: 0.9, ..Default::default() },
        );
        // Start straight out (links at z≈0, well above the surface at −0.22):
        // no initial penetration → no push-out spike.
        let mut st = Articulation3dState::zeros(cfg.ndof);
        assert!(
            kabitori_min_collider_z(&cfg, &st) > KABITORI_SURFACE_Z,
            "start pose must not pre-penetrate the surface"
        );
        let tau = vec![0.0_f32; cfg.ndof];
        // Warm-up: droop under gravity and settle onto the surface.
        for _ in 0..3000 {
            cw.step(&cfg, &mut st, &tau);
        }
        let e_settled = cfg.energy(&st);
        let mut emax = e_settled;
        for _ in 0..3000 {
            cw.step(&cfg, &mut st, &tau);
            emax = emax.max(cfg.energy(&st));
        }
        assert!(st.q.iter().all(|x| x.is_finite()), "q finite");
        assert!(st.qdot.iter().all(|x| x.is_finite()), "qdot finite");
        assert!(
            emax <= e_settled + 0.05 * e_settled.abs().max(1.0),
            "settled energy grew (pumping): e_settled={e_settled} emax={emax}"
        );
    }

    #[test]
    fn kabitori_scrub_erodes_mold_locally() {
        // Run the autonomous feed→dip→scrub program against a mold field on the
        // surface: total mold coverage must drop (mold removed), while a corner
        // far from the brush path stays fully covered (erosion is localised).
        use glam::Vec2;
        use kami_genesis::{Articulation3dState, ContactParams, ContactWorld};
        let cfg = giemon_kabitori_config();
        let cw = ContactWorld::new(
            kabitori_colliders(&cfg),
            ContactParams { ground_z: KABITORI_SURFACE_Z, friction: 0.9, ..Default::default() },
        );
        // Coverage grid on the surface plane, spanning the work zone.
        let mut mold = MoldField::new(Vec2::new(0.05, -0.30), 0.02, 45, 30, 1.0);
        let total0 = mold.total_coverage();
        let corner = mold.coverage_at(0.07, -0.28).expect("corner cell in grid");

        let mut st = Articulation3dState::zeros(cfg.ndof);
        st.q[2] = 0.3;
        let mut removed = 0.0_f32;
        for k in 0..4000 {
            let mut tau = vec![0.0_f32; cfg.ndof];
            tau[0] = 6.0; // feed in
            tau[1] = 1.2 * (k as f32 / 240.0 * 0.7).sin(); // yaw sweep (scrub L↔R)
            tau[2] = 4.0; // pitch press-down
            tau[3] = 2.0; // seg1 curl
            tau[4] = 1.2; // seg2 curl
            tau[5] = 4.0; // brush spin
            removed += kabitori_scrub_step(&cfg, &cw, &mut st, &tau, &mut mold);
        }
        assert!(st.q.iter().all(|x| x.is_finite()), "state finite");
        assert!(removed > 0.2, "brush should remove mold coverage: removed={removed}");
        assert!(mold.total_coverage() < total0 - 0.2, "total coverage must drop");
        assert_eq!(
            mold.coverage_at(0.07, -0.28),
            Some(corner),
            "a corner far from the brush path stays untouched"
        );
        assert!(mold.coverage.iter().all(|&c| c >= 0.0), "no negative coverage");
    }

    #[test]
    fn kabitori_gap_walls_confine_the_probe() {
        // Box-in the gap with two side-wall half-spaces (the engine extension):
        // even under an aggressive yaw sweep the brush must stay between the
        // walls and the sim must stay finite.
        use kami_genesis::{Articulation3dState, ContactParams, ContactWorld, Obstacle};
        let cfg = giemon_kabitori_config();
        let cw = ContactWorld::new(
            kabitori_colliders(&cfg),
            ContactParams { ground_z: KABITORI_SURFACE_Z, friction: 0.9, ..Default::default() },
        )
        .with_obstacles(vec![
            Obstacle::Plane { normal: Vec3::new(0.0, 1.0, 0.0), offset: -0.18 },
            Obstacle::Plane { normal: Vec3::new(0.0, -1.0, 0.0), offset: -0.18 },
        ]);
        let bi = cfg.body_index("link_brush").expect("brush body");
        let mut st = Articulation3dState::zeros(cfg.ndof);
        st.q[2] = 0.3;
        let mut y_abs_max = 0.0_f32;
        for k in 0..4000 {
            let mut tau = vec![0.0_f32; cfg.ndof];
            tau[0] = 6.0;
            tau[1] = 2.5 * (k as f32 / 240.0 * 1.2).sin(); // aggressive yaw → push on walls
            tau[2] = 4.0;
            tau[3] = 2.0;
            tau[4] = 1.2;
            tau[5] = 4.0;
            cw.step(&cfg, &mut st, &tau);
            let (r, p0) = cfg.link_world(&st.q)[bi];
            for tip in KABITORI_BRUSH_TIPS {
                y_abs_max = y_abs_max.max((p0 + r * tip).y.abs());
            }
        }
        assert!(st.q.iter().all(|x| x.is_finite()), "state finite");
        assert!(
            y_abs_max <= 0.18 + KABITORI_BRUSH_RADIUS + 0.03,
            "brush escaped the gap walls: |y|max={y_abs_max}"
        );
    }

    // ── otete (御手) 6-axis arm + gripper ────────────────────────────────────

    #[test]
    fn otete_urdf_loads_7dof_mixed() {
        use kami_genesis::JointType3d;
        let cfg = giemon_otete_config();
        assert_eq!(cfg.ndof, 7, "6 revolute arm + 1 prismatic gripper");
        assert_eq!(cfg.n_bodies(), 8, "base + 7 links");
        let prismatic = cfg.bodies.iter().filter(|b| b.joint_type == JointType3d::Prismatic).count();
        let revolute = cfg.bodies.iter().filter(|b| b.joint_type == JointType3d::Revolute).count();
        assert_eq!(prismatic, 1, "gripper is prismatic");
        assert_eq!(revolute, 6, "6 arm joints");
        assert!(cfg.body_index("link_grip").is_some());
    }

    #[test]
    fn otete_steps_under_gravity_stays_finite() {
        let cfg = giemon_otete_config();
        let mut st = kami_genesis::Articulation3dState::zeros(cfg.ndof);
        st.q[1] = 0.5;
        st.q[2] = 0.9;
        let q0 = st.q.clone();
        let tau = vec![0.0_f32; cfg.ndof];
        for _ in 0..240 {
            cfg.step(&mut st, &tau);
        }
        assert!(st.q.iter().all(|x| x.is_finite()), "q finite");
        assert!(st.qdot.iter().all(|x| x.is_finite()), "qdot finite");
        assert!(st.q != q0, "arm should move under gravity");
    }
}
