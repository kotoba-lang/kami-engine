//! TerrainChunk: mesh generation from heightmap + splatmap.
//!
//! Generates triangle mesh with per-vertex normals and material weights.
//! LOD support via stride (skip vertices for coarser mesh).

use crate::heightmap::Heightmap;
use crate::splatmap::Splatmap;
use bytemuck::{Pod, Zeroable};

/// Terrain vertex: position (3) + normal (3) + uv (2) + splat weights (4) = 12 floats.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub splat: [f32; 4],
}

/// Generated terrain chunk ready for GPU upload.
pub struct TerrainChunk {
    pub vertices: Vec<TerrainVertex>,
    pub indices: Vec<u32>,
    /// World-space origin of this chunk.
    pub origin: [f32; 3],
    /// LOD level (0 = full detail).
    pub lod: u32,
}

/// Generate triangle mesh for a terrain chunk.
///
/// `stride`: vertex skip for LOD (1 = full, 2 = half, 4 = quarter).
/// `scale`: world-space distance between adjacent vertices at LOD 0.
pub fn generate_chunk_mesh(
    hm: &Heightmap,
    splat: &Splatmap,
    origin_x: f32,
    origin_z: f32,
    stride: u32,
    scale: f32,
    lod: u32,
) -> TerrainChunk {
    let step = stride.max(1);
    let cols = (hm.width - 1) / step;
    let rows = (hm.depth - 1) / step;

    let mut vertices = Vec::with_capacity(((cols + 1) * (rows + 1)) as usize);
    let mut indices = Vec::with_capacity((cols * rows * 6) as usize);

    // Generate vertices
    for row in 0..=rows {
        for col in 0..=cols {
            let hx = (col * step).min(hm.width - 1);
            let hz = (row * step).min(hm.depth - 1);
            let idx = (hz * hm.width + hx) as usize;

            let h = hm.data[idx];
            let n = hm.normal(hx, hz);
            let sw = splat.data[idx];

            let wx = origin_x + col as f32 * step as f32 * scale;
            let wz = origin_z + row as f32 * step as f32 * scale;

            vertices.push(TerrainVertex {
                position: [wx, h, wz],
                normal: n,
                uv: [col as f32 / cols as f32, row as f32 / rows as f32],
                splat: sw.weights,
            });
        }
    }

    // Generate indices (two triangles per quad)
    let row_verts = cols + 1;
    for row in 0..rows {
        for col in 0..cols {
            let tl = row * row_verts + col;
            let tr = tl + 1;
            let bl = (row + 1) * row_verts + col;
            let br = bl + 1;

            // Triangle 1: tl, bl, tr
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);

            // Triangle 2: tr, bl, br
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    TerrainChunk {
        vertices,
        indices,
        origin: [origin_x, 0.0, origin_z],
        lod,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightmap::{Heightmap, HeightmapConfig};
    use crate::splatmap::Splatmap;

    #[test]
    fn chunk_mesh_lod0() {
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(33, 33, 0.0, 0.0, &cfg);
        let splat = Splatmap::from_heightmap(&hm, 10.0, 90.0, 0.4);
        let chunk = generate_chunk_mesh(&hm, &splat, 0.0, 0.0, 1, 1.0, 0);
        assert_eq!(chunk.vertices.len(), 33 * 33);
        assert_eq!(chunk.indices.len(), 32 * 32 * 6);
    }

    #[test]
    fn chunk_mesh_lod2() {
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(33, 33, 0.0, 0.0, &cfg);
        let splat = Splatmap::from_heightmap(&hm, 10.0, 90.0, 0.4);
        let chunk = generate_chunk_mesh(&hm, &splat, 0.0, 0.0, 2, 1.0, 2);
        // stride=2 → 16+1=17 per axis
        assert_eq!(chunk.vertices.len(), 17 * 17);
        assert_eq!(chunk.indices.len(), 16 * 16 * 6);
    }
}
