//! Factory scene model — deserialized from the hand-authored
//! `70-tools/e7m-sim/scenes/sarutahiko-factory-r0/factory.scene.json`, plus the
//! generated `construction.order.json` 4D build order, `clashes.json` and
//! `robots.json`.
//!
//! Same schema as the giemon-factory scene (ADR-2606010030) so the static-box +
//! 4D-reveal machinery is shared; the truck plant adds one new array, `loaders`
//! (the 積込ロボット / straddle loaders that load finished trucks onto carriers).
//! Static building elements (walls, columns, machines) become `Obstacle::Aabb`
//! collision volumes for the kami-genesis contact solver; everything carries a
//! stable `id` so the 4D construction viewer can reveal elements in `:step/seq`
//! order.

use glam::Vec3;
use kami_genesis::Obstacle;
use serde::Deserialize;

pub const SCENE_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/sarutahiko-factory-r0/factory.scene.json");
pub const ORDER_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/sarutahiko-factory-r0/construction.order.json");
pub const CLASHES_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/sarutahiko-factory-r0/clashes.json");
pub const ROBOTS_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/sarutahiko-factory-r0/robots.json");
pub const PRODUCTION_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/sarutahiko-factory-r0/production.order.json");

// ── runtime scene ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Factory {
    pub name: String,
    pub bbox_m: [f32; 4],
    #[serde(default)]
    pub site_bbox_m: Option<[f32; 4]>,
    pub walls: Vec<Wall>,
    pub columns: Vec<Column>,
    pub beams: Vec<Beam>,
    pub zones: Vec<Zone>,
    pub machines: Vec<Machine>,
    pub conveyors: Vec<Conveyor>,
    pub cells: Vec<Cell>,
    pub agvs: Vec<Agv>,
    /// 積込ロボット (finished-truck straddle loaders) — the truck-plant addition.
    #[serde(default)]
    pub loaders: Vec<Loader>,
    // MEP + 外構 (default [] so older scenes still parse)
    #[serde(default)]
    pub service_nodes: Vec<NodeBox>,
    #[serde(default)]
    pub utilities: Vec<Utility>,
    #[serde(default)]
    pub fixtures: Vec<Fixture>,
    #[serde(default)]
    pub site_pavements: Vec<Pavement>,
    #[serde(default)]
    pub site_greens: Vec<Pavement>,
    #[serde(default)]
    pub site_structures: Vec<NodeBox>,
    #[serde(default)]
    pub site_posts: Vec<SitePost>,
}

/// A labelled equipment / structure box (service node, fence segment, gate).
#[derive(Deserialize)]
pub struct NodeBox {
    pub id: String,
    pub kind: String,
    pub aabb: [f32; 4],
    pub height: f32,
}

/// A routed utility network (electrical busway, water/gas/drain pipe, etc.).
#[derive(Deserialize)]
pub struct Utility {
    pub id: String,
    pub kind: String,
    pub path: Vec<[f32; 2]>,
    pub z: f32,
    pub width: f32,
}

/// A set of point fixtures sharing one id (e.g. 20 high-bay lights).
#[derive(Deserialize)]
pub struct Fixture {
    pub id: String,
    pub kind: String,
    pub size: f32,
    pub points: Vec<[f32; 3]>,
}

/// A flat site rectangle (asphalt pavement, walkway, green area).
#[derive(Deserialize)]
pub struct Pavement {
    pub id: String,
    pub kind: String,
    pub rect: [f32; 4],
}

/// A vertical site post (light pole, bollard, sign).
#[derive(Deserialize)]
pub struct SitePost {
    pub id: String,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub height: f32,
}

#[derive(Deserialize)]
pub struct Wall {
    pub id: String,
    /// `[min_x, min_y, max_x, max_y]` footprint in metres.
    pub aabb: [f32; 4],
    pub height: f32,
}

#[derive(Deserialize)]
pub struct Column {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub height: f32,
}

#[derive(Deserialize)]
pub struct Beam {
    pub id: String,
    pub x: f32,
    pub span_y: [f32; 2],
    pub section: f32,
    pub z: f32,
}

#[derive(Deserialize)]
pub struct Zone {
    pub id: String,
    pub label: String,
    /// `[min_x, min_y, max_x, max_y]` floor rectangle.
    pub rect: [f32; 4],
    pub tint: [f32; 3],
}

#[derive(Deserialize)]
pub struct Machine {
    pub id: String,
    pub kind: String,
    pub aabb: [f32; 4],
    pub height: f32,
}

#[derive(Deserialize)]
pub struct Conveyor {
    pub id: String,
    pub path: Vec<[f32; 2]>,
    pub width: f32,
}

#[derive(Deserialize)]
pub struct Cell {
    pub id: String,
    pub urdf: String,
    pub pos: [f32; 3],
    pub yaw: f32,
}

#[derive(Clone, Deserialize)]
pub struct Agv {
    pub id: String,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub size: [f32; 3],
    pub mass: f32,
}

/// 積込ロボット — a self-driving straddle loader that picks a finished truck off
/// its EOL staging spot (`pick`), carries it across the shipping yard, and lowers
/// it onto a carrier trailer deck (`drop`, at `deck_z`). The loaded truck is the
/// `payload`; `vehicle` names the staged truck render element it removes.
#[derive(Clone, Deserialize)]
pub struct Loader {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub size: [f32; 3],
    pub mass: f32,
    pub payload: [f32; 3],
    pub payload_mass: f32,
    pub pick: [f32; 2],
    pub drop: [f32; 2],
    pub deck_z: f32,
    #[serde(default)]
    pub vehicle: String,
}

impl Factory {
    pub fn load() -> Self {
        serde_json::from_str(SCENE_JSON).expect("sarutahiko factory scene parses")
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            0.5 * (self.bbox_m[0] + self.bbox_m[2]),
            0.5 * (self.bbox_m[1] + self.bbox_m[3]),
            0.0,
        )
    }

    /// The full site footprint to cover with ground.
    pub fn site_extent(&self) -> [f32; 4] {
        self.site_bbox_m.unwrap_or([
            self.bbox_m[0] - 12.0,
            self.bbox_m[1] - 12.0,
            self.bbox_m[2] + 12.0,
            self.bbox_m[3] + 12.0,
        ])
    }

    /// Perimeter/partition walls → AABB collision volumes (z = 0 .. height).
    pub fn wall_obstacles(&self) -> Vec<Obstacle> {
        self.walls
            .iter()
            .map(|w| Obstacle::Aabb {
                min: Vec3::new(w.aabb[0], w.aabb[1], 0.0),
                max: Vec3::new(w.aabb[2], w.aabb[3], w.height),
            })
            .collect()
    }

    /// Structural columns → AABB collision volumes (square section, z = 0..h).
    pub fn column_obstacles(&self) -> Vec<Obstacle> {
        self.columns
            .iter()
            .map(|c| Obstacle::Aabb {
                min: Vec3::new(c.x - c.w * 0.5, c.y - c.w * 0.5, 0.0),
                max: Vec3::new(c.x + c.w * 0.5, c.y + c.w * 0.5, c.height),
            })
            .collect()
    }

    /// Production machines → AABB collision volumes (footprint × height).
    pub fn machine_obstacles(&self) -> Vec<Obstacle> {
        self.machines
            .iter()
            .map(|m| Obstacle::Aabb {
                min: Vec3::new(m.aabb[0], m.aabb[1], 0.0),
                max: Vec3::new(m.aabb[2], m.aabb[3], m.height),
            })
            .collect()
    }

    /// The deck of a named carrier machine as an obstacle (so a lowered truck
    /// payload settles physically onto it at `deck_z`). Returns `None` for
    /// non-carrier ids.
    pub fn carrier_deck(&self, id: &str, deck_z: f32) -> Option<Obstacle> {
        self.machines.iter().find(|m| m.id == id).map(|m| Obstacle::Aabb {
            min: Vec3::new(m.aabb[0], m.aabb[1], 0.0),
            max: Vec3::new(m.aabb[2], m.aabb[3], deck_z),
        })
    }

    /// Plan-position (x, y) of a render element id. Returns `None` for unknown ids.
    pub fn element_xy(&self, id: &str) -> Option<(f32, f32)> {
        if id == "floor" || id == "ground" {
            return Some((self.center().x, self.center().y));
        }
        for c in &self.columns {
            if c.id == id {
                return Some((c.x, c.y));
            }
        }
        for b in &self.beams {
            if b.id == id {
                return Some((b.x, 0.5 * (b.span_y[0] + b.span_y[1])));
            }
        }
        let aabb_c = |a: &[f32; 4]| (0.5 * (a[0] + a[2]), 0.5 * (a[1] + a[3]));
        for w in &self.walls {
            if w.id == id {
                return Some(aabb_c(&w.aabb));
            }
        }
        for z in &self.zones {
            if z.id == id {
                return Some(aabb_c(&z.rect));
            }
        }
        for m in &self.machines {
            if m.id == id {
                return Some(aabb_c(&m.aabb));
            }
        }
        for n in self.service_nodes.iter().chain(&self.site_structures) {
            if n.id == id {
                return Some(aabb_c(&n.aabb));
            }
        }
        for p in self.site_pavements.iter().chain(&self.site_greens) {
            if p.id == id {
                return Some(aabb_c(&p.rect));
            }
        }
        for p in &self.site_posts {
            if p.id == id {
                return Some((p.x, p.y));
            }
        }
        for c in &self.cells {
            if c.id == id {
                return Some((c.pos[0], c.pos[1]));
            }
        }
        for a in &self.agvs {
            if a.id == id {
                return Some((a.pos[0], a.pos[1]));
            }
        }
        for l in &self.loaders {
            if l.id == id {
                return Some((l.pos[0], l.pos[1]));
            }
        }
        for u in &self.utilities {
            if u.id == id && !u.path.is_empty() {
                let a = u.path[0];
                let b = u.path[u.path.len() - 1];
                return Some((0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])));
            }
        }
        for cv in &self.conveyors {
            if cv.id == id && !cv.path.is_empty() {
                let a = cv.path[0];
                let b = cv.path[cv.path.len() - 1];
                return Some((0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])));
            }
        }
        for fx in &self.fixtures {
            if fx.id == id && !fx.points.is_empty() {
                let n = fx.points.len() as f32;
                let sx: f32 = fx.points.iter().map(|p| p[0]).sum();
                let sy: f32 = fx.points.iter().map(|p| p[1]).sum();
                return Some((sx / n, sy / n));
            }
        }
        None
    }

    /// Everything an AGV can hit: walls + columns + machines.
    pub fn agv_obstacles(&self) -> Vec<Obstacle> {
        let mut o = self.wall_obstacles();
        o.extend(self.column_obstacles());
        o.extend(self.machine_obstacles());
        o
    }
}

// ── 4D construction order (generated from construction.edn) ───────────────────

#[derive(Deserialize)]
pub struct ConstructionOrder {
    pub of: String,
    pub steps: Vec<OrderStep>,
}

#[derive(Deserialize)]
pub struct OrderStep {
    pub seq: u32,
    pub id: String,
    pub name: String,
    pub trade: String,
    #[serde(default)]
    pub robot: String,
    pub zone: String,
    pub duration_d: f32,
    /// Render-element ids that become visible when this step completes.
    pub reveals: Vec<String>,
}

impl ConstructionOrder {
    pub fn load() -> Self {
        serde_json::from_str(ORDER_JSON).expect("construction order parses")
    }

    /// Total nominal programme length (sum of step durations), in days.
    pub fn programme_days(&self) -> f32 {
        self.steps.iter().map(|s| s.duration_d).sum()
    }
}

// ── production line (generated from production.edn) ───────────────────────────

#[derive(Deserialize)]
pub struct ProdOrder {
    pub of: String,
    #[serde(default)]
    pub takt_s: serde_json::Value,
    pub stations: Vec<ProdStation>,
}

#[derive(Clone, Deserialize)]
pub struct ProdStation {
    pub seq: u32,
    pub id: String,
    pub name: String,
    pub layer: String,
    /// receive | frame-weld | cab-weld | paint | marriage | eol-test | stage | ship
    pub op: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub cell: String,
    pub cycle_s: f32,
}

impl ProdOrder {
    pub fn load() -> Self {
        serde_json::from_str(PRODUCTION_JSON).expect("production order parses")
    }
}

// ── engineering clashes (generated by engineering.py) ─────────────────────────

#[derive(Deserialize)]
pub struct Clashes {
    pub of: String,
    pub clashes: Vec<Clash>,
}

#[derive(Deserialize)]
pub struct Clash {
    pub id: String,
    /// "hard" (utility ∩ structure) or "coordination" (services < clearance).
    pub kind: String,
    pub systems: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Clashes {
    pub fn load() -> Self {
        serde_json::from_str(CLASHES_JSON).expect("clashes parses")
    }
}

// ── construction robots (generated from robots.edn) ───────────────────────────

#[derive(Deserialize)]
pub struct Robots {
    pub robots: Vec<Robot>,
}

#[derive(Clone, Deserialize)]
pub struct Robot {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub reach_m: f32,
    pub base: [f32; 2],
    pub cycle_min: f32,
    pub mobile: bool,
    pub process: String,
    pub maturity: String,
}

impl Robots {
    pub fn load() -> Self {
        serde_json::from_str(ROBOTS_JSON).expect("robots parses")
    }

    pub fn get(&self, id: &str) -> Option<&Robot> {
        self.robots.iter().find(|r| r.id == id)
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn scene_loads() {
        let f = Factory::load();
        assert_eq!(f.name, "sarutahiko-factory-r0");
        assert!(f.walls.len() >= 4, "perimeter walls");
        assert_eq!(f.columns.len(), 12);
        assert!(!f.machines.is_empty());
        assert_eq!(f.cells.len(), 8);
        assert_eq!(f.agvs.len(), 2);
        assert_eq!(f.loaders.len(), 2, "積込ロボット present");
    }

    #[test]
    fn loaders_reference_real_carriers_and_vehicles() {
        let f = Factory::load();
        let machine_ids: HashSet<&str> = f.machines.iter().map(|m| m.id.as_str()).collect();
        for l in &f.loaders {
            assert!(
                machine_ids.contains(l.vehicle.as_str()),
                "loader {} names unknown staged vehicle {}",
                l.id,
                l.vehicle
            );
            // a carrier machine must exist near the drop point
            let near = f.machines.iter().any(|m| {
                m.kind == "carrier"
                    && l.drop[0] >= m.aabb[0] - 4.0
                    && l.drop[0] <= m.aabb[2] + 4.0
            });
            assert!(near, "loader {} drop has no carrier nearby", l.id);
        }
    }

    #[test]
    fn mep_and_site_present() {
        let f = Factory::load();
        assert!(f.service_nodes.len() >= 4, "引込/受電/受水/ガス/通信");
        assert!(f.utilities.len() >= 8, "電気/水/排水/雨水/ガス/圧空/データ/消火");
        assert!(f
            .fixtures
            .iter()
            .any(|x| x.id == "lighting" && x.points.len() >= 12));
        assert!(!f.site_pavements.is_empty(), "外部動線 舗装");
        assert!(f.site_pavements.iter().any(|p| p.id == "site_ship_yard"));
        assert!(f.site_structures.iter().any(|s| s.id == "site_fence"));
        assert!(f.site_posts.iter().any(|p| p.id == "site_poles"));
        assert!(f.site_bbox_m.is_some());
    }

    #[test]
    fn robots_load_and_steps_assigned() {
        let robots = Robots::load();
        assert_eq!(robots.robots.len(), 8);
        assert!(robots
            .get("robot:crane")
            .map(|r| r.reach_m >= 20.0)
            .unwrap_or(false));
        let order = ConstructionOrder::load();
        for s in &order.steps {
            assert!(!s.robot.is_empty(), "step {} has no robot", s.id);
            assert!(
                robots.get(&s.robot).is_some(),
                "step {} unknown robot {}",
                s.id,
                s.robot
            );
        }
    }

    #[test]
    fn clashes_load() {
        let c = Clashes::load();
        assert_eq!(c.of, "sarutahiko-factory-r0");
        assert!(c.clashes.iter().all(|x| x.x.is_finite() && x.z.is_finite()));
    }

    #[test]
    fn production_order_loads_and_resolves() {
        let f = Factory::load();
        let p = ProdOrder::load();
        assert_eq!(p.of, "sarutahiko-factory-r0");
        let seqs: Vec<u32> = p.stations.iter().map(|s| s.seq).collect();
        assert_eq!(seqs, (1..=p.stations.len() as u32).collect::<Vec<_>>());
        // 5-layer process end-to-end: first receives, last ships.
        assert_eq!(p.stations.first().unwrap().op, "receive");
        assert_eq!(p.stations.last().unwrap().op, "ship");
        assert!(p.stations.iter().any(|s| s.op == "paint"));
        assert!(p.stations.iter().any(|s| s.op == "marriage"));
        // every named production robot is a real scene cell or loader.
        let cells: HashSet<&str> = f
            .cells
            .iter()
            .map(|c| c.id.as_str())
            .chain(f.loaders.iter().map(|l| l.id.as_str()))
            .collect();
        for s in &p.stations {
            if !s.cell.is_empty() {
                assert!(cells.contains(s.cell.as_str()), "station {} unknown cell {}", s.id, s.cell);
            }
        }
    }

    #[test]
    fn obstacle_counts() {
        let f = Factory::load();
        assert_eq!(f.wall_obstacles().len(), f.walls.len());
        assert_eq!(f.column_obstacles().len(), f.columns.len());
        assert_eq!(f.machine_obstacles().len(), f.machines.len());
        assert!(f.carrier_deck("carrier_1", 1.4).is_some());
    }

    #[test]
    fn order_is_contiguous_and_reveals_resolve() {
        let f = Factory::load();
        let order = ConstructionOrder::load();
        let seqs: Vec<u32> = order.steps.iter().map(|s| s.seq).collect();
        assert_eq!(seqs, (1..=order.steps.len() as u32).collect::<Vec<_>>());

        let mut ids: HashSet<String> = ["ground", "floor"].iter().map(|s| s.to_string()).collect();
        for w in &f.walls {
            ids.insert(w.id.clone());
        }
        for c in &f.columns {
            ids.insert(c.id.clone());
        }
        for b in &f.beams {
            ids.insert(b.id.clone());
        }
        for z in &f.zones {
            ids.insert(z.id.clone());
        }
        for m in &f.machines {
            ids.insert(m.id.clone());
        }
        for c in &f.conveyors {
            ids.insert(c.id.clone());
        }
        for c in &f.cells {
            ids.insert(c.id.clone());
        }
        for a in &f.agvs {
            ids.insert(a.id.clone());
        }
        for l in &f.loaders {
            ids.insert(l.id.clone());
        }
        for n in &f.service_nodes {
            ids.insert(n.id.clone());
        }
        for u in &f.utilities {
            ids.insert(u.id.clone());
        }
        for x in &f.fixtures {
            ids.insert(x.id.clone());
        }
        for p in f.site_pavements.iter().chain(&f.site_greens) {
            ids.insert(p.id.clone());
        }
        for s in &f.site_structures {
            ids.insert(s.id.clone());
        }
        for p in &f.site_posts {
            ids.insert(p.id.clone());
        }
        for s in &order.steps {
            for r in &s.reveals {
                assert!(ids.contains(r), "step {} reveals unknown id {r}", s.id);
            }
        }
        assert!(order.programme_days() > 0.0);
    }
}
