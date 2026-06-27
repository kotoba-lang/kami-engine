//! Programmatic builder for assembling a `Vehicle`.
//!
//! Useful when you'd rather write the car as Rust code than as JBeam JSON
//! (faster iteration, type-safe coordinates, etc.). The builder hands out
//! sequential `NodeId` and `BeamId` so callers don't have to track them.

use glam::Vec3;

use crate::beam::{Beam, BeamId, BeamType, BreakGroup, DeformParams};
use crate::node::{Node, NodeGroup, NodeId};
use crate::triangle::{Triangle, TriangleGroup, TriangleId};
use crate::vehicle::Vehicle;
use crate::wheel::{PacejkaParams, Wheel, WheelId};

#[derive(Debug)]
pub struct VehicleBuilder {
    pub vehicle: Vehicle,
    next_node: NodeId,
    next_beam: BeamId,
    next_tri: TriangleId,
    next_wheel: WheelId,
}

impl VehicleBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            vehicle: Vehicle::new(name),
            next_node: 0,
            next_beam: 0,
            next_tri: 0,
            next_wheel: 0,
        }
    }

    pub fn node(&mut self, position: Vec3, mass: f32, group: NodeGroup) -> NodeId {
        let id = self.next_node;
        self.next_node += 1;
        self.vehicle
            .add_node(Node::new(id, position, mass).with_group(group));
        id
    }

    pub fn anchor(&mut self, position: Vec3) -> NodeId {
        let id = self.next_node;
        self.next_node += 1;
        self.vehicle.add_node(Node::anchor(id, position));
        id
    }

    pub fn beam(&mut self, n1: NodeId, n2: NodeId, spring: f32, damping: f32) -> BeamId {
        let p1 = self
            .vehicle
            .nodes
            .iter()
            .find(|n| n.id == n1)
            .unwrap()
            .position;
        let p2 = self
            .vehicle
            .nodes
            .iter()
            .find(|n| n.id == n2)
            .unwrap()
            .position;
        let rest = (p2 - p1).length().max(1e-3);
        let id = self.next_beam;
        self.next_beam += 1;
        self.vehicle
            .add_beam(Beam::new(id, n1, n2, rest, spring, damping));
        id
    }

    pub fn beam_typed(
        &mut self,
        n1: NodeId,
        n2: NodeId,
        spring: f32,
        damping: f32,
        ty: BeamType,
        deform: DeformParams,
        break_group: Option<BreakGroup>,
    ) -> BeamId {
        let id = self.beam(n1, n2, spring, damping);
        let b = self.vehicle.beams.last_mut().unwrap();
        b.beam_type = ty;
        b.deform = deform;
        b.break_group = break_group;
        id
    }

    pub fn triangle(
        &mut self,
        n1: NodeId,
        n2: NodeId,
        n3: NodeId,
        group: TriangleGroup,
    ) -> TriangleId {
        let id = self.next_tri;
        self.next_tri += 1;
        self.vehicle
            .add_triangle(Triangle::new(id, n1, n2, n3).with_group(group));
        id
    }

    pub fn wheel(
        &mut self,
        axle_n1: NodeId,
        axle_n2: NodeId,
        radius: f32,
        width: f32,
        tire: PacejkaParams,
    ) -> WheelId {
        let id = self.next_wheel;
        self.next_wheel += 1;
        let mut w = Wheel::new(id, axle_n1, axle_n2, radius, width);
        w.tire = tire;
        // Hub nodes default to the two axle endpoints; ring nodes are filled
        // in by `add_tire_ring` if used.
        w.hub_nodes.push(axle_n1);
        w.hub_nodes.push(axle_n2);
        self.vehicle.add_wheel(w);
        id
    }

    /// Generate `count` tire-ring nodes evenly around `centre` in the plane
    /// perpendicular to `axle_axis`, tied to the hub by side-wall pressured
    /// beams. Returns the IDs of the new ring nodes.
    pub fn add_tire_ring(
        &mut self,
        wheel_id: WheelId,
        centre: Vec3,
        axle_axis: Vec3,
        radius: f32,
        count: u32,
        node_mass: f32,
        sidewall_spring: f32,
        sidewall_damping: f32,
        reference_pressure: f32,
    ) -> Vec<NodeId> {
        let axis = axle_axis.normalize_or_zero();
        // Build two basis vectors orthogonal to the axis.
        let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
        let u = helper.cross(axis).normalize_or_zero();
        let v = axis.cross(u).normalize_or_zero();

        let mut ids = Vec::with_capacity(count as usize);
        let (axle_n1, axle_n2) = {
            let w = self
                .vehicle
                .wheels
                .iter()
                .find(|w| w.id == wheel_id)
                .expect("wheel id");
            (w.axle_n1, w.axle_n2)
        };

        for i in 0..count {
            let angle = (i as f32) / (count as f32) * std::f32::consts::TAU;
            let p = centre + (u * angle.cos() + v * angle.sin()) * radius;
            let id = self.node(p, node_mass, NodeGroup::WheelTire);
            ids.push(id);
            self.beam_typed(
                id,
                axle_n1,
                sidewall_spring,
                sidewall_damping,
                BeamType::Pressured {
                    pressure_factor: 0.05,
                    reference_pressure,
                },
                DeformParams {
                    deform_limit: 0.30,
                    break_limit: 0.85,
                    max_plastic_strain: 0.50,
                },
                None,
            );
            self.beam_typed(
                id,
                axle_n2,
                sidewall_spring,
                sidewall_damping,
                BeamType::Pressured {
                    pressure_factor: 0.05,
                    reference_pressure,
                },
                DeformParams {
                    deform_limit: 0.30,
                    break_limit: 0.85,
                    max_plastic_strain: 0.50,
                },
                None,
            );
        }

        // Tread beams (ring chord neighbours) for tire structural integrity.
        // High break limit because the ring routinely deforms 30-40% under
        // hard cornering / pothole impacts — only a real crash should pop it.
        let tread_deform = DeformParams {
            deform_limit: 0.35,
            break_limit: 0.85,
            max_plastic_strain: 0.50,
        };
        for i in 0..count as usize {
            let a = ids[i];
            let b = ids[(i + 1) % ids.len()];
            self.beam_typed(
                a,
                b,
                sidewall_spring * 1.2,
                sidewall_damping * 0.8,
                BeamType::Normal,
                tread_deform,
                None,
            );
        }

        // Add the ring to the wheel definition.
        if let Some(w) = self.vehicle.wheels.iter_mut().find(|w| w.id == wheel_id) {
            w.tire_nodes.extend(ids.iter().copied());
        }

        ids
    }

    pub fn vehicle_mut(&mut self) -> &mut Vehicle {
        &mut self.vehicle
    }

    pub fn build(self) -> Vehicle {
        self.vehicle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_assigns_sequential_ids() {
        let mut b = VehicleBuilder::new("test");
        let n0 = b.node(Vec3::ZERO, 1.0, NodeGroup::Body);
        let n1 = b.node(Vec3::X, 1.0, NodeGroup::Body);
        let n2 = b.node(Vec3::Y, 1.0, NodeGroup::Body);
        assert_eq!(n0, 0);
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
        b.beam(n0, n1, 1000.0, 10.0);
        b.beam(n1, n2, 1000.0, 10.0);
        let v = b.build();
        assert_eq!(v.nodes.len(), 3);
        assert_eq!(v.beams.len(), 2);
    }

    #[test]
    fn rest_length_is_set_from_geometry() {
        let mut b = VehicleBuilder::new("test");
        let n0 = b.node(Vec3::ZERO, 1.0, NodeGroup::Body);
        let n1 = b.node(Vec3::new(3.0, 4.0, 0.0), 1.0, NodeGroup::Body);
        b.beam(n0, n1, 1.0, 1.0);
        let v = b.build();
        assert!((v.beams[0].rest_length - 5.0).abs() < 1e-3);
    }

    #[test]
    fn add_tire_ring_creates_count_nodes_and_beams() {
        let mut b = VehicleBuilder::new("ring");
        let h1 = b.node(Vec3::new(0.0, 0.0, 0.0), 5.0, NodeGroup::WheelHub);
        let h2 = b.node(Vec3::new(0.2, 0.0, 0.0), 5.0, NodeGroup::WheelHub);
        let w = b.wheel(h1, h2, 0.32, 0.22, PacejkaParams::road_dry());
        let before_beams = b.vehicle.beams.len();
        let ring = b.add_tire_ring(
            w,
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::X,
            0.32,
            12,
            0.30,
            120_000.0,
            450.0,
            2.4,
        );
        assert_eq!(ring.len(), 12);
        // 12 ring nodes, 2 sidewall beams each (24) + 12 tread beams = 36 new beams.
        assert_eq!(b.vehicle.beams.len() - before_beams, 36);
    }
}
