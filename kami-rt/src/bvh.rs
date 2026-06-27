//! Linear BVH (LBVH) — CPU acceleration-structure build + reference traversal.
//!
//! The host side of `kami.rt`'s `:rt/accel {:kind :bvh}`. Triangle centroids are
//! ranked by 30-bit Morton code (Z-order), sorted, and split at the median to
//! build a binary BVH whose node array is flat and GPU-uploadable (the WGSL
//! ray-query / LBVH-compute path consumes the same `nodes` layout). The CPU
//! traversal here is the reference oracle that pins the structure's correctness
//! without a GPU — the same role `cpu-trace` plays on the clj side.

use glam::Vec3;

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
    pub fn grow_point(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn union(&mut self, o: &Aabb) {
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
    }
    pub fn centroid(&self) -> Vec3 {
        0.5 * (self.min + self.max)
    }
    /// Slab test: does `ray` (origin/dir, dir need not be unit) hit within [t0,t1]?
    pub fn hit(&self, origin: Vec3, inv_dir: Vec3, t0: f32, t1: f32) -> bool {
        let mut tmin = t0;
        let mut tmax = t1;
        for a in 0..3 {
            let inv = inv_dir[a];
            let mut ta = (self.min[a] - origin[a]) * inv;
            let mut tb = (self.max[a] - origin[a]) * inv;
            if inv < 0.0 {
                std::mem::swap(&mut ta, &mut tb);
            }
            tmin = tmin.max(ta);
            tmax = tmax.min(tb);
            if tmax < tmin {
                return false;
            }
        }
        true
    }
}

/// A triangle (CCW). Carries a stable `id` so traversal can report what was hit.
#[derive(Debug, Clone, Copy)]
pub struct Tri {
    pub v0: Vec3,
    pub v1: Vec3,
    pub v2: Vec3,
    pub id: u32,
}

impl Tri {
    pub fn aabb(&self) -> Aabb {
        let mut bb = Aabb::empty();
        bb.grow_point(self.v0);
        bb.grow_point(self.v1);
        bb.grow_point(self.v2);
        bb
    }
    pub fn centroid(&self) -> Vec3 {
        (self.v0 + self.v1 + self.v2) / 3.0
    }
    /// Möller–Trumbore ray↔triangle; returns front-face distance `t` if hit.
    pub fn intersect(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let e1 = self.v1 - self.v0;
        let e2 = self.v2 - self.v0;
        let p = dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-8 {
            return None;
        }
        let inv = 1.0 / det;
        let tvec = origin - self.v0;
        let u = tvec.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = tvec.cross(e1);
        let v = dir.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = e2.dot(q) * inv;
        if t > 1e-4 { Some(t) } else { None }
    }
}

/// Flat BVH node. A leaf has `count > 0` and references `[start, start+count)`
/// in `tri_order`; an internal node has `count == 0` and child indices
/// `left`/`right` into `nodes`.
#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub aabb: Aabb,
    pub left: u32,
    pub right: u32,
    pub start: u32,
    pub count: u32,
}

/// A built BVH: a flat node array (root at 0) + the Morton-sorted triangle order.
#[derive(Debug, Clone)]
pub struct Bvh {
    pub nodes: Vec<Node>,
    pub tri_order: Vec<u32>,
    pub tris: Vec<Tri>,
}

/// Nearest hit from a traversal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub t: f32,
    pub tri_id: u32,
}

const LEAF_MAX: usize = 2;

/// Expand a 10-bit integer into 30 bits, inserting two 0s before each bit.
fn expand_bits(mut v: u32) -> u32 {
    v &= 0x3ff;
    v = (v | (v << 16)) & 0x030000ff;
    v = (v | (v << 8)) & 0x0300f00f;
    v = (v | (v << 4)) & 0x030c30c3;
    v = (v | (v << 2)) & 0x09249249;
    v
}

/// 30-bit Morton code for a point in the unit cube [0,1]³.
fn morton3(p: Vec3) -> u32 {
    let scale = |c: f32| -> u32 { (c.clamp(0.0, 1.0) * 1023.0).round() as u32 };
    (expand_bits(scale(p.x)) << 2) | (expand_bits(scale(p.y)) << 1) | expand_bits(scale(p.z))
}

impl Bvh {
    /// Build an LBVH over `tris`. Empty input yields a single empty leaf.
    pub fn build(tris: Vec<Tri>) -> Self {
        if tris.is_empty() {
            return Self {
                nodes: vec![Node {
                    aabb: Aabb::empty(),
                    left: 0,
                    right: 0,
                    start: 0,
                    count: 0,
                }],
                tri_order: vec![],
                tris,
            };
        }

        // Scene centroid bounds → normalize centroids into the unit cube.
        let mut cbounds = Aabb::empty();
        for t in &tris {
            cbounds.grow_point(t.centroid());
        }
        let extent = (cbounds.max - cbounds.min).max(Vec3::splat(1e-6));

        // (morton, tri_index), sorted by Morton code (Z-order linear layout).
        let mut keyed: Vec<(u32, u32)> = tris
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let n = (t.centroid() - cbounds.min) / extent;
                (morton3(n), i as u32)
            })
            .collect();
        keyed.sort_by_key(|(m, _)| *m);
        let tri_order: Vec<u32> = keyed.iter().map(|(_, i)| *i).collect();

        let mut nodes = Vec::new();
        build_range(&tris, &tri_order, 0, tri_order.len(), &mut nodes);
        Self {
            nodes,
            tri_order,
            tris,
        }
    }

    /// Total node count (1 for an empty BVH).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Nearest front-face hit of the ray against the BVH, or `None`.
    pub fn traverse(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        if self.tri_order.is_empty() {
            return None;
        }
        let inv_dir = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
        let mut best: Option<Hit> = None;
        let mut stack: Vec<u32> = vec![0];
        while let Some(ni) = stack.pop() {
            let node = &self.nodes[ni as usize];
            let tmax = best.map(|h| h.t).unwrap_or(f32::INFINITY);
            if !node.aabb.hit(origin, inv_dir, 0.0, tmax) {
                continue;
            }
            if node.count > 0 {
                for k in node.start..node.start + node.count {
                    let tri = &self.tris[self.tri_order[k as usize] as usize];
                    if let Some(t) = tri.intersect(origin, dir) {
                        if best.map_or(true, |h| t < h.t) {
                            best = Some(Hit { t, tri_id: tri.id });
                        }
                    }
                }
            } else {
                stack.push(node.left);
                stack.push(node.right);
            }
        }
        best
    }
}

/// Recursively build nodes for `tri_order[lo..hi]`; returns the node index.
fn build_range(tris: &[Tri], order: &[u32], lo: usize, hi: usize, nodes: &mut Vec<Node>) -> u32 {
    let mut aabb = Aabb::empty();
    for &i in &order[lo..hi] {
        aabb.union(&tris[i as usize].aabb());
    }
    let idx = nodes.len() as u32;
    nodes.push(Node {
        aabb,
        left: 0,
        right: 0,
        start: lo as u32,
        count: 0,
    });

    let count = hi - lo;
    if count <= LEAF_MAX {
        nodes[idx as usize].count = count as u32;
        return idx;
    }
    let mid = lo + count / 2; // median in Morton order
    let left = build_range(tris, order, lo, mid, nodes);
    let right = build_range(tris, order, mid, hi, nodes);
    nodes[idx as usize].left = left;
    nodes[idx as usize].right = right;
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_grid(n: i32) -> Vec<Tri> {
        // n×n unit quads on the z=0 plane, two triangles each, spaced by 1 on x/y.
        let mut tris = Vec::new();
        let mut id = 0u32;
        for gy in 0..n {
            for gx in 0..n {
                let x = gx as f32;
                let y = gy as f32;
                let a = Vec3::new(x, y, 0.0);
                let b = Vec3::new(x + 0.9, y, 0.0);
                let c = Vec3::new(x, y + 0.9, 0.0);
                let d = Vec3::new(x + 0.9, y + 0.9, 0.0);
                tris.push(Tri {
                    v0: a,
                    v1: b,
                    v2: c,
                    id,
                });
                id += 1;
                tris.push(Tri {
                    v0: b,
                    v1: d,
                    v2: c,
                    id,
                });
                id += 1;
            }
        }
        tris
    }

    #[test]
    fn empty_bvh_is_safe() {
        let bvh = Bvh::build(vec![]);
        assert_eq!(bvh.node_count(), 1);
        assert!(bvh.traverse(Vec3::ZERO, -Vec3::Z).is_none());
    }

    #[test]
    fn hits_a_single_triangle() {
        let tris = vec![Tri {
            v0: Vec3::new(-1.0, -1.0, -5.0),
            v1: Vec3::new(1.0, -1.0, -5.0),
            v2: Vec3::new(0.0, 1.0, -5.0),
            id: 42,
        }];
        let bvh = Bvh::build(tris);
        let hit = bvh
            .traverse(Vec3::new(0.0, 0.0, 0.0), -Vec3::Z)
            .expect("should hit");
        assert_eq!(hit.tri_id, 42);
        assert!((hit.t - 5.0).abs() < 1e-4);
    }

    #[test]
    fn misses_when_off_axis() {
        let tris = vec![Tri {
            v0: Vec3::new(-1.0, -1.0, -5.0),
            v1: Vec3::new(1.0, -1.0, -5.0),
            v2: Vec3::new(0.0, 1.0, -5.0),
            id: 0,
        }];
        let bvh = Bvh::build(tris);
        assert!(bvh.traverse(Vec3::new(50.0, 50.0, 0.0), -Vec3::Z).is_none());
    }

    #[test]
    fn bvh_matches_brute_force_over_a_grid() {
        let tris = quad_grid(8); // 128 triangles
        let bvh = Bvh::build(tris.clone());
        // Internal nodes must exist (it actually built a tree, not one big leaf).
        assert!(bvh.node_count() > 1);

        // Cast rays straight down at many cells; BVH nearest must equal brute force.
        for gy in 0..8 {
            for gx in 0..8 {
                let origin = Vec3::new(gx as f32 + 0.3, gy as f32 + 0.3, 5.0);
                let dir = -Vec3::Z;
                let bvh_hit = bvh.traverse(origin, dir);

                let mut brute: Option<Hit> = None;
                for t in &tris {
                    if let Some(d) = t.intersect(origin, dir) {
                        if brute.map_or(true, |h| d < h.t) {
                            brute = Some(Hit { t: d, tri_id: t.id });
                        }
                    }
                }
                match (bvh_hit, brute) {
                    (Some(a), Some(b)) => {
                        assert!((a.t - b.t).abs() < 1e-4, "t mismatch at ({gx},{gy})");
                        assert_eq!(a.tri_id, b.tri_id, "id mismatch at ({gx},{gy})");
                    }
                    (None, None) => {}
                    _ => panic!("BVH/brute-force disagree at ({gx},{gy})"),
                }
            }
        }
    }

    #[test]
    fn morton_is_monotone_on_a_diagonal() {
        let a = morton3(Vec3::new(0.1, 0.1, 0.1));
        let b = morton3(Vec3::new(0.5, 0.5, 0.5));
        let c = morton3(Vec3::new(0.9, 0.9, 0.9));
        assert!(a < b && b < c);
    }
}
