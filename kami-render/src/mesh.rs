//! Procedural mesh generation + KAMI Column conversion.
//! glTF loading is behind the `gltf-loader` feature.

use kami_core::ipc::{Column, Dtype, Frame};

/// GPU-ready interleaved mesh data.
pub struct LoadedMesh {
    /// Interleaved vertex data: [pos3, norm3, uv2] × N = 8 floats per vertex.
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub vertex_count: u32,
    pub index_count: u32,
}

/// Interleave separate position/normal/uv arrays into 32B/vertex buffer.
pub fn interleave(positions: &[f32], normals: &[f32], uvs: &[f32]) -> Vec<f32> {
    let vertex_count = positions.len() / 3;
    let mut out = Vec::with_capacity(vertex_count * 8);
    for i in 0..vertex_count {
        out.extend_from_slice(&positions[i * 3..i * 3 + 3]);
        out.extend_from_slice(&normals[i * 3..i * 3 + 3]);
        out.extend_from_slice(&uvs[i * 2..i * 2 + 2]);
    }
    out
}

/// Interleave skinned vertex data into 56B/vertex buffer (bytes).
///
/// Layout per vertex: pos(12B f32x3) + normal(12B f32x3) + uv(8B f32x2) +
/// joints(8B u16x4) + weights(16B f32x4) = 56 bytes.
///
/// `joints` length must equal `4 * vertex_count`; same for `weights`.
/// If `joints`/`weights` are empty, a vertex is emitted with all-zero joints
/// and weight (1,0,0,0) — shader's identity fallback will skip skinning.
pub fn interleave_skinned(
    positions: &[f32],
    normals: &[f32],
    uvs: &[f32],
    joints: &[u16],
    weights: &[f32],
) -> Vec<u8> {
    let vertex_count = positions.len() / 3;
    let has_skin = joints.len() >= vertex_count * 4 && weights.len() >= vertex_count * 4;
    let mut out = Vec::with_capacity(vertex_count * 56);
    for i in 0..vertex_count {
        // pos, normal, uv (f32)
        for f in &positions[i * 3..i * 3 + 3] {
            out.extend_from_slice(&f.to_le_bytes());
        }
        for f in &normals[i * 3..i * 3 + 3] {
            out.extend_from_slice(&f.to_le_bytes());
        }
        for f in &uvs[i * 2..i * 2 + 2] {
            out.extend_from_slice(&f.to_le_bytes());
        }
        // joints (u16 x 4)
        if has_skin {
            for j in &joints[i * 4..i * 4 + 4] {
                out.extend_from_slice(&j.to_le_bytes());
            }
        } else {
            out.extend_from_slice(&[0u8; 8]);
        }
        // weights (f32 x 4)
        if has_skin {
            for w in &weights[i * 4..i * 4 + 4] {
                out.extend_from_slice(&w.to_le_bytes());
            }
        } else {
            // weight (1,0,0,0) — but fallback branch in shader also handles all-zero
            out.extend_from_slice(&1.0f32.to_le_bytes());
            out.extend_from_slice(&[0u8; 12]);
        }
    }
    out
}

/// Generate a UV sphere mesh. Returns (positions, normals, uvs, indices).
pub fn sphere(stacks: u32, slices: u32) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=stacks {
        let phi = std::f32::consts::PI * i as f32 / stacks as f32;
        let y = phi.cos();
        let r = phi.sin();

        for j in 0..=slices {
            let theta = 2.0 * std::f32::consts::PI * j as f32 / slices as f32;
            let x = r * theta.cos();
            let z = r * theta.sin();

            positions.extend_from_slice(&[x * 0.5, y * 0.5, z * 0.5]);
            normals.extend_from_slice(&[x, y, z]);
            uvs.extend_from_slice(&[j as f32 / slices as f32, i as f32 / stacks as f32]);
        }
    }

    let ring = slices + 1;
    for i in 0..stacks {
        for j in 0..slices {
            let a = i * ring + j;
            let b = a + ring;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    (positions, normals, uvs, indices)
}

/// Generate a subdivided plane mesh on XZ plane. Returns (positions, normals, uvs, indices).
pub fn plane(
    width: f32,
    depth: f32,
    subdivisions: u32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) {
    let segs = subdivisions + 1;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for iz in 0..=segs {
        for ix in 0..=segs {
            let u = ix as f32 / segs as f32;
            let v = iz as f32 / segs as f32;
            positions.extend_from_slice(&[(u - 0.5) * width, 0.0, (v - 0.5) * depth]);
            normals.extend_from_slice(&[0.0, 1.0, 0.0]);
            uvs.extend_from_slice(&[u, v]);
        }
    }

    let row = segs + 1;
    for iz in 0..segs {
        for ix in 0..segs {
            let a = iz * row + ix;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    (positions, normals, uvs, indices)
}

/// Build a LoadedMesh from separate arrays (interleaves automatically).
pub fn loaded_mesh(positions: &[f32], normals: &[f32], uvs: &[f32], indices: &[u32]) -> LoadedMesh {
    let vertices = interleave(positions, normals, uvs);
    let vertex_count = positions.len() as u32 / 3;
    let index_count = indices.len() as u32;
    LoadedMesh {
        vertices,
        indices: indices.to_vec(),
        vertex_count,
        index_count,
    }
}

/// Compute tangent vectors for normal mapping (MikkTSpace-lite).
/// Returns vec4 per vertex: xyz = tangent direction, w = handedness (+1 or -1).
pub fn compute_tangents(
    positions: &[f32],
    normals: &[f32],
    uvs: &[f32],
    indices: &[u32],
) -> Vec<f32> {
    let vertex_count = positions.len() / 3;
    let mut tangents = vec![0.0f32; vertex_count * 3];
    let mut bitangents = vec![0.0f32; vertex_count * 3];

    // Accumulate per-triangle tangent/bitangent
    for tri in indices.chunks(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = &positions[i0 * 3..i0 * 3 + 3];
        let p1 = &positions[i1 * 3..i1 * 3 + 3];
        let p2 = &positions[i2 * 3..i2 * 3 + 3];

        let uv0 = &uvs[i0 * 2..i0 * 2 + 2];
        let uv1 = &uvs[i1 * 2..i1 * 2 + 2];
        let uv2 = &uvs[i2 * 2..i2 * 2 + 2];

        let edge1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let edge2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

        let duv1 = [uv1[0] - uv0[0], uv1[1] - uv0[1]];
        let duv2 = [uv2[0] - uv0[0], uv2[1] - uv0[1]];

        let det = duv1[0] * duv2[1] - duv1[1] * duv2[0];
        let r = if det.abs() > 1e-8 { 1.0 / det } else { 0.0 };

        let t = [
            r * (duv2[1] * edge1[0] - duv1[1] * edge2[0]),
            r * (duv2[1] * edge1[1] - duv1[1] * edge2[1]),
            r * (duv2[1] * edge1[2] - duv1[1] * edge2[2]),
        ];
        let b = [
            r * (-duv2[0] * edge1[0] + duv1[0] * edge2[0]),
            r * (-duv2[0] * edge1[1] + duv1[0] * edge2[1]),
            r * (-duv2[0] * edge1[2] + duv1[0] * edge2[2]),
        ];

        for &idx in &[i0, i1, i2] {
            for k in 0..3 {
                tangents[idx * 3 + k] += t[k];
                bitangents[idx * 3 + k] += b[k];
            }
        }
    }

    // Orthonormalize and compute handedness → vec4 per vertex
    let mut result = Vec::with_capacity(vertex_count * 4);
    for i in 0..vertex_count {
        let n = [normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]];
        let t = [tangents[i * 3], tangents[i * 3 + 1], tangents[i * 3 + 2]];
        let b = [
            bitangents[i * 3],
            bitangents[i * 3 + 1],
            bitangents[i * 3 + 2],
        ];

        // Gram-Schmidt orthogonalize: t' = normalize(t - n * dot(n, t))
        let n_dot_t = n[0] * t[0] + n[1] * t[1] + n[2] * t[2];
        let ot = [
            t[0] - n[0] * n_dot_t,
            t[1] - n[1] * n_dot_t,
            t[2] - n[2] * n_dot_t,
        ];
        let len = (ot[0] * ot[0] + ot[1] * ot[1] + ot[2] * ot[2]).sqrt();
        let ot = if len > 1e-8 {
            [ot[0] / len, ot[1] / len, ot[2] / len]
        } else {
            [1.0, 0.0, 0.0] // fallback
        };

        // Handedness: sign(dot(cross(n, t), b))
        let cross = [
            n[1] * ot[2] - n[2] * ot[1],
            n[2] * ot[0] - n[0] * ot[2],
            n[0] * ot[1] - n[1] * ot[0],
        ];
        let dot_cb = cross[0] * b[0] + cross[1] * b[1] + cross[2] * b[2];
        let w = if dot_cb < 0.0 { -1.0 } else { 1.0 };

        result.extend_from_slice(&[ot[0], ot[1], ot[2], w]);
    }

    result
}

/// Interleave position + normal + uv + tangent into 48B/vertex buffer.
pub fn interleave_with_tangents(
    positions: &[f32],
    normals: &[f32],
    uvs: &[f32],
    tangents: &[f32],
) -> Vec<f32> {
    let vertex_count = positions.len() / 3;
    let mut out = Vec::with_capacity(vertex_count * 12); // 12 floats per vertex
    for i in 0..vertex_count {
        out.extend_from_slice(&positions[i * 3..i * 3 + 3]);
        out.extend_from_slice(&normals[i * 3..i * 3 + 3]);
        out.extend_from_slice(&uvs[i * 2..i * 2 + 2]);
        out.extend_from_slice(&tangents[i * 4..i * 4 + 4]);
    }
    out
}

/// Generate a unit cube mesh. Returns (positions, normals, uvs, indices).
/// All returned as owned Vec<f32> / Vec<u32> for KAMI Column consumption.
pub fn cube() -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) {
    #[rustfmt::skip]
    let positions: Vec<f32> = vec![
        // Front face
        -0.5, -0.5,  0.5,   0.5, -0.5,  0.5,   0.5,  0.5,  0.5,  -0.5,  0.5,  0.5,
        // Back face
         0.5, -0.5, -0.5,  -0.5, -0.5, -0.5,  -0.5,  0.5, -0.5,   0.5,  0.5, -0.5,
        // Top face
        -0.5,  0.5,  0.5,   0.5,  0.5,  0.5,   0.5,  0.5, -0.5,  -0.5,  0.5, -0.5,
        // Bottom face
        -0.5, -0.5, -0.5,   0.5, -0.5, -0.5,   0.5, -0.5,  0.5,  -0.5, -0.5,  0.5,
        // Right face
         0.5, -0.5,  0.5,   0.5, -0.5, -0.5,   0.5,  0.5, -0.5,   0.5,  0.5,  0.5,
        // Left face
        -0.5, -0.5, -0.5,  -0.5, -0.5,  0.5,  -0.5,  0.5,  0.5,  -0.5,  0.5, -0.5,
    ];

    #[rustfmt::skip]
    let normals: Vec<f32> = vec![
        0.0, 0.0, 1.0,  0.0, 0.0, 1.0,  0.0, 0.0, 1.0,  0.0, 0.0, 1.0,
        0.0, 0.0,-1.0,  0.0, 0.0,-1.0,  0.0, 0.0,-1.0,  0.0, 0.0,-1.0,
        0.0, 1.0, 0.0,  0.0, 1.0, 0.0,  0.0, 1.0, 0.0,  0.0, 1.0, 0.0,
        0.0,-1.0, 0.0,  0.0,-1.0, 0.0,  0.0,-1.0, 0.0,  0.0,-1.0, 0.0,
        1.0, 0.0, 0.0,  1.0, 0.0, 0.0,  1.0, 0.0, 0.0,  1.0, 0.0, 0.0,
       -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0,
    ];

    #[rustfmt::skip]
    let uvs: Vec<f32> = vec![
        0.0, 0.0,  1.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        0.0, 0.0,  1.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        0.0, 0.0,  1.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        0.0, 0.0,  1.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        0.0, 0.0,  1.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        0.0, 0.0,  1.0, 0.0,  1.0, 1.0,  0.0, 1.0,
    ];

    #[rustfmt::skip]
    let indices: Vec<u32> = vec![
         0, 1, 2,  0, 2, 3,   // front
         4, 5, 6,  4, 6, 7,   // back
         8, 9,10,  8,10,11,   // top
        12,13,14, 12,14,15,   // bottom
        16,17,18, 16,18,19,   // right
        20,21,22, 20,22,23,   // left
    ];

    (positions, normals, uvs, indices)
}

/// Generate instance transforms for N entities in a grid pattern.
/// Returns Vec<f32> with 16 floats (Mat4) per entity — ready for KAMI Column.
pub fn grid_instances(count: u32, spacing: f32) -> Vec<f32> {
    let side = (count as f32).sqrt().ceil() as u32;
    let offset = (side as f32 * spacing) / 2.0;
    let mut transforms = Vec::with_capacity(count as usize * 16);

    for i in 0..count {
        let x = (i % side) as f32 * spacing - offset;
        let z = (i / side) as f32 * spacing - offset;
        let mat = glam::Mat4::from_translation(glam::Vec3::new(x, 0.0, z));
        transforms.extend_from_slice(&mat.to_cols_array());
    }

    transforms
}

/// Build a KAMI Frame with instance transforms for GPU upload.
pub fn instances_to_frame(transforms: &[f32], tick: u32) -> Frame {
    let n_entities = transforms.len() / 16;
    let mut frame = Frame::new(tick, n_entities as u32);
    let data = bytemuck::cast_slice::<f32, u8>(transforms).to_vec();
    frame.push_column_owned(data, Dtype::Mat4, 1);
    frame
}

// ═══════════════════════════════════════════════════════════════════════════
// GIS / Maps mesh generators — hex grid, cylinder pipe, building extrusion
// Used by maps.etzhayyim.com for KAMI-based infrastructure & spatial rendering
// ═══════════════════════════════════════════════════════════════════════════

/// Generate a hexagonal prism (flat-top). Center at origin, extends along Y axis.
/// `radius`: circumradius (center to vertex), `height`: Y extent.
/// Returns (positions, normals, uvs, indices).
pub fn hex_prism(radius: f32, height: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let half_h = height * 0.5;
    let sides = 6u32;

    // Generate hex corner angles (flat-top: first vertex at 0°)
    let angles: Vec<f32> = (0..sides)
        .map(|i| std::f32::consts::PI * 2.0 * i as f32 / sides as f32)
        .collect();

    // Top face (Y = +half_h, normal up)
    let top_center = positions.len() as u32 / 3;
    positions.extend_from_slice(&[0.0, half_h, 0.0]);
    normals.extend_from_slice(&[0.0, 1.0, 0.0]);
    uvs.extend_from_slice(&[0.5, 0.5]);
    for &a in &angles {
        positions.extend_from_slice(&[radius * a.cos(), half_h, radius * a.sin()]);
        normals.extend_from_slice(&[0.0, 1.0, 0.0]);
        uvs.extend_from_slice(&[0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()]);
    }
    for i in 0..sides {
        let next = (i + 1) % sides;
        indices.extend_from_slice(&[top_center, top_center + 1 + i, top_center + 1 + next]);
    }

    // Bottom face (Y = -half_h, normal down)
    let bot_center = positions.len() as u32 / 3;
    positions.extend_from_slice(&[0.0, -half_h, 0.0]);
    normals.extend_from_slice(&[0.0, -1.0, 0.0]);
    uvs.extend_from_slice(&[0.5, 0.5]);
    for &a in &angles {
        positions.extend_from_slice(&[radius * a.cos(), -half_h, radius * a.sin()]);
        normals.extend_from_slice(&[0.0, -1.0, 0.0]);
        uvs.extend_from_slice(&[0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()]);
    }
    for i in 0..sides {
        let next = (i + 1) % sides;
        indices.extend_from_slice(&[bot_center, bot_center + 1 + next, bot_center + 1 + i]);
    }

    // Side faces (6 quads)
    for i in 0..sides {
        let next = (i + 1) % sides;
        let a0 = angles[i as usize];
        let a1 = angles[next as usize];
        let mid_angle = (a0 + a1) * 0.5;
        let nx = mid_angle.cos();
        let nz = mid_angle.sin();

        let base = positions.len() as u32 / 3;
        // 4 vertices per side quad
        let x0 = radius * a0.cos();
        let z0 = radius * a0.sin();
        let x1 = radius * a1.cos();
        let z1 = radius * a1.sin();

        positions.extend_from_slice(&[
            x0, half_h, z0, x1, half_h, z1, x1, -half_h, z1, x0, -half_h, z0,
        ]);
        for _ in 0..4 {
            normals.extend_from_slice(&[nx, 0.0, nz]);
        }
        let u0 = i as f32 / sides as f32;
        let u1 = (i + 1) as f32 / sides as f32;
        uvs.extend_from_slice(&[u0, 0.0, u1, 0.0, u1, 1.0, u0, 1.0]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    (positions, normals, uvs, indices)
}

/// Generate a cylinder pipe along Y axis. For infrastructure rendering (water/gas/electric).
/// `radius`: outer radius, `thickness`: wall thickness (0 = solid), `height`: Y extent, `segments`: circumference subdivision.
pub fn cylinder_pipe(
    radius: f32,
    thickness: f32,
    height: f32,
    segments: u32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let half_h = height * 0.5;
    let inner_radius = if thickness > 0.0 {
        (radius - thickness).max(0.0)
    } else {
        0.0
    };
    let is_hollow = inner_radius > 0.0;

    // Outer wall
    for ring in 0..=1u32 {
        let y = if ring == 0 { half_h } else { -half_h };
        let v = ring as f32;
        for i in 0..=segments {
            let angle = std::f32::consts::PI * 2.0 * i as f32 / segments as f32;
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            positions.extend_from_slice(&[x, y, z]);
            normals.extend_from_slice(&[angle.cos(), 0.0, angle.sin()]);
            uvs.extend_from_slice(&[i as f32 / segments as f32, v]);
        }
    }
    let row = segments + 1;
    for i in 0..segments {
        let a = i;
        let b = a + row;
        indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
    }

    if is_hollow {
        // Inner wall (normals pointing inward)
        let inner_base = positions.len() as u32 / 3;
        for ring in 0..=1u32 {
            let y = if ring == 0 { half_h } else { -half_h };
            let v = ring as f32;
            for i in 0..=segments {
                let angle = std::f32::consts::PI * 2.0 * i as f32 / segments as f32;
                let x = inner_radius * angle.cos();
                let z = inner_radius * angle.sin();
                positions.extend_from_slice(&[x, y, z]);
                normals.extend_from_slice(&[-angle.cos(), 0.0, -angle.sin()]);
                uvs.extend_from_slice(&[i as f32 / segments as f32, v]);
            }
        }
        for i in 0..segments {
            let a = inner_base + i;
            let b = a + row;
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }

        // Top annulus ring
        let top_base = positions.len() as u32 / 3;
        for i in 0..=segments {
            let angle = std::f32::consts::PI * 2.0 * i as f32 / segments as f32;
            let u = i as f32 / segments as f32;
            positions.extend_from_slice(&[radius * angle.cos(), half_h, radius * angle.sin()]);
            normals.extend_from_slice(&[0.0, 1.0, 0.0]);
            uvs.extend_from_slice(&[u, 0.0]);
            positions.extend_from_slice(&[
                inner_radius * angle.cos(),
                half_h,
                inner_radius * angle.sin(),
            ]);
            normals.extend_from_slice(&[0.0, 1.0, 0.0]);
            uvs.extend_from_slice(&[u, 1.0]);
        }
        for i in 0..segments {
            let a = top_base + i * 2;
            indices.extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }

        // Bottom annulus ring
        let bot_base = positions.len() as u32 / 3;
        for i in 0..=segments {
            let angle = std::f32::consts::PI * 2.0 * i as f32 / segments as f32;
            let u = i as f32 / segments as f32;
            positions.extend_from_slice(&[radius * angle.cos(), -half_h, radius * angle.sin()]);
            normals.extend_from_slice(&[0.0, -1.0, 0.0]);
            uvs.extend_from_slice(&[u, 0.0]);
            positions.extend_from_slice(&[
                inner_radius * angle.cos(),
                -half_h,
                inner_radius * angle.sin(),
            ]);
            normals.extend_from_slice(&[0.0, -1.0, 0.0]);
            uvs.extend_from_slice(&[u, 1.0]);
        }
        for i in 0..segments {
            let a = bot_base + i * 2;
            indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    } else {
        // Solid caps
        let top_center = positions.len() as u32 / 3;
        positions.extend_from_slice(&[0.0, half_h, 0.0]);
        normals.extend_from_slice(&[0.0, 1.0, 0.0]);
        uvs.extend_from_slice(&[0.5, 0.5]);
        for i in 0..=segments {
            let angle = std::f32::consts::PI * 2.0 * i as f32 / segments as f32;
            positions.extend_from_slice(&[radius * angle.cos(), half_h, radius * angle.sin()]);
            normals.extend_from_slice(&[0.0, 1.0, 0.0]);
            uvs.extend_from_slice(&[0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
        }
        for i in 0..segments {
            indices.extend_from_slice(&[top_center, top_center + 1 + i, top_center + 2 + i]);
        }

        let bot_center = positions.len() as u32 / 3;
        positions.extend_from_slice(&[0.0, -half_h, 0.0]);
        normals.extend_from_slice(&[0.0, -1.0, 0.0]);
        uvs.extend_from_slice(&[0.5, 0.5]);
        for i in 0..=segments {
            let angle = std::f32::consts::PI * 2.0 * i as f32 / segments as f32;
            positions.extend_from_slice(&[radius * angle.cos(), -half_h, radius * angle.sin()]);
            normals.extend_from_slice(&[0.0, -1.0, 0.0]);
            uvs.extend_from_slice(&[0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
        }
        for i in 0..segments {
            indices.extend_from_slice(&[bot_center, bot_center + 2 + i, bot_center + 1 + i]);
        }
    }

    (positions, normals, uvs, indices)
}

/// Generate a building extrusion from a 2D footprint polygon.
/// `footprint`: pairs of (x, z) coordinates forming a closed polygon (CCW winding).
/// `height`: building height (Y extent from 0).
/// Returns (positions, normals, uvs, indices).
pub fn building_extrusion(
    footprint: &[(f32, f32)],
    height: f32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let n = footprint.len();
    if n < 3 {
        return (positions, normals, uvs, indices);
    }

    // Top face (fan triangulation from first vertex)
    let top_base = 0u32;
    for &(x, z) in footprint {
        positions.extend_from_slice(&[x, height, z]);
        normals.extend_from_slice(&[0.0, 1.0, 0.0]);
        uvs.extend_from_slice(&[x, z]); // world-space UV
    }
    for i in 1..n as u32 - 1 {
        indices.extend_from_slice(&[top_base, top_base + i, top_base + i + 1]);
    }

    // Bottom face (reverse winding)
    let bot_base = positions.len() as u32 / 3;
    for &(x, z) in footprint {
        positions.extend_from_slice(&[x, 0.0, z]);
        normals.extend_from_slice(&[0.0, -1.0, 0.0]);
        uvs.extend_from_slice(&[x, z]);
    }
    for i in 1..n as u32 - 1 {
        indices.extend_from_slice(&[bot_base, bot_base + i + 1, bot_base + i]);
    }

    // Side walls
    for i in 0..n {
        let next = (i + 1) % n;
        let (x0, z0) = footprint[i];
        let (x1, z1) = footprint[next];

        // Outward normal for this edge
        let dx = x1 - x0;
        let dz = z1 - z0;
        let len = (dx * dx + dz * dz).sqrt();
        let (nx, nz) = if len > 1e-8 {
            (dz / len, -dx / len)
        } else {
            (0.0, 1.0)
        };

        let wall_base = positions.len() as u32 / 3;
        let edge_len = len;

        positions.extend_from_slice(&[x0, height, z0, x1, height, z1, x1, 0.0, z1, x0, 0.0, z0]);
        for _ in 0..4 {
            normals.extend_from_slice(&[nx, 0.0, nz]);
        }
        uvs.extend_from_slice(&[0.0, 0.0, edge_len, 0.0, edge_len, height, 0.0, height]);
        indices.extend_from_slice(&[
            wall_base,
            wall_base + 1,
            wall_base + 2,
            wall_base,
            wall_base + 2,
            wall_base + 3,
        ]);
    }

    (positions, normals, uvs, indices)
}

/// Generate an H3-style hex grid on XZ plane. `rings` = number of hex rings around center.
/// Each hex is a flat hex prism with given height. Returns a combined LoadedMesh.
pub fn hex_grid(rings: u32, hex_radius: f32, hex_height: f32, spacing: f32) -> LoadedMesh {
    let (hex_pos, hex_norm, hex_uv, hex_idx) = hex_prism(hex_radius, hex_height);
    let hex_vert_count = hex_pos.len() / 3;

    let mut all_positions = Vec::new();
    let mut all_normals = Vec::new();
    let mut all_uvs = Vec::new();
    let mut all_indices = Vec::new();

    let mut hex_count = 0u32;
    let step = hex_radius * 2.0 * spacing;
    let row_h = step * (3.0f32).sqrt() * 0.5;

    for q in -(rings as i32)..=(rings as i32) {
        let r_min = (-(rings as i32)).max(-q - rings as i32);
        let r_max = (rings as i32).min(-q + rings as i32);
        for r in r_min..=r_max {
            let cx = step * (q as f32 + r as f32 * 0.5);
            let cz = row_h * r as f32;

            let base_idx = hex_count * hex_vert_count as u32;
            for i in 0..hex_vert_count {
                all_positions.push(hex_pos[i * 3] + cx);
                all_positions.push(hex_pos[i * 3 + 1]);
                all_positions.push(hex_pos[i * 3 + 2] + cz);
            }
            all_normals.extend_from_slice(&hex_norm);
            all_uvs.extend_from_slice(&hex_uv);
            for &idx in &hex_idx {
                all_indices.push(idx + base_idx);
            }
            hex_count += 1;
        }
    }

    loaded_mesh(&all_positions, &all_normals, &all_uvs, &all_indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_counts() {
        let (pos, norm, uv, idx) = cube();
        assert_eq!(pos.len(), 24 * 3); // 24 vertices × 3 floats
        assert_eq!(norm.len(), 24 * 3);
        assert_eq!(uv.len(), 24 * 2);
        assert_eq!(idx.len(), 36); // 12 triangles × 3 indices
    }

    #[test]
    fn sphere_valid() {
        let (pos, norm, uv, idx) = sphere(8, 16);
        assert!(!pos.is_empty());
        assert_eq!(pos.len(), norm.len());
        assert_eq!(pos.len() / 3, uv.len() / 2);
        assert!(!idx.is_empty());
        // Normals should be unit vectors
        for i in 0..norm.len() / 3 {
            let nx = norm[i * 3];
            let ny = norm[i * 3 + 1];
            let nz = norm[i * 3 + 2];
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            assert!((len - 1.0).abs() < 0.01, "normal not unit: {}", len);
        }
        // UVs in [0, 1]
        for &u in &uv {
            assert!(u >= 0.0 && u <= 1.0, "uv out of range: {}", u);
        }
    }

    #[test]
    fn plane_counts() {
        let (pos, norm, uv, idx) = plane(10.0, 10.0, 3);
        let segs = 4; // subdivisions + 1
        let verts = (segs + 1) * (segs + 1);
        assert_eq!(pos.len(), verts * 3);
        assert_eq!(norm.len(), verts * 3);
        assert_eq!(uv.len(), verts * 2);
        assert_eq!(idx.len(), segs * segs * 6);
    }

    #[test]
    fn interleave_stride() {
        let pos = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let norm = vec![0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let uv = vec![0.0, 0.0, 1.0, 1.0];
        let out = interleave(&pos, &norm, &uv);
        assert_eq!(out.len(), 2 * 8); // 2 vertices × 8 floats
        // First vertex: pos(1,2,3) norm(0,1,0) uv(0,0)
        assert_eq!(&out[0..3], &[1.0, 2.0, 3.0]);
        assert_eq!(&out[3..6], &[0.0, 1.0, 0.0]);
        assert_eq!(&out[6..8], &[0.0, 0.0]);
    }

    #[test]
    fn loaded_mesh_from_cube() {
        let (pos, norm, uv, idx) = cube();
        let m = loaded_mesh(&pos, &norm, &uv, &idx);
        assert_eq!(m.vertex_count, 24);
        assert_eq!(m.index_count, 36);
        assert_eq!(m.vertices.len(), 24 * 8);
    }

    #[test]
    fn tangent_computation() {
        // Flat quad on XZ plane: tangent should be ~(1,0,0), handedness +1
        let pos = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let norm = vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];
        let uv = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let idx = vec![0, 1, 2, 0, 2, 3];
        let tangents = compute_tangents(&pos, &norm, &uv, &idx);
        assert_eq!(tangents.len(), 4 * 4); // 4 vertices × 4 floats
        // First vertex tangent should be approximately (1, 0, 0, +/-1)
        assert!(
            (tangents[0] - 1.0).abs() < 0.1,
            "tangent x: {}",
            tangents[0]
        );
        assert!(tangents[1].abs() < 0.1, "tangent y: {}", tangents[1]);
        assert!(tangents[2].abs() < 0.1, "tangent z: {}", tangents[2]);
        assert!(tangents[3].abs() > 0.5, "handedness: {}", tangents[3]);
    }

    #[test]
    fn interleave_with_tangents_stride() {
        let pos = vec![1.0, 2.0, 3.0];
        let norm = vec![0.0, 1.0, 0.0];
        let uv = vec![0.5, 0.5];
        let tan = vec![1.0, 0.0, 0.0, 1.0];
        let out = interleave_with_tangents(&pos, &norm, &uv, &tan);
        assert_eq!(out.len(), 12); // 1 vertex × 12 floats (3+3+2+4)
    }

    #[test]
    fn hex_prism_valid() {
        let (pos, norm, uv, idx) = hex_prism(1.0, 2.0);
        assert!(!pos.is_empty());
        assert_eq!(pos.len(), norm.len());
        assert_eq!(pos.len() / 3, uv.len() / 2);
        assert!(!idx.is_empty());
        // Should have top (7) + bottom (7) + sides (6*4=24) = 38 vertices
        assert_eq!(pos.len() / 3, 38);
    }

    #[test]
    fn cylinder_pipe_solid() {
        let (pos, _norm, _uv, idx) = cylinder_pipe(0.5, 0.0, 2.0, 16);
        assert!(!pos.is_empty());
        assert_eq!(pos.len(), _norm.len());
        assert!(!idx.is_empty());
    }

    #[test]
    fn cylinder_pipe_hollow() {
        let (pos, _norm, _uv, _idx) = cylinder_pipe(0.5, 0.1, 2.0, 16);
        assert!(!pos.is_empty());
        // Hollow pipe should have more vertices than solid (inner wall + annulus rings)
        let (pos_solid, _, _, _) = cylinder_pipe(0.5, 0.0, 2.0, 16);
        assert!(pos.len() > pos_solid.len());
    }

    #[test]
    fn building_extrusion_square() {
        let footprint = vec![(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let (pos, _norm, _uv, idx) = building_extrusion(&footprint, 10.0);
        assert!(!pos.is_empty());
        // 4 top + 4 bottom + 4*4 sides = 24 vertices
        assert_eq!(pos.len() / 3, 24);
        // 2 top tris + 2 bottom tris + 4*2 side tris = 12 tris = 36 indices
        assert_eq!(idx.len(), 36);
    }

    #[test]
    fn hex_grid_ring1() {
        let mesh = hex_grid(1, 1.0, 0.2, 1.05);
        // Ring 1 = 1 center + 6 surrounding = 7 hexes
        assert!(mesh.vertex_count > 0);
        assert!(mesh.index_count > 0);
        // 7 hexes × 38 verts each = 266
        assert_eq!(mesh.vertex_count, 7 * 38);
    }
}
