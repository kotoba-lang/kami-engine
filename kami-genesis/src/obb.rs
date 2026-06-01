//! obb — oriented-bounding-box SAT + contact **manifold** (stable resting).
//!
//! A single deepest-point contact (the EPA result) lets a box wobble/rotate on a
//! surface; **stable stacking** needs a multi-point contact *manifold* — the
//! piece PhysX / Box2D generate by reference/incident-face clipping. This adds
//! OBB–OBB Separating-Axis-Test (15 axes) for the normal + min penetration, and
//! a multi-point manifold (vertex-in-face, both directions) so a box resting on
//! another yields ~4 contact points and does not tip.
//!
//! Honest scope: OBB–OBB (box) only — general convex manifold needs hull-face
//! topology (decompose, or use this for the box case). Edge–edge crossings on
//! partial overlap are approximated by the vertices-in-face set (sufficient for
//! face-face resting). Clean-room, CPU/WASM, f32.

use glam::{Quat, Vec3};

/// An oriented box: centre, half-extents, and 3 orthonormal axes (rotation).
#[derive(Clone, Debug)]
pub struct Obb {
    pub center: Vec3,
    pub half: Vec3,
    pub axes: [Vec3; 3],
}

impl Obb {
    pub fn new(center: Vec3, half: Vec3, rot: Quat) -> Self {
        Self {
            center,
            half,
            axes: [rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z],
        }
    }

    /// Projection radius of the box onto a (unit) axis.
    fn radius(&self, axis: Vec3) -> f32 {
        self.half.x * self.axes[0].dot(axis).abs()
            + self.half.y * self.axes[1].dot(axis).abs()
            + self.half.z * self.axes[2].dot(axis).abs()
    }

    /// The 8 corners.
    pub fn corners(&self) -> [Vec3; 8] {
        let mut c = [Vec3::ZERO; 8];
        let mut k = 0;
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    c[k] = self.center
                        + self.axes[0] * (sx * self.half.x)
                        + self.axes[1] * (sy * self.half.y)
                        + self.axes[2] * (sz * self.half.z);
                    k += 1;
                }
            }
        }
        c
    }

    /// Signed depth of a world point below this box's surface along `n`
    /// (positive = inside), and whether it lies within the box's lateral extent
    /// of the face whose outward normal is `n`.
    fn point_under_face(&self, p: Vec3, n: Vec3) -> Option<f32> {
        let d = p - self.center;
        // lateral coords (the two axes not aligned with n)
        let mut depth_along_n = f32::INFINITY;
        let mut lateral_ok = true;
        for k in 0..3 {
            let comp = d.dot(self.axes[k]);
            let aligned = self.axes[k].dot(n).abs();
            if aligned > 0.9 {
                // axis ~ n: penetration depth below the +n face
                depth_along_n = self.half[k] - comp * n.dot(self.axes[k]).signum();
            } else if comp.abs() > self.half[k] + 1e-4 {
                lateral_ok = false;
            }
        }
        (lateral_ok && depth_along_n.is_finite() && depth_along_n > -1e-4).then_some(depth_along_n)
    }
}

/// A contact manifold: a shared world normal (a → b) and up to several points
/// with their penetration depths.
#[derive(Clone, Debug)]
pub struct Manifold {
    pub normal: Vec3,
    pub points: Vec<(Vec3, f32)>,
}

/// OBB–OBB SAT: returns `(normal a→b, penetration)` along the min-overlap axis,
/// or `None` if separated.
pub fn obb_sat(a: &Obb, b: &Obb) -> Option<(Vec3, f32)> {
    let t = b.center - a.center;
    let mut axes: Vec<Vec3> = Vec::with_capacity(15);
    axes.extend_from_slice(&a.axes);
    axes.extend_from_slice(&b.axes);
    for i in 0..3 {
        for j in 0..3 {
            let c = a.axes[i].cross(b.axes[j]);
            if c.length_squared() > 1e-8 {
                axes.push(c.normalize());
            }
        }
    }
    let mut best_overlap = f32::INFINITY;
    let mut best_axis = Vec3::Z;
    for l in axes {
        let ln = l.normalize_or_zero();
        if ln.length_squared() < 0.5 {
            continue;
        }
        let dist = t.dot(ln).abs();
        let overlap = a.radius(ln) + b.radius(ln) - dist;
        if overlap < 0.0 {
            return None; // separating axis found
        }
        if overlap < best_overlap {
            best_overlap = overlap;
            best_axis = ln;
        }
    }
    // orient the normal from a toward b
    let n = if best_axis.dot(t) < 0.0 {
        -best_axis
    } else {
        best_axis
    };
    Some((n, best_overlap))
}

/// OBB–OBB contact manifold (multi-point for stable resting), or `None` if the
/// boxes are separated.
pub fn obb_manifold(a: &Obb, b: &Obb) -> Option<Manifold> {
    let (n, _depth) = obb_sat(a, b)?;
    let mut points: Vec<(Vec3, f32)> = Vec::new();

    // b's corners pressed into a's +n face.
    for c in b.corners() {
        if let Some(d) = a.point_under_face(c, n) {
            if d >= -1e-4 {
                points.push((c, d));
            }
        }
    }
    // a's corners pressed into b's −n face (other direction).
    for c in a.corners() {
        if let Some(d) = b.point_under_face(c, -n) {
            if d >= -1e-4 {
                points.push((c, d));
            }
        }
    }

    // de-duplicate near-coincident points (keep the deeper one).
    let mut uniq: Vec<(Vec3, f32)> = Vec::new();
    for (p, d) in points {
        if let Some(e) = uniq.iter_mut().find(|(q, _)| (*q - p).length() < 1e-3) {
            if d > e.1 {
                e.1 = d;
            }
        } else {
            uniq.push((p, d));
        }
    }
    if uniq.is_empty() {
        return None;
    }
    Some(Manifold {
        normal: n,
        points: uniq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(c: Vec3) -> Obb {
        Obb::new(c, Vec3::splat(0.5), Quat::IDENTITY)
    }

    #[test]
    fn separated_boxes_no_manifold() {
        let a = unit(Vec3::ZERO);
        let b = unit(Vec3::new(3.0, 0.0, 0.0));
        assert!(obb_sat(&a, &b).is_none());
        assert!(obb_manifold(&a, &b).is_none());
    }

    #[test]
    fn stacked_boxes_give_four_point_manifold() {
        // b resting on a with a small overlap along +z.
        let a = unit(Vec3::ZERO);
        let b = unit(Vec3::new(0.0, 0.0, 0.95)); // overlap 1.0 - 0.95 = 0.05
        let (n, depth) = obb_sat(&a, &b).expect("overlap");
        assert!(n.z.abs() > 0.9, "normal should be vertical: {n:?}");
        assert!((depth - 0.05).abs() < 0.02, "depth={depth}");
        let m = obb_manifold(&a, &b).expect("manifold");
        // a square face-face contact yields the 4 incident corners
        assert!(m.points.len() >= 4, "manifold points: {}", m.points.len());
        assert!(m.points.iter().all(|(_, d)| *d > 0.0 && d.is_finite()));
        // points span the face (not all at one spot) → stable (no tipping).
        let xs: Vec<f32> = m.points.iter().map(|(p, _)| p.x).collect();
        let spread = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.5, "points not spread across the face: {spread}");
    }

    #[test]
    fn sat_is_rotationally_covariant() {
        // Rotating the whole scene by R must rotate the contact normal by R and
        // leave the penetration depth invariant (SAT is a frame-covariant
        // geometric query). After an arbitrary R the contact axis is no longer a
        // coordinate axis, so this exercises the rotated face + cross-product
        // axes — the 9 edge×edge axes that distinguish OBB SAT from plain AABB.
        let a0 = Obb::new(Vec3::ZERO, Vec3::splat(0.5), Quat::IDENTITY);
        let b0 = Obb::new(Vec3::new(0.0, 0.0, 0.95), Vec3::splat(0.5), Quat::IDENTITY);
        let (n0, d0) = obb_sat(&a0, &b0).expect("overlap");

        let r = Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.7);
        let a = Obb::new(Vec3::ZERO, Vec3::splat(0.5), r);
        let b = Obb::new(r * Vec3::new(0.0, 0.0, 0.95), Vec3::splat(0.5), r);
        let (n, d) = obb_sat(&a, &b).expect("overlap after rotation");

        assert!(
            (d - d0).abs() < 1e-4,
            "depth not invariant under rotation: {d0} -> {d}"
        );
        assert!(
            (n - r * n0).length() < 1e-3,
            "normal not covariant: {n:?} vs R·n0 {:?}",
            r * n0
        );

        // the manifold contact points are likewise the rotated originals.
        let m0 = obb_manifold(&a0, &b0).expect("manifold");
        let m = obb_manifold(&a, &b).expect("manifold after rotation");
        assert_eq!(m.points.len(), m0.points.len());
        for (p, _) in &m.points {
            // every rotated contact point matches some R·(original point).
            assert!(
                m0.points.iter().any(|(q, _)| (*p - r * *q).length() < 1e-3),
                "manifold point {p:?} is not a rotated original"
            );
        }
    }

    #[test]
    fn overlapping_centered_boxes_have_normal_and_points() {
        let a = unit(Vec3::ZERO);
        let b = unit(Vec3::new(0.2, 0.0, 0.0));
        let m = obb_manifold(&a, &b).expect("manifold");
        assert!(!m.points.is_empty());
        assert!(m.normal.length() > 0.9);
    }
}
