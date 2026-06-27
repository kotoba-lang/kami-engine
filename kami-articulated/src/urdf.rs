//! Minimum URDF parser for Cartpole-class topologies.
//!
//! Supports: prismatic + revolute joints, link mass + inertia + visual box/cylinder,
//! joint axis + limits + dynamics damping/friction. Drops: visual meshes, collision,
//! mimic, transmission, gazebo extensions.

use glam::Vec3;
use roxmltree::{Document, Node};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("xml parse: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("missing required element: {0}")]
    MissingElement(&'static str),
    #[error("missing required attribute `{attr}` on element `{elem}`")]
    MissingAttr {
        elem: &'static str,
        attr: &'static str,
    },
    #[error("invalid number `{0}` in {1}")]
    InvalidNumber(String, &'static str),
    #[error("unsupported joint type `{0}` (expected prismatic | revolute | fixed | continuous)")]
    UnsupportedJointType(String),
    #[error("unknown link `{0}` referenced by joint `{1}`")]
    UnknownLink(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub xyz: Vec3,
    pub rpy: Vec3,
}

impl Default for Pose {
    fn default() -> Self {
        Pose {
            xyz: Vec3::ZERO,
            rpy: Vec3::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inertia {
    pub mass: f32,
    pub ixx: f32,
    pub iyy: f32,
    pub izz: f32,
    pub ixy: f32,
    pub ixz: f32,
    pub iyz: f32,
    pub com: Pose,
}

impl Default for Inertia {
    fn default() -> Self {
        Inertia {
            mass: 0.0,
            ixx: 0.0,
            iyy: 0.0,
            izz: 0.0,
            ixy: 0.0,
            ixz: 0.0,
            iyz: 0.0,
            com: Pose::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub name: String,
    pub inertia: Inertia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointKind {
    Fixed,
    Prismatic,
    Revolute,
    Continuous,
}

#[derive(Debug, Clone)]
pub struct Joint {
    pub name: String,
    pub kind: JointKind,
    pub parent: String,
    pub child: String,
    pub origin: Pose,
    pub axis: Vec3,
    pub lower: f32,
    pub upper: f32,
    pub effort: f32,
    pub velocity: f32,
    pub damping: f32,
    pub friction: f32,
}

#[derive(Debug, Clone)]
pub struct ArticulatedSystem {
    pub name: String,
    pub links: Vec<Link>,
    pub joints: Vec<Joint>,
}

impl ArticulatedSystem {
    pub fn link_index(&self, name: &str) -> Option<usize> {
        self.links.iter().position(|l| l.name == name)
    }

    pub fn joint_index(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }
}

pub fn parse_urdf(xml: &str) -> Result<ArticulatedSystem, ParseError> {
    let doc = Document::parse(xml)?;
    let root = doc.root_element();
    let name = root.attribute("name").unwrap_or("robot").to_string();

    let mut links: Vec<Link> = Vec::new();
    let mut joints: Vec<Joint> = Vec::new();

    for child in root.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "link" => links.push(parse_link(child)?),
            "joint" => joints.push(parse_joint(child)?),
            _ => {}
        }
    }

    // validate joint parent/child references
    let known: Vec<&str> = links.iter().map(|l| l.name.as_str()).collect();
    for j in &joints {
        if j.parent != "world" && !known.contains(&j.parent.as_str()) {
            return Err(ParseError::UnknownLink(j.parent.clone(), j.name.clone()));
        }
        if !known.contains(&j.child.as_str()) {
            return Err(ParseError::UnknownLink(j.child.clone(), j.name.clone()));
        }
    }

    Ok(ArticulatedSystem {
        name,
        links,
        joints,
    })
}

fn parse_link(node: Node) -> Result<Link, ParseError> {
    let name = node
        .attribute("name")
        .ok_or(ParseError::MissingAttr {
            elem: "link",
            attr: "name",
        })?
        .to_string();
    let mut inertia = Inertia::default();

    if let Some(in_node) = node.children().find(|n| n.has_tag_name("inertial")) {
        if let Some(origin) = in_node.children().find(|n| n.has_tag_name("origin")) {
            inertia.com = parse_pose(origin)?;
        }
        if let Some(mass) = in_node.children().find(|n| n.has_tag_name("mass")) {
            inertia.mass = parse_attr_f32(mass, "value", "mass")?;
        }
        if let Some(i) = in_node.children().find(|n| n.has_tag_name("inertia")) {
            inertia.ixx = parse_attr_f32(i, "ixx", "inertia")?;
            inertia.iyy = parse_attr_f32(i, "iyy", "inertia")?;
            inertia.izz = parse_attr_f32(i, "izz", "inertia")?;
            inertia.ixy = parse_attr_f32(i, "ixy", "inertia").unwrap_or(0.0);
            inertia.ixz = parse_attr_f32(i, "ixz", "inertia").unwrap_or(0.0);
            inertia.iyz = parse_attr_f32(i, "iyz", "inertia").unwrap_or(0.0);
        }
    }

    Ok(Link { name, inertia })
}

fn parse_joint(node: Node) -> Result<Joint, ParseError> {
    let name = node
        .attribute("name")
        .ok_or(ParseError::MissingAttr {
            elem: "joint",
            attr: "name",
        })?
        .to_string();
    let kind_str = node.attribute("type").ok_or(ParseError::MissingAttr {
        elem: "joint",
        attr: "type",
    })?;
    let kind = match kind_str {
        "fixed" => JointKind::Fixed,
        "prismatic" => JointKind::Prismatic,
        "revolute" => JointKind::Revolute,
        "continuous" => JointKind::Continuous,
        other => return Err(ParseError::UnsupportedJointType(other.to_string())),
    };

    let parent = node
        .children()
        .find(|n| n.has_tag_name("parent"))
        .and_then(|n| n.attribute("link"))
        .ok_or(ParseError::MissingElement("joint/parent"))?
        .to_string();
    let child = node
        .children()
        .find(|n| n.has_tag_name("child"))
        .and_then(|n| n.attribute("link"))
        .ok_or(ParseError::MissingElement("joint/child"))?
        .to_string();

    let origin = node
        .children()
        .find(|n| n.has_tag_name("origin"))
        .map(parse_pose)
        .transpose()?
        .unwrap_or_default();

    let axis = node
        .children()
        .find(|n| n.has_tag_name("axis"))
        .and_then(|n| n.attribute("xyz"))
        .map(parse_vec3)
        .transpose()?
        .unwrap_or(Vec3::X);

    let (lower, upper, effort, velocity) = node
        .children()
        .find(|n| n.has_tag_name("limit"))
        .map(|n| {
            (
                parse_attr_f32(n, "lower", "limit").unwrap_or(f32::NEG_INFINITY),
                parse_attr_f32(n, "upper", "limit").unwrap_or(f32::INFINITY),
                parse_attr_f32(n, "effort", "limit").unwrap_or(0.0),
                parse_attr_f32(n, "velocity", "limit").unwrap_or(0.0),
            )
        })
        .unwrap_or((f32::NEG_INFINITY, f32::INFINITY, 0.0, 0.0));

    let (damping, friction) = node
        .children()
        .find(|n| n.has_tag_name("dynamics"))
        .map(|n| {
            (
                parse_attr_f32(n, "damping", "dynamics").unwrap_or(0.0),
                parse_attr_f32(n, "friction", "dynamics").unwrap_or(0.0),
            )
        })
        .unwrap_or((0.0, 0.0));

    Ok(Joint {
        name,
        kind,
        parent,
        child,
        origin,
        axis,
        lower,
        upper,
        effort,
        velocity,
        damping,
        friction,
    })
}

fn parse_pose(node: Node) -> Result<Pose, ParseError> {
    let xyz = node
        .attribute("xyz")
        .map(parse_vec3)
        .transpose()?
        .unwrap_or(Vec3::ZERO);
    let rpy = node
        .attribute("rpy")
        .map(parse_vec3)
        .transpose()?
        .unwrap_or(Vec3::ZERO);
    Ok(Pose { xyz, rpy })
}

fn parse_vec3(s: &str) -> Result<Vec3, ParseError> {
    let parts: Vec<f32> = s
        .split_ascii_whitespace()
        .map(|t| t.parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ParseError::InvalidNumber(s.to_string(), "vec3"))?;
    if parts.len() != 3 {
        return Err(ParseError::InvalidNumber(s.to_string(), "vec3"));
    }
    Ok(Vec3::new(parts[0], parts[1], parts[2]))
}

fn parse_attr_f32(node: Node, attr: &str, ctx: &'static str) -> Result<f32, ParseError> {
    let raw = node.attribute(attr).ok_or(ParseError::MissingAttr {
        elem: ctx,
        attr: leak(attr),
    })?;
    raw.parse::<f32>()
        .map_err(|_| ParseError::InvalidNumber(raw.to_string(), ctx))
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");

    #[test]
    fn parses_cartpole_urdf() {
        let sys = parse_urdf(CARTPOLE_URDF).expect("cartpole.urdf must parse");
        assert_eq!(sys.name, "cartpole");
        assert_eq!(sys.links.len(), 3, "world + cart + pole_link");
        assert_eq!(sys.joints.len(), 2, "slider_to_cart + cart_to_pole");
    }

    #[test]
    fn cartpole_topology_correct() {
        let sys = parse_urdf(CARTPOLE_URDF).unwrap();
        let slider = sys
            .joints
            .iter()
            .find(|j| j.name == "slider_to_cart")
            .unwrap();
        let revolute = sys
            .joints
            .iter()
            .find(|j| j.name == "cart_to_pole")
            .unwrap();

        assert_eq!(slider.kind, JointKind::Prismatic);
        assert_eq!(slider.parent, "world");
        assert_eq!(slider.child, "cart");
        assert_eq!(slider.axis, Vec3::X);
        assert!((slider.lower + 2.4).abs() < 1e-6);
        assert!((slider.upper - 2.4).abs() < 1e-6);

        assert_eq!(revolute.kind, JointKind::Revolute);
        assert_eq!(revolute.parent, "cart");
        assert_eq!(revolute.child, "pole_link");
        assert_eq!(revolute.axis, Vec3::Y);
    }

    #[test]
    fn cartpole_masses_match_isaaclab_baseline() {
        let sys = parse_urdf(CARTPOLE_URDF).unwrap();
        let cart = sys.links.iter().find(|l| l.name == "cart").unwrap();
        let pole = sys.links.iter().find(|l| l.name == "pole_link").unwrap();
        assert!((cart.inertia.mass - 1.0).abs() < 1e-6);
        assert!((pole.inertia.mass - 0.1).abs() < 1e-6);
    }

    #[test]
    fn rejects_unknown_joint_type() {
        let xml = r#"<robot name="bad">
          <link name="a"/><link name="b"/>
          <joint name="j" type="ball">
            <parent link="a"/><child link="b"/>
          </joint>
        </robot>"#;
        let err = parse_urdf(xml).unwrap_err();
        assert!(matches!(err, ParseError::UnsupportedJointType(_)));
    }
}
