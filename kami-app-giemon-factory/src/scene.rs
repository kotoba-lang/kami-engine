//! Factory scene model — deserialized from the hand-authored
//! `70-tools/e7m-sim/scenes/giemon-factory-r0/factory.scene.json`, plus the
//! generated `construction.order.json` 4D build order.
//!
//! Mirrors the shibuya `Scene` pattern: static building elements (walls,
//! columns, machines) become `Obstacle::Aabb` collision volumes for the
//! kami-genesis contact solver; everything carries a stable `id` so the 4D
//! construction viewer can reveal elements in `:step/seq` order.

use glam::Vec3;
use kami_genesis::Obstacle;
use serde::Deserialize;

pub const SCENE_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon-factory-r0/factory.scene.json");
pub const ORDER_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon-factory-r0/construction.order.json");
pub const CLASHES_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon-factory-r0/clashes.json");
pub const ROBOTS_JSON: &str =
    include_str!("../../../../70-tools/e7m-sim/scenes/giemon-factory-r0/robots.json");

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
/// `z` is the design centreline height; negative = underground. `kind` drives
/// the render colour.
#[derive(Deserialize)]
pub struct Utility {
    pub id: String,
    pub kind: String,
    pub path: Vec<[f32; 2]>,
    pub z: f32,
    pub width: f32,
}

/// A set of point fixtures sharing one id (e.g. 24 high-bay lights).
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

#[derive(Deserialize)]
pub struct Agv {
    pub id: String,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub size: [f32; 3],
    pub mass: f32,
}

impl Factory {
    pub fn load() -> Self {
        serde_json::from_str(SCENE_JSON).expect("giemon factory scene parses")
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            0.5 * (self.bbox_m[0] + self.bbox_m[2]),
            0.5 * (self.bbox_m[1] + self.bbox_m[3]),
            0.0,
        )
    }

    /// The full site footprint to cover with ground: `site_bbox_m` if present,
    /// else the building bbox padded out.
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

    /// Plan-position (x, y) of a render element id — for placing a construction
    /// robot at a step's work zone. Returns `None` for unknown ids.
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
        // polyline / multi-point elements: midpoint of their first/last point
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

    /// Work-zone centre (x, y) of a construction step from its revealed ids.
    pub fn step_center(&self, reveals: &[String]) -> (f32, f32) {
        let mut n = 0.0_f32;
        let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
        for r in reveals {
            if let Some((x, y)) = self.element_xy(r) {
                sx += x;
                sy += y;
                n += 1.0;
            }
        }
        if n > 0.0 {
            (sx / n, sy / n)
        } else {
            (self.center().x, self.center().y)
        }
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
    /// "deposition" | "thermal-weld" | "none" — drives the material-process field.
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
        assert_eq!(f.name, "giemon-factory-r0");
        assert!(f.walls.len() >= 4, "perimeter walls");
        assert_eq!(f.columns.len(), 8);
        assert!(!f.machines.is_empty());
        assert_eq!(f.cells.len(), 4);
        assert_eq!(f.agvs.len(), 2);
    }

    #[test]
    fn mep_and_site_present() {
        let f = Factory::load();
        assert!(f.service_nodes.len() >= 4, "引込/受電/受水/ガス/通信");
        assert!(
            f.utilities.len() >= 8,
            "電気/水/排水/雨水/ガス/圧空/データ/消火"
        );
        assert!(
            f.fixtures
                .iter()
                .any(|x| x.id == "lighting" && x.points.len() >= 12)
        );
        assert!(!f.site_pavements.is_empty(), "外部動線 舗装");
        assert!(f.site_structures.iter().any(|s| s.id == "site_fence"));
        assert!(f.site_posts.iter().any(|p| p.id == "site_poles"));
        assert!(f.site_bbox_m.is_some());
    }

    #[test]
    fn robots_load_and_steps_assigned() {
        let robots = Robots::load();
        assert_eq!(robots.robots.len(), 7);
        assert!(
            robots
                .get("robot:printer")
                .map(|r| r.process == "deposition")
                .unwrap_or(false)
        );
        assert!(
            robots
                .get("robot:bolter")
                .map(|r| r.process == "thermal-weld")
                .unwrap_or(false)
        );
        // every construction step has a known robot assigned
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
        assert_eq!(c.of, "giemon-factory-r0");
        // engineering.py finds service-corridor + penetration clashes
        assert!(!c.clashes.is_empty(), "expected detected clashes");
        assert!(c.clashes.iter().all(|x| x.x.is_finite() && x.z.is_finite()));
        assert!(
            c.clashes
                .iter()
                .any(|x| x.kind == "hard" || x.kind == "coordination")
        );
    }

    #[test]
    fn obstacle_counts() {
        let f = Factory::load();
        assert_eq!(f.wall_obstacles().len(), f.walls.len());
        assert_eq!(f.column_obstacles().len(), f.columns.len());
        assert_eq!(f.machine_obstacles().len(), f.machines.len());
        assert_eq!(
            f.agv_obstacles().len(),
            f.walls.len() + f.columns.len() + f.machines.len()
        );
    }

    #[test]
    fn order_is_contiguous_and_reveals_resolve() {
        let f = Factory::load();
        let order = ConstructionOrder::load();
        // seq is 1..=N contiguous.
        let seqs: Vec<u32> = order.steps.iter().map(|s| s.seq).collect();
        assert_eq!(seqs, (1..=order.steps.len() as u32).collect::<Vec<_>>());

        // every revealed id exists as a scene element (or a synthetic one).
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
