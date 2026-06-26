//! kami-app-isekai · Omniverse / PhysX / OpenUSD facade entry.
//!
//! This module wires the kami-engine nv-compat layer (`kami-usd`,
//! `kami-genesis`, `kami-articulated`) into the ISEKAI runtime so a USDA
//! scene description can drive both the visual layer (voxel sandbox +
//! sky + terrain) and the physics layer (PhysX-shaped `World` with
//! reduced-coordinate articulations).
//!
//! Constitutional invariants (ADR-2605261800 §D10.3 + §G7 of
//! ADR-2605262500): all NVIDIA-branded APIs are accessed exclusively
//! through the `kami-*` facade namespace. NO direct PhysX / OmniKit /
//! OpenUSD library imports.
//!
//! ```text
//!   USDA source ──► kami-usd::parse_usda ──► Stage
//!   Stage  ┐
//!          ├── PhysicsScene  ──► kami-genesis::World (PxScene-shaped)
//!          ├── Cartpole prim ──► kami-articulated::parse_urdf
//!          │                    ─► kami-genesis::Articulation
//!          │                       (PxArticulationReducedCoordinate)
//!          ├── Cube / Sphere / Plane prims ──► kami-pipelines voxels
//!          └── Xform                     ──► scene-graph transforms
//! ```

use kami_genesis::{ArticulationHandle, World as GenesisWorld};
use kami_usd::{PrimKind, Stage};

#[cfg(target_family = "wasm")]
use kami_app::{CameraMode, InputMode, KamiApp, Position};
#[cfg(target_family = "wasm")]
use log::Level;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

// Bundled URDF — same physical cartpole used by `kami-cartpole-wasm` so
// trained policies trained against either entry remain valid.
const BUNDLED_CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");

/// Built-in USDA used when the JS side does not supply a custom one.
/// One PhysicsScene + a ground plane + one Cartpole articulation that
/// spawns above the demo house at the same world coordinates as the
/// v3-demos paper-row so the camera framing matches the existing scenes.
pub const DEFAULT_ISEKAI_USDA: &str = r#"#usda 1.0
(
    upAxis = "Y"
    metersPerUnit = 1.0
)

def PhysicsScene "physics"
{
    vector3f physics:gravityDirection = (0, -1, 0)
    float physics:gravityMagnitude = 9.81
}

def Plane "ground"
{
    double3 xformOp:translate = (0, 0, 0)
    double width = 32.0
    double length = 32.0
}

def Cartpole "cart_alpha"
{
    double3 xformOp:translate = (-11, 33.5, 18)
    custom string urdf = "@./cartpole.urdf@"
}
"#;

/// nv-compat banner; useful for HUD strings and audit trails.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = isekaiOmniverseBanner)]
pub fn isekai_omniverse_banner() -> String {
    format!(
        "kami-usd@{} (omni.usd compat) + kami-genesis@{} (PhysX 5 / isaacsim.core.api compat) — {}",
        kami_usd::PHASE,
        kami_genesis::PHASE,
        kami_usd::ADR
    )
}

/// JS-callable: return the bundled default USDA so the JS side can
/// display / edit / re-submit it.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = isekaiOmniverseDefaultUsda)]
pub fn isekai_omniverse_default_usda() -> String {
    DEFAULT_ISEKAI_USDA.to_string()
}

/// Player-injected cart force, in newtons, shared from the JS keyboard
/// bridge into the physics tick. WASM is single-threaded so a plain
/// `Cell` is sufficient. Edge-triggered by `isekaiSetCartForce`.
///
/// R1 activation of ADR-2605272000 §Consequences/Negative-2: the R0 cut
/// ticked `World::step()` with force pinned at 0 ("watchable"); this makes
/// the cartpole an interactive PhysX-shaped control loop.
#[cfg(target_family = "wasm")]
thread_local! {
    static CART_FORCE: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

/// JS-callable: set the force (N) applied to every cartpole cart each
/// physics step. `omniverse.htm` / `worlds.htm` bind J/L (or ←/→) to
/// `isekaiSetCartForce(±F)` on keydown and `isekaiSetCartForce(0)` on
/// keyup.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = isekaiSetCartForce)]
pub fn isekai_set_cart_force(force: f32) {
    CART_FORCE.with(|c| c.set(force.clamp(-50.0, 50.0)));
}

/// Build a generated ISEKAI world `Stage` programmatically and emit it as
/// USDA via `kami_usd::to_usda`. These are the worlds surfaced in the
/// `worlds.htm` gallery — authored as `Stage` values (not hand-written
/// USDA), proving the USD writer + the omni.usd facade end-to-end.
#[cfg(target_family = "wasm")]
fn gravity_scene() -> kami_usd::Prim {
    kami_usd::Prim {
        path: "/physics".into(),
        kind: PrimKind::PhysicsScene {
            gravity: [0.0, -9.81, 0.0],
        },
        xform: kami_usd::Xform {
            scale: [1.0, 1.0, 1.0],
            ..Default::default()
        },
        attrs: vec![],
    }
}

#[cfg(target_family = "wasm")]
fn cube(name: &str, t: [f32; 3], size: f32) -> kami_usd::Prim {
    kami_usd::Prim {
        path: format!("/{name}"),
        kind: PrimKind::Cube { size },
        xform: kami_usd::Xform {
            translate: t,
            scale: [1.0, 1.0, 1.0],
            ..Default::default()
        },
        attrs: vec![],
    }
}

#[cfg(target_family = "wasm")]
fn sphere(name: &str, t: [f32; 3], radius: f32) -> kami_usd::Prim {
    kami_usd::Prim {
        path: format!("/{name}"),
        kind: PrimKind::Sphere { radius },
        xform: kami_usd::Xform {
            translate: t,
            scale: [1.0, 1.0, 1.0],
            ..Default::default()
        },
        attrs: vec![],
    }
}

#[cfg(target_family = "wasm")]
fn cartpole(name: &str, t: [f32; 3]) -> kami_usd::Prim {
    kami_usd::Prim {
        path: format!("/{name}"),
        kind: PrimKind::Cartpole {
            urdf_ref: "./cartpole.urdf".into(),
        },
        xform: kami_usd::Xform {
            translate: t,
            scale: [1.0, 1.0, 1.0],
            ..Default::default()
        },
        attrs: vec![],
    }
}

/// All worlds anchor near the demo-house spawn (`-5, 35.5, 18.5`) so the
/// first-person camera frames them on load.
#[cfg(target_family = "wasm")]
fn built_in_worlds() -> Vec<(&'static str, &'static str, kami_usd::Stage)> {
    use kami_usd::{Stage, UpAxis};
    let mk = |prims: Vec<kami_usd::Prim>| Stage {
        up_axis: UpAxis::Y,
        meters_per_unit: 1.0,
        prims,
    };

    let courtyard = mk(vec![
        gravity_scene(),
        cube("pillar_nw", [-13.0, 34.0, 16.0], 1.5),
        cube("pillar_ne", [-13.0, 34.0, 21.0], 1.5),
        cube("pillar_sw", [-8.0, 34.0, 16.0], 1.5),
        cube("pillar_se", [-8.0, 34.0, 21.0], 1.5),
        sphere("fountain", [-10.5, 35.0, 18.5], 1.2),
    ]);

    let twin_cartpole = mk(vec![
        gravity_scene(),
        cartpole("cart_left", [-12.0, 33.5, 17.0]),
        cartpole("cart_right", [-9.0, 33.5, 20.0]),
        cube("plinth", [-10.5, 33.0, 18.5], 2.0),
    ]);

    let sphere_garden = mk(vec![
        gravity_scene(),
        sphere("orb_a", [-13.0, 34.5, 17.0], 0.8),
        sphere("orb_b", [-11.0, 35.0, 18.0], 1.1),
        sphere("orb_c", [-9.0, 34.5, 19.0], 0.8),
        sphere("orb_d", [-7.0, 35.5, 20.0], 1.4),
    ]);

    let physics_tower = mk(vec![
        gravity_scene(),
        cube("tower_0", [-10.5, 33.5, 18.5], 1.6),
        cube("tower_1", [-10.5, 35.0, 18.5], 1.3),
        cube("tower_2", [-10.5, 36.2, 18.5], 1.0),
        cartpole("crown", [-10.5, 37.5, 18.5]),
    ]);

    vec![
        (
            "twin-cartpole",
            "Twin Cartpole Arena — two PhysX-shaped articulations (J/L to push)",
            twin_cartpole,
        ),
        (
            "courtyard",
            "Courtyard — four pillars + central sphere",
            courtyard,
        ),
        (
            "sphere-garden",
            "Sphere Garden — scattered orbs of varying radius",
            sphere_garden,
        ),
        (
            "physics-tower",
            "Physics Tower — stacked cubes crowned by a cartpole",
            physics_tower,
        ),
    ]
}

/// JS-callable: return the built-in world catalog as a JSON string
/// `[{id, name, usda}]`. Each `usda` field is produced by
/// `kami_usd::to_usda` over a programmatically-built `Stage`.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = isekaiOmniverseWorldCatalog)]
pub fn isekai_omniverse_world_catalog() -> String {
    let entries: Vec<serde_json::Value> = built_in_worlds()
        .into_iter()
        .map(|(id, name, stage)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "usda": kami_usd::to_usda(&stage),
            })
        })
        .collect();
    serde_json::Value::Array(entries).to_string()
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy)]
enum PropKind {
    Cube,
    Sphere,
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy)]
struct Prop {
    pos: [f32; 3],
    size: f32,
    kind: PropKind,
    tint: [f32; 3],
}

/// Run the ISEKAI omniverse entry.
///
/// `canvas_id`  — WebGPU canvas DOM id.
/// `usda_src`   — USDA stage text. Pass empty string to use
///                `DEFAULT_ISEKAI_USDA`.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = runIsekaiOmniverse)]
pub async fn run_isekai_omniverse(canvas_id: &str, usda_src: &str) -> Result<(), JsValue> {
    use crate::pipelines;
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(Level::Info);

    let stage = parse_or_default(usda_src);
    log::info!(
        "[isekai-omniverse] stage prims={} up_axis={:?} mpu={}",
        stage.prims.len(),
        stage.up_axis,
        stage.meters_per_unit
    );

    // Build PhysX-shaped World from PhysicsScene prim (or default 9.81 -Y).
    let mut world = build_world_from_stage(&stage);
    let articulations: Vec<(String, ArticulationHandle, [f32; 3])> =
        spawn_articulations(&mut world, &stage).map_err(|e| JsValue::from_str(&e))?;

    // Static props (Cube / Sphere prims) become sprite-cluster visuals so
    // a generated world reads distinctly from the bare voxel sandbox.
    let props_for_tick: Vec<Prop> = stage
        .prims
        .iter()
        .filter_map(|p| match &p.kind {
            PrimKind::Cube { size } => Some(Prop {
                pos: p.xform.translate,
                size: *size,
                kind: PropKind::Cube,
                tint: [0.55, 0.8, 1.0],
            }),
            PrimKind::Sphere { radius } => Some(Prop {
                pos: p.xform.translate,
                size: *radius * 2.0,
                kind: PropKind::Sphere,
                tint: [1.0, 0.78, 0.5],
            }),
            _ => None,
        })
        .collect();

    let spawn = Position::new(-5.0, 35.5, 18.5);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("isekai-omniverse")
        .with_hud_publish(true)
        .with_camera(CameraMode::FirstPerson {
            spawn,
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: -0.25,
        })
        .with_input(InputMode::WasdFps);

    // Visual layer: re-use the same sky + streaming terrain + atlas
    // adapters the v3-demos scene already validates. Voxel world gives
    // the cartpole something to land on.
    let sky = pipelines::SkyAdapter::new(app.render_context());
    // Executor edge (ADR-0044/0046): Plains terrain from kami-terrain-scene's biomes.edn.
    let terrain = crate::plains_terrain(app.render_context());
    let voxels = crate::build_voxel_world(app.render_context());
    let voxels_for_probe = voxels.clone();
    let voxels_for_wall = voxels.clone();
    crate::build_demo_house(app.render_context(), &voxels);

    // Atlas sprite layer renders the cart + pole tip so we don't need a
    // separate articulated-body mesh pipeline for the R0 cut. Pole is a
    // bobbing sparkle; cart is a flame_medium with neutral tint.
    let atlas = kami_pipelines::AtlasVisAdapter::new(app.render_context(), 2048);
    let atlas_for_tick = atlas.clone();

    // Mutable physics handle moved into the closure. World owns the
    // articulations; we read state each frame and emit atlas sprites at
    // the resolved cart + pole world positions.
    let mut tick: u64 = 0;
    let mut world_cell = std::cell::RefCell::new(world);
    // `move` captures world_cell + articulations.
    let articulations_for_tick = articulations.clone();
    let app = app
        .with_floor_probe(move |p| voxels_for_probe.sample_floor(p))
        .with_eye_height(1.8)
        .with_collider_probe(move |min, max| voxels_for_wall.aabb_solid(min, max))
        .with_player_radius(0.35)
        .with_gravity(0.0)
        .with_jump_impulse(0.0)
        .with_pipeline(sky)
        .with_pipeline(terrain)
        .with_pipeline(voxels)
        .with_pipeline(atlas)
        .on_update(move |_world_ecs, _camera, _dt| {
            tick = tick.wrapping_add(1);

            // Player-injected cart force (J/L keys). R1 activation of the
            // PhysX `PxArticulation` drive target: applied to every cart
            // before the world steps.
            let force = CART_FORCE.with(|c| c.get());

            let mut w = world_cell.borrow_mut();
            for (_, handle, _) in &articulations_for_tick {
                if let Ok(a) = w.get_mut(*handle) {
                    a.set_cart_force(force);
                }
            }
            // PhysX-style World::step() per frame (PxScene::simulate shape).
            w.step();

            // Generated-world static props: render Cube as an 8-corner
            // sparkle box + a flame core; Sphere as a 6-axis sparkle shell.
            for prop in &props_for_tick {
                let c = glam::Vec3::from(prop.pos);
                match prop.kind {
                    PropKind::Cube => {
                        let h = (prop.size * 0.5).max(0.25);
                        for &sx in &[-1.0_f32, 1.0] {
                            for &sy in &[-1.0_f32, 1.0] {
                                for &sz in &[-1.0_f32, 1.0] {
                                    atlas_for_tick.emit_static(
                                        c + glam::Vec3::new(sx * h, sy * h, sz * h),
                                        kami_pipelines::atlas_slot::SPARKLE_STAR,
                                        prop.tint,
                                        0.5,
                                        0.1,
                                    );
                                }
                            }
                        }
                        atlas_for_tick.emit_static(
                            c,
                            kami_pipelines::atlas_slot::FLAME_MEDIUM,
                            prop.tint,
                            prop.size,
                            0.1,
                        );
                    }
                    PropKind::Sphere => {
                        let r = (prop.size * 0.5).max(0.25);
                        for axis in [
                            glam::Vec3::X,
                            glam::Vec3::NEG_X,
                            glam::Vec3::Y,
                            glam::Vec3::NEG_Y,
                            glam::Vec3::Z,
                            glam::Vec3::NEG_Z,
                        ] {
                            atlas_for_tick.emit_bobbing(
                                c + axis * r,
                                kami_pipelines::atlas_slot::SPARKLE_STAR,
                                prop.tint,
                                0.55,
                                0.12,
                                tick as f32 * 0.2,
                            );
                        }
                    }
                }
            }

            // For each cartpole articulation, read the state and render the
            // cart (flame core) + the pole as a sparkle line from pivot to
            // tip so the tilt is legible as it responds to J/L force.
            for (name, handle, origin) in &articulations_for_tick {
                let state = w.get(*handle).ok().and_then(|a| a.cartpole_state());
                let cart_x_off = state.map(|s| s.x).unwrap_or(0.0);
                let pole_theta = state.map(|s| s.theta).unwrap_or(0.0);

                let cart_pos = glam::Vec3::new(origin[0] + cart_x_off, origin[1] + 0.5, origin[2]);
                // Pole pivots about +y; theta=0 ⇒ straight up. Length ~1 m.
                let pole_len = 1.0_f32;
                let pole_tip = cart_pos
                    + glam::Vec3::new(
                        pole_theta.sin() * pole_len,
                        pole_theta.cos() * pole_len,
                        0.0,
                    );

                atlas_for_tick.emit_static(
                    cart_pos,
                    kami_pipelines::atlas_slot::FLAME_MEDIUM,
                    [0.6, 0.6, 0.95],
                    1.4,
                    0.18,
                );
                // Pole shaft: sparkle segments interpolated cart→tip.
                let segments = 6;
                for k in 1..=segments {
                    let t = k as f32 / segments as f32;
                    atlas_for_tick.emit_static(
                        cart_pos.lerp(pole_tip, t),
                        kami_pipelines::atlas_slot::SPARKLE_STAR,
                        [0.85, 0.92, 1.0],
                        0.42,
                        0.1,
                    );
                }
                // Pole tip highlight.
                atlas_for_tick.emit_bobbing(
                    pole_tip,
                    kami_pipelines::atlas_slot::SPARKLE_STAR,
                    [1.0, 0.95, 0.55],
                    0.9,
                    0.22,
                    tick as f32 * 0.4,
                );

                if tick % 120 == 0 {
                    log::info!(
                        "[isekai-omniverse] articulation `{}` x={:.3} theta={:.3} force={:.1}",
                        name,
                        cart_x_off,
                        pole_theta,
                        force
                    );
                }
            }
        });

    log::info!(
        "[isekai-omniverse] backend={:?} banner=`{}`",
        app.backend(),
        format!(
            "kami-usd@{} kami-genesis@{}",
            kami_usd::PHASE,
            kami_genesis::PHASE
        )
    );
    app.run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

fn parse_or_default(usda_src: &str) -> Stage {
    let trimmed = usda_src.trim();
    let src = if trimmed.is_empty() {
        DEFAULT_ISEKAI_USDA
    } else {
        trimmed
    };
    match kami_usd::parse_usda(src) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "[isekai-omniverse] USDA parse failed ({}); falling back to default stage",
                e
            );
            kami_usd::parse_usda(DEFAULT_ISEKAI_USDA).expect("default USDA must parse")
        }
    }
}

fn build_world_from_stage(stage: &Stage) -> GenesisWorld {
    let (gravity, dt) = stage
        .prims
        .iter()
        .find_map(|p| {
            if let PrimKind::PhysicsScene { gravity } = p.kind {
                // Use Y-axis component as scalar magnitude (kami-genesis World
                // models gravity as a scalar along -Y; full vector is a future
                // R2 extension once non-Y up-axis stages land).
                Some((gravity[1].abs(), 1.0 / 60.0))
            } else {
                None
            }
        })
        .unwrap_or((9.81, 1.0 / 60.0));
    GenesisWorld::new(gravity, dt)
}

/// Walk the stage, materialise every `Cartpole` prim as a kami-genesis
/// articulation (PhysX `PxArticulationReducedCoordinate` shape) and
/// return per-articulation (path, handle, world-origin) triples.
fn spawn_articulations(
    world: &mut GenesisWorld,
    stage: &Stage,
) -> Result<Vec<(String, ArticulationHandle, [f32; 3])>, String> {
    let mut out = Vec::new();
    for prim in &stage.prims {
        if let PrimKind::Cartpole { .. } = &prim.kind {
            // R0: always use the bundled cartpole URDF, regardless of
            // the `urdf` attr value. R1 will fetch via substrate so the
            // USDA truly authorises the URDF binding.
            let sys = kami_articulated::parse_urdf(BUNDLED_CARTPOLE_URDF)
                .map_err(|e| format!("parse_urdf: {e}"))?;
            let handle = world
                .add_articulation(sys)
                .map_err(|e| format!("add_articulation: {e}"))?;
            log::info!(
                "[isekai-omniverse] spawned articulation `{}` at {:?}",
                prim.path,
                prim.xform.translate
            );
            out.push((prim.path.clone(), handle, prim.xform.translate));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_usda_parses_with_one_cartpole() {
        let st = kami_usd::parse_usda(DEFAULT_ISEKAI_USDA).expect("parse");
        let carts: Vec<_> = st
            .prims
            .iter()
            .filter(|p| matches!(p.kind, PrimKind::Cartpole { .. }))
            .collect();
        assert_eq!(carts.len(), 1);
    }

    #[test]
    fn build_world_reads_gravity_from_stage() {
        let st = kami_usd::parse_usda(DEFAULT_ISEKAI_USDA).expect("parse");
        let w = build_world_from_stage(&st);
        assert!((w.gravity - 9.81).abs() < 1e-3);
    }
}
