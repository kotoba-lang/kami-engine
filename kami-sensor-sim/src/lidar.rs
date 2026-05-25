//! RotatingLidar — analytic raycast lidar against simple scene primitives.
//!
//! Mirrors `isaacsim.sensors.RotatingLidarPhysX` (Isaac Sim 4.x) at the level
//! of public Python API surface. At R1.1 the scene is a small list of analytic
//! primitives (sphere, ground plane, AABB) with explicit ray-intersection
//! formulas — no BVH yet. R1.2+ swaps in WGSL ray-query against a real BVH
//! built by kami-rt.
//!
//! Conventions: ROS REP-105 sensor frame (+x forward, +y left, +z up) — the
//! Isaac Sim default for lidar. This differs from the Camera convention
//! (+z forward, +y down) intentionally; both match upstream NVIDIA / ROS
//! defaults for their respective sensor types.
//! Lidar emits a regular grid of beams within (HFOV × VFOV) and returns one
//! 3D hit point per beam in sensor frame (∞ for non-hits).

use glam::{Affine3A, Vec3};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LidarIntrinsics {
    /// Horizontal field of view in radians.
    pub hfov: f32,
    /// Vertical field of view in radians.
    pub vfov: f32,
    /// Number of horizontal beams (azimuth resolution).
    pub h_beams: u32,
    /// Number of vertical channels (elevation resolution).
    pub v_beams: u32,
    pub range_min: f32,
    pub range_max: f32,
}

impl LidarIntrinsics {
    /// Velodyne VLP-16-equivalent: 360° HFOV × 30° VFOV, 1800 az × 16 el.
    pub fn vlp16() -> Self {
        LidarIntrinsics {
            hfov: 2.0 * PI,
            vfov: 30f32.to_radians(),
            h_beams: 1800,
            v_beams: 16,
            range_min: 0.5,
            range_max: 100.0,
        }
    }

    /// A small toy intrinsics convenient for unit tests.
    pub fn toy() -> Self {
        LidarIntrinsics {
            hfov: PI / 2.0,           // 90°
            vfov: 30f32.to_radians(), // 30°
            h_beams: 8,
            v_beams: 4,
            range_min: 0.05,
            range_max: 50.0,
        }
    }
}

/// Scene primitives supported at R1.1 (analytic intersection only).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    /// Axis-aligned ground plane: z = `height`.
    GroundPlane { height: f32 },
    /// Sphere with centre and radius (world coords).
    Sphere { center: Vec3, radius: f32 },
    /// Axis-aligned box (world coords).
    Aabb { min: Vec3, max: Vec3 },
}

impl Primitive {
    /// Slab / quadratic intersection. Returns the smallest positive t along
    /// ray(o, d) (with d **normalised**), or None.
    pub fn intersect(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        match *self {
            Primitive::GroundPlane { height } => {
                if dir.z.abs() < 1e-6 {
                    return None;
                }
                let t = (height - origin.z) / dir.z;
                if t > 0.0 { Some(t) } else { None }
            }
            Primitive::Sphere { center, radius } => {
                let oc = origin - center;
                let b = oc.dot(dir);
                let c = oc.dot(oc) - radius * radius;
                let disc = b * b - c;
                if disc < 0.0 {
                    return None;
                }
                let sq = disc.sqrt();
                let t0 = -b - sq;
                let t1 = -b + sq;
                if t0 > 1e-4 {
                    Some(t0)
                } else if t1 > 1e-4 {
                    Some(t1)
                } else {
                    None
                }
            }
            Primitive::Aabb { min, max } => {
                let inv = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
                let t1 = (min - origin) * inv;
                let t2 = (max - origin) * inv;
                let tmin = t1.min(t2).max_element();
                let tmax = t1.max(t2).min_element();
                if tmax < 0.0 || tmin > tmax {
                    None
                } else if tmin > 1e-4 {
                    Some(tmin)
                } else if tmax > 1e-4 {
                    Some(tmax)
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub primitives: Vec<Primitive>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, p: Primitive) -> &mut Self {
        self.primitives.push(p);
        self
    }
    /// Find the nearest hit along ray(origin, dir.normalize_or_zero()).
    /// Returns (t, hit_index_in_scene) or None.
    pub fn nearest_hit(&self, origin: Vec3, dir: Vec3) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, p) in self.primitives.iter().enumerate() {
            if let Some(t) = p.intersect(origin, dir) {
                if best.map_or(true, |(b, _)| t < b) {
                    best = Some((t, i));
                }
            }
        }
        best
    }
}

/// One lidar return per beam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LidarReturn {
    /// `range == f32::INFINITY` for a non-hit.
    pub range: f32,
    /// Hit point in the lidar sensor frame.
    pub point_sensor: Vec3,
    /// Index of the primitive struck in `Scene.primitives`, or `usize::MAX`.
    pub prim_index: usize,
}

impl LidarReturn {
    pub fn miss() -> Self {
        LidarReturn {
            range: f32::INFINITY,
            point_sensor: Vec3::ZERO,
            prim_index: usize::MAX,
        }
    }
}

/// Mirror of `isaacsim.sensors.RotatingLidarPhysX` (subset).
#[derive(Debug, Clone)]
pub struct Lidar {
    pub name: String,
    pub prim_path: String,
    pub intrinsics: LidarIntrinsics,
    /// world → sensor transform.
    pub view: Affine3A,
}

impl Lidar {
    pub fn new(name: impl Into<String>, prim_path: impl Into<String>, intr: LidarIntrinsics) -> Self {
        Lidar {
            name: name.into(),
            prim_path: prim_path.into(),
            intrinsics: intr,
            view: Affine3A::IDENTITY,
        }
    }

    /// `sensor → world` transform (inverse of `view`).
    fn sensor_to_world(&self) -> Affine3A {
        self.view.inverse()
    }

    /// One beam direction in sensor frame for grid (i_h, i_v).
    /// Sensor frame is ROS REP-105: +x forward, +y left, +z up. At zero
    /// azimuth + zero elevation the beam points along +x. Azimuth is
    /// measured counter-clockwise from +x in the horizontal plane (+y
    /// direction is positive azimuth); elevation is measured above the
    /// horizontal plane (+z direction is positive elevation).
    fn beam_dir_sensor(&self, i_h: u32, i_v: u32) -> Vec3 {
        let i = self.intrinsics;
        let az_min = -i.hfov * 0.5;
        let el_min = -i.vfov * 0.5;
        let az_step = if i.h_beams > 1 { i.hfov / i.h_beams as f32 } else { 0.0 };
        let el_step = if i.v_beams > 1 { i.vfov / i.v_beams as f32 } else { 0.0 };
        let az = az_min + (i_h as f32 + 0.5) * az_step;
        let el = el_min + (i_v as f32 + 0.5) * el_step;
        let (sa, ca) = (az.sin(), az.cos());
        let (se, ce) = (el.sin(), el.cos());
        Vec3::new(ce * ca, ce * sa, se).normalize()
    }

    /// Cast all (h_beams × v_beams) rays into `scene` and return one
    /// LidarReturn per beam (row-major: idx = v * h_beams + h).
    pub fn acquire_data(&self, scene: &Scene) -> Vec<LidarReturn> {
        let s2w = self.sensor_to_world();
        let origin_world = s2w.transform_point3(Vec3::ZERO);
        let total = (self.intrinsics.h_beams * self.intrinsics.v_beams) as usize;
        let mut out = Vec::with_capacity(total);
        for v in 0..self.intrinsics.v_beams {
            for h in 0..self.intrinsics.h_beams {
                let dir_sensor = self.beam_dir_sensor(h, v);
                let dir_world = s2w.transform_vector3(dir_sensor).normalize_or_zero();
                let hit = scene.nearest_hit(origin_world, dir_world);
                let item = match hit {
                    Some((t, idx))
                        if t >= self.intrinsics.range_min
                            && t <= self.intrinsics.range_max =>
                    {
                        LidarReturn {
                            range: t,
                            point_sensor: dir_sensor * t,
                            prim_index: idx,
                        }
                    }
                    _ => LidarReturn::miss(),
                };
                out.push(item);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lidar_at_origin() -> Lidar {
        let mut l = Lidar::new("test_lidar", "/World/test_lidar", LidarIntrinsics::toy());
        l.view = Affine3A::IDENTITY;
        l
    }

    #[test]
    fn vlp16_intrinsics_match_expected() {
        let i = LidarIntrinsics::vlp16();
        assert!((i.hfov - 2.0 * PI).abs() < 1e-5);
        assert_eq!(i.h_beams, 1800);
        assert_eq!(i.v_beams, 16);
    }

    #[test]
    fn ground_plane_intersection() {
        // Camera at (0,0,5) looking down → hits z=0 ground at t=5.
        let p = Primitive::GroundPlane { height: 0.0 };
        let origin = Vec3::new(0.0, 0.0, 5.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);
        assert!((p.intersect(origin, dir).unwrap() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn sphere_intersection_picks_nearest_root() {
        // Sphere at (0,0,10) radius 1, ray origin at zero forward → hits at t=9.
        let p = Primitive::Sphere { center: Vec3::new(0.0, 0.0, 10.0), radius: 1.0 };
        let t = p.intersect(Vec3::ZERO, Vec3::Z).unwrap();
        assert!((t - 9.0).abs() < 1e-4, "t={t}");
    }

    #[test]
    fn sphere_miss_returns_none() {
        let p = Primitive::Sphere { center: Vec3::new(10.0, 0.0, 10.0), radius: 0.5 };
        assert!(p.intersect(Vec3::ZERO, Vec3::Z).is_none());
    }

    #[test]
    fn aabb_intersection_smallest_positive_t() {
        // 1m cube at (0,0,10), ray from origin forward → hits z=9 face.
        let p = Primitive::Aabb {
            min: Vec3::new(-0.5, -0.5, 9.5),
            max: Vec3::new(0.5, 0.5, 10.5),
        };
        let t = p.intersect(Vec3::ZERO, Vec3::Z).unwrap();
        assert!((t - 9.5).abs() < 1e-4);
    }

    #[test]
    fn scene_returns_nearest_among_many() {
        let mut scene = Scene::new();
        scene
            .add(Primitive::GroundPlane { height: -2.0 })
            .add(Primitive::Sphere { center: Vec3::new(0.0, 0.0, 10.0), radius: 0.3 })
            .add(Primitive::Sphere { center: Vec3::new(0.0, 0.0, 5.0), radius: 0.3 });
        let (t, idx) = scene.nearest_hit(Vec3::ZERO, Vec3::Z).unwrap();
        assert!((t - 4.7).abs() < 1e-3, "t={t}");
        assert_eq!(idx, 2);
    }

    #[test]
    fn lidar_beams_grid_size_matches_intrinsics() {
        let l = lidar_at_origin();
        let scene = Scene::new(); // empty scene → all misses
        let returns = l.acquire_data(&scene);
        assert_eq!(returns.len(), (l.intrinsics.h_beams * l.intrinsics.v_beams) as usize);
        assert!(returns.iter().all(|r| r.range.is_infinite()));
    }

    #[test]
    fn lidar_hits_ground_plane_below() {
        // Lidar at (0,0,1) in world (1 m above ground at z=0). Sensor frame
        // is ROS REP-105 (+x forward, +z up). View = world→sensor translation
        // that maps (0,0,1) world → (0,0,0) sensor.
        let mut l = lidar_at_origin();
        l.view = Affine3A::from_translation(Vec3::new(0.0, 0.0, -1.0));
        let mut scene = Scene::new();
        scene.add(Primitive::GroundPlane { height: 0.0 });
        let returns = l.acquire_data(&scene);
        // Beams in the lower half of the VFOV (negative elevation) should hit
        // the ground plane.
        let hits = returns.iter().filter(|r| r.range.is_finite()).count();
        assert!(hits > 0, "expected at least one beam to strike the ground plane");
        // Steepest downward beam at el = -hfov/2 + (hfov / v_beams / 2) for
        // v_beams=4, vfov=30°: el ≈ -15° + 3.75° = -11.25°.
        // Range = 1 / sin(11.25°) ≈ 5.13 m.
        let r_min = returns
            .iter()
            .filter_map(|r| r.range.is_finite().then_some(r.range))
            .fold(f32::INFINITY, f32::min);
        assert!(r_min > 1.0 && r_min < 10.0, "r_min={r_min}");
    }

    #[test]
    fn lidar_hits_sphere_in_front_and_indexes_it() {
        // ROS REP-105: "in front" is +x. Sphere large enough that the
        // beam-sampling offset (each beam is at (i+0.5)/N of its FoV step)
        // can't miss.
        let mut l = lidar_at_origin();
        l.view = Affine3A::IDENTITY;
        let mut scene = Scene::new();
        scene
            .add(Primitive::GroundPlane { height: -100.0 }) // way out of range
            .add(Primitive::Sphere { center: Vec3::new(5.0, 0.0, 0.0), radius: 1.0 });
        let returns = l.acquire_data(&scene);
        // At least one beam in the centre column should strike the sphere.
        let hits_on_sphere: Vec<_> = returns
            .iter()
            .filter(|r| r.range.is_finite() && r.prim_index == 1)
            .collect();
        assert!(!hits_on_sphere.is_empty(), "expected at least one beam to strike the sphere");
        // Nearest hit on sphere surface ≈ 4.0 m (5 m - radius 1 m).
        let r_min = hits_on_sphere
            .iter()
            .map(|r| r.range)
            .fold(f32::INFINITY, f32::min);
        assert!(r_min > 3.5 && r_min < 4.5, "expected ~4.0 m sphere range, got {r_min}");
    }
}
