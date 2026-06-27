//! GPU-uploadable layout for the LBVH — the bytes the WGSL software-BVH compute
//! traversal (`kami-render::raytrace`) binds as storage buffers. `#[repr(C)]`
//! Pod structs match the WGSL `BvhNode` / `Tri` declarations field-for-field
//! (std430, 16-byte aligned rows). Packing is pure + unit-tested; the actual
//! buffer upload + dispatch lives in kami-render (needs a `wgpu::Device`).

use crate::bvh::{Bvh, Node, Tri};
use bytemuck::{Pod, Zeroable};

/// 48-byte BVH node. Leaf when `count > 0` (`start` indexes the triangle list);
/// internal otherwise (`left`/`right` index the node list).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuNode {
    pub min: [f32; 3],
    pub left: u32,
    pub max: [f32; 3],
    pub right: u32,
    pub start: u32,
    pub count: u32,
    pub _pad: [u32; 2],
}

/// 48-byte triangle (positions + stable id), in Morton-sorted traversal order.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuTri {
    pub v0: [f32; 3],
    pub id: u32,
    pub v1: [f32; 3],
    pub _p1: u32,
    pub v2: [f32; 3],
    pub _p2: u32,
}

impl From<&Node> for GpuNode {
    fn from(n: &Node) -> Self {
        GpuNode {
            min: n.aabb.min.to_array(),
            left: n.left,
            max: n.aabb.max.to_array(),
            right: n.right,
            start: n.start,
            count: n.count,
            _pad: [0; 2],
        }
    }
}

impl GpuTri {
    fn from_tri(t: &Tri) -> Self {
        GpuTri {
            v0: t.v0.to_array(),
            id: t.id,
            v1: t.v1.to_array(),
            _p1: 0,
            v2: t.v2.to_array(),
            _p2: 0,
        }
    }
}

impl Bvh {
    /// Pack the BVH into GPU buffers: the node array (root at 0) and the
    /// triangle array already in Morton-sorted traversal order, so the WGSL
    /// leaf range `[start, start+count)` indexes straight into it.
    pub fn to_gpu(&self) -> (Vec<GpuNode>, Vec<GpuTri>) {
        let nodes = self.nodes.iter().map(GpuNode::from).collect();
        let tris = self
            .tri_order
            .iter()
            .map(|&i| GpuTri::from_tri(&self.tris[i as usize]))
            .collect();
        (nodes, tris)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bvh::Tri;
    use glam::Vec3;

    #[test]
    fn gpu_structs_are_48_bytes() {
        assert_eq!(std::mem::size_of::<GpuNode>(), 48);
        assert_eq!(std::mem::size_of::<GpuTri>(), 48);
    }

    #[test]
    fn pack_preserves_traversal_order_and_ids() {
        let tris = (0..5)
            .map(|i| {
                let x = i as f32;
                Tri {
                    v0: Vec3::new(x, 0.0, 0.0),
                    v1: Vec3::new(x + 0.5, 0.0, 0.0),
                    v2: Vec3::new(x, 0.5, 0.0),
                    id: i,
                }
            })
            .collect::<Vec<_>>();
        let bvh = Bvh::build(tris);
        let (nodes, gtris) = bvh.to_gpu();

        assert_eq!(nodes.len(), bvh.nodes.len());
        assert_eq!(gtris.len(), bvh.tri_order.len());
        // GPU triangle k corresponds to the k-th entry of the Morton order.
        for (k, gt) in gtris.iter().enumerate() {
            let src = &bvh.tris[bvh.tri_order[k] as usize];
            assert_eq!(gt.id, src.id);
            assert_eq!(gt.v0, src.v0.to_array());
        }
        // Every leaf's [start,start+count) lies within the triangle array.
        for n in &nodes {
            if n.count > 0 {
                assert!((n.start + n.count) as usize <= gtris.len());
            }
        }
    }
}
