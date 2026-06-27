//! Voxel-to-mesh conversion: greedy meshing and naive meshing.

use crate::voxel::{BlockType, CHUNK_SIZE, VoxelChunk};

/// Meshed output ready for GPU upload.
pub struct VoxelMesh {
    /// Interleaved: [pos3, norm3, uv2, color4] = 12 floats per vertex.
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub vertex_count: u32,
    pub index_count: u32,
}

impl VoxelMesh {
    /// Bake a world-space offset into all vertex positions (Minecraft-style).
    ///
    /// Shifts every vertex position by `offset` so that vertices are in world
    /// coordinates. This eliminates floating-point seams at chunk boundaries
    /// that occur when using per-chunk instance transforms — integer chunk
    /// offsets are added to small integer local coords, keeping full f32
    /// precision instead of relying on matrix multiplication.
    pub fn offset_positions(&mut self, offset: [f32; 3]) {
        // Vertex layout: [pos.x, pos.y, pos.z, norm.x, norm.y, norm.z, u, v, r, g, b, a]
        //                  0      1      2      3       4       5       6  7  8  9  10 11
        const STRIDE: usize = 12;
        let len = self.vertices.len();
        let mut i = 0;
        while i + 2 < len {
            self.vertices[i] += offset[0];
            self.vertices[i + 1] += offset[1];
            self.vertices[i + 2] += offset[2];
            i += STRIDE;
        }
    }
}

/// Face direction for voxel meshing.
#[derive(Clone, Copy)]
enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Face {
    fn normal(self) -> [f32; 3] {
        match self {
            Face::PosX => [1.0, 0.0, 0.0],
            Face::NegX => [-1.0, 0.0, 0.0],
            Face::PosY => [0.0, 1.0, 0.0],
            Face::NegY => [0.0, -1.0, 0.0],
            Face::PosZ => [0.0, 0.0, 1.0],
            Face::NegZ => [0.0, 0.0, -1.0],
        }
    }
}

/// Naive meshing: one quad per exposed face. Simple but more vertices.
pub fn naive_mesh(chunk: &VoxelChunk, palette: &[[f32; 4]]) -> VoxelMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z);
                if !block.is_solid() {
                    continue;
                }
                let color = palette.get(block as usize).copied().unwrap_or([1.0; 4]);
                let fx = x as f32;
                let fy = y as f32;
                let fz = z as f32;

                // Check each face: emit quad if neighbor is air/transparent
                let neighbors = [
                    (
                        Face::PosX,
                        if x + 1 < CHUNK_SIZE {
                            chunk.get(x + 1, y, z)
                        } else {
                            BlockType::Air
                        },
                    ),
                    (
                        Face::NegX,
                        if x > 0 {
                            chunk.get(x - 1, y, z)
                        } else {
                            BlockType::Air
                        },
                    ),
                    (
                        Face::PosY,
                        if y + 1 < CHUNK_SIZE {
                            chunk.get(x, y + 1, z)
                        } else {
                            BlockType::Air
                        },
                    ),
                    (
                        Face::NegY,
                        if y > 0 {
                            chunk.get(x, y - 1, z)
                        } else {
                            BlockType::Air
                        },
                    ),
                    (
                        Face::PosZ,
                        if z + 1 < CHUNK_SIZE {
                            chunk.get(x, y, z + 1)
                        } else {
                            BlockType::Air
                        },
                    ),
                    (
                        Face::NegZ,
                        if z > 0 {
                            chunk.get(x, y, z - 1)
                        } else {
                            BlockType::Air
                        },
                    ),
                ];

                for (face, neighbor) in &neighbors {
                    if neighbor.is_solid() && !neighbor.is_transparent() {
                        continue;
                    }
                    emit_face(&mut vertices, &mut indices, fx, fy, fz, *face, color);
                }
            }
        }
    }

    let vertex_count = vertices.len() as u32 / 12;
    let index_count = indices.len() as u32;
    VoxelMesh {
        vertices,
        indices,
        vertex_count,
        index_count,
    }
}

/// Neighbor boundary data for chunk-aware greedy meshing.
/// Each face stores a 16×16 slice of block types from the adjacent chunk's boundary.
/// `None` means no neighbor (chunk edge = Air).
pub struct ChunkNeighbors {
    /// Neighbor at +X (chunk boundary x=15 → neighbor x=0)
    pub pos_x: Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]>,
    /// Neighbor at -X
    pub neg_x: Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]>,
    /// Neighbor at +Y
    pub pos_y: Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]>,
    /// Neighbor at -Y
    pub neg_y: Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]>,
    /// Neighbor at +Z
    pub pos_z: Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]>,
    /// Neighbor at -Z
    pub neg_z: Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]>,
}

impl Default for ChunkNeighbors {
    fn default() -> Self {
        Self {
            pos_x: None,
            neg_x: None,
            pos_y: None,
            neg_y: None,
            pos_z: None,
            neg_z: None,
        }
    }
}

/// Greedy meshing: merge coplanar adjacent faces of the same block type
/// into larger quads. Reduces vertex count by 80-95% for typical terrain.
pub fn greedy_mesh(chunk: &VoxelChunk, palette: &[[f32; 4]]) -> VoxelMesh {
    greedy_mesh_with_neighbors(chunk, palette, &ChunkNeighbors::default())
}

/// Greedy meshing with neighbor-aware boundary culling.
/// Faces at chunk boundaries are only generated if the neighbor block is Air/transparent.
pub fn greedy_mesh_with_neighbors(
    chunk: &VoxelChunk,
    palette: &[[f32; 4]],
    neighbors: &ChunkNeighbors,
) -> VoxelMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let s = CHUNK_SIZE;

    // Process each of 6 face directions
    for axis in 0..3 {
        let (u_axis, v_axis) = match axis {
            0 => (1, 2), // X-facing: sweep Y, Z
            1 => (0, 2), // Y-facing: sweep X, Z
            _ => (0, 1), // Z-facing: sweep X, Y
        };

        for positive in [true, false] {
            for d in 0..s {
                // Build mask: which blocks have an exposed face at this slice?
                let mut mask = [[BlockType::Air; CHUNK_SIZE]; CHUNK_SIZE];

                for v in 0..s {
                    for u in 0..s {
                        let mut pos = [0usize; 3];
                        pos[axis] = d;
                        pos[u_axis] = u;
                        pos[v_axis] = v;

                        let block = chunk.get(pos[0], pos[1], pos[2]);
                        if !block.is_solid() {
                            continue;
                        }

                        // Check neighbor — use adjacent chunk boundary data if available
                        let nd = if positive { d + 1 } else { d.wrapping_sub(1) };
                        let neighbor = if nd < s {
                            let mut npos = pos;
                            npos[axis] = nd;
                            chunk.get(npos[0], npos[1], npos[2])
                        } else {
                            // Chunk boundary — look up neighbor data
                            let nb_slice = match (axis, positive) {
                                (0, true) => &neighbors.pos_x,
                                (0, false) => &neighbors.neg_x,
                                (1, true) => &neighbors.pos_y,
                                (1, false) => &neighbors.neg_y,
                                (_, true) => &neighbors.pos_z,
                                (_, false) => &neighbors.neg_z,
                            };
                            if let Some(slice) = nb_slice {
                                // Neighbor slice is indexed by [u_axis][v_axis]
                                let idx = v * CHUNK_SIZE + u;
                                slice[idx]
                            } else {
                                BlockType::Air // no neighbor = exposed
                            }
                        };

                        if !neighbor.is_solid() || neighbor.is_transparent() {
                            mask[v][u] = block;
                        }
                    }
                }

                // Greedy merge: find maximal rectangles of same block type
                let mut visited = [[false; CHUNK_SIZE]; CHUNK_SIZE];
                for v in 0..s {
                    for u in 0..s {
                        if visited[v][u] || mask[v][u] == BlockType::Air {
                            continue;
                        }
                        let block = mask[v][u];

                        // Expand width
                        let mut w = 1;
                        while u + w < s && mask[v][u + w] == block && !visited[v][u + w] {
                            w += 1;
                        }

                        // Expand height
                        let mut h = 1;
                        'outer: while v + h < s {
                            for du in 0..w {
                                if mask[v + h][u + du] != block || visited[v + h][u + du] {
                                    break 'outer;
                                }
                            }
                            h += 1;
                        }

                        // Mark visited
                        for dv in 0..h {
                            for du in 0..w {
                                visited[v + dv][u + du] = true;
                            }
                        }

                        // Emit quad
                        let color = palette.get(block as usize).copied().unwrap_or([1.0; 4]);
                        let face = match (axis, positive) {
                            (0, true) => Face::PosX,
                            (0, false) => Face::NegX,
                            (1, true) => Face::PosY,
                            (1, false) => Face::NegY,
                            (_, true) => Face::PosZ,
                            (_, false) => Face::NegZ,
                        };

                        let mut origin = [0.0f32; 3];
                        origin[axis] = if positive { d as f32 + 1.0 } else { d as f32 };
                        origin[u_axis] = u as f32;
                        origin[v_axis] = v as f32;

                        let mut du_vec = [0.0f32; 3];
                        du_vec[u_axis] = w as f32;
                        let mut dv_vec = [0.0f32; 3];
                        dv_vec[v_axis] = h as f32;

                        emit_quad(
                            &mut vertices,
                            &mut indices,
                            origin,
                            du_vec,
                            dv_vec,
                            face.normal(),
                            color,
                            positive,
                        );
                    }
                }
            }
        }
    }

    let vertex_count = vertices.len() as u32 / 12;
    let index_count = indices.len() as u32;
    VoxelMesh {
        vertices,
        indices,
        vertex_count,
        index_count,
    }
}

/// Emit a single face quad (4 vertices, 6 indices) at position with given normal and color.
fn emit_face(
    vertices: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    x: f32,
    y: f32,
    z: f32,
    face: Face,
    color: [f32; 4],
) {
    let base = vertices.len() as u32 / 12;
    let n = face.normal();
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let corners: [[f32; 3]; 4] = match face {
        Face::PosX => [
            [x + 1.0, y, z],
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z],
        ],
        Face::NegX => [
            [x, y, z + 1.0],
            [x, y, z],
            [x, y + 1.0, z],
            [x, y + 1.0, z + 1.0],
        ],
        Face::PosY => [
            [x, y + 1.0, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y + 1.0, z + 1.0],
            [x, y + 1.0, z + 1.0],
        ],
        Face::NegY => [
            [x, y, z + 1.0],
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y, z],
            [x, y, z],
        ],
        Face::PosZ => [
            [x, y, z + 1.0],
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x, y + 1.0, z + 1.0],
        ],
        Face::NegZ => [
            [x + 1.0, y, z],
            [x, y, z],
            [x, y + 1.0, z],
            [x + 1.0, y + 1.0, z],
        ],
    };

    for i in 0..4 {
        vertices.extend_from_slice(&corners[i]);
        vertices.extend_from_slice(&n);
        vertices.extend_from_slice(&uv[i]);
        vertices.extend_from_slice(&color);
    }

    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Emit a greedy-merged quad at arbitrary origin with du/dv extents.
fn emit_quad(
    vertices: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    origin: [f32; 3],
    du: [f32; 3],
    dv: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    positive: bool,
) {
    let base = vertices.len() as u32 / 12;

    let p0 = origin;
    let p1 = [origin[0] + du[0], origin[1] + du[1], origin[2] + du[2]];
    let p2 = [
        origin[0] + du[0] + dv[0],
        origin[1] + du[1] + dv[1],
        origin[2] + du[2] + dv[2],
    ];
    let p3 = [origin[0] + dv[0], origin[1] + dv[1], origin[2] + dv[2]];

    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let corners = if positive {
        [p0, p1, p2, p3]
    } else {
        [p1, p0, p3, p2]
    };

    for i in 0..4 {
        vertices.extend_from_slice(&corners[i]);
        vertices.extend_from_slice(&normal);
        vertices.extend_from_slice(&uv[i]);
        vertices.extend_from_slice(&color);
    }

    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Down-sample a chunk by `factor` (2 or 4) using majority-vote of each sub-block.
/// Returns (flat block data for reduced grid, reduced grid size).
fn downsample_majority(chunk: &VoxelChunk, factor: usize) -> (Vec<u8>, usize) {
    let size = CHUNK_SIZE / factor;
    let mut data = vec![0u8; size * size * size];
    for gy in 0..size {
        for gz in 0..size {
            for gx in 0..size {
                // Count occurrences of each block type in the sub-block.
                let mut counts = [0u32; 16];
                for dy in 0..factor {
                    for dz in 0..factor {
                        for dx in 0..factor {
                            let b = chunk.get(gx * factor + dx, gy * factor + dy, gz * factor + dz);
                            if b.is_solid() {
                                counts[b as usize] += 1;
                            }
                        }
                    }
                }
                // Majority vote: pick the solid type with most occurrences.
                let mut best = 0u8;
                let mut best_count = 0u32;
                for (i, &c) in counts.iter().enumerate() {
                    if c > best_count {
                        best_count = c;
                        best = i as u8;
                    }
                }
                let idx = gy * size * size + gz * size + gx;
                data[idx] = best;
            }
        }
    }
    (data, size)
}

/// Greedy mesh a reduced grid (size < CHUNK_SIZE) packed into a flat array,
/// then scale all vertex positions by `scale`.
fn greedy_mesh_reduced(data: &[u8], size: usize, palette: &[[f32; 4]], scale: f32) -> VoxelMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for axis in 0..3 {
        let (u_axis, v_axis) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };

        for positive in [true, false] {
            for d in 0..size {
                let mut mask = vec![vec![BlockType::Air; size]; size];

                for v in 0..size {
                    for u in 0..size {
                        let mut pos = [0usize; 3];
                        pos[axis] = d;
                        pos[u_axis] = u;
                        pos[v_axis] = v;

                        let idx = pos[1] * size * size + pos[2] * size + pos[0];
                        let block = BlockType::from_u8(data[idx]);
                        if !block.is_solid() {
                            continue;
                        }

                        let nd = if positive { d + 1 } else { d.wrapping_sub(1) };
                        let neighbor = if nd < size {
                            let mut npos = pos;
                            npos[axis] = nd;
                            let ni = npos[1] * size * size + npos[2] * size + npos[0];
                            BlockType::from_u8(data[ni])
                        } else {
                            BlockType::Air
                        };

                        if !neighbor.is_solid() || neighbor.is_transparent() {
                            mask[v][u] = block;
                        }
                    }
                }

                let mut visited = vec![vec![false; size]; size];
                for v in 0..size {
                    for u in 0..size {
                        if visited[v][u] || mask[v][u] == BlockType::Air {
                            continue;
                        }
                        let block = mask[v][u];

                        let mut w = 1;
                        while u + w < size && mask[v][u + w] == block && !visited[v][u + w] {
                            w += 1;
                        }

                        let mut h = 1;
                        'outer: while v + h < size {
                            for du in 0..w {
                                if mask[v + h][u + du] != block || visited[v + h][u + du] {
                                    break 'outer;
                                }
                            }
                            h += 1;
                        }

                        for dv in 0..h {
                            for du in 0..w {
                                visited[v + dv][u + du] = true;
                            }
                        }

                        let color = palette.get(block as usize).copied().unwrap_or([1.0; 4]);
                        let face = match (axis, positive) {
                            (0, true) => Face::PosX,
                            (0, false) => Face::NegX,
                            (1, true) => Face::PosY,
                            (1, false) => Face::NegY,
                            (_, true) => Face::PosZ,
                            (_, false) => Face::NegZ,
                        };

                        let mut origin = [0.0f32; 3];
                        origin[axis] = if positive {
                            (d as f32 + 1.0) * scale
                        } else {
                            d as f32 * scale
                        };
                        origin[u_axis] = u as f32 * scale;
                        origin[v_axis] = v as f32 * scale;

                        let mut du_vec = [0.0f32; 3];
                        du_vec[u_axis] = w as f32 * scale;
                        let mut dv_vec = [0.0f32; 3];
                        dv_vec[v_axis] = h as f32 * scale;

                        emit_quad(
                            &mut vertices,
                            &mut indices,
                            origin,
                            du_vec,
                            dv_vec,
                            face.normal(),
                            color,
                            positive,
                        );
                    }
                }
            }
        }
    }

    let vertex_count = vertices.len() as u32 / 12;
    let index_count = indices.len() as u32;
    VoxelMesh {
        vertices,
        indices,
        vertex_count,
        index_count,
    }
}

/// Generate a single-cube mesh (LOD 3) with the dominant color from the chunk.
fn single_cube_mesh(chunk: &VoxelChunk, palette: &[[f32; 4]]) -> VoxelMesh {
    // Find dominant block type by counting.
    let mut counts = [0u32; 16];
    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let b = chunk.get(x, y, z);
                if b.is_solid() {
                    counts[b as usize] += 1;
                }
            }
        }
    }
    let mut best = 0u8;
    let mut best_count = 0u32;
    for (i, &c) in counts.iter().enumerate() {
        if c > best_count {
            best_count = c;
            best = i as u8;
        }
    }
    if best_count == 0 {
        return VoxelMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            vertex_count: 0,
            index_count: 0,
        };
    }
    let color = palette.get(best as usize).copied().unwrap_or([1.0; 4]);
    let s = CHUNK_SIZE as f32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // 6 faces of a cube from (0,0,0) to (s,s,s).
    let faces: [(Face, [f32; 3], [f32; 3], [f32; 3]); 6] = [
        (Face::PosX, [s, 0.0, 0.0], [0.0, 0.0, s], [0.0, s, 0.0]),
        (Face::NegX, [0.0, 0.0, s], [0.0, 0.0, -s], [0.0, s, 0.0]),
        (Face::PosY, [0.0, s, 0.0], [s, 0.0, 0.0], [0.0, 0.0, s]),
        (Face::NegY, [0.0, 0.0, s], [s, 0.0, 0.0], [0.0, 0.0, -s]),
        (Face::PosZ, [0.0, 0.0, s], [s, 0.0, 0.0], [0.0, s, 0.0]),
        (Face::NegZ, [s, 0.0, 0.0], [-s, 0.0, 0.0], [0.0, s, 0.0]),
    ];

    for (face, origin, du, dv) in &faces {
        emit_quad(
            &mut vertices,
            &mut indices,
            *origin,
            *du,
            *dv,
            face.normal(),
            color,
            matches!(face, Face::PosX | Face::PosY | Face::PosZ),
        );
    }

    let vertex_count = vertices.len() as u32 / 12;
    let index_count = indices.len() as u32;
    VoxelMesh {
        vertices,
        indices,
        vertex_count,
        index_count,
    }
}

/// Level-of-detail mesh generation for voxel chunks.
///
/// - **LOD 0**: Full greedy mesh (delegates to `greedy_mesh`).
/// - **LOD 1**: Down-sample 2x2x2 blocks via majority-vote, greedy mesh at half resolution, scale x2.
/// - **LOD 2**: Down-sample 4x4x4, greedy mesh at quarter resolution, scale x4.
/// - **LOD 3**: Single cube with dominant color (24 vertices).
pub fn lod_mesh(chunk: &VoxelChunk, palette: &[[f32; 4]], lod_level: u32) -> VoxelMesh {
    match lod_level {
        0 => greedy_mesh(chunk, palette),
        1 => {
            let (data, size) = downsample_majority(chunk, 2);
            greedy_mesh_reduced(&data, size, palette, 2.0)
        }
        2 => {
            let (data, size) = downsample_majority(chunk, 4);
            greedy_mesh_reduced(&data, size, palette, 4.0)
        }
        _ => single_cube_mesh(chunk, palette),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::{VoxelChunk, default_palette};

    #[test]
    fn naive_mesh_single_block() {
        let mut chunk = VoxelChunk::new();
        chunk.set(0, 0, 0, BlockType::Stone);
        let palette = default_palette();
        let mesh = naive_mesh(&chunk, &palette);
        // Single block exposed on all 6 sides → 6 quads × 4 verts = 24 verts
        assert_eq!(mesh.vertex_count, 24);
        assert_eq!(mesh.index_count, 36);
    }

    #[test]
    fn naive_mesh_empty() {
        let chunk = VoxelChunk::new();
        let palette = default_palette();
        let mesh = naive_mesh(&chunk, &palette);
        assert_eq!(mesh.vertex_count, 0);
        assert_eq!(mesh.index_count, 0);
    }

    #[test]
    fn greedy_solid_chunk_6_quads() {
        // A solid single-type chunk should produce exactly 6 large quads (one per face)
        let chunk = VoxelChunk::solid(BlockType::Dirt);
        let palette = default_palette();
        let mesh = greedy_mesh(&chunk, &palette);
        assert_eq!(mesh.vertex_count, 24, "6 faces × 4 verts = 24");
        assert_eq!(mesh.index_count, 36, "6 faces × 6 indices = 36");
    }

    #[test]
    fn greedy_single_block() {
        let mut chunk = VoxelChunk::new();
        chunk.set(8, 8, 8, BlockType::Stone);
        let palette = default_palette();
        let mesh = greedy_mesh(&chunk, &palette);
        assert_eq!(mesh.vertex_count, 24); // 6 faces × 4 verts
        assert_eq!(mesh.index_count, 36);
    }

    #[test]
    fn greedy_fewer_verts_than_naive() {
        // Two adjacent blocks: greedy should merge some faces
        let mut chunk = VoxelChunk::new();
        chunk.set(0, 0, 0, BlockType::Stone);
        chunk.set(1, 0, 0, BlockType::Stone);
        let palette = default_palette();
        let naive = naive_mesh(&chunk, &palette);
        let greedy = greedy_mesh(&chunk, &palette);
        // Naive: 2 blocks, each has some exposed faces (shared face hidden)
        // Greedy: merged faces should have fewer or equal vertices
        assert!(
            greedy.vertex_count <= naive.vertex_count,
            "greedy {} should be <= naive {}",
            greedy.vertex_count,
            naive.vertex_count
        );
    }

    #[test]
    fn vertex_stride() {
        let mut chunk = VoxelChunk::new();
        chunk.set(0, 0, 0, BlockType::Grass);
        let palette = default_palette();
        let mesh = naive_mesh(&chunk, &palette);
        // 12 floats per vertex: pos(3) + norm(3) + uv(2) + color(4)
        assert_eq!(mesh.vertices.len(), mesh.vertex_count as usize * 12);
    }

    #[test]
    fn lod0_equals_greedy() {
        let chunk = VoxelChunk::solid(BlockType::Stone);
        let palette = default_palette();
        let lod0 = lod_mesh(&chunk, &palette, 0);
        let greedy = greedy_mesh(&chunk, &palette);
        assert_eq!(lod0.vertex_count, greedy.vertex_count);
        assert_eq!(lod0.index_count, greedy.index_count);
    }

    #[test]
    fn lod1_fewer_verts_than_lod0() {
        let chunk = VoxelChunk::solid(BlockType::Dirt);
        let palette = default_palette();
        let lod0 = lod_mesh(&chunk, &palette, 0);
        let lod1 = lod_mesh(&chunk, &palette, 1);
        assert!(
            lod1.vertex_count <= lod0.vertex_count,
            "LOD1 {} should be <= LOD0 {}",
            lod1.vertex_count,
            lod0.vertex_count
        );
        assert!(lod1.vertex_count > 0);
    }

    #[test]
    fn lod2_fewer_verts_than_lod1() {
        let chunk = VoxelChunk::solid(BlockType::Dirt);
        let palette = default_palette();
        let lod1 = lod_mesh(&chunk, &palette, 1);
        let lod2 = lod_mesh(&chunk, &palette, 2);
        assert!(
            lod2.vertex_count <= lod1.vertex_count,
            "LOD2 {} should be <= LOD1 {}",
            lod2.vertex_count,
            lod1.vertex_count
        );
        assert!(lod2.vertex_count > 0);
    }

    #[test]
    fn lod3_single_cube() {
        let chunk = VoxelChunk::solid(BlockType::Stone);
        let palette = default_palette();
        let lod3 = lod_mesh(&chunk, &palette, 3);
        assert_eq!(lod3.vertex_count, 24, "LOD3 = 6 faces × 4 verts");
        assert_eq!(lod3.index_count, 36);
    }

    #[test]
    fn lod3_empty_chunk() {
        let chunk = VoxelChunk::new();
        let palette = default_palette();
        let lod3 = lod_mesh(&chunk, &palette, 3);
        assert_eq!(lod3.vertex_count, 0);
        assert_eq!(lod3.index_count, 0);
    }

    #[test]
    fn downsample_majority_vote() {
        let mut chunk = VoxelChunk::new();
        // Fill a 2x2x2 region mostly with Stone (7 stone, 1 dirt).
        for y in 0..2 {
            for z in 0..2 {
                for x in 0..2 {
                    chunk.set(x, y, z, BlockType::Stone);
                }
            }
        }
        chunk.set(0, 0, 0, BlockType::Dirt);
        let (data, size) = downsample_majority(&chunk, 2);
        assert_eq!(size, 8);
        // The first cell should be Stone (majority).
        assert_eq!(BlockType::from_u8(data[0]), BlockType::Stone);
    }

    #[test]
    fn neighbor_culling_solid_neighbor_removes_boundary_face() {
        // Two adjacent solid chunks: shared face should be culled.
        let chunk_a = VoxelChunk::solid(BlockType::Dirt);
        let palette = default_palette();

        // Without neighbors: 6 faces (all exposed)
        let mesh_no_nb = greedy_mesh(&chunk_a, &palette);
        assert_eq!(mesh_no_nb.vertex_count, 24, "6 faces without neighbors");

        // With solid +X neighbor: -X face of neighbor = solid → chunk_a's +X face is hidden
        let mut nb = ChunkNeighbors::default();
        nb.pos_x = Some([BlockType::Dirt; CHUNK_SIZE * CHUNK_SIZE]);
        let mesh_with_nb = greedy_mesh_with_neighbors(&chunk_a, &palette, &nb);
        assert_eq!(
            mesh_with_nb.vertex_count, 20,
            "5 faces — +X face culled by solid neighbor"
        );
    }

    #[test]
    fn neighbor_culling_all_solid_neighbors_only_leaves_no_faces() {
        // Solid chunk fully surrounded by solid neighbors — no exposed faces
        let chunk = VoxelChunk::solid(BlockType::Stone);
        let palette = default_palette();
        let solid = [BlockType::Stone; CHUNK_SIZE * CHUNK_SIZE];
        let nb = ChunkNeighbors {
            pos_x: Some(solid),
            neg_x: Some(solid),
            pos_y: Some(solid),
            neg_y: Some(solid),
            pos_z: Some(solid),
            neg_z: Some(solid),
        };
        let mesh = greedy_mesh_with_neighbors(&chunk, &palette, &nb);
        assert_eq!(mesh.vertex_count, 0, "fully enclosed = 0 faces");
    }

    #[test]
    fn neighbor_culling_air_neighbor_keeps_face() {
        let chunk = VoxelChunk::solid(BlockType::Grass);
        let palette = default_palette();
        let mut nb = ChunkNeighbors::default();
        // Air neighbor at +X — face should be kept
        nb.pos_x = Some([BlockType::Air; CHUNK_SIZE * CHUNK_SIZE]);
        // Solid everywhere else
        let solid = [BlockType::Stone; CHUNK_SIZE * CHUNK_SIZE];
        nb.neg_x = Some(solid);
        nb.pos_y = Some(solid);
        nb.neg_y = Some(solid);
        nb.pos_z = Some(solid);
        nb.neg_z = Some(solid);
        let mesh = greedy_mesh_with_neighbors(&chunk, &palette, &nb);
        assert_eq!(
            mesh.vertex_count, 4,
            "only +X face exposed (1 quad = 4 verts)"
        );
    }

    #[test]
    fn offset_positions_shifts_all_vertices() {
        let mut chunk = VoxelChunk::new();
        chunk.set(0, 0, 0, BlockType::Stone);
        let palette = default_palette();
        let mut mesh = greedy_mesh(&chunk, &palette);

        // Record original first vertex position
        let orig_x = mesh.vertices[0];
        let orig_y = mesh.vertices[1];
        let orig_z = mesh.vertices[2];

        mesh.offset_positions([16.0, 32.0, 48.0]);

        // First vertex should be shifted by the offset
        assert_eq!(mesh.vertices[0], orig_x + 16.0);
        assert_eq!(mesh.vertices[1], orig_y + 32.0);
        assert_eq!(mesh.vertices[2], orig_z + 48.0);

        // Normals (stride offset 3..5) must be unchanged
        let norm_x = mesh.vertices[3];
        let norm_y = mesh.vertices[4];
        let norm_z = mesh.vertices[5];
        let norm_mag = (norm_x * norm_x + norm_y * norm_y + norm_z * norm_z).sqrt();
        assert!(
            (norm_mag - 1.0).abs() < 1e-5,
            "normal should remain unit length"
        );
    }

    #[test]
    fn offset_preserves_vertex_count() {
        let chunk = VoxelChunk::solid(BlockType::Dirt);
        let palette = default_palette();
        let mut mesh = greedy_mesh(&chunk, &palette);
        let vc = mesh.vertex_count;
        let ic = mesh.index_count;
        mesh.offset_positions([100.0, 200.0, 300.0]);
        assert_eq!(mesh.vertex_count, vc);
        assert_eq!(mesh.index_count, ic);
        assert_eq!(mesh.vertices.len(), vc as usize * 12);
    }

    #[test]
    fn chunk_boundary_vertices_align_with_offset() {
        // Two adjacent chunks: chunk A at [0,0,0], chunk B at [1,0,0].
        // After offset, chunk A's +X boundary (x=16) should exactly match
        // chunk B's -X boundary (x=0 + offset 16 = 16). This verifies that
        // integer offset addition produces identical f32 values (no gap).
        let chunk_a = VoxelChunk::solid(BlockType::Stone);
        let chunk_b = VoxelChunk::solid(BlockType::Stone);
        let palette = default_palette();

        // Mesh with air neighbors at +X/-X boundary so faces are generated
        let air = [BlockType::Air; CHUNK_SIZE * CHUNK_SIZE];
        let solid = [BlockType::Stone; CHUNK_SIZE * CHUNK_SIZE];

        let mut nb_a = ChunkNeighbors::default();
        nb_a.pos_x = Some(air); // expose +X face
        nb_a.neg_x = Some(solid);
        nb_a.pos_y = Some(solid);
        nb_a.neg_y = Some(solid);
        nb_a.pos_z = Some(solid);
        nb_a.neg_z = Some(solid);

        let mut nb_b = ChunkNeighbors::default();
        nb_b.neg_x = Some(air); // expose -X face
        nb_b.pos_x = Some(solid);
        nb_b.pos_y = Some(solid);
        nb_b.neg_y = Some(solid);
        nb_b.pos_z = Some(solid);
        nb_b.neg_z = Some(solid);

        let mut mesh_a = greedy_mesh_with_neighbors(&chunk_a, &palette, &nb_a);
        let mut mesh_b = greedy_mesh_with_neighbors(&chunk_b, &palette, &nb_b);

        // Apply world-space offsets (integer * CHUNK_SIZE)
        mesh_a.offset_positions([0.0, 0.0, 0.0]); // chunk [0,0,0]
        mesh_b.offset_positions([16.0, 0.0, 0.0]); // chunk [1,0,0]

        // Collect all X coordinates from each mesh
        let xs_a: Vec<f32> = mesh_a.vertices.chunks(12).map(|v| v[0]).collect();
        let xs_b: Vec<f32> = mesh_b.vertices.chunks(12).map(|v| v[0]).collect();

        // Chunk A's +X face should have x=16.0 (local 16 + offset 0)
        let max_a = xs_a.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        // Chunk B's -X face should have x=16.0 (local 0 + offset 16)
        let min_b = xs_b.iter().cloned().fold(f32::INFINITY, f32::min);

        assert_eq!(max_a, 16.0, "chunk A +X boundary should be at x=16.0");
        assert_eq!(min_b, 16.0, "chunk B -X boundary should be at x=16.0");
        // Exact bit equality — no floating point gap
        assert_eq!(
            max_a.to_bits(),
            min_b.to_bits(),
            "boundary vertices must be bit-identical: A={} B={}",
            max_a,
            min_b
        );
    }

    /// Extract per-vertex color (RGBA) from vertex at given index.
    fn vertex_color(mesh: &VoxelMesh, vertex_idx: usize) -> [f32; 4] {
        let base = vertex_idx * 12;
        [
            mesh.vertices[base + 8],
            mesh.vertices[base + 9],
            mesh.vertices[base + 10],
            mesh.vertices[base + 11],
        ]
    }

    /// Extract per-vertex normal from vertex at given index.
    fn vertex_normal(mesh: &VoxelMesh, vertex_idx: usize) -> [f32; 3] {
        let base = vertex_idx * 12;
        [
            mesh.vertices[base + 3],
            mesh.vertices[base + 4],
            mesh.vertices[base + 5],
        ]
    }

    /// Extract per-vertex position from vertex at given index.
    fn vertex_pos(mesh: &VoxelMesh, vertex_idx: usize) -> [f32; 3] {
        let base = vertex_idx * 12;
        [
            mesh.vertices[base],
            mesh.vertices[base + 1],
            mesh.vertices[base + 2],
        ]
    }

    /// All 15 solid block types (Air excluded) produce a valid single-block
    /// mesh with 6 faces, correct palette color, and unit-length normals.
    #[test]
    fn all_block_types_single_block_geometry() {
        let palette = default_palette();
        let solid_types = [
            BlockType::Dirt,
            BlockType::Grass,
            BlockType::Stone,
            BlockType::Water,
            BlockType::Sand,
            BlockType::Wood,
            BlockType::Leaf,
            BlockType::Ore,
            BlockType::Brick,
            BlockType::Glass,
            BlockType::Metal,
            BlockType::Snow,
            BlockType::Lava,
            BlockType::Ice,
            BlockType::Gravel,
        ];

        for block in &solid_types {
            let mut chunk = VoxelChunk::new();
            chunk.set(7, 7, 7, *block);
            let mesh = naive_mesh(&chunk, &palette);

            // Single block: 6 faces × 4 verts = 24, 6 faces × 6 idx = 36
            assert_eq!(
                mesh.vertex_count, 24,
                "block {:?}: expected 24 verts, got {}",
                block, mesh.vertex_count
            );
            assert_eq!(
                mesh.index_count, 36,
                "block {:?}: expected 36 indices, got {}",
                block, mesh.index_count
            );

            // Verify color matches palette
            let expected_color = palette[*block as usize];
            let actual_color = vertex_color(&mesh, 0);
            assert_eq!(
                actual_color, expected_color,
                "block {:?}: color mismatch — expected {:?}, got {:?}",
                block, expected_color, actual_color
            );

            // All vertices should have same color (single block type)
            for v in 0..24 {
                assert_eq!(
                    vertex_color(&mesh, v),
                    expected_color,
                    "block {:?}: vertex {} has wrong color",
                    block,
                    v
                );
            }

            // All normals should be unit length
            for v in 0..24 {
                let n = vertex_normal(&mesh, v);
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!(
                    (len - 1.0).abs() < 1e-5,
                    "block {:?}: vertex {} normal not unit: {:?} (len={})",
                    block,
                    v,
                    n,
                    len
                );
            }
        }
    }

    /// Verify all 6 face normals are present for a single block.
    #[test]
    fn single_block_has_all_six_face_normals() {
        let mut chunk = VoxelChunk::new();
        chunk.set(5, 5, 5, BlockType::Stone);
        let palette = default_palette();
        let mesh = naive_mesh(&chunk, &palette);

        // Collect unique normals (4 verts per face share a normal)
        let mut normals = std::collections::HashSet::new();
        for v in 0..mesh.vertex_count as usize {
            let n = vertex_normal(&mesh, v);
            // Round to avoid float comparison issues
            let key = (
                (n[0] * 10.0) as i32,
                (n[1] * 10.0) as i32,
                (n[2] * 10.0) as i32,
            );
            normals.insert(key);
        }

        assert_eq!(
            normals.len(),
            6,
            "single block should have 6 unique face normals, got {:?}",
            normals
        );
        // Verify specific directions
        assert!(normals.contains(&(10, 0, 0)), "missing +X normal");
        assert!(normals.contains(&(-10, 0, 0)), "missing -X normal");
        assert!(normals.contains(&(0, 10, 0)), "missing +Y normal");
        assert!(normals.contains(&(0, -10, 0)), "missing -Y normal");
        assert!(normals.contains(&(0, 0, 10)), "missing +Z normal");
        assert!(normals.contains(&(0, 0, -10)), "missing -Z normal");
    }

    /// Verify block positions: each face's vertices are within [x, x+1] × [y, y+1] × [z, z+1].
    #[test]
    fn single_block_vertex_positions_within_unit_cube() {
        let mut chunk = VoxelChunk::new();
        chunk.set(3, 4, 5, BlockType::Brick);
        let palette = default_palette();
        let mesh = naive_mesh(&chunk, &palette);

        for v in 0..mesh.vertex_count as usize {
            let p = vertex_pos(&mesh, v);
            assert!(
                p[0] >= 3.0 && p[0] <= 4.0,
                "vertex {} x={} not in [3,4]",
                v,
                p[0]
            );
            assert!(
                p[1] >= 4.0 && p[1] <= 5.0,
                "vertex {} y={} not in [4,5]",
                v,
                p[1]
            );
            assert!(
                p[2] >= 5.0 && p[2] <= 6.0,
                "vertex {} z={} not in [5,6]",
                v,
                p[2]
            );
        }
    }

    /// Transparent blocks allow opaque neighbors to keep their shared face,
    /// but opaque blocks cull transparent blocks' shared face (can't see through opaque).
    #[test]
    fn transparent_block_culling_asymmetry() {
        let palette = default_palette();

        // Place Stone at (0,0,0) and Water at (1,0,0)
        let mut chunk = VoxelChunk::new();
        chunk.set(0, 0, 0, BlockType::Stone);
        chunk.set(1, 0, 0, BlockType::Water);
        let mesh = naive_mesh(&chunk, &palette);

        // Stone keeps +X face (Water is transparent → visible through Water)
        // Water loses -X face (Stone is opaque → can't see Water's face behind Stone)
        // Stone: 6 faces = 24 verts, Water: 5 faces = 20 verts → 44 total
        assert_eq!(
            mesh.vertex_count, 44,
            "stone(6)+water(5): got {} verts",
            mesh.vertex_count
        );
    }

    /// Two adjacent transparent blocks both keep their shared faces.
    #[test]
    fn two_transparent_blocks_keep_shared_faces() {
        let palette = default_palette();
        let mut chunk = VoxelChunk::new();
        chunk.set(0, 0, 0, BlockType::Water);
        chunk.set(1, 0, 0, BlockType::Glass);
        let mesh = naive_mesh(&chunk, &palette);

        // Both transparent: each keeps all 6 faces (neither culls the other)
        assert_eq!(
            mesh.vertex_count, 48,
            "water(6)+glass(6): got {} verts",
            mesh.vertex_count
        );
    }

    /// Greedy mesh preserves color per block type — mixed blocks don't merge.
    #[test]
    fn greedy_mesh_separates_different_block_colors() {
        let palette = default_palette();
        let mut chunk = VoxelChunk::new();
        // Two adjacent blocks of different types
        chunk.set(0, 0, 0, BlockType::Grass);
        chunk.set(1, 0, 0, BlockType::Stone);
        let mesh = greedy_mesh(&chunk, &palette);

        // Collect all unique colors in the mesh
        let mut colors = std::collections::HashSet::new();
        for v in 0..mesh.vertex_count as usize {
            let c = vertex_color(&mesh, v);
            colors.insert((
                (c[0] * 1000.0) as i32,
                (c[1] * 1000.0) as i32,
                (c[2] * 1000.0) as i32,
                (c[3] * 1000.0) as i32,
            ));
        }

        assert_eq!(
            colors.len(),
            2,
            "two different block types should produce 2 distinct colors, got {}",
            colors.len()
        );
    }

    /// Air blocks produce zero geometry.
    #[test]
    fn air_block_produces_no_mesh() {
        let mut chunk = VoxelChunk::new();
        // Only air — no blocks set
        let palette = default_palette();
        let mesh_naive = naive_mesh(&chunk, &palette);
        let mesh_greedy = greedy_mesh(&chunk, &palette);
        assert_eq!(mesh_naive.vertex_count, 0);
        assert_eq!(mesh_greedy.vertex_count, 0);
    }

    #[test]
    fn far_chunk_boundary_precision() {
        // Verify precision at distant chunks (e.g., chunk [100,0,0]).
        // With instance transforms, 100*16=1600.0 + local 16.0 via mat4
        // could lose precision. With offset_positions, 1600.0 + 16.0 = 1616.0
        // is exact in f32 (both are exact integers).
        let chunk = VoxelChunk::solid(BlockType::Stone);
        let palette = default_palette();
        let air = [BlockType::Air; CHUNK_SIZE * CHUNK_SIZE];
        let solid = [BlockType::Stone; CHUNK_SIZE * CHUNK_SIZE];

        // Chunk A at [99,0,0], expose +X
        let mut nb_a = ChunkNeighbors {
            pos_x: Some(air),
            neg_x: Some(solid),
            pos_y: Some(solid),
            neg_y: Some(solid),
            pos_z: Some(solid),
            neg_z: Some(solid),
        };
        // Chunk B at [100,0,0], expose -X
        let nb_b = ChunkNeighbors {
            neg_x: Some(air),
            pos_x: Some(solid),
            pos_y: Some(solid),
            neg_y: Some(solid),
            pos_z: Some(solid),
            neg_z: Some(solid),
        };

        let mut mesh_a = greedy_mesh_with_neighbors(&chunk, &palette, &nb_a);
        let mut mesh_b = greedy_mesh_with_neighbors(&chunk, &palette, &nb_b);

        mesh_a.offset_positions([99.0 * 16.0, 0.0, 0.0]); // 1584.0
        mesh_b.offset_positions([100.0 * 16.0, 0.0, 0.0]); // 1600.0

        let max_a: f32 = mesh_a
            .vertices
            .chunks(12)
            .map(|v| v[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_b: f32 = mesh_b
            .vertices
            .chunks(12)
            .map(|v| v[0])
            .fold(f32::INFINITY, f32::min);

        // 1584 + 16 = 1600, 1600 + 0 = 1600 — both exact in f32
        assert_eq!(max_a, 1600.0);
        assert_eq!(min_b, 1600.0);
        assert_eq!(
            max_a.to_bits(),
            min_b.to_bits(),
            "far boundary must be bit-identical"
        );
    }
}
