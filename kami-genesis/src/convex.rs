//! convex — GJK distance + intersection and EPA penetration for convex shapes.
//!
//! Narrow-phase collision for arbitrary convex polytopes (boxes, hulls), the
//! piece kami-genesis's contact solver lacked (it only had ground-plane / AABB /
//! sphere / capsule proxies). Brings the same *class* of narrow-phase that
//! PhysX uses (GJK distance + EPA penetration); clean-room, CPU/WASM, f32.
//!
//! - `gjk_distance(a, b)`  → separation distance (0 if intersecting).
//! - `gjk_intersects(a, b)`→ boolean overlap (origin ∈ Minkowski difference).
//! - `epa_penetration(a, b)` → (depth, world-space normal) when intersecting.
//!
//! Honest scope: convex–convex only (concave = decompose), f32 single precision,
//! no manifold generation / persistent contacts yet (one deepest point/normal).

use glam::Vec3;

/// A convex polytope given by its world-space vertices (support = argmax dot).
#[derive(Clone, Debug)]
pub struct ConvexPoly {
    pub verts: Vec<Vec3>,
}

impl ConvexPoly {
    pub fn new(verts: Vec<Vec3>) -> Self {
        Self { verts }
    }

    /// Axis-aligned (or transformed) box from a centre, half-extents, and an
    /// optional rotation applied to the 8 corners.
    pub fn box_at(center: Vec3, half: Vec3, rot: glam::Quat) -> Self {
        let mut v = Vec::with_capacity(8);
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let local = Vec3::new(sx * half.x, sy * half.y, sz * half.z);
                    v.push(center + rot * local);
                }
            }
        }
        Self { verts: v }
    }

    #[inline]
    fn support(&self, dir: Vec3) -> Vec3 {
        let mut best = self.verts[0];
        let mut bd = best.dot(dir);
        for &v in &self.verts[1..] {
            let d = v.dot(dir);
            if d > bd {
                bd = d;
                best = v;
            }
        }
        best
    }
}

#[inline]
fn cso_support(a: &ConvexPoly, b: &ConvexPoly, dir: Vec3) -> Vec3 {
    a.support(dir) - b.support(-dir)
}

// ── closest point on a simplex to the ORIGIN (Ericson) ────────────────────────

fn closest_on_segment(a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let t = (-a.dot(ab)) / ab.dot(ab).max(1e-12);
    a + ab * t.clamp(0.0, 1.0)
}

fn closest_on_triangle(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = -a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = -b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = -c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

fn point_outside_plane(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> bool {
    // is the origin on the opposite side of plane(abc) from d?
    let n = (b - a).cross(c - a);
    let signp = (-a).dot(n);
    let signd = (d - a).dot(n);
    signp * signd < 0.0
}

fn closest_on_tetra(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> (Vec3, bool) {
    // returns (closest point to origin, origin_inside)
    let mut best = Vec3::ZERO;
    let mut best_d2 = f32::INFINITY;
    let mut any_outside = false;
    let faces = [(a, b, c, d), (a, c, d, b), (a, d, b, c), (b, d, c, a)];
    for (p, q, r, opp) in faces {
        if point_outside_plane(p, q, r, opp) {
            any_outside = true;
            let cp = closest_on_triangle(p, q, r);
            let d2 = cp.length_squared();
            if d2 < best_d2 {
                best_d2 = d2;
                best = cp;
            }
        }
    }
    if !any_outside {
        (Vec3::ZERO, true) // origin inside the tetrahedron
    } else {
        (best, false)
    }
}

/// GJK closest distance between two convex polytopes (0.0 if intersecting).
pub fn gjk_distance(a: &ConvexPoly, b: &ConvexPoly) -> f32 {
    gjk_closest_vec(a, b).length()
}

/// GJK closest-point vector on the Minkowski difference (a ⊖ b) to the origin —
/// i.e. the separation vector pointing from `b` toward `a`. `Vec3::ZERO` when the
/// shapes intersect. Witness for contact-normal generation.
pub fn gjk_closest_vec(a: &ConvexPoly, b: &ConvexPoly) -> Vec3 {
    let mut dir = Vec3::X;
    let mut simplex: Vec<Vec3> = vec![cso_support(a, b, dir)];
    let mut closest = simplex[0];
    for _ in 0..64 {
        dir = -closest;
        if dir.length_squared() < 1e-12 {
            return Vec3::ZERO; // origin on the simplex → touching/penetrating
        }
        let p = cso_support(a, b, dir);
        // no progress toward the origin → converged.
        if p.dot(dir) - closest.dot(dir) < 1e-7 {
            return closest;
        }
        simplex.push(p);
        // reduce simplex to the feature closest to the origin.
        let (cp, inside) = match simplex.len() {
            1 => (simplex[0], false),
            2 => (closest_on_segment(simplex[0], simplex[1]), false),
            3 => (
                closest_on_triangle(simplex[0], simplex[1], simplex[2]),
                false,
            ),
            _ => closest_on_tetra(simplex[0], simplex[1], simplex[2], simplex[3]),
        };
        if inside {
            return Vec3::ZERO;
        }
        closest = cp;
        // keep only the simplex vertices that define the closest feature: drop
        // the farthest vertex when we have a full tetra to stay ≤3 for the next
        // iteration's progress (Johnson-style pruning, simplified).
        if simplex.len() == 4 {
            // remove the vertex farthest from the closest point.
            let mut worst = 0;
            let mut wd = -1.0;
            for (i, v) in simplex.iter().enumerate() {
                let d = (*v - closest).length_squared();
                if d > wd {
                    wd = d;
                    worst = i;
                }
            }
            simplex.remove(worst);
        }
    }
    closest
}

// ── boolean GJK (origin enclosure) for EPA seeding ────────────────────────────

fn triple_cross(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    a.cross(b).cross(c)
}

fn do_simplex(s: &mut Vec<Vec3>, dir: &mut Vec3) -> bool {
    let ao = -*s.last().unwrap();
    match s.len() {
        2 => {
            let a = s[1];
            let b = s[0];
            let ab = b - a;
            if ab.dot(ao) > 0.0 {
                *dir = triple_cross(ab, ao, ab);
            } else {
                *s = vec![a];
                *dir = ao;
            }
            false
        }
        3 => {
            let a = s[2];
            let b = s[1];
            let c = s[0];
            let ab = b - a;
            let ac = c - a;
            let abc = ab.cross(ac);
            if abc.cross(ac).dot(ao) > 0.0 {
                if ac.dot(ao) > 0.0 {
                    *s = vec![c, a];
                    *dir = triple_cross(ac, ao, ac);
                } else {
                    *s = vec![b, a];
                    return star_line(s, dir);
                }
            } else if ab.cross(abc).dot(ao) > 0.0 {
                *s = vec![b, a];
                return star_line(s, dir);
            } else if abc.dot(ao) > 0.0 {
                *dir = abc;
            } else {
                *s = vec![b, c, a];
                *dir = -abc;
            }
            false
        }
        4 => {
            let a = s[3];
            let b = s[2];
            let c = s[1];
            let d = s[0];
            let abc = (b - a).cross(c - a);
            let acd = (c - a).cross(d - a);
            let adb = (d - a).cross(b - a);
            if abc.dot(ao) > 0.0 {
                *s = vec![c, b, a];
                *dir = abc;
                false
            } else if acd.dot(ao) > 0.0 {
                *s = vec![d, c, a];
                *dir = acd;
                false
            } else if adb.dot(ao) > 0.0 {
                *s = vec![b, d, a];
                *dir = adb;
                false
            } else {
                true // origin enclosed
            }
        }
        _ => false,
    }
}

fn star_line(s: &mut [Vec3], dir: &mut Vec3) -> bool {
    let ao = -s[1];
    let ab = s[0] - s[1];
    *dir = triple_cross(ab, ao, ab);
    false
}

fn gjk_simplex(a: &ConvexPoly, b: &ConvexPoly) -> Option<[Vec3; 4]> {
    let mut dir = Vec3::X;
    let mut s: Vec<Vec3> = vec![cso_support(a, b, dir)];
    dir = -s[0];
    for _ in 0..64 {
        if dir.length_squared() < 1e-12 {
            dir = Vec3::Y;
        }
        let p = cso_support(a, b, dir);
        if p.dot(dir) < 0.0 {
            return None;
        }
        s.push(p);
        if do_simplex(&mut s, &mut dir) {
            return Some([s[0], s[1], s[2], s[3]]);
        }
    }
    None
}

/// Boolean convex overlap test.
pub fn gjk_intersects(a: &ConvexPoly, b: &ConvexPoly) -> bool {
    gjk_simplex(a, b).is_some()
}

// ── EPA penetration depth + normal ────────────────────────────────────────────

struct Face {
    i: [usize; 3],
    n: Vec3,
    dist: f32,
}

fn make_face(verts: &[Vec3], i: usize, j: usize, k: usize) -> Face {
    let mut n = (verts[j] - verts[i]).cross(verts[k] - verts[i]);
    let mut dist = n.dot(verts[i]);
    let nl = n.length();
    if nl > 1e-12 {
        n /= nl;
        dist /= nl;
    }
    // ensure outward (origin on negative side → flip).
    if dist < 0.0 {
        n = -n;
        dist = -dist;
        return Face {
            i: [i, k, j],
            n,
            dist,
        };
    }
    Face {
        i: [i, j, k],
        n,
        dist,
    }
}

/// EPA: penetration depth + outward normal (b pushed out of a along +normal),
/// or `None` if the shapes do not intersect.
pub fn epa_penetration(a: &ConvexPoly, b: &ConvexPoly) -> Option<(f32, Vec3)> {
    let tet = gjk_simplex(a, b)?;
    let mut verts: Vec<Vec3> = tet.to_vec();
    let mut faces = vec![
        make_face(&verts, 0, 1, 2),
        make_face(&verts, 0, 2, 3),
        make_face(&verts, 0, 3, 1),
        make_face(&verts, 1, 3, 2),
    ];
    for _ in 0..48 {
        // closest face to origin
        let mut ci = 0;
        let mut cd = f32::INFINITY;
        for (idx, f) in faces.iter().enumerate() {
            if f.dist < cd {
                cd = f.dist;
                ci = idx;
            }
        }
        let n = faces[ci].n;
        let p = cso_support(a, b, n);
        let pd = p.dot(n);
        if pd - cd < 1e-4 {
            return Some((cd, n));
        }
        // expand: remove faces visible from p, retriangulate the horizon.
        let pi = verts.len();
        verts.push(p);
        let mut horizon: Vec<(usize, usize)> = Vec::new();
        let mut kept: Vec<Face> = Vec::new();
        for f in faces.drain(..) {
            if f.n.dot(p) - f.dist > 1e-6 {
                // visible — its edges go to the horizon (xor)
                let e = [(f.i[0], f.i[1]), (f.i[1], f.i[2]), (f.i[2], f.i[0])];
                for (x, y) in e {
                    if let Some(pos) = horizon.iter().position(|&(a, b)| a == y && b == x) {
                        horizon.remove(pos);
                    } else {
                        horizon.push((x, y));
                    }
                }
            } else {
                kept.push(f);
            }
        }
        faces = kept;
        for (x, y) in horizon {
            faces.push(make_face(&verts, x, y, pi));
        }
        if faces.is_empty() {
            return Some((cd, n));
        }
    }
    // fall back to the best face found
    let f = faces.iter().min_by(|a, b| a.dist.total_cmp(&b.dist))?;
    Some((f.dist, f.n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn unit_box(center: Vec3) -> ConvexPoly {
        ConvexPoly::box_at(center, Vec3::splat(0.5), Quat::IDENTITY)
    }

    #[test]
    fn separated_boxes_have_positive_distance() {
        let a = unit_box(Vec3::ZERO);
        let b = unit_box(Vec3::new(3.0, 0.0, 0.0)); // gap = 3 - 1 = 2
        assert!(!gjk_intersects(&a, &b));
        let d = gjk_distance(&a, &b);
        assert!((d - 2.0).abs() < 0.05, "distance={d}");
    }

    #[test]
    fn touching_boxes_zero_distance() {
        let a = unit_box(Vec3::ZERO);
        let b = unit_box(Vec3::new(1.0, 0.0, 0.0)); // faces touch
        let d = gjk_distance(&a, &b);
        assert!(d < 0.05, "distance={d}");
    }

    #[test]
    fn overlapping_boxes_intersect_and_epa_depth() {
        let a = unit_box(Vec3::ZERO);
        let b = unit_box(Vec3::new(0.7, 0.0, 0.0)); // overlap = 1 - 0.7 = 0.3
        assert!(gjk_intersects(&a, &b));
        assert!(gjk_distance(&a, &b) < 1e-3);
        let (depth, n) = epa_penetration(&a, &b).expect("penetration");
        assert!((depth - 0.3).abs() < 0.05, "depth={depth}");
        // normal should be along ±x
        assert!(n.x.abs() > 0.9, "normal={n:?}");
    }

    #[test]
    fn rotated_box_overlap_detected() {
        let a = unit_box(Vec3::ZERO);
        let b = ConvexPoly::box_at(
            Vec3::new(0.9, 0.0, 0.0),
            Vec3::splat(0.5),
            Quat::from_rotation_z(0.785),
        );
        assert!(gjk_intersects(&a, &b));
        let (depth, _) = epa_penetration(&a, &b).expect("pen");
        assert!(depth > 0.0 && depth.is_finite(), "depth={depth}");
    }

    #[test]
    fn diagonal_gap_distance_is_corner_to_corner() {
        // Boxes offset on BOTH x and y: the closest features are the parallel
        // vertical edges at opposite corners, so the gap is the xy diagonal
        // √(1²+1²) = √2 — exercises GJK on an edge/vertex simplex, not the
        // face-aligned axial case the other tests cover.
        let a = unit_box(Vec3::ZERO); // [-0.5, 0.5]³
        let b = unit_box(Vec3::new(2.0, 2.0, 0.0)); // corner (0.5,0.5) ↔ (1.5,1.5)
        assert!(!gjk_intersects(&a, &b));
        let d = gjk_distance(&a, &b);
        let expect = 2.0_f32.sqrt();
        assert!(
            (d - expect).abs() < 0.05,
            "diagonal distance={d}, expected {expect}"
        );
    }

    #[test]
    fn penetration_resolves_along_the_minimum_overlap_axis() {
        // Overlap on the y axis only → EPA must return depth = the y overlap and
        // a normal along ±y (the existing depth test only checks the x axis).
        let a = unit_box(Vec3::ZERO);
        let b = unit_box(Vec3::new(0.0, 0.6, 0.0)); // y overlap = 1 - 0.6 = 0.4
        assert!(gjk_intersects(&a, &b));
        let (depth, n) = epa_penetration(&a, &b).expect("penetration");
        assert!((depth - 0.4).abs() < 0.05, "depth={depth}");
        assert!(
            n.y.abs() > 0.9 && n.x.abs() < 0.3 && n.z.abs() < 0.3,
            "normal not along ±y: {n:?}"
        );
    }

    #[test]
    fn far_apart_not_intersecting() {
        let a = unit_box(Vec3::ZERO);
        let b = unit_box(Vec3::new(0.0, 10.0, 0.0));
        assert!(!gjk_intersects(&a, &b));
        assert!((gjk_distance(&a, &b) - 9.0).abs() < 0.1);
        assert!(epa_penetration(&a, &b).is_none());
    }
}
