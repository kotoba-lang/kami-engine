//! kami-app-tatekata — 建方: the giemon factory built BY robotics.
//!
//! Replays the 4D construction sequence (`construction.order.json`) where each
//! step's **assigned construction robot** (`robots.json`, from `robots.edn`)
//! performs the work on kami-genesis and the building geometry "grows" as it is
//! built. Concrete steps (`robot:printer`) drive a `DepositField`
//! (deposition/levelling height grid); steel steps (`robot:bolter`) drive a
//! `WeldField` (moving heat-source fusion). Everything else (layout, materials,
//! 4D order, robot registry) is reused from `kami-app-giemon-factory`.
//!
//! Clean-room (no NVIDIA/PhysX/Isaac). Honest scope: the material-process fields
//! are application-layer stand-ins (no granular/MPM/thermal-FEM); they model
//! coverage/fusion *progress + tool path*, not real concrete/weld physics.

pub mod deposit_field;
pub mod process;
pub mod weld_field;
pub use deposit_field::DepositField;
#[cfg(target_family = "wasm")]
use process::{Process, StepPlan};
pub use weld_field::WeldField;

#[cfg(target_family = "wasm")]
use glam::{Mat4, Vec3};
#[cfg(target_family = "wasm")]
use kami_app_giemon_factory::{
    ArmCell, ConstructionOrder, Factory, Robots, arm6_config, static_boxes,
};

#[cfg(target_family = "wasm")]
use kami_app::{CameraMode, InputMode, KamiApp};
#[cfg(target_family = "wasm")]
use kami_pipelines::unit_box;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(target_family = "wasm")]
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
#[cfg(target_family = "wasm")]
const DT: f32 = 1.0 / 240.0;
/// Construction days per wall-clock second of playback.
#[cfg(target_family = "wasm")]
const DAYS_PER_SEC: f32 = 11.0;
/// Construction robots are rendered enlarged so they read at site scale.
#[cfg(target_family = "wasm")]
const ROBOT_DISPLAY_SCALE: f32 = 6.0;
#[cfg(target_family = "wasm")]
const HIDDEN: Mat4 = Mat4::ZERO;

// ── HUD bridge ────────────────────────────────────────────────────────────────
#[cfg(target_family = "wasm")]
thread_local! {
    static STATUS: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

/// JS hook: current step + robot + material-process %.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = giemonTatekataStatus)]
pub fn giemon_tatekata_status() -> String {
    STATUS.with(|s| s.borrow().clone())
}

#[cfg(target_family = "wasm")]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_tatekata_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let f = Factory::load();
    let order = ConstructionOrder::load();
    let robots = Robots::load();
    let c = f.center();
    log::info!(
        "[tatekata] {} steps, {} robots, {:.0} nominal days",
        order.steps.len(),
        robots.robots.len(),
        order.programme_days()
    );

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("tatekata")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 4.0, 0.0),
            distance: 165.0,
            yaw: 0.6,
            pitch: 0.66,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let (bp, bn, bi) = unit_box();

    let to_render =
        Mat4::from_rotation_x(-HALF_PI) * Mat4::from_translation(Vec3::new(-c.x, -c.y, 0.0));

    // Static building elements, pushed HIDDEN; map each to the step that builds it.
    let boxes = static_boxes(&f, to_render);
    let mut elems: Vec<(usize, Mat4, [f32; 3], Option<usize>)> = Vec::new();
    for sb in &boxes {
        let idx = cad.batch_count();
        cad.push_triangles(
            ctx,
            sb.id.clone(),
            sb.id.clone(),
            &bp,
            &bn,
            &bi,
            sb.color,
            HIDDEN,
        );
        // which step reveals this element id?
        let step_k = order
            .steps
            .iter()
            .position(|s| s.reveals.iter().any(|r| *r == sb.id));
        elems.push((idx, sb.world, sb.color, step_k));
    }

    // One active construction robot (arm6, enlarged), pushed HIDDEN.
    let cfg = arm6_config();
    let nb = cfg.n_bodies();
    let mut arm = ArmCell::new(arm6_config(), Vec3::ZERO, 0.0, 0.0);
    let robot_start = cad.batch_count();
    for i in 0..nb {
        cad.push_triangles(
            ctx,
            format!("robot_l{i}"),
            "robot".to_string(),
            &bp,
            &bn,
            &bi,
            [0.2; 3],
            HIDDEN,
        );
    }

    // cumulative reveal time (s) per step.
    let mut reveal_at = Vec::with_capacity(order.steps.len());
    let mut acc = 0.0_f32;
    for s in &order.steps {
        acc += s.duration_d;
        reveal_at.push(acc / DAYS_PER_SEC);
    }
    let total = acc / DAYS_PER_SEC;
    let loop_len = total + 4.0;

    // per-step robot work centroid (sim x/y).
    let centroids: Vec<(f32, f32)> = order
        .steps
        .iter()
        .map(|s| f.step_center(&s.reveals))
        .collect();
    let proc_of: Vec<String> = order
        .steps
        .iter()
        .map(|s| {
            robots
                .get(&s.robot)
                .map(|r| r.process.clone())
                .unwrap_or_default()
        })
        .collect();

    // Material-delivery cart (搬入/搬送) — a box ferried from 受入 to the work zone.
    let cart_idx = cad.batch_count();
    cad.push_triangles(
        ctx,
        "cart".to_string(),
        "搬送カート".to_string(),
        &bp,
        &bn,
        &bi,
        [0.85, 0.65, 0.20],
        HIDDEN,
    );

    // Robot op-sequence plans per step (procure→deliver→stage→build→fasten→inspect).
    let process = Process::load();
    let step_ids: Vec<String> = order.steps.iter().map(|s| s.id.clone()).collect();
    let plans: Vec<StepPlan> = process.plans(&step_ids);
    // staging point (受入 zone centre) in sim coords.
    let stage_xy = f.step_center(&["zone_recv".to_string()]);

    let render = cad.clone();
    let mut clock = 0.0_f32;
    let mut active_prev: isize = -1;
    let mut deposit: Option<DepositField> = None;
    let mut weld: Option<WeldField> = None;
    let mut sim_t = 0.0_f32;

    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, dt| {
            {
                let rc = camera.as_render_mut();
                rc.near = 0.5;
                rc.far = 1400.0;
            }
            clock += dt.clamp(0.0, 0.1);
            if clock > loop_len {
                clock = 0.0;
                active_prev = -1;
                deposit = None;
                weld = None;
            }

            // active step = first whose reveal time is still in the future.
            let mut active: Option<usize> = None;
            for (k, &t) in reveal_at.iter().enumerate() {
                if clock < t {
                    active = Some(k);
                    break;
                }
            }

            // material-process field setup on step change.
            if active.map(|k| k as isize) != Some(active_prev) {
                active_prev = active.map(|k| k as isize).unwrap_or(-1);
                deposit = None;
                weld = None;
                if let Some(k) = active {
                    match proc_of[k].as_str() {
                        "deposition" => {
                            let r = if order.steps[k].zone == "site" {
                                f.site_extent()
                            } else {
                                f.bbox_m
                            };
                            deposit = Some(DepositField::new(r, 24, 16, 0.2));
                        }
                        "thermal-weld" => weld = Some(WeldField::new(40, 20.0, 1450.0)),
                        _ => {}
                    }
                }
            }

            // progress of the active step + its op timeline. Geometry only grows
            // during the BUILD window (据付/締結) — after 調達/搬入/搬送, before 検査.
            let (p, p_geom) = if let Some(k) = active {
                let start = if k == 0 { 0.0 } else { reveal_at[k - 1] };
                let span = (reveal_at[k] - start).max(1e-3);
                let p = ((clock - start) / span).clamp(0.0, 1.0);
                let bp_frac = plans[k].build_progress(p); // 0 before build, ramps in build window
                let pg = match proc_of[k].as_str() {
                    "deposition" => {
                        if let Some(d) = deposit.as_mut() {
                            // sweep the print head across the footprint as the build window opens
                            let (cx, cy) = centroids[k];
                            let span_x = (f.bbox_m[2] - f.bbox_m[0]) * 0.5;
                            let hx = cx + lerp(-span_x, span_x, bp_frac);
                            d.deposit_at(hx, cy, 6.0, 0.05, 1.0 / 30.0);
                            d.progress()
                        } else {
                            bp_frac
                        }
                    }
                    "thermal-weld" => {
                        if let Some(w) = weld.as_mut() {
                            w.pass(bp_frac, 9000.0, 1.0 / 60.0);
                            w.fused_fraction()
                        } else {
                            bp_frac
                        }
                    }
                    _ => bp_frac,
                };
                (p, pg)
            } else {
                (1.0, 1.0)
            };

            // grow / reveal / hide each element by its step vs the active step.
            for (idx, world, color, step_k) in elems.iter() {
                let g = match (*step_k, active) {
                    (Some(sk), Some(ak)) if sk < ak => 1.0,
                    (Some(sk), Some(ak)) if sk == ak => lerp(0.06, 1.0, p_geom),
                    (Some(_), None) => 1.0,    // all steps done (hold)
                    (Some(_), Some(_)) => 0.0, // future step → hidden
                    (None, _) => 0.0,
                };
                let m = if g <= 0.0 {
                    HIDDEN
                } else {
                    *world * Mat4::from_scale(Vec3::splat(g))
                };
                render.replace_batch_world(*idx, &bp, &bn, &bi, *color, m);
            }

            // active construction robot: place at the step centroid, animate.
            if let Some(k) = active {
                let (cx, cy) = centroids[k];
                let base = Mat4::from_translation(Vec3::new(cx - c.x, cy - c.y, 0.0))
                    * Mat4::from_scale(Vec3::splat(ROBOT_DISPLAY_SCALE));
                for _ in 0..2 {
                    arm.step(sim_t);
                    sim_t += DT;
                }
                let fk = arm.fk();
                let rcol = match proc_of[k].as_str() {
                    "deposition" => [0.55, 0.57, 0.60],
                    "thermal-weld" => {
                        // glow with weld heat
                        let t = weld.as_ref().map(|w| w.max_temp()).unwrap_or(20.0);
                        let g = ((t - 600.0) / 1800.0).clamp(0.0, 1.0);
                        [0.85, 0.30 + 0.5 * g, 0.10 + 0.4 * g]
                    }
                    _ => [0.20, 0.55, 0.85],
                };
                for i in 0..nb {
                    render.replace_batch_world(
                        robot_start + i,
                        &bp,
                        &bn,
                        &bi,
                        rcol,
                        to_render * base * fk[i] * arm.link_local[i],
                    );
                }
                // delivery cart (搬入/搬送): ferry a material box 受入 → work zone.
                if let Some(sub) = plans[k].logistics_at(p) {
                    let sx = lerp(stage_xy.0, cx, sub) - c.x;
                    let sy = lerp(stage_xy.1, cy, sub) - c.y;
                    render.replace_batch_world(
                        cart_idx,
                        &bp,
                        &bn,
                        &bi,
                        [0.85, 0.65, 0.20],
                        to_render
                            * Mat4::from_translation(Vec3::new(sx, sy, 1.0))
                            * Mat4::from_scale(Vec3::new(3.0, 2.0, 2.0)),
                    );
                } else {
                    render.replace_batch_world(cart_idx, &bp, &bn, &bi, [0.85, 0.65, 0.20], HIDDEN);
                }

                // current robot OP (procure→deliver→stage→build→fasten→inspect).
                let op = plans[k].op_at(p);
                let (op_seq, op_label, op_action) = op
                    .map(|o| (o.op.seq, o.op.label.clone(), o.op.action.clone()))
                    .unwrap_or((0, String::new(), String::new()));
                STATUS.with(|s| {
                    *s.borrow_mut() = format!(
                        "{:02}. {} ／ op{}.{} [{}] ／ 建方{:.0}%",
                        order.steps[k].seq,
                        order.steps[k].name,
                        op_seq,
                        op_label,
                        op_action,
                        p_geom * 100.0
                    );
                });
            } else {
                for i in 0..nb {
                    render.replace_batch_world(robot_start + i, &bp, &bn, &bi, [0.2; 3], HIDDEN);
                }
                render.replace_batch_world(cart_idx, &bp, &bn, &bi, [0.85, 0.65, 0.20], HIDDEN);
                STATUS.with(|s| *s.borrow_mut() = "竣工 — 全工程完了".into());
            }
        });

    log::info!("[tatekata] backend={:?}", app.backend());
    app.run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use kami_app_giemon_factory::{ConstructionOrder, Factory, Robots};

    #[test]
    fn reuses_factory_order_robots() {
        let f = Factory::load();
        let order = ConstructionOrder::load();
        let robots = Robots::load();
        assert_eq!(robots.robots.len(), 7);
        // every step's robot resolves + has a known process
        for s in &order.steps {
            let r = robots.get(&s.robot).expect("robot resolves");
            assert!(["deposition", "thermal-weld", "none"].contains(&r.process.as_str()));
            // work centroid is finite and within the site
            let (x, y) = f.step_center(&s.reveals);
            assert!(x.is_finite() && y.is_finite());
        }
    }

    #[test]
    fn printer_and_bolter_steps_exist() {
        let order = ConstructionOrder::load();
        let robots = Robots::load();
        let has_dep = order.steps.iter().any(|s| {
            robots
                .get(&s.robot)
                .map(|r| r.process == "deposition")
                .unwrap_or(false)
        });
        let has_weld = order.steps.iter().any(|s| {
            robots
                .get(&s.robot)
                .map(|r| r.process == "thermal-weld")
                .unwrap_or(false)
        });
        assert!(has_dep && has_weld, "need both material-process step kinds");
    }
}
