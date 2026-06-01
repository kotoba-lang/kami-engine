//! kami-app-shibuya — Shibuya street digital-twin physics sim.
//!
//! Loads a real OpenStreetMap city block (Shibuya Scramble, baked offline to
//! `70-tools/e7m-sim/scenes/shibuya/shibuya_scramble.scene.json` by
//! `osm_to_citymesh.py`, ODbL) and runs multiple full-physics agents on the
//! streets with the kami-genesis 3-D spatial solver + contact:
//!   - buildings → static `Obstacle::Aabb` collision volumes (footprint × height),
//!   - the road network → the drivable ground plane (z = 0) + ribbon render,
//!   - each agent → a 4-DOF floating-base articulation (x, y, z translate + yaw)
//!     carrying a box body with corner sphere colliders; it falls under gravity,
//!     rests on the road, drives forward, and collides with buildings.
//!
//! Clean-room (no NVIDIA/PhysX/Isaac); sim is z-up, the renderer is y-up, so the
//! whole scene is rotated −90° about X for display.

use glam::{Mat4, Vec3};
#[cfg(target_family = "wasm")]
use kami_app::{CameraMode, InputMode, KamiApp};
use kami_genesis::{
    Articulation3dConfig, Articulation3dState, Collider, ContactParams, ContactWorld, Obstacle,
};
#[cfg(target_family = "wasm")]
use kami_pipelines::unit_box;
use serde::Deserialize;

#[cfg(target_family = "wasm")]
use kami_pipelines::{GsplatAdapter, GsplatFormat};
#[cfg(target_family = "wasm")]
use kami_render::RenderContext;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

// 3-D Gaussian-Splat overlay handle, set during `run_shibuya_v1`, driven by the
// JS load/clear hooks. The detailed splat comes from `trainGsplatFromMapillary`
// (Mapillary→COLMAP→gsplat, ADR-2605092800); a coarse placeholder ships for the
// render-path proof.
#[cfg(target_family = "wasm")]
thread_local! {
    static SPLAT: std::cell::RefCell<Option<GsplatAdapter>> = const { std::cell::RefCell::new(None) };
}

/// JS hook: load a `.splat` (antimatter15 32-byte) cloud into the 3DGS overlay.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = shibuyaLoadSplat)]
pub fn shibuya_load_splat(bytes: &[u8]) -> bool {
    SPLAT.with(|s| {
        s.borrow()
            .as_ref()
            .map(|a| a.upsert_from_bytes("shibuya", bytes, GsplatFormat::Splat).is_ok())
            .unwrap_or(false)
    })
}

/// JS hook: load a `.ply` (gsplat training output) cloud into the 3DGS overlay.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = shibuyaLoadSplatPly)]
pub fn shibuya_load_splat_ply(bytes: &[u8]) -> bool {
    SPLAT.with(|s| {
        s.borrow()
            .as_ref()
            .map(|a| a.upsert_from_bytes("shibuya", bytes, GsplatFormat::Ply).is_ok())
            .unwrap_or(false)
    })
}

/// JS hook: remove the 3DGS overlay (back to the box city).
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = shibuyaClearSplat)]
pub fn shibuya_clear_splat() {
    SPLAT.with(|s| {
        if let Some(a) = s.borrow().as_ref() {
            a.remove("shibuya");
        }
    });
}

const SCENE_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/shibuya/shibuya_scramble.scene.json");
const DT: f32 = 1.0 / 120.0;

// ── scene model ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Scene {
    pub name: String,
    pub bbox_m: [f32; 4],
    pub buildings: Vec<Building>,
    pub roads: Vec<Road>,
    #[serde(default)]
    pub objects: Vec<CityObject>,
}

/// A point asset (pole / lamp / signal / tree / hydrant …) — rendered as
/// clickable geometry, registered as a kotoba EAVT entity (`*.assets.edn`).
#[derive(Deserialize, Clone)]
pub struct CityObject {
    pub id: String,
    pub kind: String,
    pub pos: [f32; 2],
    pub h: f32,
    pub attrs: ObjectAttrs,
}

#[derive(Deserialize, Clone)]
pub struct ObjectAttrs {
    #[serde(rename = "installYear")]
    pub install_year: i64,
    pub company: String,
    #[serde(rename = "costJpy")]
    pub cost_jpy: i64,
    pub provenance: String,
}

#[derive(Deserialize)]
pub struct Building {
    /// `[min_x, min_y, max_x, max_y]` footprint in local metres.
    pub aabb: [f32; 4],
    pub height: f32,
}

#[derive(Deserialize)]
pub struct Road {
    pub path: Vec<[f32; 2]>,
    pub width: f32,
}

impl Scene {
    pub fn load() -> Self {
        serde_json::from_str(SCENE_JSON).expect("shibuya scene parses")
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            0.5 * (self.bbox_m[0] + self.bbox_m[2]),
            0.5 * (self.bbox_m[1] + self.bbox_m[3]),
            0.0,
        )
    }

    /// Building footprints → static AABB collision volumes (z = 0 .. height).
    pub fn building_obstacles(&self) -> Vec<Obstacle> {
        self.buildings
            .iter()
            .map(|b| Obstacle::Aabb {
                min: Vec3::new(b.aabb[0], b.aabb[1], 0.0),
                max: Vec3::new(b.aabb[2], b.aabb[3], b.height),
            })
            .collect()
    }

    /// A handful of agent spawn points sampled along the road network.
    pub fn spawn_points(&self, n: usize) -> Vec<(Vec3, f32)> {
        let mut out = Vec::new();
        let mut i = 0usize;
        while out.len() < n && i < self.roads.len() {
            let r = &self.roads[(i * 7) % self.roads.len().max(1)];
            if r.path.len() >= 2 {
                let mid = r.path.len() / 2;
                let p = r.path[mid];
                let q = r.path[mid - 1];
                let yaw = (p[1] - q[1]).atan2(p[0] - q[0]);
                out.push((Vec3::new(p[0], p[1], 3.0), yaw));
            }
            i += 1;
        }
        out
    }
}

// ── floating-base agent (4-DOF: x, y, z translate + yaw) ─────────────────────

/// Box-inertia URDF for a 4-DOF floating-base agent: world → px → py → pz →
/// (yaw) body. Intermediate links are ~massless; the body link carries the
/// mass + box inertia. Parsed by the validated `from_articulated_system` path.
fn agent_urdf(size: Vec3, mass: f32) -> String {
    let (l, w, h) = (size.x, size.y, size.z);
    let ixx = mass * (w * w + h * h) / 12.0;
    let iyy = mass * (l * l + h * h) / 12.0;
    let izz = mass * (l * l + w * w) / 12.0;
    let prismatic = |name: &str, parent: &str, child: &str, axis: &str| {
        format!(
            r#"<joint name="{name}" type="prismatic"><parent link="{parent}"/><child link="{child}"/><origin xyz="0 0 0"/><axis xyz="{axis}"/><limit lower="-100000" upper="100000" effort="100000000" velocity="1000"/></joint>
<link name="{child}"><inertial><mass value="0.0001"/><inertia ixx="1e-7" iyy="1e-7" izz="1e-7" ixy="0" ixz="0" iyz="0"/></inertial></link>"#
        )
    };
    format!(
        r#"<robot name="agent">
<link name="world"/>
{jx}
{jy}
{jz}
<joint name="jyaw" type="continuous"><parent link="lz"/><child link="body"/><origin xyz="0 0 0"/><axis xyz="0 0 1"/><dynamics damping="0"/></joint>
<link name="body"><inertial><origin xyz="0 0 0"/><mass value="{mass}"/><inertia ixx="{ixx}" iyy="{iyy}" izz="{izz}" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#,
        jx = prismatic("jx", "world", "lx", "1 0 0"),
        jy = prismatic("jy", "lx", "ly", "0 1 0"),
        jz = prismatic("jz", "ly", "lz", "0 0 1"),
    )
}

/// A full-physics street agent.
pub struct Agent {
    pub cfg: Articulation3dConfig,
    pub state: Articulation3dState,
    pub contact: ContactWorld,
    pub body_idx: usize,
    pub size: Vec3,
    pub color: [f32; 3],
    pub drive: f32, // forward force (N)
    pub steer_phase: f32,
}

impl Agent {
    pub fn new(size: Vec3, mass: f32, obstacles: Vec<Obstacle>, color: [f32; 3]) -> Self {
        let sys = kami_articulated::parse_urdf(&agent_urdf(size, mass)).expect("agent urdf");
        let cfg =
            Articulation3dConfig::from_articulated_system(&sys, Vec3::new(0.0, 0.0, -9.81), DT);
        let ndof = cfg.ndof;
        let body_idx = cfg.body_index("body").expect("body link");
        // Corner sphere colliders on the body, for ground rest + wall collision.
        let r = size.min_element() * 0.32;
        let mut colliders = Vec::new();
        for sx in [-1.0_f32, 1.0] {
            for sy in [-1.0_f32, 1.0] {
                for sz in [-1.0_f32, 1.0] {
                    colliders.push((
                        body_idx,
                        Collider::Sphere {
                            center: Vec3::new(
                                sx * size.x * 0.5,
                                sy * size.y * 0.5,
                                sz * size.z * 0.5,
                            ),
                            radius: r,
                        },
                    ));
                }
            }
        }
        let contact = ContactWorld::new(
            colliders,
            ContactParams { ground_z: 0.0, friction: 1.0, ..Default::default() },
        )
        .with_obstacles(obstacles);
        Self {
            cfg,
            state: Articulation3dState::zeros(ndof),
            contact,
            body_idx,
            size,
            color,
            drive: mass * 2.2, // ≈ 2.2 m/s² forward
            steer_phase: 0.0,
        }
    }

    pub fn place(&mut self, pos: Vec3, yaw: f32) {
        // dof order: jx=0, jy=1, jz=2, jyaw=3.
        self.state.q[0] = pos.x;
        self.state.q[1] = pos.y;
        self.state.q[2] = pos.z;
        self.state.q[3] = yaw;
    }

    /// Drive forward along the current heading with light steering + drag, then
    /// one full-physics contact step (gravity + ground + buildings + friction).
    pub fn step(&mut self, t: f32) {
        let yaw = self.state.q[3];
        let (sy, cy) = yaw.sin_cos();
        let vx = self.state.qdot[0];
        let vy = self.state.qdot[1];
        let drag = 12.0;
        let mut tau = vec![0.0_f32; self.cfg.ndof];
        tau[0] = self.drive * cy - drag * vx;
        tau[1] = self.drive * sy - drag * vy;
        // Gentle wandering steer torque.
        tau[3] = 200.0 * (t * 0.6 + self.steer_phase).sin() - 40.0 * self.state.qdot[3];
        self.contact.step(&self.cfg, &mut self.state, &tau);
    }

    /// World transform of the body for rendering (sim frame).
    pub fn body_world(&self) -> Mat4 {
        self.cfg.fk_world(&self.state.q)[self.body_idx]
    }
}

// ── render mesh accumulation (merged batches → few draw calls) ───────────────

#[cfg(target_family = "wasm")]
struct MeshAcc {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    i: Vec<u32>,
}

#[cfg(target_family = "wasm")]
impl MeshAcc {
    fn new() -> Self {
        Self { p: Vec::new(), n: Vec::new(), i: Vec::new() }
    }
    /// Append a unit box transformed by `m` (already includes the y-up rotation).
    fn add_box(&mut self, m: Mat4) {
        let (bp, bn, bi) = unit_box();
        let base = self.p.len() as u32;
        let nm = m.inverse().transpose();
        for v in &bp {
            let w = m.transform_point3(Vec3::from_array(*v));
            self.p.push([w.x, w.y, w.z]);
        }
        for nr in &bn {
            let w = nm.transform_vector3(Vec3::from_array(*nr)).normalize_or_zero();
            self.n.push([w.x, w.y, w.z]);
        }
        for idx in &bi {
            self.i.push(base + *idx);
        }
    }
}

// ── colours ──────────────────────────────────────────────────────────────────
const ASPHALT: [f32; 3] = [0.16, 0.16, 0.18];
const ROAD: [f32; 3] = [0.30, 0.30, 0.33];
const CONCRETE: [f32; 3] = [0.58, 0.57, 0.55];
const GLASS: [f32; 3] = [0.42, 0.52, 0.60];
const AGENT_COLORS: [[f32; 3]; 6] = [
    [0.93, 0.35, 0.25],
    [0.20, 0.62, 0.86],
    [0.96, 0.72, 0.18],
    [0.30, 0.72, 0.55],
    [0.74, 0.40, 0.85],
    [0.90, 0.50, 0.70],
];

/// Render footprint (metres) for a point asset by kind.
fn object_footprint(kind: &str) -> (f32, f32) {
    match kind {
        "tree" => (4.0, 4.0),
        "bench" => (1.6, 0.5),
        "vending_machine" => (1.2, 0.8),
        "telephone" => (0.9, 0.9),
        "fire_hydrant" => (0.6, 0.6),
        "waste_basket" => (0.7, 0.7),
        "advertising" => (1.2, 0.3),
        _ => (0.5, 0.5), // poles: lamp / utility / signal
    }
}

fn object_color(kind: &str) -> [f32; 3] {
    match kind {
        "tree" => [0.20, 0.58, 0.24],
        "traffic_signals" => [0.96, 0.74, 0.10],
        "street_lamp" => [0.78, 0.78, 0.82],
        "utility_pole" => [0.52, 0.46, 0.40],
        "fire_hydrant" => [0.86, 0.16, 0.13],
        "bench" => [0.60, 0.45, 0.30],
        "vending_machine" => [0.22, 0.52, 0.82],
        "telephone" => [0.12, 0.60, 0.42],
        "advertising" => [0.90, 0.30, 0.55],
        _ => [0.70, 0.70, 0.72],
    }
}

/// Publish the clicked asset's kotoba record to `window.__shibuya_pick` (JSON).
#[cfg(target_family = "wasm")]
fn publish_pick(json: &str) {
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(
            &w,
            &JsValue::from_str("__shibuya_pick"),
            &JsValue::from_str(json),
        );
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_shibuya_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let scene = Scene::load();
    let c = scene.center();
    log::info!(
        "[shibuya] {} — {} buildings, {} roads",
        scene.name,
        scene.buildings.len(),
        scene.roads.len()
    );

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("shibuya")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 20.0, 0.0),
            distance: 480.0,
            yaw: 0.6,
            pitch: 0.62,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    // 3-D Gaussian-Splat overlay (empty until a .splat/.ply is loaded via JS).
    let gsplat = GsplatAdapter::new(ctx);
    SPLAT.with(|s| *s.borrow_mut() = Some(gsplat.clone()));

    // sim (z-up) → render (y-up); recentre the block on the origin.
    let to_render = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
        * Mat4::from_translation(Vec3::new(-c.x, -c.y, 0.0));

    // Ground.
    let bw = scene.bbox_m[2] - scene.bbox_m[0] + 80.0;
    let bh = scene.bbox_m[3] - scene.bbox_m[1] + 80.0;
    let mut ground = MeshAcc::new();
    ground.add_box(
        to_render
            * Mat4::from_translation(Vec3::new(c.x, c.y, -0.05))
            * Mat4::from_scale(Vec3::new(bw, bh, 0.1)),
    );
    push_mesh(ctx, &cad, "ground", &ground, ASPHALT);

    // Roads (one merged batch).
    let mut roads = MeshAcc::new();
    for r in &scene.roads {
        for seg in r.path.windows(2) {
            let a = Vec3::new(seg[0][0], seg[0][1], 0.05);
            let b = Vec3::new(seg[1][0], seg[1][1], 0.05);
            let d = b - a;
            let len = d.length();
            if len < 0.5 {
                continue;
            }
            let yaw = d.y.atan2(d.x);
            roads.add_box(
                to_render
                    * Mat4::from_translation((a + b) * 0.5)
                    * Mat4::from_rotation_z(yaw)
                    * Mat4::from_scale(Vec3::new(len, r.width, 0.08)),
            );
        }
    }
    push_mesh(ctx, &cad, "roads", &roads, ROAD);

    // Buildings (split concrete / glass by height for a little variety).
    let mut low = MeshAcc::new();
    let mut tall = MeshAcc::new();
    for b in &scene.buildings {
        let cx = 0.5 * (b.aabb[0] + b.aabb[2]);
        let cy = 0.5 * (b.aabb[1] + b.aabb[3]);
        let dx = (b.aabb[2] - b.aabb[0]).max(1.0);
        let dy = (b.aabb[3] - b.aabb[1]).max(1.0);
        let m = to_render
            * Mat4::from_translation(Vec3::new(cx, cy, b.height * 0.5))
            * Mat4::from_scale(Vec3::new(dx, dy, b.height));
        if b.height >= 30.0 {
            tall.add_box(m);
        } else {
            low.add_box(m);
        }
    }
    push_mesh(ctx, &cad, "buildings_low", &low, CONCRETE);
    push_mesh(ctx, &cad, "buildings_tall", &tall, GLASS);

    // Point assets (poles / lamps / signals / trees / hydrants …) — each is its
    // own pickable batch keyed by its kotoba entity id, so a click resolves to
    // the EAVT record (kind / company / installYear / costJpy / provenance).
    let mut pick_map: std::collections::HashMap<String, CityObject> = std::collections::HashMap::new();
    for o in &scene.objects {
        let (fx, fy) = object_footprint(&o.kind);
        let m = to_render
            * Mat4::from_translation(Vec3::new(o.pos[0], o.pos[1], o.h * 0.5))
            * Mat4::from_scale(Vec3::new(fx, fy, o.h));
        let (lp, ln, li) = unit_box();
        let wp: Vec<[f32; 3]> = lp.iter().map(|v| {
            let w = m.transform_point3(Vec3::from_array(*v));
            [w.x, w.y, w.z]
        }).collect();
        cad.push_triangles(ctx, o.id.clone(), o.kind.clone(), &wp, &ln, &li, object_color(&o.kind), Mat4::IDENTITY);
        pick_map.insert(o.id.clone(), o.clone());
    }
    log::info!("[shibuya] {} clickable assets (kotoba-linked)", scene.objects.len());

    // Agents.
    let obstacles = scene.building_obstacles();
    let spawns = scene.spawn_points(6);
    let mut agents: Vec<Agent> = Vec::new();
    let mut agent_batch: Vec<(usize, Vec<[f32; 3]>)> = Vec::new();
    let (ap, an, ai) = unit_box();
    for (k, (pos, yaw)) in spawns.iter().enumerate() {
        let size = Vec3::new(4.2, 1.9, 1.6);
        let mut a = Agent::new(size, 1100.0, obstacles.clone(), AGENT_COLORS[k % 6]);
        a.steer_phase = k as f32 * 1.3;
        a.place(*pos, *yaw);
        let lp: Vec<[f32; 3]> = ap
            .iter()
            .map(|v| [v[0] * size.x, v[1] * size.y, v[2] * size.z])
            .collect();
        let idx = cad.batch_count();
        cad.push_triangles(ctx, format!("agent_{k}"), format!("Agent {k}"),
            &lp, &an, &ai, a.color, to_render * a.body_world());
        agent_batch.push((idx, lp));
        agents.push(a);
    }
    log::info!("[shibuya] spawned {} agents", agents.len());

    let render = cad.clone();
    let picker = cad.clone();
    let mut step: u64 = 0;
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .with_pipeline(gsplat)
        .on_update(move |_world, camera, _dt| {
            // The default far plane (256 m) clips this ~900 m city block; widen
            // the frustum so the whole scene is visible (reset each frame in
            // case a resize rebuilds the projection).
            {
                let rc = camera.as_render_mut();
                rc.near = 1.0;
                rc.far = 4000.0;
            }
            // Click an asset → resolve its kotoba EAVT record → publish to HUD.
            if let Some(p) = picker.pick_from_camera_if_clicked(camera) {
                picker.set_highlighted_by_id(&p.feature_id);
                if let Some(o) = pick_map.get(&p.feature_id) {
                    publish_pick(&format!(
                        r#"{{"id":"{}","kind":"{}","company":"{}","installYear":{},"costJpy":{},"provenance":"{}"}}"#,
                        o.id, o.kind, o.attrs.company, o.attrs.install_year, o.attrs.cost_jpy, o.attrs.provenance
                    ));
                }
            }
            // 2 substeps/frame at 120 Hz ≈ 60 fps wall.
            for _ in 0..2 {
                let t = step as f32 * DT;
                for a in agents.iter_mut() {
                    a.step(t);
                }
                step += 1;
            }
            for (k, a) in agents.iter().enumerate() {
                let (idx, lp) = &agent_batch[k];
                render.replace_batch_world(*idx, lp, &an, &ai, a.color, to_render * a.body_world());
            }
        });

    log::info!("[shibuya] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Standalone 3-D Gaussian-Splat viewer: sky + GsplatAdapter, orbit camera
/// framed on a cloud centred at the origin (radius ≈ 60, normalised by
/// `opensfm_to_splat.py`). The JS shell loads a `.splat` via `shibuyaLoadSplat`.
/// Used to view REAL Mapillary-SfM point clouds (Tsuru / Boston / …).
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_splat_viewer_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("splat")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::ZERO,
            distance: 170.0,
            yaw: 0.6,
            pitch: 0.22,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let gsplat = GsplatAdapter::new(ctx);
    SPLAT.with(|s| *s.borrow_mut() = Some(gsplat.clone()));

    let app = app
        .with_pipeline(sky)
        .with_pipeline(gsplat)
        .on_update(move |_world, camera, _dt| {
            let rc = camera.as_render_mut();
            rc.near = 0.5;
            rc.far = 3000.0;
        });

    log::info!("[splat-viewer] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

/// #3 — Splat backdrop + live physics: the real Mapillary-SfM cloud as the
/// visual world, with kami-genesis floating-base agents doing full physics on a
/// ground plane inside it. (Pairs the GsplatAdapter overlay with the
/// ContactWorld sim — a coarse "physics on a captured city" integration.)
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_splat_physics_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("splat")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit { target: Vec3::new(0.0, 6.0, 0.0), distance: 150.0, yaw: 0.6, pitch: 0.25 })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let gsplat = GsplatAdapter::new(ctx);
    SPLAT.with(|s| *s.borrow_mut() = Some(gsplat.clone()));

    let to_render = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let (ap, an, ai) = unit_box();

    // Ground plane the agents rest on (render y = 0).
    let mut ground = MeshAcc::new();
    ground.add_box(Mat4::from_translation(Vec3::new(0.0, -0.05, 0.0)) * Mat4::from_scale(Vec3::new(70.0, 0.1, 70.0)));
    push_mesh(ctx, &cad, "ground", &ground, [0.22, 0.22, 0.26]);

    // A handful of physics agents on the ground inside the splat.
    let mut agents: Vec<Agent> = Vec::new();
    let mut agent_batch: Vec<(usize, Vec<[f32; 3]>)> = Vec::new();
    for k in 0..4 {
        let size = Vec3::new(3.0, 1.6, 1.4);
        let mut a = Agent::new(size, 900.0, vec![], AGENT_COLORS[k % 6]);
        a.steer_phase = k as f32 * 1.7;
        let ang = k as f32 * 1.57;
        a.place(Vec3::new(8.0 * ang.cos(), 8.0 * ang.sin(), 3.0), ang);
        let lp: Vec<[f32; 3]> = ap.iter().map(|v| [v[0] * size.x, v[1] * size.y, v[2] * size.z]).collect();
        let idx = cad.batch_count();
        cad.push_triangles(ctx, format!("agent_{k}"), format!("Agent {k}"), &lp, &an, &ai, a.color, to_render * a.body_world());
        agent_batch.push((idx, lp));
        agents.push(a);
    }

    let render = cad.clone();
    let mut step: u64 = 0;
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .with_pipeline(gsplat)
        .on_update(move |_world, camera, _dt| {
            { let rc = camera.as_render_mut(); rc.near = 0.5; rc.far = 3000.0; }
            for _ in 0..2 {
                let t = step as f32 * DT;
                for a in agents.iter_mut() { a.step(t); }
                step += 1;
            }
            for (k, a) in agents.iter().enumerate() {
                let (idx, lp) = &agent_batch[k];
                render.replace_batch_world(*idx, lp, &an, &ai, a.color, to_render * a.body_world());
            }
        });

    log::info!("[splat-physics] backend={:?}", app.backend());
    app.run().await.map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(target_family = "wasm")]
fn push_mesh(
    ctx: &RenderContext,
    cad: &kami_pipelines::CadSceneAdapter,
    id: &str,
    mesh: &MeshAcc,
    color: [f32; 3],
) {
    if mesh.i.is_empty() {
        return;
    }
    cad.push_triangles(ctx, id, id, &mesh.p, &mesh.n, &mesh.i, color, Mat4::IDENTITY);
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    #[test]
    fn scene_loads_shibuya() {
        let s = Scene::load();
        assert!(s.buildings.len() > 50, "buildings: {}", s.buildings.len());
        assert!(s.roads.len() > 50, "roads: {}", s.roads.len());
        assert_eq!(s.building_obstacles().len(), s.buildings.len());
    }

    #[test]
    fn scene_has_kotoba_linked_objects() {
        // Point assets carry the attributes a click surfaces from kotoba.
        let s = Scene::load();
        assert!(!s.objects.is_empty(), "expected clickable point assets");
        for o in &s.objects {
            assert!(!o.id.is_empty() && !o.kind.is_empty());
            assert!(o.attrs.install_year >= 1900 && o.attrs.install_year <= 2025);
            assert!(o.attrs.cost_jpy > 0 && !o.attrs.company.is_empty());
            assert!(o.attrs.provenance == "osm" || o.attrs.provenance == "synthesized-demo");
            // every asset kind has a render footprint + colour
            let _ = object_footprint(&o.kind);
            let _ = object_color(&o.kind);
        }
    }

    #[test]
    fn agent_is_4dof_floating_base() {
        let a = Agent::new(Vec3::new(4.0, 2.0, 1.5), 1000.0, vec![], [1.0; 3]);
        assert_eq!(a.cfg.ndof, 4, "x,y,z,yaw");
    }

    #[test]
    fn agent_falls_and_settles_on_road() {
        // Dropped from z=3 with no buildings, the agent must come to rest on the
        // ground plane (corner spheres on z=0), not penetrate or fly off.
        let mut a = Agent::new(Vec3::new(4.0, 2.0, 1.5), 1000.0, vec![], [1.0; 3]);
        a.drive = 0.0;
        a.place(Vec3::new(0.0, 0.0, 3.0), 0.0);
        for s in 0..1500 {
            a.step(s as f32 * DT);
        }
        let z = a.state.q[2];
        assert!(z.is_finite(), "z went non-finite");
        assert!(z > 0.2 && z < 1.4, "did not settle on road: z={z}");
        assert!(a.state.qdot[2].abs() < 0.3, "still moving vertically: {}", a.state.qdot[2]);
    }

    #[test]
    fn agent_blocked_by_building_wall() {
        // A building wall in front; the agent driving into it must not pass
        // through (x stays on the near side of the wall).
        let wall = Obstacle::Aabb {
            min: Vec3::new(10.0, -20.0, 0.0),
            max: Vec3::new(40.0, 20.0, 30.0),
        };
        let mut a = Agent::new(Vec3::new(4.0, 2.0, 1.5), 1000.0, vec![wall], [1.0; 3]);
        a.steer_phase = 0.0;
        a.place(Vec3::new(0.0, 0.0, 1.0), 0.0); // facing +x toward the wall
        for s in 0..2000 {
            a.step(s as f32 * DT);
        }
        // Body centre cannot cross x=10 (minus collision margin).
        assert!(a.state.q[0] < 9.5, "agent tunnelled into building: x={}", a.state.q[0]);
        assert!(a.state.q.iter().all(|v| v.is_finite()));
    }
}
