//! JBeam-subset JSON loader.
//!
//! BeamNG vehicles are described in a JSON dialect (JBeam) with this rough
//! shape:
//!
//! ```json
//! {
//!   "name": "sedan",
//!   "nodes": [
//!     ["id", "posX", "posY", "posZ", "mass", "group"],
//!     ["fl_lower", -0.7, 0.30, 1.30, 8.0, "body"],
//!     ...
//!   ],
//!   "beams": [
//!     ["n1", "n2", "spring", "damping", "type"],
//!     ["fl_lower", "fl_upper", 250000, 350, "normal"],
//!     ...
//!   ],
//!   "wheels": [
//!     {"axle": ["fl_axle_l", "fl_axle_r"], "radius": 0.32, "width": 0.22, "tire": "road_dry"}
//!   ]
//! }
//! ```
//!
//! We support a strict subset: node groups (`body|wheel_hub|wheel_tire|cargo|
//! anchor`), beam types (`normal|bounded|hydro|pressured|support`), and tire
//! presets (`road_dry|road_wet`). Names are resolved to numeric IDs at load
//! time so the runtime stays integer-keyed.

use std::collections::HashMap;

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::beam::{Beam, BeamType, DeformParams};
use crate::node::{Node, NodeGroup};
use crate::vehicle::Vehicle;
use crate::wheel::{PacejkaParams, Wheel, WheelContactMode};

#[derive(Debug, Error)]
pub enum JBeamError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown node id `{0}`")]
    UnknownNode(String),
    #[error("unknown group `{0}`")]
    UnknownGroup(String),
    #[error("unknown beam type `{0}`")]
    UnknownBeamType(String),
    #[error("unknown tire preset `{0}`")]
    UnknownTire(String),
    #[error("malformed entry: {0}")]
    Malformed(&'static str),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JBeamFile {
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<JBeamNode>,
    #[serde(default)]
    pub beams: Vec<JBeamBeam>,
    #[serde(default)]
    pub wheels: Vec<JBeamWheel>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JBeamNode {
    pub id: String,
    pub pos: [f32; 3],
    pub mass: f32,
    #[serde(default = "default_group")]
    pub group: String,
    #[serde(default)]
    pub friction: Option<f32>,
    #[serde(default)]
    pub drag: Option<f32>,
}

fn default_group() -> String {
    "body".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JBeamBeam {
    pub n1: String,
    pub n2: String,
    pub spring: f32,
    pub damping: f32,
    #[serde(default = "default_beam_type")]
    pub r#type: String,
    #[serde(default)]
    pub min_ratio: Option<f32>,
    #[serde(default)]
    pub max_ratio: Option<f32>,
    #[serde(default)]
    pub hydro_factor: Option<f32>,
    #[serde(default)]
    pub deform_limit: Option<f32>,
    #[serde(default)]
    pub break_limit: Option<f32>,
    #[serde(default)]
    pub break_group: Option<u32>,
}

fn default_beam_type() -> String {
    "normal".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JBeamWheel {
    pub axle: [String; 2],
    pub radius: f32,
    pub width: f32,
    #[serde(default = "default_tire")]
    pub tire: String,
    #[serde(default)]
    pub pressure: Option<f32>,
    #[serde(default)]
    pub max_steer_deg: Option<f32>,
    /// Optional flexible tire-ring node ids (typically 12 nodes
    /// distributed around the wheel circumference). When present, the
    /// loader populates `Wheel::tire_nodes` so the simulator can
    /// attribute body forces / break events to the wheel as a whole.
    /// Backwards-compatible — JBeam files without this field still
    /// load cleanly with an empty tire ring.
    #[serde(default)]
    pub tire_nodes: Vec<String>,
}

fn default_tire() -> String {
    "road_dry".to_string()
}

pub fn load_str(json: &str) -> Result<Vehicle, JBeamError> {
    let file: JBeamFile = serde_json::from_str(json)?;
    let mut id_map: HashMap<String, u32> = HashMap::with_capacity(file.nodes.len());
    let mut vehicle = Vehicle::new(file.name);

    for (idx, n) in file.nodes.iter().enumerate() {
        let group = parse_group(&n.group)?;
        let nid = idx as u32;
        let mut node = Node::new(nid, Vec3::from(n.pos), n.mass).with_group(group);
        if let Some(f) = n.friction {
            node.friction = f;
        }
        if let Some(d) = n.drag {
            node.drag = d;
        }
        if matches!(group, NodeGroup::Anchor) {
            node = Node::anchor(nid, Vec3::from(n.pos));
        }
        vehicle.add_node(node);
        id_map.insert(n.id.clone(), nid);
    }

    for (idx, b) in file.beams.iter().enumerate() {
        let n1 = *id_map
            .get(&b.n1)
            .ok_or_else(|| JBeamError::UnknownNode(b.n1.clone()))?;
        let n2 = *id_map
            .get(&b.n2)
            .ok_or_else(|| JBeamError::UnknownNode(b.n2.clone()))?;
        let p1 = vehicle.nodes.iter().find(|n| n.id == n1).unwrap().position;
        let p2 = vehicle.nodes.iter().find(|n| n.id == n2).unwrap().position;
        let rest = (p2 - p1).length().max(1e-3);

        let mut beam = Beam::new(idx as u32, n1, n2, rest, b.spring, b.damping);
        beam.beam_type = parse_beam_type(&b.r#type, b.min_ratio, b.max_ratio, b.hydro_factor)?;
        beam.deform = DeformParams {
            deform_limit: b.deform_limit.unwrap_or(0.10),
            break_limit: b.break_limit.unwrap_or(0.45),
            ..DeformParams::default()
        };
        beam.break_group = b.break_group;
        vehicle.add_beam(beam);
    }

    for (i, w) in file.wheels.iter().enumerate() {
        let a1 = *id_map
            .get(&w.axle[0])
            .ok_or_else(|| JBeamError::UnknownNode(w.axle[0].clone()))?;
        let a2 = *id_map
            .get(&w.axle[1])
            .ok_or_else(|| JBeamError::UnknownNode(w.axle[1].clone()))?;
        let mut wh = Wheel::new(i as u32, a1, a2, w.radius, w.width);
        wh.tire = parse_tire(&w.tire)?;
        if let Some(p) = w.pressure {
            wh.pressure = p;
            wh.reference_pressure = p;
        }
        if let Some(deg) = w.max_steer_deg {
            wh.max_steer_angle = deg.to_radians();
        }
        wh.hub_nodes.push(a1);
        wh.hub_nodes.push(a2);
        for tn_id in &w.tire_nodes {
            let tn = *id_map
                .get(tn_id)
                .ok_or_else(|| JBeamError::UnknownNode(tn_id.clone()))?;
            wh.tire_nodes.push(tn);
        }
        // Eight or more populated ring nodes give a meaningful contact
        // patch — flip the contact mode so the simulator routes Pacejka
        // force through the ring (Phase 2.5).
        if wh.tire_nodes.len() >= 8 {
            wh.contact_mode = WheelContactMode::TireRing;
        }
        vehicle.add_wheel(wh);
    }

    Ok(vehicle)
}

fn parse_group(s: &str) -> Result<NodeGroup, JBeamError> {
    match s {
        "body" => Ok(NodeGroup::Body),
        "wheel_hub" | "hub" => Ok(NodeGroup::WheelHub),
        "wheel_tire" | "tire" => Ok(NodeGroup::WheelTire),
        "cargo" => Ok(NodeGroup::Cargo),
        "anchor" => Ok(NodeGroup::Anchor),
        other => Err(JBeamError::UnknownGroup(other.to_string())),
    }
}

fn parse_beam_type(
    s: &str,
    min_ratio: Option<f32>,
    max_ratio: Option<f32>,
    hydro_factor: Option<f32>,
) -> Result<BeamType, JBeamError> {
    match s {
        "normal" => Ok(BeamType::Normal),
        "support" => Ok(BeamType::Support),
        "bounded" => Ok(BeamType::Bounded {
            min_ratio: min_ratio.unwrap_or(0.85),
            max_ratio: max_ratio.unwrap_or(1.15),
        }),
        "hydro" => Ok(BeamType::Hydro {
            factor: hydro_factor.unwrap_or(0.10),
            extension: 0.0,
        }),
        "pressured" => Ok(BeamType::Pressured {
            pressure_factor: 0.05,
            reference_pressure: 2.4,
        }),
        other => Err(JBeamError::UnknownBeamType(other.to_string())),
    }
}

fn parse_tire(s: &str) -> Result<PacejkaParams, JBeamError> {
    match s {
        "road_dry" => Ok(PacejkaParams::road_dry()),
        "road_wet" => Ok(PacejkaParams::road_wet()),
        other => Err(JBeamError::UnknownTire(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    {
      "name": "stub",
      "nodes": [
        {"id": "fl_lower", "pos": [-0.70, 0.30, 1.30], "mass": 8.0, "group": "body"},
        {"id": "fl_upper", "pos": [-0.70, 0.85, 1.30], "mass": 6.0, "group": "body"},
        {"id": "fr_lower", "pos": [ 0.70, 0.30, 1.30], "mass": 8.0, "group": "body"},
        {"id": "fr_upper", "pos": [ 0.70, 0.85, 1.30], "mass": 6.0, "group": "body"}
      ],
      "beams": [
        {"n1": "fl_lower", "n2": "fl_upper", "spring": 250000, "damping": 350},
        {"n1": "fr_lower", "n2": "fr_upper", "spring": 250000, "damping": 350},
        {"n1": "fl_lower", "n2": "fr_lower", "spring": 200000, "damping": 250, "type": "support"}
      ],
      "wheels": [
        {"axle": ["fl_lower", "fl_upper"], "radius": 0.32, "width": 0.22, "tire": "road_dry"}
      ]
    }
    "#;

    #[test]
    fn load_str_parses_sample() {
        let v = load_str(SAMPLE).unwrap();
        assert_eq!(v.nodes.len(), 4);
        assert_eq!(v.beams.len(), 3);
        assert_eq!(v.wheels.len(), 1);
        assert!(matches!(v.beams[2].beam_type, BeamType::Support));
    }

    #[test]
    fn unknown_node_reference_returns_error() {
        let bad = r#"{"name":"x","nodes":[],"beams":[{"n1":"a","n2":"b","spring":1,"damping":1}]}"#;
        let e = load_str(bad);
        assert!(matches!(e, Err(JBeamError::UnknownNode(_))));
    }

    #[test]
    fn rest_length_recovered_from_positions() {
        let v = load_str(SAMPLE).unwrap();
        // fl_lower (y=0.30) -> fl_upper (y=0.85): expected 0.55.
        assert!((v.beams[0].rest_length - 0.55).abs() < 1e-3);
    }
}
