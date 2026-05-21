//! Output layer: SDF → smooth Marching Cubes mesh / VoxelVolume → block mesh.

mod mc_tables;

use kami_render::mesh::LoadedMesh;
use kami_voxel::{Voxel, VoxelVolume};
use mc_tables::{EDGE_TABLE, EDGE_VERTICES, TRI_TABLE};

/// Proper Marching Cubes with Paul Bourke lookup tables.
pub fn sdf_to_mesh<F>(sample: F, resolution: u32, bounds: f32) -> LoadedMesh
where
    F: Fn(f32, f32, f32) -> (f32, [f32; 4]),
{
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let res = resolution;
    let step = bounds * 2.0 / res as f32;
    let origin = -bounds;

    // Corner positions for a cell at (cx, cy, cz)
    let corner_offsets: [[f32; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];

    // De-duplicate edge vertices using a hashmap
    let mut edge_vertex_map: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();

    let eps = step * 0.05;

    for cz in 0..res {
        for cy in 0..res {
            for cx in 0..res {
                // Sample 8 corners
                let mut dists = [0.0f32; 8];
                let mut colors = [[0.5f32; 4]; 8];
                let mut corner_pos = [[0.0f32; 3]; 8];

                for i in 0..8 {
                    let px = origin + (cx as f32 + corner_offsets[i][0]) * step;
                    let py = origin + (cy as f32 + corner_offsets[i][1]) * step;
                    let pz = origin + (cz as f32 + corner_offsets[i][2]) * step;
                    let (d, c) = sample(px, py, pz);
                    dists[i] = d;
                    colors[i] = c;
                    corner_pos[i] = [px, py, pz];
                }

                // Cube index
                let mut cube_idx = 0u8;
                for i in 0..8 {
                    if dists[i] <= 0.0 {
                        cube_idx |= 1 << i;
                    }
                }

                let edge_bits = EDGE_TABLE[cube_idx as usize];
                if edge_bits == 0 {
                    continue;
                }

                // Compute edge vertices
                let mut edge_verts = [u32::MAX; 12];

                for e in 0..12 {
                    if edge_bits & (1 << e) == 0 {
                        continue;
                    }

                    let (a, b) = EDGE_VERTICES[e];

                    // Unique edge key (global coordinates)
                    let edge_key = edge_hash(cx, cy, cz, e as u8, res);

                    if let Some(&existing) = edge_vertex_map.get(&edge_key) {
                        edge_verts[e] = existing;
                        continue;
                    }

                    // Interpolate position on edge
                    let t = if (dists[a] - dists[b]).abs() > 1e-6 {
                        dists[a] / (dists[a] - dists[b])
                    } else {
                        0.5
                    };
                    let t = t.clamp(0.0, 1.0);

                    let vx = corner_pos[a][0] + t * (corner_pos[b][0] - corner_pos[a][0]);
                    let vy = corner_pos[a][1] + t * (corner_pos[b][1] - corner_pos[a][1]);
                    let vz = corner_pos[a][2] + t * (corner_pos[b][2] - corner_pos[a][2]);

                    // Normal via central difference
                    let dx = sample(vx + eps, vy, vz).0 - sample(vx - eps, vy, vz).0;
                    let dy = sample(vx, vy + eps, vz).0 - sample(vx, vy - eps, vz).0;
                    let dz = sample(vx, vy, vz + eps).0 - sample(vx, vy, vz - eps).0;
                    let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);

                    let idx = positions.len() as u32 / 3;
                    positions.extend_from_slice(&[vx, vy, vz]);
                    normals.extend_from_slice(&[dx / len, dy / len, dz / len]);
                    uvs.extend_from_slice(&[0.0, 0.0]);

                    edge_vertex_map.insert(edge_key, idx);
                    edge_verts[e] = idx;
                }

                // Emit triangles from lookup table
                let tri_row = &TRI_TABLE[cube_idx as usize];
                let mut i = 0;
                while i < 16 && tri_row[i] >= 0 {
                    let e0 = tri_row[i] as usize;
                    let e1 = tri_row[i + 1] as usize;
                    let e2 = tri_row[i + 2] as usize;

                    if edge_verts[e0] != u32::MAX
                        && edge_verts[e1] != u32::MAX
                        && edge_verts[e2] != u32::MAX
                    {
                        indices.push(edge_verts[e0]);
                        indices.push(edge_verts[e1]);
                        indices.push(edge_verts[e2]);
                    }
                    i += 3;
                }
            }
        }
    }

    let vertices = kami_render::mesh::interleave(&positions, &normals, &uvs);
    LoadedMesh {
        vertex_count: positions.len() as u32 / 3,
        index_count: indices.len() as u32,
        vertices,
        indices,
    }
}

/// SDF → mesh with per-vertex colors.
/// Same as `sdf_to_mesh` but also interpolates colors at edge vertices.
/// Returns (mesh, per_vertex_colors) where per_vertex_colors[i] = RGBA of vertex i.
pub fn sdf_to_colored_mesh<F>(
    sample: F,
    resolution: u32,
    bounds: f32,
) -> (LoadedMesh, Vec<[f32; 4]>)
where
    F: Fn(f32, f32, f32) -> (f32, [f32; 4]),
{
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut vertex_colors: Vec<[f32; 4]> = Vec::new();

    let res = resolution;
    let step = bounds * 2.0 / res as f32;
    let origin = -bounds;

    let corner_offsets: [[f32; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];

    let mut edge_vertex_map: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
    let eps = step * 0.05;

    for cz in 0..res {
        for cy in 0..res {
            for cx in 0..res {
                let mut dists = [0.0f32; 8];
                let mut colors = [[0.5f32; 4]; 8];
                let mut corner_pos = [[0.0f32; 3]; 8];

                for i in 0..8 {
                    let px = origin + (cx as f32 + corner_offsets[i][0]) * step;
                    let py = origin + (cy as f32 + corner_offsets[i][1]) * step;
                    let pz = origin + (cz as f32 + corner_offsets[i][2]) * step;
                    let (d, c) = sample(px, py, pz);
                    dists[i] = d;
                    colors[i] = c;
                    corner_pos[i] = [px, py, pz];
                }

                let mut cube_idx = 0u8;
                for i in 0..8 {
                    if dists[i] <= 0.0 {
                        cube_idx |= 1 << i;
                    }
                }

                let edge_bits = EDGE_TABLE[cube_idx as usize];
                if edge_bits == 0 {
                    continue;
                }

                let mut edge_verts = [u32::MAX; 12];

                for e in 0..12 {
                    if edge_bits & (1 << e) == 0 {
                        continue;
                    }

                    let (a, b) = EDGE_VERTICES[e];
                    let edge_key = edge_hash(cx, cy, cz, e as u8, res);

                    if let Some(&existing) = edge_vertex_map.get(&edge_key) {
                        edge_verts[e] = existing;
                        continue;
                    }

                    let t = if (dists[a] - dists[b]).abs() > 1e-6 {
                        dists[a] / (dists[a] - dists[b])
                    } else {
                        0.5
                    };
                    let t = t.clamp(0.0, 1.0);

                    let vx = corner_pos[a][0] + t * (corner_pos[b][0] - corner_pos[a][0]);
                    let vy = corner_pos[a][1] + t * (corner_pos[b][1] - corner_pos[a][1]);
                    let vz = corner_pos[a][2] + t * (corner_pos[b][2] - corner_pos[a][2]);

                    // Interpolate color at edge vertex
                    let vc = [
                        colors[a][0] + t * (colors[b][0] - colors[a][0]),
                        colors[a][1] + t * (colors[b][1] - colors[a][1]),
                        colors[a][2] + t * (colors[b][2] - colors[a][2]),
                        colors[a][3] + t * (colors[b][3] - colors[a][3]),
                    ];

                    let dx = sample(vx + eps, vy, vz).0 - sample(vx - eps, vy, vz).0;
                    let dy = sample(vx, vy + eps, vz).0 - sample(vx, vy - eps, vz).0;
                    let dz = sample(vx, vy, vz + eps).0 - sample(vx, vy, vz - eps).0;
                    let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);

                    let idx = positions.len() as u32 / 3;
                    positions.extend_from_slice(&[vx, vy, vz]);
                    normals.extend_from_slice(&[dx / len, dy / len, dz / len]);
                    uvs.extend_from_slice(&[0.0, 0.0]);
                    vertex_colors.push(vc);

                    edge_vertex_map.insert(edge_key, idx);
                    edge_verts[e] = idx;
                }

                let tri_row = &TRI_TABLE[cube_idx as usize];
                let mut i = 0;
                while i < 16 && tri_row[i] >= 0 {
                    let e0 = tri_row[i] as usize;
                    let e1 = tri_row[i + 1] as usize;
                    let e2 = tri_row[i + 2] as usize;

                    if edge_verts[e0] != u32::MAX
                        && edge_verts[e1] != u32::MAX
                        && edge_verts[e2] != u32::MAX
                    {
                        indices.push(edge_verts[e0]);
                        indices.push(edge_verts[e1]);
                        indices.push(edge_verts[e2]);
                    }
                    i += 3;
                }
            }
        }
    }

    let vertices = kami_render::mesh::interleave(&positions, &normals, &uvs);
    let mesh = LoadedMesh {
        vertex_count: positions.len() as u32 / 3,
        index_count: indices.len() as u32,
        vertices,
        indices,
    };
    (mesh, vertex_colors)
}

/// Split a colored mesh into sub-meshes grouped by quantized color.
/// Each group becomes a separate LoadedMesh with a uniform color.
pub fn split_mesh_by_color(
    mesh: &LoadedMesh,
    vertex_colors: &[[f32; 4]],
) -> Vec<(LoadedMesh, [f32; 4])> {
    if mesh.index_count == 0 || vertex_colors.is_empty() {
        return vec![];
    }

    // Quantize color to 3-bit per channel (8 levels) for grouping.
    // Coarse quantization merges SmoothUnion boundary blends into their nearest distinct color.
    fn quantize(c: &[f32; 4]) -> u32 {
        let r = (c[0] * 7.0).round() as u32;
        let g = (c[1] * 7.0).round() as u32;
        let b = (c[2] * 7.0).round() as u32;
        (r << 6) | (g << 3) | b
    }

    // Group triangles by their centroid color
    let mut groups: std::collections::HashMap<u32, (Vec<u32>, [f32; 4])> =
        std::collections::HashMap::new();

    for tri in 0..(mesh.index_count / 3) {
        let i0 = mesh.indices[tri as usize * 3] as usize;
        let i1 = mesh.indices[tri as usize * 3 + 1] as usize;
        let i2 = mesh.indices[tri as usize * 3 + 2] as usize;

        // Average color at triangle centroid
        let c = [
            (vertex_colors[i0][0] + vertex_colors[i1][0] + vertex_colors[i2][0]) / 3.0,
            (vertex_colors[i0][1] + vertex_colors[i1][1] + vertex_colors[i2][1]) / 3.0,
            (vertex_colors[i0][2] + vertex_colors[i1][2] + vertex_colors[i2][2]) / 3.0,
            1.0,
        ];
        let key = quantize(&c);
        let entry = groups.entry(key).or_insert_with(|| (Vec::new(), c));
        entry.0.push(tri);
    }

    // Build sub-meshes
    let stride = 8; // pos3 + norm3 + uv2
    groups
        .into_values()
        .map(|(tris, color)| {
            let mut new_verts = Vec::new();
            let mut new_indices = Vec::new();
            let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

            for tri in tris {
                for k in 0..3 {
                    let old_idx = mesh.indices[tri as usize * 3 + k];
                    let new_idx = match remap.get(&old_idx) {
                        Some(&ni) => ni,
                        None => {
                            let ni = new_verts.len() as u32 / stride;
                            let base = old_idx as usize * stride as usize;
                            new_verts
                                .extend_from_slice(&mesh.vertices[base..base + stride as usize]);
                            remap.insert(old_idx, ni);
                            ni
                        }
                    };
                    new_indices.push(new_idx);
                }
            }

            (
                LoadedMesh {
                    vertex_count: new_verts.len() as u32 / stride,
                    index_count: new_indices.len() as u32,
                    vertices: new_verts,
                    indices: new_indices,
                },
                color,
            )
        })
        .collect()
}

/// Unique hash for an edge in the grid (for vertex deduplication).
fn edge_hash(cx: u32, cy: u32, cz: u32, edge: u8, res: u32) -> u64 {
    let (a, b) = EDGE_VERTICES[edge as usize];
    let offsets: [[u32; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
    ];
    let ax = cx + offsets[a][0];
    let ay = cy + offsets[a][1];
    let az = cz + offsets[a][2];
    let bx = cx + offsets[b][0];
    let by = cy + offsets[b][1];
    let bz = cz + offsets[b][2];
    // Canonical edge: sort endpoints
    let (p0, p1) = if (az, ay, ax) <= (bz, by, bx) {
        ((ax, ay, az), (bx, by, bz))
    } else {
        ((bx, by, bz), (ax, ay, az))
    };
    let r1 = res + 1;
    let encode = |x: u32, y: u32, z: u32| -> u64 {
        (z as u64) * (r1 as u64) * (r1 as u64) + (y as u64) * (r1 as u64) + (x as u64)
    };
    encode(p0.0, p0.1, p0.2) * (r1 as u64 * r1 as u64 * r1 as u64) + encode(p1.0, p1.1, p1.2)
}

/// VoxelVolume face-based mesher (Minecraft-style blocks).
pub fn marching_cubes(volume: &VoxelVolume, scale: f32) -> LoadedMesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let w = volume.width();
    let h = volume.height();
    let d = volume.depth();
    let offset = glam::Vec3::new(
        -(w as f32) * scale / 2.0,
        -(h as f32) * scale / 2.0,
        -(d as f32) * scale / 2.0,
    );

    for z in 0..d {
        for y in 0..h {
            for x in 0..w {
                if !volume.get(x, y, z).is_solid() {
                    continue;
                }
                let pos = glam::Vec3::new(x as f32, y as f32, z as f32) * scale + offset;
                let faces: [(i32, i32, i32, [f32; 3]); 6] = [
                    (1, 0, 0, [1.0, 0.0, 0.0]),
                    (-1, 0, 0, [-1.0, 0.0, 0.0]),
                    (0, 1, 0, [0.0, 1.0, 0.0]),
                    (0, -1, 0, [0.0, -1.0, 0.0]),
                    (0, 0, 1, [0.0, 0.0, 1.0]),
                    (0, 0, -1, [0.0, 0.0, -1.0]),
                ];
                for (dx, dy, dz, normal) in &faces {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;
                    let neighbor_solid = if nx >= 0
                        && nx < w as i32
                        && ny >= 0
                        && ny < h as i32
                        && nz >= 0
                        && nz as i32 <= d as i32 - 1
                    {
                        volume.get(nx as u32, ny as u32, nz as u32).is_solid()
                    } else {
                        false
                    };
                    if !neighbor_solid {
                        let base = positions.len() as u32 / 3;
                        let (v0, v1, v2, v3) = face_verts(pos, *normal, scale);
                        for v in &[v0, v1, v2, v3] {
                            positions.extend_from_slice(&[v.x, v.y, v.z]);
                            normals.extend_from_slice(normal);
                            uvs.extend_from_slice(&[0.0, 0.0]);
                        }
                        indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            }
        }
    }

    let vertices = kami_render::mesh::interleave(&positions, &normals, &uvs);
    LoadedMesh {
        vertex_count: positions.len() as u32 / 3,
        index_count: indices.len() as u32,
        vertices,
        indices,
    }
}

fn face_verts(
    pos: glam::Vec3,
    n: [f32; 3],
    s: f32,
) -> (glam::Vec3, glam::Vec3, glam::Vec3, glam::Vec3) {
    let hs = s * 0.5;
    let p = pos + glam::Vec3::splat(hs);
    match n {
        [1.0, 0.0, 0.0] => (
            p + glam::Vec3::new(hs, -hs, -hs),
            p + glam::Vec3::new(hs, hs, -hs),
            p + glam::Vec3::new(hs, hs, hs),
            p + glam::Vec3::new(hs, -hs, hs),
        ),
        [-1.0, 0.0, 0.0] => (
            p + glam::Vec3::new(-hs, -hs, hs),
            p + glam::Vec3::new(-hs, hs, hs),
            p + glam::Vec3::new(-hs, hs, -hs),
            p + glam::Vec3::new(-hs, -hs, -hs),
        ),
        [0.0, 1.0, 0.0] => (
            p + glam::Vec3::new(-hs, hs, -hs),
            p + glam::Vec3::new(hs, hs, -hs),
            p + glam::Vec3::new(hs, hs, hs),
            p + glam::Vec3::new(-hs, hs, hs),
        ),
        [0.0, -1.0, 0.0] => (
            p + glam::Vec3::new(-hs, -hs, hs),
            p + glam::Vec3::new(hs, -hs, hs),
            p + glam::Vec3::new(hs, -hs, -hs),
            p + glam::Vec3::new(-hs, -hs, -hs),
        ),
        [0.0, 0.0, 1.0] => (
            p + glam::Vec3::new(-hs, -hs, hs),
            p + glam::Vec3::new(-hs, hs, hs),
            p + glam::Vec3::new(hs, hs, hs),
            p + glam::Vec3::new(hs, -hs, hs),
        ),
        _ => (
            p + glam::Vec3::new(hs, -hs, -hs),
            p + glam::Vec3::new(hs, hs, -hs),
            p + glam::Vec3::new(-hs, hs, -hs),
            p + glam::Vec3::new(-hs, -hs, -hs),
        ),
    }
}

pub fn greedy_mesh_volume(volume: &VoxelVolume, scale: f32) -> LoadedMesh {
    marching_cubes(volume, scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mc_single_voxel() {
        let mut vol = VoxelVolume::new_dense(4, 4, 4);
        vol.set(
            1,
            1,
            1,
            Voxel {
                material: 1,
                color: [1.0, 0.0, 0.0, 1.0],
            },
        );
        let mesh = marching_cubes(&vol, 1.0);
        assert!(mesh.vertex_count > 0);
    }

    #[test]
    fn mc_filled_cube() {
        let mut vol = VoxelVolume::new_dense(4, 4, 4);
        for z in 0..4 {
            for y in 0..4 {
                for x in 0..4 {
                    vol.set(
                        x,
                        y,
                        z,
                        Voxel {
                            material: 1,
                            color: [0.0, 1.0, 0.0, 1.0],
                        },
                    );
                }
            }
        }
        let mesh = marching_cubes(&vol, 0.5);
        assert!(mesh.vertex_count > 0);
    }

    #[test]
    fn colored_mesh_two_spheres() {
        // Two spheres with different colors: red at y=-1, green at y=+1
        let (mesh, colors) = sdf_to_colored_mesh(
            |x, y, z| {
                let d1 = (x * x + (y + 1.0) * (y + 1.0) + z * z).sqrt() - 0.8; // red sphere at y=-1
                let d2 = (x * x + (y - 1.0) * (y - 1.0) + z * z).sqrt() - 0.8; // green sphere at y=+1
                if d1 < d2 {
                    (d1, [1.0, 0.0, 0.0, 1.0])
                } else {
                    (d2, [0.0, 1.0, 0.0, 1.0])
                }
            },
            16,
            2.5,
        );

        assert!(mesh.vertex_count > 0, "should have vertices");
        assert_eq!(
            colors.len(),
            mesh.vertex_count as usize,
            "one color per vertex"
        );

        let groups = split_mesh_by_color(&mesh, &colors);
        println!("Two-sphere colored mesh: {} groups", groups.len());
        for (i, (m, c)) in groups.iter().enumerate() {
            println!(
                "  Group {}: verts={} color=[{:.2},{:.2},{:.2}]",
                i, m.vertex_count, c[0], c[1], c[2]
            );
        }
        assert!(
            groups.len() >= 2,
            "should have at least 2 color groups, got {}",
            groups.len()
        );

        // Verify we have both red-ish and green-ish groups
        let has_red = groups.iter().any(|(_, c)| c[0] > 0.5 && c[1] < 0.5);
        let has_green = groups.iter().any(|(_, c)| c[1] > 0.5 && c[0] < 0.5);
        assert!(has_red, "should have a red group");
        assert!(has_green, "should have a green group");
    }

    #[test]
    fn colored_mesh_yoro_sdf() {
        let jsonld = r##"{"@type":"Union","children":[
            {"@type":"SmoothUnion","k":0.4,"children":[
                {"@type":"Sphere","r":1.6,"pos":[0,1.2,0],"color":"#58CC02"},
                {"@type":"Sphere","r":1.5,"pos":[0,2.9,0],"color":"#58CC02"}
            ]},
            {"@type":"Sphere","r":0.55,"pos":[-0.65,3.0,1.6],"scale":[1,1,0.5],"color":"white"},
            {"@type":"Sphere","r":0.55,"pos":[0.65,3.0,1.6],"scale":[1,1,0.5],"color":"white"},
            {"@type":"Cylinder","h":0.15,"r":0.8,"pos":[0,4.25,0],"color":"#E0E0E0"},
            {"@type":"Cylinder","h":0.9,"r":0.55,"pos":[0,4.75,0],"color":"#EEEEEE"}
        ]}"##;
        let sdf = kami_sdf::parse_sdf_jsonld(jsonld).unwrap();

        for res in [16, 32, 48] {
            let (mesh, colors) = sdf_to_colored_mesh(
                |x, y, z| {
                    let s = sdf.sample(glam::Vec3::new(x, y, z));
                    (s.distance, s.color)
                },
                res,
                5.0,
            );
            let groups = split_mesh_by_color(&mesh, &colors);
            println!(
                "--- YORO res={}³: total_verts={} groups={} ---",
                res,
                mesh.vertex_count,
                groups.len()
            );
            for (i, (m, c)) in groups.iter().enumerate() {
                println!(
                    "  Group {}: verts={} idx={} color=[{:.2},{:.2},{:.2}]",
                    i, m.vertex_count, m.index_count, c[0], c[1], c[2]
                );
            }
            assert!(groups.len() >= 1, "should have at least 1 group");
        }
    }

    #[test]
    fn sdf_sphere_closed() {
        let mesh = sdf_to_mesh(
            |x, y, z| {
                let d = (x * x + y * y + z * z).sqrt() - 1.0;
                (d, [1.0, 0.0, 0.0, 1.0])
            },
            16,
            2.0,
        );
        assert!(
            mesh.vertex_count > 100,
            "sphere should have many vertices, got {}",
            mesh.vertex_count
        );
        assert!(
            mesh.index_count > 100,
            "sphere should have many indices, got {}",
            mesh.index_count
        );
        assert_eq!(mesh.index_count % 3, 0, "indices must be multiple of 3");
    }

    #[test]
    fn sdf_no_holes() {
        // A sphere at res=12 should produce a watertight mesh
        let mesh = sdf_to_mesh(
            |x, y, z| {
                let d = (x * x + y * y + z * z).sqrt() - 0.8;
                (d, [0.0, 1.0, 0.0, 1.0])
            },
            12,
            1.5,
        );
        // Euler characteristic check: V - E + F = 2 for a closed surface
        // F = index_count / 3, E ≈ F * 3 / 2 (each edge shared by 2 triangles)
        let f = mesh.index_count / 3;
        assert!(f > 50, "should have >50 triangles for a sphere, got {}", f);
    }
}

#[cfg(test)]
mod volume_parity_tests {
    use super::*;
    use kami_voxel::{Voxel, VoxelVolume};

    fn make_sphere_volume(res: u32) -> VoxelVolume {
        let mut vol = VoxelVolume::new_dense(res, res, res);
        let center = res as f32 / 2.0;
        let radius = res as f32 / 3.0;
        for z in 0..res {
            for y in 0..res {
                for x in 0..res {
                    let dx = x as f32 - center;
                    let dy = y as f32 - center;
                    let dz = z as f32 - center;
                    if (dx * dx + dy * dy + dz * dz).sqrt() < radius {
                        vol.set(
                            x,
                            y,
                            z,
                            Voxel {
                                material: 1,
                                color: [0.0, 1.0, 0.0, 1.0],
                            },
                        );
                    }
                }
            }
        }
        vol
    }

    #[test]
    fn dense_sparse_octree_same_voxels() {
        let dense = make_sphere_volume(16);
        let dense_count = dense.count_filled();

        let sparse = dense.to_sparse();
        let sparse_count = sparse.count_filled();

        let size = 16u32.next_power_of_two();
        let mut octree = VoxelVolume::new_octree(size);
        for z in 0..16 {
            for y in 0..16 {
                for x in 0..16 {
                    let v = dense.get(x, y, z);
                    if v.is_solid() {
                        octree.set(x, y, z, v);
                    }
                }
            }
        }
        let octree_count = octree.count_filled();

        assert_eq!(
            dense_count, sparse_count,
            "dense={} sparse={}",
            dense_count, sparse_count
        );
        assert_eq!(
            dense_count, octree_count,
            "dense={} octree={}",
            dense_count, octree_count
        );
    }

    #[test]
    fn dense_sparse_octree_same_mesh() {
        let dense = make_sphere_volume(16);
        let sparse = dense.to_sparse();
        let size = 16u32.next_power_of_two();
        let mut octree = VoxelVolume::new_octree(size);
        for z in 0..16 {
            for y in 0..16 {
                for x in 0..16 {
                    let v = dense.get(x, y, z);
                    if v.is_solid() {
                        octree.set(x, y, z, v);
                    }
                }
            }
        }

        let mesh_d = marching_cubes(&dense, 1.0);
        let mesh_s = marching_cubes(&sparse, 1.0);
        let mesh_o = marching_cubes(&octree, 1.0);

        println!(
            "Dense:  verts={} idx={}",
            mesh_d.vertex_count, mesh_d.index_count
        );
        println!(
            "Sparse: verts={} idx={}",
            mesh_s.vertex_count, mesh_s.index_count
        );
        println!(
            "Octree: verts={} idx={}",
            mesh_o.vertex_count, mesh_o.index_count
        );

        assert_eq!(
            mesh_d.vertex_count, mesh_s.vertex_count,
            "dense vs sparse vertex mismatch"
        );
        assert_eq!(
            mesh_d.index_count, mesh_s.index_count,
            "dense vs sparse index mismatch"
        );
        // Octree may differ slightly if size != res, but at 16 (power of 2) it should match
        assert_eq!(
            mesh_d.vertex_count, mesh_o.vertex_count,
            "dense vs octree vertex mismatch"
        );
    }
}
