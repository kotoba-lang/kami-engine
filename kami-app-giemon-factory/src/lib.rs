//! kami-app-giemon-factory — the factory that manufactures the giemon robot
//! line (arm6 + kabitori), designed end-to-end in kami-engine + kotoba.
//!
//! Two WASM entries over one hand-authored scene
//! (`70-tools/e7m-sim/scenes/giemon-factory-r0/`):
//!
//!   * `run_giemon_factory_v1`       — the COMPLETED plant: building shell +
//!     production machines as kami-genesis static geometry/obstacles, with live
//!     giemon arm6 work-cells (fixed-base `Articulation3d` running a PD work
//!     cycle) and free-roaming 4-DOF AGV carts that collide with walls / columns
//!     / machines (the shibuya floating-base agent path).
//!
//!   * `run_giemon_factory_build_v1` — the 4D 建築手順 PLAYBACK: every element
//!     starts hidden and is revealed in `construction.order.json` `:seq` order
//!     (site-prep → foundation → steel → roof → cladding → floor → machines →
//!     conveyor → robots → commissioning), so you watch the plant get built
//!     per the construction sequence stored as kotoba datoms.
//!
//! Clean-room (no NVIDIA / PhysX / Isaac). Sim is z-up; the renderer is y-up, so
//! the whole scene is rotated −90° about X for display (shibuya convention).

pub mod scene;
pub use scene::{Clashes, ConstructionOrder, Factory, Robots};

use glam::{Mat4, Quat, Vec3};
use kami_genesis::{
    Articulation3dConfig, Articulation3dState, Collider, ContactParams, ContactWorld, Obstacle,
};

#[cfg(target_family = "wasm")]
use kami_app::{CameraMode, InputMode, KamiApp};
#[cfg(target_family = "wasm")]
use kami_pipelines::unit_box;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

const DT: f32 = 1.0 / 240.0;
#[cfg(target_family = "wasm")]
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

// ── arm6 work-cell (fixed-base manipulator, the product redeployed) ───────────

const ARM6_URDF: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon_arm6/giemon_arm6.urdf");

/// Parse the arm6 URDF into a 3-D articulation config (fixed base at origin).
pub fn arm6_config() -> Articulation3dConfig {
    let sys = kami_articulated::parse_urdf(ARM6_URDF).expect("giemon_arm6.urdf parses");
    Articulation3dConfig::from_articulated_system(&sys, Vec3::new(0.0, 0.0, -9.81), DT)
}

/// One live arm6 work-cell: its own state + ground contact, driven by a PD
/// work-cycle that holds a working pose against gravity and sweeps it slowly.
pub struct ArmCell {
    pub cfg: Articulation3dConfig,
    pub state: Articulation3dState,
    pub contact: ContactWorld,
    pub cell_world: Mat4, // sim-space placement (z-up) of the cell base
    pub phase: f32,
    pub nb: usize,
    pub segs: Vec<Vec3>,
    pub thicks: Vec<f32>,
    pub link_local: Vec<Mat4>,
}

impl ArmCell {
    pub fn new(cfg: Articulation3dConfig, pos: Vec3, yaw: f32, phase: f32) -> Self {
        let nb = cfg.n_bodies();
        let segs: Vec<Vec3> = (0..nb).map(|i| link_segment(&cfg, i)).collect();
        let thicks: Vec<f32> = (0..nb)
            .map(|i| (0.040 - 0.004 * i as f32).max(0.014))
            .collect();
        let link_local: Vec<Mat4> = (0..nb).map(|i| segment_box(segs[i], thicks[i])).collect();
        // distal sphere per link vs the cell floor (z = 0).
        let colliders: Vec<(usize, Collider)> = (1..nb)
            .map(|i| {
                (
                    i,
                    Collider::Sphere {
                        center: segs[i],
                        radius: thicks[i] * 0.6,
                    },
                )
            })
            .collect();
        let contact = ContactWorld::new(
            colliders,
            ContactParams {
                ground_z: 0.0,
                friction: 0.9,
                ..Default::default()
            },
        );
        let mut state = Articulation3dState::zeros(cfg.ndof);
        // start in a raised working pose.
        if cfg.ndof >= 3 {
            state.q[1] = -0.5;
            state.q[2] = 0.8;
        }
        Self {
            cfg,
            state,
            contact,
            cell_world: Mat4::from_translation(pos) * Mat4::from_rotation_z(yaw),
            phase,
            nb,
            segs,
            thicks,
            link_local,
        }
    }

    /// PD work-cycle: track a slowly-sweeping target pose, then one contact step.
    pub fn step(&mut self, t: f32) {
        let n = self.cfg.ndof;
        let mut tau = vec![0.0_f32; n];
        let target = |base: f32, amp: f32, w: f32| base + amp * (t * w + self.phase).sin();
        let kp = 26.0_f32;
        let kd = 1.8_f32;
        for j in 0..n {
            let tgt = match j {
                0 => target(0.0, 0.6, 0.5),
                1 => target(-0.5, 0.25, 0.7),
                2 => target(0.8, 0.30, 0.8),
                4 => target(0.4, 0.35, 1.1),
                _ => target(0.0, 0.2, 0.9),
            };
            tau[j] = kp * (tgt - self.state.q[j]) - kd * self.state.qdot[j];
        }
        self.contact.step(&self.cfg, &mut self.state, &tau);
    }

    pub fn fk(&self) -> Vec<Mat4> {
        self.cfg.fk_world(&self.state.q)
    }
}

/// Segment vector (body frame) from body `i` origin to its child's origin.
fn link_segment(cfg: &Articulation3dConfig, i: usize) -> Vec3 {
    cfg.bodies
        .iter()
        .find(|b| b.parent == i as isize)
        .map(|c| c.r_tree)
        .unwrap_or(Vec3::new(0.0, 0.0, 0.05))
}

/// Local box transform for a link segment (body frame): beam from origin to seg.
fn segment_box(seg: Vec3, thick: f32) -> Mat4 {
    let len = seg.length().max(1.0e-4);
    let dir = seg / len;
    Mat4::from_scale_rotation_translation(
        Vec3::new(thick, thick, len),
        Quat::from_rotation_arc(Vec3::Z, dir),
        seg * 0.5,
    )
}

// ── AGV cart (4-DOF floating base — shibuya agent path) ───────────────────────

/// Box-inertia URDF for a 4-DOF floating-base agent: world → px → py → pz →
/// (yaw) body. (Copied from kami-app-shibuya — the validated agent topology.)
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
        r#"<robot name="agv">
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

/// A factory-floor AGV: full-physics 4-DOF cart that collides with the building.
pub struct Agv {
    pub cfg: Articulation3dConfig,
    pub state: Articulation3dState,
    pub contact: ContactWorld,
    pub body_idx: usize,
    pub size: Vec3,
    pub drive: f32,
    pub steer_phase: f32,
}

impl Agv {
    pub fn new(size: Vec3, mass: f32, obstacles: Vec<Obstacle>) -> Self {
        let sys = kami_articulated::parse_urdf(&agent_urdf(size, mass)).expect("agv urdf");
        let cfg =
            Articulation3dConfig::from_articulated_system(&sys, Vec3::new(0.0, 0.0, -9.81), DT);
        let ndof = cfg.ndof;
        let body_idx = cfg.body_index("body").expect("body link");
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
            ContactParams {
                ground_z: 0.0,
                friction: 1.0,
                ..Default::default()
            },
        )
        .with_obstacles(obstacles);
        Self {
            cfg,
            state: Articulation3dState::zeros(ndof),
            contact,
            body_idx,
            size,
            drive: mass * 1.6,
            steer_phase: 0.0,
        }
    }

    pub fn place(&mut self, pos: Vec3, yaw: f32) {
        self.state.q[0] = pos.x;
        self.state.q[1] = pos.y;
        self.state.q[2] = pos.z;
        self.state.q[3] = yaw;
    }

    pub fn step(&mut self, t: f32) {
        let yaw = self.state.q[3];
        let (sy, cy) = yaw.sin_cos();
        let drag = 10.0;
        let mut tau = vec![0.0_f32; self.cfg.ndof];
        tau[0] = self.drive * cy - drag * self.state.qdot[0];
        tau[1] = self.drive * sy - drag * self.state.qdot[1];
        tau[3] = 90.0 * (t * 0.5 + self.steer_phase).sin() - 22.0 * self.state.qdot[3];
        self.contact.step(&self.cfg, &mut self.state, &tau);
    }

    /// Free body: zero control — gravity + ground/obstacle contact only. A
    /// material payload dropped from height settles physically with this.
    pub fn step_free(&mut self) {
        let tau = vec![0.0_f32; self.cfg.ndof];
        self.contact.step(&self.cfg, &mut self.state, &tau);
    }

    /// Drive physically toward a world-frame target with a clamped
    /// position-PD on the (world-axis) prismatic joints, then one contact step.
    /// Yaw is steered to the heading cosmetically. Used by the delivery cart.
    pub fn step_toward(&mut self, tx: f32, ty: f32) {
        let dx = tx - self.state.q[0];
        let dy = ty - self.state.q[1];
        // gains sized to overcome ground friction (μN ≈ mass·g; drive = mass·1.6).
        let kp = self.drive * 6.0;
        let cap = self.drive * 10.0;
        let drag = 60.0;
        let mut tau = vec![0.0_f32; self.cfg.ndof];
        tau[0] = (kp * dx).clamp(-cap, cap) - drag * self.state.qdot[0];
        tau[1] = (kp * dy).clamp(-cap, cap) - drag * self.state.qdot[1];
        // cosmetic yaw toward the bearing.
        let target_yaw = dy.atan2(dx);
        let mut dyaw = target_yaw - self.state.q[3];
        let pi = std::f32::consts::PI;
        while dyaw > pi {
            dyaw -= 2.0 * pi;
        }
        while dyaw < -pi {
            dyaw += 2.0 * pi;
        }
        tau[3] = 60.0 * dyaw - 20.0 * self.state.qdot[3];
        self.contact.step(&self.cfg, &mut self.state, &tau);
    }

    pub fn pos_z(&self) -> f32 {
        self.state.q[2]
    }

    pub fn body_world(&self) -> Mat4 {
        self.cfg.fk_world(&self.state.q)[self.body_idx]
    }
}

// ── static building geometry (every element a unit-box instance, id-tagged) ───

/// One static, revealable building element rendered as a transformed unit box.
pub struct StaticBox {
    pub id: String,
    pub world: Mat4, // already includes the y-up `to_render` rotation
    pub color: [f32; 3],
}

const C_GROUND: [f32; 3] = [0.13, 0.14, 0.13];
const C_FLOOR: [f32; 3] = [0.54, 0.54, 0.52];
const C_WALL: [f32; 3] = [0.71, 0.72, 0.73];
const C_STEEL: [f32; 3] = [0.45, 0.47, 0.52];
const C_CONV: [f32; 3] = [0.24, 0.24, 0.27];
const C_EQUIP: [f32; 3] = [0.50, 0.52, 0.55]; // service-node equipment
const C_LIGHT: [f32; 3] = [0.98, 0.95, 0.70]; // luminaire
const C_EXIT: [f32; 3] = [0.30, 0.85, 0.40]; // 誘導灯
const C_GREEN: [f32; 3] = [0.24, 0.42, 0.22]; // landscaping
const C_FENCE: [f32; 3] = [0.40, 0.42, 0.46];
const C_GATE: [f32; 3] = [0.55, 0.55, 0.58];
const C_POLE: [f32; 3] = [0.32, 0.32, 0.34];
const C_BOLLARD: [f32; 3] = [0.92, 0.72, 0.12];
const C_SIGN: [f32; 3] = [0.92, 0.92, 0.92];

fn machine_color(kind: &str) -> [f32; 3] {
    match kind {
        "cnc-mill" => [0.38, 0.46, 0.56],
        "lathe" => [0.46, 0.42, 0.40],
        "cmm" => [0.80, 0.82, 0.85],
        "bench" => [0.28, 0.55, 0.49],
        "rack" => [0.72, 0.50, 0.24],
        "packing" => [0.58, 0.50, 0.30],
        _ => [0.50, 0.50, 0.52],
    }
}

/// Build every static element of the plant as id-tagged boxes (pure geometry,
/// no GPU). `to_render` maps sim z-up → render y-up.
pub fn static_boxes(f: &Factory, to_render: Mat4) -> Vec<StaticBox> {
    let mut out = Vec::new();
    let mut push = |id: &str, world: Mat4, color: [f32; 3]| {
        out.push(StaticBox {
            id: id.to_string(),
            world,
            color,
        });
    };

    let bw = f.bbox_m[2] - f.bbox_m[0];
    let bh = f.bbox_m[3] - f.bbox_m[1];
    let c = f.center();
    // ground covers the whole SITE (incl. 外構); structural floor slab = building bay.
    let se = f.site_extent();
    let (gcx, gcy) = (0.5 * (se[0] + se[2]), 0.5 * (se[1] + se[3]));
    let (gw, gh) = ((se[2] - se[0]).max(1.0), (se[3] - se[1]).max(1.0));
    push(
        "ground",
        to_render
            * Mat4::from_translation(Vec3::new(gcx, gcy, -0.10))
            * Mat4::from_scale(Vec3::new(gw, gh, 0.12)),
        C_GROUND,
    );
    push(
        "floor",
        to_render
            * Mat4::from_translation(Vec3::new(c.x, c.y, 0.02))
            * Mat4::from_scale(Vec3::new(bw, bh, 0.06)),
        C_FLOOR,
    );

    for col in &f.columns {
        push(
            &col.id,
            to_render
                * Mat4::from_translation(Vec3::new(col.x, col.y, col.height * 0.5))
                * Mat4::from_scale(Vec3::new(col.w, col.w, col.height)),
            C_STEEL,
        );
    }
    for b in &f.beams {
        let len = (b.span_y[1] - b.span_y[0]).abs();
        let mid = 0.5 * (b.span_y[0] + b.span_y[1]);
        push(
            &b.id,
            to_render
                * Mat4::from_translation(Vec3::new(b.x, mid, b.z))
                * Mat4::from_scale(Vec3::new(b.section, len, b.section)),
            C_STEEL,
        );
    }
    for w in &f.walls {
        let (cx, cy, dx, dy) = aabb_box(&w.aabb);
        push(
            &w.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, w.height * 0.5))
                * Mat4::from_scale(Vec3::new(dx, dy, w.height)),
            C_WALL,
        );
    }
    for z in &f.zones {
        let (cx, cy, dx, dy) = aabb_box(&z.rect);
        push(
            &z.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, 0.07))
                * Mat4::from_scale(Vec3::new(dx - 0.4, dy - 0.4, 0.02)),
            z.tint,
        );
    }
    for m in &f.machines {
        let (cx, cy, dx, dy) = aabb_box(&m.aabb);
        push(
            &m.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, m.height * 0.5))
                * Mat4::from_scale(Vec3::new(dx, dy, m.height)),
            machine_color(&m.kind),
        );
    }
    for cv in &f.conveyors {
        for seg in cv.path.windows(2) {
            let a = Vec3::new(seg[0][0], seg[0][1], 0.0);
            let b = Vec3::new(seg[1][0], seg[1][1], 0.0);
            let d = b - a;
            let len = d.length();
            if len < 0.2 {
                continue;
            }
            let yaw = d.y.atan2(d.x);
            push(
                &cv.id,
                to_render
                    * Mat4::from_translation((a + b) * 0.5 + Vec3::new(0.0, 0.0, 0.45))
                    * Mat4::from_rotation_z(yaw)
                    * Mat4::from_scale(Vec3::new(len, cv.width, 0.25)),
                C_CONV,
            );
        }
    }

    // ── MEP service nodes (引込/受電/受水/整圧/通信) ──────────────────────────
    for n in &f.service_nodes {
        let (cx, cy, dx, dy) = aabb_box(&n.aabb);
        push(
            &n.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, n.height * 0.5))
                * Mat4::from_scale(Vec3::new(dx, dy, n.height)),
            C_EQUIP,
        );
    }

    // ── routed utility networks (電気/給排水/ガス/圧空/データ/消火) ───────────
    // Underground runs (z < 0) are traced just above the slab so they stay
    // visible in the design viewer; the design z is retained in the EDN/SBOM.
    for u in &f.utilities {
        let col = utility_color(&u.kind);
        let zr = if u.z >= 0.0 { u.z } else { 0.10 };
        let sec = u.width.max(0.12);
        for seg in u.path.windows(2) {
            let a = Vec3::new(seg[0][0], seg[0][1], zr);
            let b = Vec3::new(seg[1][0], seg[1][1], zr);
            let d = b - a;
            let len = d.length();
            if len < 0.15 {
                continue;
            }
            let yaw = d.y.atan2(d.x);
            push(
                &u.id,
                to_render
                    * Mat4::from_translation((a + b) * 0.5)
                    * Mat4::from_rotation_z(yaw)
                    * Mat4::from_scale(Vec3::new(len, sec, sec)),
                col,
            );
        }
    }

    // ── point fixtures (照明 / 誘導灯) ───────────────────────────────────────
    for x in &f.fixtures {
        let col = if x.id == "exit_lights" {
            C_EXIT
        } else {
            C_LIGHT
        };
        for p in &x.points {
            push(
                &x.id,
                to_render
                    * Mat4::from_translation(Vec3::new(p[0], p[1], p[2]))
                    * Mat4::from_scale(Vec3::new(x.size, x.size, 0.25)),
                col,
            );
        }
    }

    // ── 外構: pavements + greens (flat) ──────────────────────────────────────
    for p in &f.site_pavements {
        let (cx, cy, dx, dy) = aabb_box(&p.rect);
        push(
            &p.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, 0.005))
                * Mat4::from_scale(Vec3::new(dx, dy, 0.04)),
            site_color(&p.kind),
        );
    }
    for p in &f.site_greens {
        let (cx, cy, dx, dy) = aabb_box(&p.rect);
        push(
            &p.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, 0.015))
                * Mat4::from_scale(Vec3::new(dx, dy, 0.05)),
            C_GREEN,
        );
    }

    // ── 外構: fences / gate ──────────────────────────────────────────────────
    for s in &f.site_structures {
        let (cx, cy, dx, dy) = aabb_box(&s.aabb);
        let col = if s.id == "site_gate" { C_GATE } else { C_FENCE };
        push(
            &s.id,
            to_render
                * Mat4::from_translation(Vec3::new(cx, cy, s.height * 0.5))
                * Mat4::from_scale(Vec3::new(dx, dy, s.height)),
            col,
        );
    }

    // ── 外構: posts (外灯ポール / 車止め / 看板) ──────────────────────────────
    for p in &f.site_posts {
        if p.kind.contains("外灯") {
            push(
                &p.id,
                to_render
                    * Mat4::from_translation(Vec3::new(p.x, p.y, p.height * 0.5))
                    * Mat4::from_scale(Vec3::new(0.18, 0.18, p.height)),
                C_POLE,
            );
            // lamp head (same id → revealed together)
            push(
                &p.id,
                to_render
                    * Mat4::from_translation(Vec3::new(p.x, p.y, p.height + 0.15))
                    * Mat4::from_scale(Vec3::new(0.55, 0.55, 0.3)),
                C_LIGHT,
            );
        } else if p.kind.contains("看板") {
            push(
                &p.id,
                to_render
                    * Mat4::from_translation(Vec3::new(p.x, p.y, p.height * 0.5))
                    * Mat4::from_scale(Vec3::new(2.6, 0.2, p.height)),
                C_SIGN,
            );
        } else {
            // bollard
            push(
                &p.id,
                to_render
                    * Mat4::from_translation(Vec3::new(p.x, p.y, p.height * 0.5))
                    * Mat4::from_scale(Vec3::new(0.25, 0.25, p.height)),
                C_BOLLARD,
            );
        }
    }

    out
}

/// Colour-code a routed utility network by its `kind`.
fn utility_color(kind: &str) -> [f32; 3] {
    match kind {
        "electrical-busway" => [0.95, 0.45, 0.10],
        "electrical-branch" => [0.95, 0.62, 0.22],
        "electrical-underground" => [0.85, 0.30, 0.15],
        "water-supply" => [0.15, 0.45, 0.92],
        "hot-water" => [0.90, 0.35, 0.55],
        "drainage" => [0.45, 0.45, 0.50],
        "storm-drain" => [0.35, 0.60, 0.80],
        "gas-supply" => [0.95, 0.80, 0.15],
        "compressed-air" => [0.20, 0.78, 0.82],
        "data-backbone" => [0.25, 0.78, 0.38],
        "fire-main" => [0.85, 0.12, 0.12],
        _ => [0.62, 0.62, 0.64],
    }
}

/// Colour for a site pavement by its `kind` (substring match on JP labels).
fn site_color(kind: &str) -> [f32; 3] {
    if kind.contains("walkway") || kind.contains("歩") {
        [0.55, 0.55, 0.50]
    } else if kind.contains("park") || kind.contains("駐") {
        [0.20, 0.20, 0.23]
    } else {
        [0.17, 0.17, 0.19] // asphalt road / apron / drive
    }
}

fn aabb_box(a: &[f32; 4]) -> (f32, f32, f32, f32) {
    (
        0.5 * (a[0] + a[2]),
        0.5 * (a[1] + a[3]),
        (a[2] - a[0]).max(0.05),
        (a[3] - a[1]).max(0.05),
    )
}

// ── shared placement: render transform for a cell's body `i` ─────────────────

#[cfg(target_family = "wasm")]
fn cell_body_world(to_render: Mat4, cell_world: Mat4, fk_i: Mat4, link_local: Mat4) -> Mat4 {
    to_render * cell_world * fk_i * link_local
}

// ════════════════════════════════════════════════════════════════════════════
//  Entry 1 — completed plant, live robots + AGVs
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_family = "wasm")]
const ARM_COLOR: [f32; 3] = [0.96, 0.55, 0.13];
#[cfg(target_family = "wasm")]
const AGV_COLOR: [f32; 3] = [0.93, 0.78, 0.20];

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_factory_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let f = Factory::load();
    let c = f.center();
    log::info!(
        "[giemon-factory] {} — {} walls, {} columns, {} machines, {} cells, {} AGVs",
        f.name,
        f.walls.len(),
        f.columns.len(),
        f.machines.len(),
        f.cells.len(),
        f.agvs.len()
    );

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon-factory")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 4.0, 0.0),
            distance: 155.0,
            yaw: 0.7,
            pitch: 0.62,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let (bp, bn, bi) = unit_box();

    let to_render =
        Mat4::from_rotation_x(-HALF_PI) * Mat4::from_translation(Vec3::new(-c.x, -c.y, 0.0));

    // Static building + machines.
    for sb in static_boxes(&f, to_render) {
        cad.push_triangles(
            ctx,
            sb.id.clone(),
            sb.id.clone(),
            &bp,
            &bn,
            &bi,
            sb.color,
            sb.world,
        );
    }

    // Arm6 work-cells (relative to the recentred origin).
    let mut arms: Vec<(ArmCell, usize)> = Vec::new();
    for (k, cell) in f.cells.iter().enumerate() {
        let pos = Vec3::new(cell.pos[0] - c.x, cell.pos[1] - c.y, cell.pos[2]);
        let mut arm = ArmCell::new(arm6_config(), pos, cell.yaw, k as f32 * 1.7);
        let start = cad.batch_count();
        let fk0 = arm.fk();
        for i in 0..arm.nb {
            cad.push_triangles(
                ctx,
                format!("{}_l{i}", cell.id),
                format!("{} link {i}", cell.id),
                &bp,
                &bn,
                &bi,
                ARM_COLOR,
                cell_body_world(to_render, arm.cell_world, fk0[i], arm.link_local[i]),
            );
        }
        arm.state = Articulation3dState::zeros(arm.cfg.ndof);
        if arm.cfg.ndof >= 3 {
            arm.state.q[1] = -0.5;
            arm.state.q[2] = 0.8;
        }
        arms.push((arm, start));
    }

    // AGV carts (factory-frame floating base; collide with building).
    let obstacles = f.agv_obstacles();
    let mut agvs: Vec<(Agv, usize, Vec<[f32; 3]>)> = Vec::new();
    for (k, a) in f.agvs.iter().enumerate() {
        let size = Vec3::new(a.size[0], a.size[1], a.size[2]);
        let mut agv = Agv::new(size, a.mass, obstacles.clone());
        agv.steer_phase = k as f32 * 2.1;
        agv.place(Vec3::new(a.pos[0], a.pos[1], a.pos[2]), a.yaw);
        let lp: Vec<[f32; 3]> = bp
            .iter()
            .map(|v| [v[0] * size.x, v[1] * size.y, v[2] * size.z])
            .collect();
        let idx = cad.batch_count();
        cad.push_triangles(
            ctx,
            a.id.clone(),
            a.id.clone(),
            &lp,
            &bn,
            &bi,
            AGV_COLOR,
            to_render * agv.body_world(),
        );
        agvs.push((agv, idx, lp));
    }

    // Engineering clash markers (red = hard structure penetration, orange =
    // service-coordination clash) — generated by engineering.py → clashes.json.
    let clashes = scene::Clashes::load();
    for cl in &clashes.clashes {
        let col = if cl.kind == "hard" {
            [0.96, 0.12, 0.12]
        } else {
            [0.98, 0.58, 0.10]
        };
        let zr = if cl.z >= 0.0 { cl.z } else { 0.15 };
        cad.push_triangles(
            ctx,
            cl.id.clone(),
            format!("CLASH {} [{}]", cl.systems.join("×"), cl.kind),
            &bp,
            &bn,
            &bi,
            col,
            to_render
                * Mat4::from_translation(Vec3::new(cl.x, cl.y, zr))
                * Mat4::from_scale(Vec3::splat(1.3)),
        );
    }
    log::info!(
        "[giemon-factory] batches={} clashes={}",
        cad.batch_count(),
        clashes.clashes.len()
    );

    let render = cad.clone();
    let pick = cad.clone();
    let mut step: u64 = 0;
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, _dt| {
            {
                let rc = camera.as_render_mut();
                rc.near = 0.5;
                rc.far = 1200.0;
            }
            for _ in 0..4 {
                let t = step as f32 * DT;
                for (arm, _) in arms.iter_mut() {
                    arm.step(t);
                }
                for (agv, _, _) in agvs.iter_mut() {
                    agv.step(t);
                }
                step += 1;
            }
            for (arm, start) in arms.iter() {
                let fk = arm.fk();
                for i in 0..arm.nb {
                    render.replace_batch_world(
                        start + i,
                        &bp,
                        &bn,
                        &bi,
                        ARM_COLOR,
                        cell_body_world(to_render, arm.cell_world, fk[i], arm.link_local[i]),
                    );
                }
            }
            for (agv, idx, lp) in agvs.iter() {
                render.replace_batch_world(
                    *idx,
                    lp,
                    &bn,
                    &bi,
                    AGV_COLOR,
                    to_render * agv.body_world(),
                );
            }
            if let Some(p) = pick.pick_from_camera_if_clicked(camera) {
                pick.set_highlighted_by_id(&p.feature_id);
                log::info!("[giemon-factory] pick id={}", p.feature_id);
            }
        });

    log::info!("[giemon-factory] backend={:?}", app.backend());
    app.run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ════════════════════════════════════════════════════════════════════════════
//  Entry 2 — 4D construction playback (建築手順)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_family = "wasm")]
thread_local! {
    static CURRENT_STEP: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

/// JS hook: the htm HUD polls this for the current 建築手順 step label.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = giemonFactoryStep)]
pub fn giemon_factory_step() -> String {
    CURRENT_STEP.with(|s| s.borrow().clone())
}

/// JS hook: number of engineering clashes detected (red/orange markers).
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = giemonFactoryClashCount)]
pub fn giemon_factory_clash_count() -> usize {
    scene::Clashes::load().clashes.len()
}

#[cfg(target_family = "wasm")]
const HIDDEN: Mat4 = Mat4::ZERO;
/// Playback speed: construction days per wall-clock second.
#[cfg(target_family = "wasm")]
const DAYS_PER_SEC: f32 = 9.0;

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_giemon_factory_build_v1(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    let f = Factory::load();
    let order = ConstructionOrder::load();
    let c = f.center();
    log::info!(
        "[giemon-factory-build] {} steps, {} nominal days",
        order.steps.len(),
        order.programme_days()
    );

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("giemon-factory-build")
        .with_hud_publish(true)
        .with_camera(CameraMode::Orbit {
            target: Vec3::new(0.0, 4.0, 0.0),
            distance: 165.0,
            yaw: 0.6,
            pitch: 0.7,
        })
        .with_input(InputMode::OrbitMouse);

    let ctx = app.render_context();
    let sky = kami_pipelines::SkyAdapter::new(ctx);
    let cad = kami_pipelines::CadSceneAdapter::new(ctx);
    let (bp, bn, bi) = unit_box();

    let to_render =
        Mat4::from_rotation_x(-HALF_PI) * Mat4::from_translation(Vec3::new(-c.x, -c.y, 0.0));

    // Push every static box HIDDEN; record (id, batch_idx, real_world, color).
    let boxes = static_boxes(&f, to_render);
    let mut elems: Vec<(String, usize, Mat4, [f32; 3])> = Vec::new();
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
        elems.push((sb.id.clone(), idx, sb.world, sb.color));
    }

    // Robot cells + AGVs as static silhouettes (revealed at their step).
    for cell in &f.cells {
        let pos = Vec3::new(cell.pos[0] - c.x, cell.pos[1] - c.y, cell.pos[2]);
        let arm = ArmCell::new(arm6_config(), pos, cell.yaw, 0.0);
        let fk0 = arm.fk();
        for i in 0..arm.nb {
            let idx = cad.batch_count();
            cad.push_triangles(
                ctx,
                format!("{}_l{i}", cell.id),
                cell.id.clone(),
                &bp,
                &bn,
                &bi,
                ARM_COLOR,
                HIDDEN,
            );
            elems.push((
                cell.id.clone(),
                idx,
                cell_body_world(to_render, arm.cell_world, fk0[i], arm.link_local[i]),
                ARM_COLOR,
            ));
        }
    }
    for a in &f.agvs {
        let size = Vec3::new(a.size[0], a.size[1], a.size[2]);
        let idx = cad.batch_count();
        // Bake the cart size into the world transform of a unit box, so all
        // elements share the same unit-box geometry for hide/reveal.
        cad.push_triangles(
            ctx,
            a.id.clone(),
            a.id.clone(),
            &bp,
            &bn,
            &bi,
            AGV_COLOR,
            HIDDEN,
        );
        let world = to_render
            * Mat4::from_translation(Vec3::new(a.pos[0] - c.x, a.pos[1] - c.y, a.pos[2]))
            * Mat4::from_rotation_z(a.yaw)
            * Mat4::from_scale(size);
        elems.push((a.id.clone(), idx, world, AGV_COLOR));
    }
    log::info!("[giemon-factory-build] batches={}", cad.batch_count());

    // Cumulative reveal time (seconds) per step.
    let mut reveal_at: Vec<f32> = Vec::with_capacity(order.steps.len());
    let mut acc = 0.0_f32;
    for s in &order.steps {
        acc += s.duration_d;
        reveal_at.push(acc / DAYS_PER_SEC);
    }
    let total = acc / DAYS_PER_SEC;
    let hold = 4.0_f32;
    let loop_len = total + hold;

    let render = cad.clone();
    let mut clock = 0.0_f32;
    let mut shown = vec![false; order.steps.len()];
    let app = app
        .with_pipeline(sky)
        .with_pipeline(cad)
        .on_update(move |_world, camera, dt| {
            {
                let rc = camera.as_render_mut();
                rc.near = 0.5;
                rc.far = 1400.0;
            }
            clock += dt.max(0.0).min(0.1);
            if clock > loop_len {
                // restart: hide everything.
                clock = 0.0;
                for (_, idx, _, color) in &elems {
                    render.replace_batch_world(*idx, &bp, &bn, &bi, *color, HIDDEN);
                }
                for s in shown.iter_mut() {
                    *s = false;
                }
            }
            for (k, s) in order.steps.iter().enumerate() {
                if !shown[k] && clock >= reveal_at[k] {
                    shown[k] = true;
                    CURRENT_STEP.with(|cur| {
                        *cur.borrow_mut() = format!("{:02}. {}", s.seq, s.name);
                    });
                    for (id, idx, world, color) in &elems {
                        if s.reveals.iter().any(|r| r == id) {
                            render.replace_batch_world(*idx, &bp, &bn, &bi, *color, *world);
                        }
                    }
                    log::info!("[giemon-factory-build] step {:02} {}", s.seq, s.name);
                }
            }
        });

    log::info!("[giemon-factory-build] backend={:?}", app.backend());
    app.run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── native unit tests ─────────────────────────────────────────────────────────

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    #[test]
    fn arm6_is_6dof() {
        let cfg = arm6_config();
        assert_eq!(cfg.ndof, 6, "giemon arm6 has 6 revolute joints");
        assert_eq!(cfg.n_bodies(), 7, "base + 6 links");
    }

    #[test]
    fn arm_cell_settles_finite() {
        let cfg = arm6_config();
        let mut arm = ArmCell::new(cfg, Vec3::ZERO, 0.0, 0.0);
        for s in 0..1200 {
            arm.step(s as f32 * DT);
        }
        assert!(
            arm.state.q.iter().all(|v| v.is_finite()),
            "arm went non-finite"
        );
    }

    #[test]
    fn agv_is_4dof_and_blocked_by_obstacle() {
        let f = Factory::load();
        let wall = Obstacle::Aabb {
            min: Vec3::new(10.0, -20.0, 0.0),
            max: Vec3::new(40.0, 20.0, 8.0),
        };
        let mut agv = Agv::new(Vec3::new(2.0, 1.2, 0.8), 220.0, vec![wall]);
        assert_eq!(agv.cfg.ndof, 4);
        agv.place(Vec3::new(0.0, 0.0, 0.6), 0.0); // facing +x into the wall
        for s in 0..2500 {
            agv.step(s as f32 * DT);
        }
        assert!(
            agv.state.q[0] < 9.6,
            "AGV tunnelled into wall: x={}",
            agv.state.q[0]
        );
        assert!(agv.state.q.iter().all(|v| v.is_finite()));
        // sanity: the loaded factory has AGVs to spawn.
        assert!(!f.agvs.is_empty());
    }

    #[test]
    fn payload_free_falls_and_settles() {
        // a dropped material payload settles physically on the ground (z→rest).
        let mut p = Agv::new(Vec3::new(1.5, 1.5, 1.5), 400.0, vec![]);
        p.place(Vec3::new(0.0, 0.0, 14.0), 0.0);
        for _ in 0..2000 {
            p.step_free();
        }
        let z = p.pos_z();
        assert!(z.is_finite() && z < 1.5, "payload did not fall: z={z}");
        assert!(
            p.state.qdot[2].abs() < 0.3,
            "still falling: {}",
            p.state.qdot[2]
        );
    }

    #[test]
    fn cart_drives_toward_target() {
        // the delivery cart physically approaches a target point.
        let mut cart = Agv::new(Vec3::new(3.0, 2.0, 1.2), 300.0, vec![]);
        cart.place(Vec3::new(-30.0, 0.0, 0.6), 0.0);
        let (tx, ty) = (10.0, 6.0);
        let d0 = ((tx + 30.0_f32).powi(2) + (ty as f32).powi(2)).sqrt();
        for _ in 0..3000 {
            cart.step_toward(tx, ty);
        }
        let d1 = ((tx - cart.state.q[0]).powi(2) + (ty - cart.state.q[1]).powi(2)).sqrt();
        assert!(d1 < d0 * 0.4, "cart did not approach: d0={d0} d1={d1}");
        assert!(cart.state.q.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn static_boxes_cover_every_element() {
        let f = Factory::load();
        let boxes = static_boxes(&f, Mat4::IDENTITY);
        // ground + floor + columns + beams + walls + zones + machines + >=1 conveyor seg
        let expect_min = 2
            + f.columns.len()
            + f.beams.len()
            + f.walls.len()
            + f.zones.len()
            + f.machines.len()
            + 1;
        assert!(
            boxes.len() >= expect_min,
            "boxes={} expect>={}",
            boxes.len(),
            expect_min
        );
        // ground + floor present.
        assert!(boxes.iter().any(|b| b.id == "ground"));
        assert!(boxes.iter().any(|b| b.id == "floor"));
    }
}
