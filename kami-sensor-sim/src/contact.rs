//! ContactSensor — sphere-approximated link vs scene-primitive contact.
//!
//! Mirrors `isaacsim.sensors.ContactSensor` (Isaac Sim 4.x) at the public API
//! level. Each articulation link is approximated as a sphere (center + radius);
//! at sample time the sensor walks the scene's primitives (from `lidar.rs`:
//! GroundPlane / Sphere / AABB) and reports:
//!
//!   - `in_contact: bool`      — sphere overlaps any primitive
//!   - `penetration_depth: f32`— how far the sphere penetrates (0 if not in contact)
//!   - `contact_normal: Vec3`  — world-frame unit vector from primitive surface
//!     toward the sphere center; arbitrary when not in contact
//!   - `closest_distance: f32` — signed distance from sphere surface to nearest
//!     primitive surface (negative when penetrating)
//!   - `closest_primitive: usize` — index of nearest primitive (`usize::MAX` if
//!     the scene has none)
//!
//! Distance metric for sphere-vs-primitive: closest point on primitive surface
//! to sphere center, minus sphere radius.

use crate::lidar::{Primitive, Scene};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContactReading {
    pub in_contact: bool,
    pub penetration_depth: f32,
    pub contact_normal: Vec3,
    pub closest_distance: f32,
    pub closest_primitive: usize,
    pub time: f32,
}

impl Default for ContactReading {
    fn default() -> Self {
        ContactReading {
            in_contact: false,
            penetration_depth: 0.0,
            contact_normal: Vec3::Z,
            closest_distance: f32::INFINITY,
            closest_primitive: usize::MAX,
            time: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContactSensor {
    pub name: String,
    pub prim_path: String,
    pub link_name: String,
    /// Sphere radius approximating the link's collision geometry.
    pub radius: f32,
}

impl ContactSensor {
    pub fn new(
        name: impl Into<String>,
        prim_path: impl Into<String>,
        link_name: impl Into<String>,
        radius: f32,
    ) -> Self {
        ContactSensor {
            name: name.into(),
            prim_path: prim_path.into(),
            link_name: link_name.into(),
            radius,
        }
    }

    /// Sample the sensor: `link_position` is the world-frame position of the
    /// link's collision sphere center.
    pub fn sample(&self, link_position: Vec3, scene: &Scene, time: f32) -> ContactReading {
        let mut closest_d = f32::INFINITY;
        let mut closest_idx = usize::MAX;
        let mut closest_normal = Vec3::Z;
        for (i, p) in scene.primitives.iter().enumerate() {
            let (d, n) = primitive_closest(*p, link_position);
            // signed: positive = outside primitive; negative = inside
            // sphere surface distance = d − radius
            let sphere_surface_d = d - self.radius;
            if sphere_surface_d < closest_d {
                closest_d = sphere_surface_d;
                closest_idx = i;
                closest_normal = n;
            }
        }
        let in_contact = closest_d < 0.0;
        let penetration_depth = if in_contact { -closest_d } else { 0.0 };
        ContactReading {
            in_contact,
            penetration_depth,
            contact_normal: closest_normal,
            closest_distance: closest_d,
            closest_primitive: closest_idx,
            time,
        }
    }
}

/// Closest-point distance from `p_world` to the surface of `prim`, with the
/// world-frame outward normal of the surface at that closest point. Distance
/// is signed: negative when `p_world` is inside the primitive.
pub fn primitive_closest(prim: Primitive, p_world: Vec3) -> (f32, Vec3) {
    match prim {
        Primitive::GroundPlane { height } => {
            // Plane normal = +z. Signed distance = p.z - height.
            let d = p_world.z - height;
            (d, Vec3::Z)
        }
        Primitive::Sphere { center, radius } => {
            let delta = p_world - center;
            let d_centers = delta.length();
            if d_centers < 1e-12 {
                // p_world at center: normal arbitrary, distance = -radius (deepest)
                (-radius, Vec3::Z)
            } else {
                let normal = delta / d_centers; // from sphere center outward
                (d_centers - radius, normal)
            }
        }
        Primitive::Aabb { min, max } => {
            // Closest point on AABB surface to p_world.
            let clamped = p_world.clamp(min, max);
            let inside = p_world == clamped;
            if !inside {
                let delta = p_world - clamped;
                let d = delta.length();
                if d < 1e-12 {
                    return (0.0, Vec3::Z);
                }
                (d, delta / d)
            } else {
                // p_world is inside the AABB: distance is negative (depth to nearest face).
                let dx_min = p_world.x - min.x;
                let dx_max = max.x - p_world.x;
                let dy_min = p_world.y - min.y;
                let dy_max = max.y - p_world.y;
                let dz_min = p_world.z - min.z;
                let dz_max = max.z - p_world.z;
                // Pick the smallest distance to a face (positive numbers).
                let depths: [(f32, Vec3); 6] = [
                    (dx_min, Vec3::NEG_X),
                    (dx_max, Vec3::X),
                    (dy_min, Vec3::NEG_Y),
                    (dy_max, Vec3::Y),
                    (dz_min, Vec3::NEG_Z),
                    (dz_max, Vec3::Z),
                ];
                let mut min_depth = f32::INFINITY;
                let mut normal = Vec3::Z;
                for (d, n) in depths {
                    if d < min_depth {
                        min_depth = d;
                        normal = n;
                    }
                }
                (-min_depth, normal)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor_radius_1() -> ContactSensor {
        ContactSensor::new("c", "/W/cart/contact", "cart", 1.0)
    }

    // ── ground plane ────────────────────────────────────────────────

    #[test]
    fn ground_plane_no_contact_above() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::GroundPlane { height: 0.0 });
        let r = sensor.sample(Vec3::new(0.0, 0.0, 5.0), &scene, 0.0);
        assert!(!r.in_contact);
        assert!((r.closest_distance - 4.0).abs() < 1e-5); // 5 - 1 = 4
        assert_eq!(r.contact_normal, Vec3::Z);
    }

    #[test]
    fn ground_plane_just_touching() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::GroundPlane { height: 0.0 });
        let r = sensor.sample(Vec3::new(0.0, 0.0, 1.0), &scene, 0.0);
        // sphere bottom at z=0, exact contact
        assert!((r.closest_distance).abs() < 1e-5);
    }

    #[test]
    fn ground_plane_penetration() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::GroundPlane { height: 0.0 });
        // sphere center at z=0.3, radius 1 → penetration depth 0.7
        let r = sensor.sample(Vec3::new(0.0, 0.0, 0.3), &scene, 0.0);
        assert!(r.in_contact);
        assert!((r.penetration_depth - 0.7).abs() < 1e-5);
        assert!((r.closest_distance + 0.7).abs() < 1e-5);
    }

    // ── sphere ───────────────────────────────────────────────────────

    #[test]
    fn sphere_vs_sphere_no_contact() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::Sphere { center: Vec3::new(5.0, 0.0, 0.0), radius: 1.0 });
        let r = sensor.sample(Vec3::ZERO, &scene, 0.0);
        // centers 5 apart, two radii of 1 → 3m gap
        assert!(!r.in_contact);
        assert!((r.closest_distance - 3.0).abs() < 1e-5);
    }

    #[test]
    fn sphere_vs_sphere_overlap() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::Sphere { center: Vec3::new(1.0, 0.0, 0.0), radius: 1.0 });
        let r = sensor.sample(Vec3::ZERO, &scene, 0.0);
        // centers 1 apart, two radii of 1 → 1m overlap → 1m penetration
        assert!(r.in_contact);
        assert!((r.penetration_depth - 1.0).abs() < 1e-5);
        assert!((r.contact_normal.x + 1.0).abs() < 1e-5); // normal from prim toward sensor = -x
    }

    // ── AABB ─────────────────────────────────────────────────────────

    #[test]
    fn aabb_outside() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::Aabb {
            min: Vec3::new(-0.5, -0.5, 4.5),
            max: Vec3::new(0.5, 0.5, 5.5),
        });
        let r = sensor.sample(Vec3::ZERO, &scene, 0.0);
        // closest point on AABB = (0, 0, 4.5); distance = 4.5; sphere surface = 3.5
        assert!(!r.in_contact);
        assert!((r.closest_distance - 3.5).abs() < 1e-5);
    }

    #[test]
    fn aabb_overlap() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene.add(Primitive::Aabb {
            min: Vec3::new(-0.5, -0.5, 0.5),
            max: Vec3::new(0.5, 0.5, 1.5),
        });
        let r = sensor.sample(Vec3::ZERO, &scene, 0.0);
        // closest point = (0, 0, 0.5); distance = 0.5; sphere surface = -0.5
        assert!(r.in_contact);
        assert!((r.penetration_depth - 0.5).abs() < 1e-5);
    }

    #[test]
    fn aabb_sensor_center_inside_box() {
        // Sensor sphere center is inside the AABB → penetration > radius
        let sensor = ContactSensor::new("c", "/c", "cart", 0.1);
        let mut scene = Scene::new();
        scene.add(Primitive::Aabb {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        });
        let r = sensor.sample(Vec3::ZERO, &scene, 0.0);
        // Inside box, smallest face distance = 1.0; AABB-closest returns -1.0;
        // sphere surface distance = -1.0 - 0.1 = -1.1 → penetration 1.1
        assert!(r.in_contact);
        assert!((r.penetration_depth - 1.1).abs() < 1e-5);
    }

    // ── multi-primitive scene ────────────────────────────────────────

    #[test]
    fn picks_nearest_among_many() {
        let sensor = sensor_radius_1();
        let mut scene = Scene::new();
        scene
            .add(Primitive::Sphere { center: Vec3::new(10.0, 0.0, 0.0), radius: 1.0 })
            .add(Primitive::Sphere { center: Vec3::new(3.0, 0.0, 0.0), radius: 0.5 }) // nearest
            .add(Primitive::GroundPlane { height: -10.0 });
        let r = sensor.sample(Vec3::ZERO, &scene, 0.0);
        // Sensor at origin, radius 1. Nearest = sphere idx 1 at (3,0,0) r=0.5.
        // Distance = 3 - 1 - 0.5 = 1.5
        assert_eq!(r.closest_primitive, 1);
        assert!((r.closest_distance - 1.5).abs() < 1e-5);
        assert!(!r.in_contact);
    }
}
