//! Mass-point node — atomic unit of the BeamNG-style soft body.
//!
//! Every vehicle is a cloud of nodes connected by beams. A node carries
//! position, velocity, accumulated force, mass, and a few per-node material
//! coefficients (drag, ground friction). Plastic deformation lives on the
//! beam; the node only integrates Newton's 2nd law.

use glam::Vec3;
use serde::{Deserialize, Serialize};

pub type NodeId = u32;

/// Logical grouping of nodes (drives collision filtering and contact dispatch).
///
/// `Body` = chassis / sheet metal. `WheelHub` / `WheelTire` are the rim and
/// tire ring of a wheel. `Cargo` is a free body bolted to the chassis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeGroup {
    Body,
    WheelHub,
    WheelTire,
    Cargo,
    Anchor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub position: Vec3,
    pub velocity: Vec3,
    #[serde(skip)]
    pub force: Vec3,
    /// Mass in kg. A `mass` of 0.0 means the node is fixed (anchor).
    pub mass: f32,
    /// Cached `1.0 / mass` (set to 0 for anchors).
    #[serde(skip)]
    pub inv_mass: f32,
    /// Quadratic air-drag coefficient (`F_drag = -drag * v * |v|`).
    pub drag: f32,
    /// Coulomb friction coefficient against ground.
    pub friction: f32,
    /// Restitution (bounce) on ground contact in `[0, 1]`.
    pub restitution: f32,
    pub group: NodeGroup,
}

impl Node {
    pub fn new(id: NodeId, position: Vec3, mass: f32) -> Self {
        let inv_mass = if mass > 0.0 { 1.0 / mass } else { 0.0 };
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            force: Vec3::ZERO,
            mass,
            inv_mass,
            drag: 0.4,
            friction: 1.0,
            restitution: 0.05,
            group: NodeGroup::Body,
        }
    }

    pub fn anchor(id: NodeId, position: Vec3) -> Self {
        let mut n = Self::new(id, position, 0.0);
        n.group = NodeGroup::Anchor;
        n
    }

    pub fn with_group(mut self, group: NodeGroup) -> Self {
        self.group = group;
        self
    }

    pub fn with_drag(mut self, drag: f32) -> Self {
        self.drag = drag;
        self
    }

    pub fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction;
        self
    }

    pub fn is_fixed(&self) -> bool {
        self.mass <= 0.0
    }

    /// Recompute `inv_mass` after loading from JBeam JSON (where it is skipped).
    pub fn refresh_inv_mass(&mut self) {
        self.inv_mass = if self.mass > 0.0 {
            1.0 / self.mass
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_has_zero_inv_mass() {
        let n = Node::anchor(0, Vec3::ZERO);
        assert!(n.is_fixed());
        assert_eq!(n.inv_mass, 0.0);
    }

    #[test]
    fn dynamic_node_has_correct_inv_mass() {
        let n = Node::new(1, Vec3::ZERO, 4.0);
        assert!((n.inv_mass - 0.25).abs() < 1e-6);
    }
}
