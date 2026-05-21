//! Nintendo-style procedural mesh generators for brainrot characters and objects.
//! Each function returns `(Vec<f32>, Vec<u32>)` in interleaved format: pos3 + norm3 + uv2 = 8 floats per vertex.

use std::f32::consts::PI;

/// Merge multiple meshes into a single mesh, offsetting indices.
pub fn merge_meshes(meshes: &[(Vec<f32>, Vec<u32>)]) -> (Vec<f32>, Vec<u32>) {
    let total_verts: usize = meshes.iter().map(|(v, _)| v.len()).sum();
    let total_idx: usize = meshes.iter().map(|(_, i)| i.len()).sum();
    let mut vertices = Vec::with_capacity(total_verts);
    let mut indices = Vec::with_capacity(total_idx);
    let mut base_vertex: u32 = 0;
    for (verts, idxs) in meshes {
        let vert_count = verts.len() as u32 / 8;
        vertices.extend_from_slice(verts);
        for &i in idxs {
            indices.push(i + base_vertex);
        }
        base_vertex += vert_count;
    }
    (vertices, indices)
}

/// Generate a UV sphere with given stacks/slices, radius, and center offset.
/// Returns interleaved (pos3+norm3+uv2) vertices and indices.
pub fn sphere_mesh(
    stacks: u32,
    slices: u32,
    radius: f32,
    cx: f32,
    cy: f32,
    cz: f32,
) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=stacks {
        let phi = PI * i as f32 / stacks as f32;
        let y = phi.cos();
        let r = phi.sin();
        for j in 0..=slices {
            let theta = 2.0 * PI * j as f32 / slices as f32;
            let nx = r * theta.cos();
            let nz = r * theta.sin();
            let ny = y;
            vertices.extend_from_slice(&[
                cx + nx * radius,
                cy + ny * radius,
                cz + nz * radius,
                nx,
                ny,
                nz,
                j as f32 / slices as f32,
                i as f32 / stacks as f32,
            ]);
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
    (vertices, indices)
}

/// Generate a cylinder along Y axis with given segments, radius, half_height, and center offset.
pub fn cylinder_mesh(
    segments: u32,
    radius: f32,
    half_height: f32,
    cx: f32,
    cy: f32,
    cz: f32,
) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    // Side vertices: 2 rings
    for ring in 0..2u32 {
        let y = if ring == 0 { -half_height } else { half_height };
        let v = ring as f32;
        for j in 0..=segments {
            let theta = 2.0 * PI * j as f32 / segments as f32;
            let nx = theta.cos();
            let nz = theta.sin();
            vertices.extend_from_slice(&[
                cx + nx * radius,
                cy + y,
                cz + nz * radius,
                nx,
                0.0,
                nz,
                j as f32 / segments as f32,
                v,
            ]);
        }
    }
    let ring_size = segments + 1;
    for j in 0..segments {
        let a = j;
        let b = a + ring_size;
        indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
    }
    // Top cap
    let top_center = vertices.len() as u32 / 8;
    vertices.extend_from_slice(&[cx, cy + half_height, cz, 0.0, 1.0, 0.0, 0.5, 0.5]);
    for j in 0..=segments {
        let theta = 2.0 * PI * j as f32 / segments as f32;
        let nx = theta.cos();
        let nz = theta.sin();
        vertices.extend_from_slice(&[
            cx + nx * radius,
            cy + half_height,
            cz + nz * radius,
            0.0,
            1.0,
            0.0,
            0.5 + nx * 0.5,
            0.5 + nz * 0.5,
        ]);
    }
    for j in 0..segments {
        indices.extend_from_slice(&[top_center, top_center + 1 + j, top_center + 2 + j]);
    }
    // Bottom cap
    let bot_center = vertices.len() as u32 / 8;
    vertices.extend_from_slice(&[cx, cy - half_height, cz, 0.0, -1.0, 0.0, 0.5, 0.5]);
    for j in 0..=segments {
        let theta = 2.0 * PI * j as f32 / segments as f32;
        let nx = theta.cos();
        let nz = theta.sin();
        vertices.extend_from_slice(&[
            cx + nx * radius,
            cy - half_height,
            cz + nz * radius,
            0.0,
            -1.0,
            0.0,
            0.5 + nx * 0.5,
            0.5 + nz * 0.5,
        ]);
    }
    for j in 0..segments {
        indices.extend_from_slice(&[bot_center, bot_center + 2 + j, bot_center + 1 + j]);
    }
    (vertices, indices)
}

/// Capsule: cylinder with sphere caps.
pub fn capsule(radius: f32, half_height: f32, segments: u32) -> (Vec<f32>, Vec<u32>) {
    let cyl = cylinder_mesh(segments, radius, half_height, 0.0, 0.0, 0.0);
    let top_cap = sphere_mesh(segments / 2, segments, radius, 0.0, half_height, 0.0);
    let bot_cap = sphere_mesh(segments / 2, segments, radius, 0.0, -half_height, 0.0);
    merge_meshes(&[cyl, top_cap, bot_cap])
}

/// Rounded box with beveled edges. Approximated as a box with slightly inset faces.
pub fn rounded_box(w: f32, h: f32, d: f32, bevel: f32) -> (Vec<f32>, Vec<u32>) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    let hd = d * 0.5;
    let b = bevel.min(hw.min(hh.min(hd)));
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Helper to add a quad (4 verts + 6 indices)
    let mut add_quad = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], p3: [f32; 3], n: [f32; 3]| {
        let base = vertices.len() as u32 / 8;
        vertices.extend_from_slice(&[p0[0], p0[1], p0[2], n[0], n[1], n[2], 0.0, 0.0]);
        vertices.extend_from_slice(&[p1[0], p1[1], p1[2], n[0], n[1], n[2], 1.0, 0.0]);
        vertices.extend_from_slice(&[p2[0], p2[1], p2[2], n[0], n[1], n[2], 1.0, 1.0]);
        vertices.extend_from_slice(&[p3[0], p3[1], p3[2], n[0], n[1], n[2], 0.0, 1.0]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // Main faces (slightly inset by bevel for a softer look)
    let iw = hw - b;
    let ih = hh - b;
    let id = hd - b;

    // Front/back (Z faces)
    add_quad(
        [-iw, -ih, hd],
        [iw, -ih, hd],
        [iw, ih, hd],
        [-iw, ih, hd],
        [0.0, 0.0, 1.0],
    );
    add_quad(
        [iw, -ih, -hd],
        [-iw, -ih, -hd],
        [-iw, ih, -hd],
        [iw, ih, -hd],
        [0.0, 0.0, -1.0],
    );
    // Top/bottom (Y faces)
    add_quad(
        [-iw, hh, -id],
        [iw, hh, -id],
        [iw, hh, id],
        [-iw, hh, id],
        [0.0, 1.0, 0.0],
    );
    add_quad(
        [-iw, -hh, id],
        [iw, -hh, id],
        [iw, -hh, -id],
        [-iw, -hh, -id],
        [0.0, -1.0, 0.0],
    );
    // Left/right (X faces)
    add_quad(
        [hw, -ih, -id],
        [hw, -ih, id],
        [hw, ih, id],
        [hw, ih, -id],
        [1.0, 0.0, 0.0],
    );
    add_quad(
        [-hw, -ih, id],
        [-hw, -ih, -id],
        [-hw, ih, -id],
        [-hw, ih, id],
        [-1.0, 0.0, 0.0],
    );

    // Edge bevels: horizontal edges (top/bottom, front/back)
    // Top-front bevel
    let diag = 1.0 / 2.0_f32.sqrt();
    add_quad(
        [-iw, ih, hd],
        [iw, ih, hd],
        [iw, hh, id],
        [-iw, hh, id],
        [0.0, diag, diag],
    );
    // Top-back bevel
    add_quad(
        [iw, ih, -hd],
        [-iw, ih, -hd],
        [-iw, hh, -id],
        [iw, hh, -id],
        [0.0, diag, -diag],
    );
    // Bottom-front bevel
    add_quad(
        [-iw, -hh, id],
        [iw, -hh, id],
        [iw, -ih, hd],
        [-iw, -ih, hd],
        [0.0, -diag, diag],
    );
    // Bottom-back bevel
    add_quad(
        [iw, -hh, -id],
        [-iw, -hh, -id],
        [-iw, -ih, -hd],
        [iw, -ih, -hd],
        [0.0, -diag, -diag],
    );

    // Right-front bevel
    add_quad(
        [iw, -ih, hd],
        [hw, -ih, id],
        [hw, ih, id],
        [iw, ih, hd],
        [diag, 0.0, diag],
    );
    // Right-back bevel
    add_quad(
        [hw, -ih, -id],
        [iw, -ih, -hd],
        [iw, ih, -hd],
        [hw, ih, -id],
        [diag, 0.0, -diag],
    );
    // Left-front bevel
    add_quad(
        [-hw, -ih, id],
        [-iw, -ih, hd],
        [-iw, ih, hd],
        [-hw, ih, id],
        [-diag, 0.0, diag],
    );
    // Left-back bevel
    add_quad(
        [-iw, -ih, -hd],
        [-hw, -ih, -id],
        [-hw, ih, -id],
        [-iw, ih, -hd],
        [-diag, 0.0, -diag],
    );

    (vertices, indices)
}

/// Add an offset to all vertex positions in-place.
pub fn offset_mesh(mesh: &mut (Vec<f32>, Vec<u32>), dx: f32, dy: f32, dz: f32) {
    let verts = &mut mesh.0;
    let count = verts.len() / 8;
    for i in 0..count {
        verts[i * 8] += dx;
        verts[i * 8 + 1] += dy;
        verts[i * 8 + 2] += dz;
    }
}

/// Scale all vertex positions in-place.
pub fn scale_mesh(mesh: &mut (Vec<f32>, Vec<u32>), sx: f32, sy: f32, sz: f32) {
    let verts = &mut mesh.0;
    let count = verts.len() / 8;
    for i in 0..count {
        verts[i * 8] *= sx;
        verts[i * 8 + 1] *= sy;
        verts[i * 8 + 2] *= sz;
    }
}

// =============================================================================
// Public mesh generators
// =============================================================================

/// Giant Skibidi Toilet: bowl (squashed sphere bottom half) + tank (rounded box) + seat ring (torus quads) + lid (disc).
pub fn toilet_mesh() -> (Vec<f32>, Vec<u32>) {
    let segs = 16;
    // Bowl: squashed sphere, bottom half (scale y by 0.6)
    let mut bowl = sphere_mesh(segs, segs, 1.2, 0.0, 0.0, 0.0);
    // Squash Y and keep only bottom half by scaling
    scale_mesh(&mut bowl, 1.0, 0.6, 1.0);

    // Tank: rounded box behind the bowl
    let mut tank = rounded_box(1.0, 1.8, 0.8, 0.1);
    offset_mesh(&mut tank, 0.0, 0.5, -1.1);

    // Seat ring: torus-like ring of quads
    let ring_segs = 24;
    let ring_r_major = 1.0;
    let ring_r_minor = 0.12;
    let mut ring_verts = Vec::new();
    let mut ring_idxs = Vec::new();
    for i in 0..=ring_segs {
        let theta = 2.0 * PI * i as f32 / ring_segs as f32;
        let ct = theta.cos();
        let st = theta.sin();
        for j in 0..=8u32 {
            let phi = 2.0 * PI * j as f32 / 8.0;
            let cp = phi.cos();
            let sp = phi.sin();
            let x = (ring_r_major + ring_r_minor * cp) * ct;
            let z = (ring_r_major + ring_r_minor * cp) * st;
            let y = ring_r_minor * sp;
            let nx = cp * ct;
            let nz = cp * st;
            let ny = sp;
            ring_verts.extend_from_slice(&[
                x,
                y + 0.65,
                z,
                nx,
                ny,
                nz,
                i as f32 / ring_segs as f32,
                j as f32 / 8.0,
            ]);
        }
    }
    let tube_ring = 9u32;
    for i in 0..ring_segs {
        for j in 0..8u32 {
            let a = i * tube_ring + j;
            let b = a + tube_ring;
            ring_idxs.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let seat = (ring_verts, ring_idxs);

    // Lid: thin disc on top
    let mut lid_verts = Vec::new();
    let mut lid_idxs = Vec::new();
    let lid_segs = 16u32;
    let lid_center = 0u32;
    lid_verts.extend_from_slice(&[0.0, 0.85, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5]);
    for j in 0..=lid_segs {
        let theta = 2.0 * PI * j as f32 / lid_segs as f32;
        let x = theta.cos() * 0.95;
        let z = theta.sin() * 0.95;
        lid_verts.extend_from_slice(&[
            x,
            0.85,
            z,
            0.0,
            1.0,
            0.0,
            0.5 + 0.5 * theta.cos(),
            0.5 + 0.5 * theta.sin(),
        ]);
    }
    for j in 0..lid_segs {
        lid_idxs.extend_from_slice(&[lid_center, lid_center + 1 + j, lid_center + 2 + j]);
    }
    let lid = (lid_verts, lid_idxs);

    merge_meshes(&[bowl, tank, seat, lid])
}

/// Nintendo Mii-style character body from primitives.
/// `body_build`: "slim", "average", "stocky", "athletic", "tall".
/// `height`: overall scale factor.
pub fn character_mesh(body_build: &str, height: f32) -> (Vec<f32>, Vec<u32>) {
    let segs = 12;
    let (body_w, body_h) = match body_build {
        "slim" => (0.25, 0.5),
        "stocky" => (0.45, 0.45),
        "athletic" => (0.35, 0.55),
        "tall" => (0.3, 0.65),
        _ => (0.35, 0.5), // average
    };

    // Head: sphere
    let head_r = 0.3;
    let head = sphere_mesh(segs, segs, head_r, 0.0, body_h + head_r + 0.05, 0.0);

    // Body: capsule (cylinder + sphere caps)
    let body = capsule(body_w, body_h * 0.5, segs);

    // Left arm
    let arm_r = 0.08;
    let arm_h = 0.35;
    let mut left_arm = cylinder_mesh(segs, arm_r, arm_h * 0.5, 0.0, 0.0, 0.0);
    offset_mesh(&mut left_arm, -(body_w + arm_r + 0.05), body_h * 0.2, 0.0);

    // Right arm
    let mut right_arm = cylinder_mesh(segs, arm_r, arm_h * 0.5, 0.0, 0.0, 0.0);
    offset_mesh(&mut right_arm, body_w + arm_r + 0.05, body_h * 0.2, 0.0);

    // Left leg
    let leg_r = 0.1;
    let leg_h = 0.4;
    let mut left_leg = cylinder_mesh(segs, leg_r, leg_h * 0.5, 0.0, 0.0, 0.0);
    offset_mesh(
        &mut left_leg,
        -body_w * 0.5,
        -(body_h * 0.5 + leg_h * 0.5 + 0.05),
        0.0,
    );

    // Right leg
    let mut right_leg = cylinder_mesh(segs, leg_r, leg_h * 0.5, 0.0, 0.0, 0.0);
    offset_mesh(
        &mut right_leg,
        body_w * 0.5,
        -(body_h * 0.5 + leg_h * 0.5 + 0.05),
        0.0,
    );

    let mut result = merge_meshes(&[head, body, left_arm, right_arm, left_leg, right_leg]);
    scale_mesh(&mut result, height, height, height);
    result
}

/// Sigma Gym dumbbell: two spheres connected by a thin cylinder bar.
pub fn dumbbell_mesh() -> (Vec<f32>, Vec<u32>) {
    let segs = 12;
    let weight_r = 0.4;
    let bar_r = 0.08;
    let bar_half = 0.7;

    let left_weight = sphere_mesh(segs, segs, weight_r, -bar_half, 0.0, 0.0);
    let right_weight = sphere_mesh(segs, segs, weight_r, bar_half, 0.0, 0.0);

    // Bar: cylinder rotated to lie along X (we generate along Y, then swap axes)
    let mut bar = cylinder_mesh(segs, bar_r, bar_half, 0.0, 0.0, 0.0);
    // Rotate 90 degrees: swap Y and X in vertex positions and normals
    {
        let v = &mut bar.0;
        let count = v.len() / 8;
        for i in 0..count {
            let px = v[i * 8];
            let py = v[i * 8 + 1];
            v[i * 8] = py;
            v[i * 8 + 1] = px;
            let nx = v[i * 8 + 3];
            let ny = v[i * 8 + 4];
            v[i * 8 + 3] = ny;
            v[i * 8 + 4] = nx;
        }
    }

    merge_meshes(&[left_weight, right_weight, bar])
}

/// Ohio Obelisk: tall tapered box (wider at base) + glowing sphere on top.
pub fn obelisk_mesh() -> (Vec<f32>, Vec<u32>) {
    // Tapered box: manual quad construction (base wider, top narrower)
    let base_w = 1.0;
    let top_w = 0.4;
    let h = 4.0;
    let depth = 0.8;
    let top_d = 0.35;

    let mut verts = Vec::new();
    let mut idxs = Vec::new();

    let mut add_quad = |p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], p3: [f32; 3], n: [f32; 3]| {
        let base = verts.len() as u32 / 8;
        verts.extend_from_slice(&[p0[0], p0[1], p0[2], n[0], n[1], n[2], 0.0, 0.0]);
        verts.extend_from_slice(&[p1[0], p1[1], p1[2], n[0], n[1], n[2], 1.0, 0.0]);
        verts.extend_from_slice(&[p2[0], p2[1], p2[2], n[0], n[1], n[2], 1.0, 1.0]);
        verts.extend_from_slice(&[p3[0], p3[1], p3[2], n[0], n[1], n[2], 0.0, 1.0]);
        idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    let bw = base_w * 0.5;
    let tw = top_w * 0.5;
    let bd = depth * 0.5;
    let td = top_d * 0.5;

    // Front face
    add_quad(
        [-bw, 0.0, bd],
        [bw, 0.0, bd],
        [tw, h, td],
        [-tw, h, td],
        [0.0, 0.0, 1.0],
    );
    // Back face
    add_quad(
        [bw, 0.0, -bd],
        [-bw, 0.0, -bd],
        [-tw, h, -td],
        [tw, h, -td],
        [0.0, 0.0, -1.0],
    );
    // Right face
    add_quad(
        [bw, 0.0, bd],
        [bw, 0.0, -bd],
        [tw, h, -td],
        [tw, h, td],
        [1.0, 0.0, 0.0],
    );
    // Left face
    add_quad(
        [-bw, 0.0, -bd],
        [-bw, 0.0, bd],
        [-tw, h, td],
        [-tw, h, -td],
        [-1.0, 0.0, 0.0],
    );
    // Top face
    add_quad(
        [-tw, h, td],
        [tw, h, td],
        [tw, h, -td],
        [-tw, h, -td],
        [0.0, 1.0, 0.0],
    );

    let obelisk = (verts, idxs);

    // Glowing sphere on top
    let orb = sphere_mesh(10, 10, 0.3, 0.0, h + 0.5, 0.0);

    merge_meshes(&[obelisk, orb])
}

/// Grimace Blob: distorted sphere using sine-wave displacement.
/// `wobble_phase` (0.0-1.0) creates different wobble shapes.
pub fn blob_mesh(wobble_phase: f32) -> (Vec<f32>, Vec<u32>) {
    let stacks = 20;
    let slices = 20;
    let base_radius = 1.0;
    let wobble_amp = 0.15;
    let phase = wobble_phase * 2.0 * PI;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=stacks {
        let phi = PI * i as f32 / stacks as f32;
        let y = phi.cos();
        let r = phi.sin();
        for j in 0..=slices {
            let theta = 2.0 * PI * j as f32 / slices as f32;
            let nx = r * theta.cos();
            let nz = r * theta.sin();
            let ny = y;

            // Sine-wave displacement for organic blobby feel
            let displacement = wobble_amp
                * ((3.0 * phi + phase).sin() * 0.5
                    + (2.0 * theta + phase * 1.3).sin() * 0.3
                    + (4.0 * phi + 3.0 * theta + phase * 0.7).sin() * 0.2);
            let radius = base_radius + displacement;

            vertices.extend_from_slice(&[
                nx * radius,
                ny * radius,
                nz * radius,
                nx,
                ny,
                nz,
                j as f32 / slices as f32,
                i as f32 / stacks as f32,
            ]);
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
    (vertices, indices)
}

/// Fanum Food Crate: rounded box with slightly bulging sides.
pub fn food_crate_mesh() -> (Vec<f32>, Vec<u32>) {
    let mut crate_mesh = rounded_box(1.2, 0.8, 1.0, 0.08);
    // Slight bulge: nudge center vertices outward a bit
    let v = &mut crate_mesh.0;
    let count = v.len() / 8;
    for i in 0..count {
        let px = v[i * 8];
        let py = v[i * 8 + 1];
        let pz = v[i * 8 + 2];
        // Bulge based on how centered the vertex is (abs distance from edge)
        let center_factor_y = 1.0 - (py.abs() / 0.4).min(1.0);
        let bulge = 0.03 * center_factor_y;
        let len_xz = (px * px + pz * pz).sqrt();
        if len_xz > 0.01 {
            v[i * 8] += px / len_xz * bulge;
            v[i * 8 + 2] += pz / len_xz * bulge;
        }
    }
    crate_mesh
}

/// Gyatt/Item Orb: sphere with sparkle/pulse displacement.
/// `pulse_phase` (0.0-1.0) scales radius between 0.9-1.1.
pub fn orb_mesh(pulse_phase: f32) -> (Vec<f32>, Vec<u32>) {
    let stacks = 16;
    let slices = 16;
    let base_radius = 0.5;
    let phase = pulse_phase * 2.0 * PI;
    let pulse_scale = 1.0 + 0.1 * phase.sin();

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=stacks {
        let phi = PI * i as f32 / stacks as f32;
        let y = phi.cos();
        let r = phi.sin();
        for j in 0..=slices {
            let theta = 2.0 * PI * j as f32 / slices as f32;
            let nx = r * theta.cos();
            let nz = r * theta.sin();
            let ny = y;

            // Subtle sparkle displacement
            let sparkle = 0.02
                * ((5.0 * theta + 3.0 * phi).sin() * 0.7
                    + (7.0 * theta - 2.0 * phi + phase).sin() * 0.3);
            let radius = base_radius * pulse_scale + sparkle;

            vertices.extend_from_slice(&[
                nx * radius,
                ny * radius,
                nz * radius,
                nx,
                ny,
                nz,
                j as f32 / slices as f32,
                i as f32 / stacks as f32,
            ]);
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
    (vertices, indices)
}

/// Torii Gate: two vertical pillars (cylinders) + two horizontal beams (boxes).
pub fn torii_gate_mesh() -> (Vec<f32>, Vec<u32>) {
    let segs = 12;
    let pillar_r = 0.15;
    let pillar_h = 3.0;
    let gate_w = 3.0;
    let beam_h = 0.2;
    let beam_d = 0.25;

    // Left pillar
    let left_pillar = cylinder_mesh(
        segs,
        pillar_r,
        pillar_h * 0.5,
        -gate_w * 0.5,
        pillar_h * 0.5,
        0.0,
    );
    // Right pillar
    let right_pillar = cylinder_mesh(
        segs,
        pillar_r,
        pillar_h * 0.5,
        gate_w * 0.5,
        pillar_h * 0.5,
        0.0,
    );

    // Top beam (kasagi): extends beyond pillars
    let mut top_beam = rounded_box(gate_w + 0.6, beam_h, beam_d, 0.04);
    offset_mesh(&mut top_beam, 0.0, pillar_h + beam_h * 0.5, 0.0);

    // Lower beam (nuki): between pillars
    let mut lower_beam = rounded_box(gate_w * 0.9, beam_h * 0.7, beam_d * 0.8, 0.03);
    offset_mesh(&mut lower_beam, 0.0, pillar_h * 0.75, 0.0);

    merge_meshes(&[left_pillar, right_pillar, top_beam, lower_beam])
}

// =============================================================================
// Brainrot Evolution — Pokémon-style multi-stage model transforms
// =============================================================================

/// Brainrot character identifier for evolution mesh dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrainrotCharacter {
    Skibidi,
    Sigma,
    Ohio,
    Grimace,
    Rizz,
    Fanum,
}

impl BrainrotCharacter {
    pub fn max_stage(self) -> u8 {
        match self {
            Self::Skibidi => 3,
            Self::Sigma => 4,
            Self::Ohio => 2,
            Self::Grimace => 3,
            Self::Rizz => 2,
            Self::Fanum => 3,
        }
    }

    pub fn stage_name(self, stage: u8) -> &'static str {
        match (self, stage) {
            (Self::Skibidi, 0) => "Mini Toilet",
            (Self::Skibidi, 1) => "Skibidi Soldier",
            (Self::Skibidi, 2) => "Skibidi Tank",
            (Self::Skibidi, 3) => "Skibidi Titan",
            (Self::Sigma, 0) => "Scrawny Kid",
            (Self::Sigma, 1) => "Gym Bro",
            (Self::Sigma, 2) => "Sigma Male",
            (Self::Sigma, 3) => "Gigachad",
            (Self::Sigma, 4) => "Sigma Ascended",
            (Self::Ohio, 0) => "Ohio Anomaly",
            (Self::Ohio, 1) => "Ohio Nightmare",
            (Self::Ohio, 2) => "Ohio Eldritch",
            (Self::Grimace, 0) => "Purple Puddle",
            (Self::Grimace, 1) => "Grimace Blob",
            (Self::Grimace, 2) => "Grimace Tide",
            (Self::Grimace, 3) => "Grimace Singularity",
            (Self::Rizz, 0) => "Awkward Kid",
            (Self::Rizz, 1) => "Rizz Master",
            (Self::Rizz, 2) => "Rizz Sensei",
            (Self::Fanum, 0) => "Street Kid",
            (Self::Fanum, 1) => "Tax Collector",
            (Self::Fanum, 2) => "Tax Baron",
            (Self::Fanum, 3) => "Fanum Mogul",
            _ => "Unknown",
        }
    }

    pub fn stage_scale(self, stage: u8) -> f32 {
        match (self, stage) {
            (Self::Skibidi, 0) => 0.6,
            (Self::Skibidi, 1) => 1.0,
            (Self::Skibidi, 2) => 1.8,
            (Self::Skibidi, 3) => 3.0,
            (Self::Sigma, 0) => 0.7,
            (Self::Sigma, 1) => 1.0,
            (Self::Sigma, 2) => 1.1,
            (Self::Sigma, 3) => 1.3,
            (Self::Sigma, 4) => 1.5,
            (Self::Ohio, 0) => 1.0,
            (Self::Ohio, 1) => 2.0,
            (Self::Ohio, 2) => 4.0,
            (Self::Grimace, 0) => 0.5,
            (Self::Grimace, 1) => 1.0,
            (Self::Grimace, 2) => 1.8,
            (Self::Grimace, 3) => 2.5,
            (Self::Rizz, 0) => 0.8,
            (Self::Rizz, 1) => 1.0,
            (Self::Rizz, 2) => 1.1,
            (Self::Fanum, 0) => 0.8,
            (Self::Fanum, 1) => 1.0,
            (Self::Fanum, 2) => 1.1,
            (Self::Fanum, 3) => 1.4,
            _ => 1.0,
        }
    }
}

/// Stage-aware mesh generator for all brainrot characters.
/// Returns (vertices, indices) in interleaved pos3+norm3+uv2 format.
/// `phase` is animation phase (0.0-1.0) for wobble/pulse/orbit effects.
pub fn brainrot_evolution_mesh(
    character: BrainrotCharacter,
    stage: u8,
    phase: f32,
) -> (Vec<f32>, Vec<u32>) {
    let stage = stage.min(character.max_stage());
    match character {
        BrainrotCharacter::Skibidi => skibidi_evolution_mesh(stage),
        BrainrotCharacter::Sigma => sigma_evolution_mesh(stage),
        BrainrotCharacter::Ohio => ohio_evolution_mesh(stage, phase),
        BrainrotCharacter::Grimace => grimace_evolution_mesh(stage, phase),
        BrainrotCharacter::Rizz => rizz_evolution_mesh(stage),
        BrainrotCharacter::Fanum => fanum_evolution_mesh(stage),
    }
}

/// Skibidi: Mini Toilet(0) → Soldier(1) → Tank(2) → Titan(3)
fn skibidi_evolution_mesh(stage: u8) -> (Vec<f32>, Vec<u32>) {
    match stage {
        0 => {
            // Mini Toilet — small toilet + 2 stubby legs
            let mut t = toilet_mesh();
            scale_mesh(&mut t, 0.6, 0.6, 0.6);
            let leg_l = cylinder_mesh(8, 0.08, 0.15, -0.15, -0.4, 0.0);
            let leg_r = cylinder_mesh(8, 0.08, 0.15, 0.15, -0.4, 0.0);
            merge_meshes(&[t, leg_l, leg_r])
        }
        1 => {
            // Soldier — human torso emerging from toilet
            let t = toilet_mesh();
            let mut torso = character_mesh("stocky", 0.7);
            offset_mesh(&mut torso, 0.0, 0.8, 0.0);
            merge_meshes(&[t, torso])
        }
        2 => {
            // Tank — mega toilet + 4 camera heads + treads
            let mut base = toilet_mesh();
            scale_mesh(&mut base, 2.0, 2.0, 2.0);
            let cams: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    sphere_mesh(8, 12, 0.25, angle.cos() * 1.2, 1.8, angle.sin() * 1.2)
                })
                .collect();
            let mut tread_l = rounded_box(2.0, 0.3, 0.4, 0.05);
            offset_mesh(&mut tread_l, -1.5, -0.8, 0.0);
            let mut tread_r = rounded_box(2.0, 0.3, 0.4, 0.05);
            offset_mesh(&mut tread_r, 1.5, -0.8, 0.0);
            let mut all = vec![base, tread_l, tread_r];
            all.extend(cams);
            merge_meshes(&all)
        }
        _ => {
            // Titan — fortress toilet + 4 obelisk towers + 8 orbital heads
            let mut base = toilet_mesh();
            scale_mesh(&mut base, 3.0, 3.0, 3.0);
            let towers: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let mut ob = obelisk_mesh();
                    scale_mesh(&mut ob, 0.5, 0.6, 0.5);
                    offset_mesh(&mut ob, angle.cos() * 2.5, 0.0, angle.sin() * 2.5);
                    ob
                })
                .collect();
            let heads: Vec<_> = (0..8)
                .map(|i| {
                    let angle = i as f32 * PI / 4.0;
                    sphere_mesh(8, 12, 0.3, angle.cos() * 3.5, 4.0, angle.sin() * 3.5)
                })
                .collect();
            let mut all = vec![base];
            all.extend(towers);
            all.extend(heads);
            merge_meshes(&all)
        }
    }
}

/// Sigma: Scrawny Kid(0) → Gym Bro(1) → Sigma Male(2) → Gigachad(3) → Ascended(4)
fn sigma_evolution_mesh(stage: u8) -> (Vec<f32>, Vec<u32>) {
    match stage {
        0 => character_mesh("slim", 0.85),
        1 => {
            // Gym Bro — athletic + single dumbbell
            let body = character_mesh("athletic", 1.0);
            let mut db = dumbbell_mesh();
            scale_mesh(&mut db, 0.6, 0.6, 0.6);
            offset_mesh(&mut db, 0.5, 0.2, 0.0);
            merge_meshes(&[body, db])
        }
        2 => {
            // Sigma Male — larger athletic + barbell
            let body = character_mesh("athletic", 1.1);
            let bar = cylinder_mesh(12, 0.04, 0.8, 0.0, 0.8, 0.0);
            let w_l = sphere_mesh(10, 10, 0.25, -0.8, 0.8, 0.0);
            let w_r = sphere_mesh(10, 10, 0.25, 0.8, 0.8, 0.0);
            merge_meshes(&[body, bar, w_l, w_r])
        }
        3 => {
            // Gigachad — stocky giant + jaw plate + throne
            let body = character_mesh("stocky", 1.3);
            let mut jaw = rounded_box(0.5, 0.15, 0.3, 0.03);
            offset_mesh(&mut jaw, 0.0, 0.85, 0.15);
            let mut throne = rounded_box(1.5, 2.0, 1.0, 0.1);
            offset_mesh(&mut throne, 0.0, 0.0, -0.8);
            merge_meshes(&[body, jaw, throne])
        }
        _ => {
            // Sigma Ascended — tall + 6 orbiting aura orbs + armor plates
            let body = character_mesh("tall", 1.5);
            let orbs: Vec<_> = (0..6)
                .map(|i| {
                    let angle = i as f32 * PI / 3.0;
                    orb_mesh(i as f32 / 6.0)
                })
                .collect();
            let mut orbs_positioned: Vec<(Vec<f32>, Vec<u32>)> = orbs
                .into_iter()
                .enumerate()
                .map(|(i, mut o)| {
                    let angle = i as f32 * PI / 3.0;
                    offset_mesh(
                        &mut o,
                        angle.cos() * 1.0,
                        1.0 + (i as f32 * 0.15),
                        angle.sin() * 1.0,
                    );
                    o
                })
                .collect();
            let mut plates: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let mut plate = rounded_box(0.4, 0.8, 0.05, 0.02);
                    offset_mesh(&mut plate, angle.cos() * 0.6, 0.3, angle.sin() * 0.6);
                    plate
                })
                .collect();
            let mut all = vec![body];
            all.append(&mut orbs_positioned);
            all.append(&mut plates);
            merge_meshes(&all)
        }
    }
}

/// Ohio: Anomaly(0) → Nightmare(1) → Eldritch(2)
fn ohio_evolution_mesh(stage: u8, phase: f32) -> (Vec<f32>, Vec<u32>) {
    match stage {
        0 => {
            // Ohio Anomaly — single obelisk + 6 floating cubes
            let ob = obelisk_mesh();
            let cubes: Vec<_> = (0..6)
                .map(|i| {
                    let angle = i as f32 * PI / 3.0;
                    let mut c = rounded_box(0.6, 0.6, 0.6, 0.05);
                    offset_mesh(
                        &mut c,
                        angle.cos() * 2.5,
                        1.5 + i as f32 * 0.4,
                        angle.sin() * 2.5,
                    );
                    c
                })
                .collect();
            let mut all = vec![ob];
            all.extend(cubes);
            merge_meshes(&all)
        }
        1 => {
            // Nightmare — 3 fused obelisks + tentacles + eye
            let mut ob1 = obelisk_mesh();
            let mut ob2 = obelisk_mesh();
            scale_mesh(&mut ob2, 0.8, 0.8, 0.8);
            offset_mesh(&mut ob2, 1.0, 0.0, 0.5);
            let mut ob3 = obelisk_mesh();
            scale_mesh(&mut ob3, 0.7, 0.7, 0.7);
            offset_mesh(&mut ob3, -0.8, 0.0, -0.6);
            let eye = sphere_mesh(12, 12, 0.5, 0.0, 5.0, 0.0);
            let tentacles: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let mut t = capsule(0.12, 1.5, 10);
                    offset_mesh(&mut t, angle.cos() * 1.5, 2.0, angle.sin() * 1.5);
                    t
                })
                .collect();
            let mut all = vec![ob1, ob2, ob3, eye];
            all.extend(tentacles);
            let mut result = merge_meshes(&all);
            scale_mesh(&mut result, 2.0, 2.0, 2.0);
            result
        }
        _ => {
            // Eldritch — fractal obelisks + rotating torii ring + 12 orbiting orbs
            let mut core = obelisk_mesh();
            scale_mesh(&mut core, 1.5, 1.5, 1.5);
            let sub_obs: Vec<_> = (0..6)
                .map(|i| {
                    let angle = i as f32 * PI / 3.0;
                    let mut ob = obelisk_mesh();
                    scale_mesh(&mut ob, 0.4, 0.5, 0.4);
                    offset_mesh(&mut ob, angle.cos() * 3.0, 0.0, angle.sin() * 3.0);
                    ob
                })
                .collect();
            let mut gate = torii_gate_mesh();
            scale_mesh(&mut gate, 1.5, 1.2, 1.5);
            offset_mesh(&mut gate, 0.0, 2.0, 0.0);
            let orbs: Vec<_> = (0..12)
                .map(|i| {
                    let angle = i as f32 * PI / 6.0 + phase * 2.0 * PI;
                    let r = 4.0;
                    orb_mesh(phase + i as f32 / 12.0)
                })
                .collect();
            let mut orbs_pos: Vec<_> = orbs
                .into_iter()
                .enumerate()
                .map(|(i, mut o)| {
                    let angle = i as f32 * PI / 6.0;
                    offset_mesh(
                        &mut o,
                        angle.cos() * 4.0,
                        3.0 + (i as f32 * 0.2),
                        angle.sin() * 4.0,
                    );
                    o
                })
                .collect();
            let mut all = vec![core, gate];
            all.extend(sub_obs);
            all.append(&mut orbs_pos);
            let mut result = merge_meshes(&all);
            scale_mesh(&mut result, 2.0, 2.0, 2.0);
            result
        }
    }
}

/// Grimace: Purple Puddle(0) → Blob(1) → Tide(2) → Singularity(3)
fn grimace_evolution_mesh(stage: u8, phase: f32) -> (Vec<f32>, Vec<u32>) {
    match stage {
        0 => {
            // Purple Puddle — flat blob
            let mut b = blob_mesh(0.0);
            scale_mesh(&mut b, 1.0, 0.3, 1.0);
            b
        }
        1 => {
            // Grimace Blob — full sphere + 2 eyes
            let b = blob_mesh(phase);
            let eye_l = sphere_mesh(8, 8, 0.15, -0.3, 0.7, 0.5);
            let eye_r = sphere_mesh(8, 8, 0.15, 0.3, 0.7, 0.5);
            merge_meshes(&[b, eye_l, eye_r])
        }
        2 => {
            // Grimace Tide — mega blob + 4 satellite blobs
            let mut main = blob_mesh(phase);
            scale_mesh(&mut main, 1.8, 1.8, 1.8);
            let sats: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let mut s = blob_mesh(phase + i as f32 * 0.25);
                    scale_mesh(&mut s, 0.4, 0.4, 0.4);
                    offset_mesh(&mut s, angle.cos() * 2.5, 0.5, angle.sin() * 2.5);
                    s
                })
                .collect();
            let mut all = vec![main];
            all.extend(sats);
            merge_meshes(&all)
        }
        _ => {
            // Grimace Singularity — hollow shell + void core + 8 vortex arms + 20 particles
            let shell = blob_mesh(phase);
            let mut shell_scaled = shell;
            scale_mesh(&mut shell_scaled, 2.5, 2.5, 2.5);
            let core = orb_mesh(phase);
            let mut core_pos = core;
            scale_mesh(&mut core_pos, 0.8, 0.8, 0.8);
            let arms: Vec<_> = (0..8)
                .map(|i| {
                    let angle = i as f32 * PI / 4.0;
                    let mut arm = capsule(0.1, 1.2, 8);
                    offset_mesh(&mut arm, angle.cos() * 2.0, 0.0, angle.sin() * 2.0);
                    arm
                })
                .collect();
            let particles: Vec<_> = (0..20)
                .map(|i| {
                    let angle = i as f32 * PI / 10.0 + phase * PI;
                    let r = 1.5 + (i as f32 * 0.1);
                    let y = (i as f32 * 0.3).sin() * 1.5;
                    sphere_mesh(4, 4, 0.06, angle.cos() * r, y, angle.sin() * r)
                })
                .collect();
            let mut all = vec![shell_scaled, core_pos];
            all.extend(arms);
            all.extend(particles);
            merge_meshes(&all)
        }
    }
}

/// Rizz: Awkward Kid(0) → Rizz Master(1) → Rizz Sensei(2)
fn rizz_evolution_mesh(stage: u8) -> (Vec<f32>, Vec<u32>) {
    match stage {
        0 => character_mesh("slim", 0.8),
        1 => {
            // Rizz Master — slim + earring orb + 4 sparkle orbs
            let body = character_mesh("slim", 1.05);
            let earring = sphere_mesh(6, 6, 0.04, 0.35, 0.85, 0.0);
            let sparkles: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let mut s = orb_mesh(i as f32 / 4.0);
                    scale_mesh(&mut s, 0.2, 0.2, 0.2);
                    offset_mesh(&mut s, angle.cos() * 0.6, 1.2, angle.sin() * 0.6);
                    s
                })
                .collect();
            let mut all = vec![body, earring];
            all.extend(sparkles);
            merge_meshes(&all)
        }
        _ => {
            // Rizz Sensei — slim + robe panels + crown frame + scroll props
            let body = character_mesh("slim", 1.1);
            let robe_panels: Vec<_> = (0..4)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let mut panel = rounded_box(0.4, 0.8, 0.05, 0.02);
                    offset_mesh(&mut panel, angle.cos() * 0.4, -0.1, angle.sin() * 0.4);
                    panel
                })
                .collect();
            let mut crown = torii_gate_mesh();
            scale_mesh(&mut crown, 0.15, 0.1, 0.15);
            offset_mesh(&mut crown, 0.0, 1.3, 0.0);
            let mut scroll_l = rounded_box(0.15, 0.3, 0.08, 0.02);
            offset_mesh(&mut scroll_l, -0.6, 0.5, 0.0);
            let mut scroll_r = rounded_box(0.15, 0.3, 0.08, 0.02);
            offset_mesh(&mut scroll_r, 0.6, 0.5, 0.0);
            let mut all = vec![body, crown, scroll_l, scroll_r];
            all.extend(robe_panels);
            merge_meshes(&all)
        }
    }
}

/// Fanum: Street Kid(0) → Tax Collector(1) → Tax Baron(2) → Fanum Mogul(3)
fn fanum_evolution_mesh(stage: u8) -> (Vec<f32>, Vec<u32>) {
    match stage {
        0 => {
            // Street Kid — small average + 1 food crate
            let body = character_mesh("average", 0.85);
            let mut crate1 = food_crate_mesh();
            scale_mesh(&mut crate1, 0.4, 0.4, 0.4);
            offset_mesh(&mut crate1, 0.4, 0.2, 0.0);
            merge_meshes(&[body, crate1])
        }
        1 => {
            // Tax Collector — average + 3 stacked backpack crates
            let body = character_mesh("average", 1.0);
            let crates: Vec<_> = (0..3)
                .map(|i| {
                    let mut c = food_crate_mesh();
                    scale_mesh(&mut c, 0.35, 0.35, 0.35);
                    offset_mesh(&mut c, 0.0, 0.1 + i as f32 * 0.3, -0.35);
                    c
                })
                .collect();
            let mut all = vec![body];
            all.extend(crates);
            merge_meshes(&all)
        }
        2 => {
            // Tax Baron — stocky + cart + 8 crates + 2 wheels
            let body = character_mesh("stocky", 1.1);
            let mut cart = rounded_box(1.5, 0.4, 0.8, 0.05);
            offset_mesh(&mut cart, 0.0, 0.3, -0.8);
            let crates: Vec<_> = (0..8)
                .map(|i| {
                    let mut c = food_crate_mesh();
                    scale_mesh(&mut c, 0.25, 0.25, 0.25);
                    let col = (i % 4) as f32;
                    let row = (i / 4) as f32;
                    offset_mesh(&mut c, -0.5 + col * 0.35, 0.55 + row * 0.22, -0.8);
                    c
                })
                .collect();
            let wheel_l = cylinder_mesh(12, 0.15, 0.05, -0.7, 0.15, -0.8);
            let wheel_r = cylinder_mesh(12, 0.15, 0.05, 0.7, 0.15, -0.8);
            let mut all = vec![body, cart, wheel_l, wheel_r];
            all.extend(crates);
            merge_meshes(&all)
        }
        _ => {
            // Fanum Mogul — big stocky on food throne + 12 orbiting crates + market gate
            let body = character_mesh("stocky", 1.4);
            let mut throne = rounded_box(1.8, 2.2, 1.2, 0.1);
            offset_mesh(&mut throne, 0.0, 0.0, -0.5);
            let crates: Vec<_> = (0..12)
                .map(|i| {
                    let angle = i as f32 * PI / 6.0;
                    let mut c = food_crate_mesh();
                    scale_mesh(&mut c, 0.3, 0.3, 0.3);
                    offset_mesh(
                        &mut c,
                        angle.cos() * 2.2,
                        0.8 + (i as f32 * 0.1).sin() * 0.3,
                        angle.sin() * 2.2,
                    );
                    c
                })
                .collect();
            let mut gate = torii_gate_mesh();
            scale_mesh(&mut gate, 0.8, 0.7, 0.8);
            offset_mesh(&mut gate, 0.0, 0.0, 2.0);
            let mut all = vec![body, throne, gate];
            all.extend(crates);
            merge_meshes(&all)
        }
    }
}

/// Gorilla mesh — SmoothUnion-style organic body for Goriketsu Dash!!
/// Massive silverback with oversized red butt (the game's star feature).
/// `anger_phase` 0.0-1.0 controls chest expansion + butt pulsation.
pub fn gorilla_mesh(anger_phase: f32) -> (Vec<f32>, Vec<u32>) {
    let chest_expand = 1.0 + anger_phase * 0.15;
    let butt_pulse = 1.0 + (anger_phase * PI * 2.0).sin().abs() * 0.2;

    // Torso — massive barrel chest
    let torso = sphere_mesh(12, 16, 1.8 * chest_expand, 0.0, 2.0, 0.0);
    // Belly — pot belly
    let belly = sphere_mesh(10, 14, 1.2 * chest_expand, 0.0, 1.5, 0.5);
    // Head
    let head = sphere_mesh(12, 16, 0.9, 0.0, 3.8, 0.3);
    // Sagittal crest
    let crest = rounded_box(0.3, 0.6, 0.8, 0.08);
    let mut crest = crest;
    offset_mesh(&mut crest, 0.0, 4.3, 0.2);
    // Brow ridge
    let brow = rounded_box(0.7, 0.15, 0.3, 0.04);
    let mut brow = brow;
    offset_mesh(&mut brow, 0.0, 3.95, 0.9);

    // Arms — gorilla arms are LONG (knuckle-walking)
    let arm_l_upper = capsule(0.45, 1.2, 12);
    let mut arm_l_upper = arm_l_upper;
    offset_mesh(&mut arm_l_upper, -1.8, 2.5, 0.0);
    let arm_l_fore = capsule(0.35, 1.0, 10);
    let mut arm_l_fore = arm_l_fore;
    offset_mesh(&mut arm_l_fore, -2.3, 1.2, 0.0);
    let fist_l = sphere_mesh(8, 10, 0.4, -2.4, 0.35, 0.2);

    let arm_r_upper = capsule(0.45, 1.2, 12);
    let mut arm_r_upper = arm_r_upper;
    offset_mesh(&mut arm_r_upper, 1.8, 2.5, 0.0);
    let arm_r_fore = capsule(0.35, 1.0, 10);
    let mut arm_r_fore = arm_r_fore;
    offset_mesh(&mut arm_r_fore, 2.3, 1.2, 0.0);
    let fist_r = sphere_mesh(8, 10, 0.4, 2.4, 0.35, 0.2);

    // Legs — short and thick
    let leg_l = capsule(0.4, 0.7, 10);
    let mut leg_l = leg_l;
    offset_mesh(&mut leg_l, -0.7, 0.7, 0.0);
    let leg_r = capsule(0.4, 0.7, 10);
    let mut leg_r = leg_r;
    offset_mesh(&mut leg_r, 0.7, 0.7, 0.0);
    let foot_l = rounded_box(0.35, 0.15, 0.5, 0.05);
    let mut foot_l = foot_l;
    offset_mesh(&mut foot_l, -0.7, 0.15, 0.3);
    let foot_r = rounded_box(0.35, 0.15, 0.5, 0.05);
    let mut foot_r = foot_r;
    offset_mesh(&mut foot_r, 0.7, 0.15, 0.3);

    // BUTT — THE STAR OF THE SHOW — bright red, oversized, pulsating
    let butt_r = 1.1 * butt_pulse;
    let butt_l_cheek = sphere_mesh(12, 16, butt_r, -0.4, 1.0, -1.0);
    let butt_r_cheek = sphere_mesh(12, 16, butt_r, 0.4, 1.0, -1.0);

    merge_meshes(&[
        torso,
        belly,
        head,
        crest,
        brow,
        arm_l_upper,
        arm_l_fore,
        fist_l,
        arm_r_upper,
        arm_r_fore,
        fist_r,
        leg_l,
        leg_r,
        foot_l,
        foot_r,
        butt_l_cheek,
        butt_r_cheek,
    ])
}

/// Baby gorilla mesh — cute small version with big eyes
pub fn baby_gorilla_mesh() -> (Vec<f32>, Vec<u32>) {
    let body = sphere_mesh(10, 12, 0.5, 0.0, 0.7, 0.0);
    let head = sphere_mesh(10, 12, 0.4, 0.0, 1.3, 0.1);
    // Big cute eyes
    let eye_l = sphere_mesh(6, 8, 0.12, -0.15, 1.4, 0.4);
    let eye_r = sphere_mesh(6, 8, 0.12, 0.15, 1.4, 0.4);
    // Stubby arms
    let arm_l = capsule(0.12, 0.3, 8);
    let mut arm_l = arm_l;
    offset_mesh(&mut arm_l, -0.5, 0.8, 0.0);
    let arm_r = capsule(0.12, 0.3, 8);
    let mut arm_r = arm_r;
    offset_mesh(&mut arm_r, 0.5, 0.8, 0.0);
    // Stubby legs
    let leg_l = capsule(0.1, 0.2, 8);
    let mut leg_l = leg_l;
    offset_mesh(&mut leg_l, -0.2, 0.25, 0.0);
    let leg_r = capsule(0.1, 0.2, 8);
    let mut leg_r = leg_r;
    offset_mesh(&mut leg_r, 0.2, 0.25, 0.0);

    merge_meshes(&[body, head, eye_l, eye_r, arm_l, arm_r, leg_l, leg_r])
}

/// Banana mesh — curved yellow fruit
pub fn banana_mesh() -> (Vec<f32>, Vec<u32>) {
    // Main curved body (capsule tilted)
    let mut body = capsule(0.12, 0.6, 10);
    // Tip
    let tip = sphere_mesh(6, 6, 0.06, 0.15, 0.62, 0.0);
    // Stem
    let stem = cylinder_mesh(6, 0.03, 0.08, -0.12, 0.02, 0.0);

    merge_meshes(&[body, tip, stem])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_mesh(name: &str, verts: &[f32], idxs: &[u32]) {
        assert!(!verts.is_empty(), "{name}: vertices empty");
        assert!(!idxs.is_empty(), "{name}: indices empty");
        assert_eq!(
            verts.len() % 8,
            0,
            "{name}: vertex count not divisible by 8 (got {})",
            verts.len()
        );
        let max_vertex = verts.len() as u32 / 8;
        for (i, &idx) in idxs.iter().enumerate() {
            assert!(
                idx < max_vertex,
                "{name}: index {i} = {idx} out of range (max {max_vertex})"
            );
        }
    }

    #[test]
    fn toilet_mesh_valid() {
        let (v, i) = toilet_mesh();
        validate_mesh("toilet", &v, &i);
    }

    #[test]
    fn character_mesh_builds() {
        for build in &["slim", "average", "stocky", "athletic", "tall"] {
            let (v, i) = character_mesh(build, 1.0);
            validate_mesh(&format!("character({build})"), &v, &i);
        }
    }

    #[test]
    fn character_mesh_scales_with_height() {
        let (v1, _) = character_mesh("average", 1.0);
        let (v2, _) = character_mesh("average", 2.0);
        // Vertices at height 2 should be roughly 2x the positions of height 1
        assert!((v2[0] - v1[0] * 2.0).abs() < 0.001);
    }

    #[test]
    fn dumbbell_mesh_valid() {
        let (v, i) = dumbbell_mesh();
        validate_mesh("dumbbell", &v, &i);
    }

    #[test]
    fn obelisk_mesh_valid() {
        let (v, i) = obelisk_mesh();
        validate_mesh("obelisk", &v, &i);
    }

    #[test]
    fn blob_mesh_valid() {
        for phase in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (v, i) = blob_mesh(phase);
            validate_mesh(&format!("blob({phase})"), &v, &i);
        }
    }

    #[test]
    fn blob_mesh_wobble_changes_shape() {
        let (v1, _) = blob_mesh(0.0);
        let (v2, _) = blob_mesh(0.5);
        // Different phases should produce different vertex positions
        let diff: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 0.01, "blob wobble should change shape");
    }

    #[test]
    fn food_crate_mesh_valid() {
        let (v, i) = food_crate_mesh();
        validate_mesh("food_crate", &v, &i);
    }

    #[test]
    fn orb_mesh_valid() {
        for phase in [0.0, 0.5, 1.0] {
            let (v, i) = orb_mesh(phase);
            validate_mesh(&format!("orb({phase})"), &v, &i);
        }
    }

    #[test]
    fn torii_gate_mesh_valid() {
        let (v, i) = torii_gate_mesh();
        validate_mesh("torii_gate", &v, &i);
    }

    #[test]
    fn rounded_box_valid() {
        let (v, i) = rounded_box(2.0, 1.0, 1.5, 0.1);
        validate_mesh("rounded_box", &v, &i);
    }

    #[test]
    fn capsule_valid() {
        let (v, i) = capsule(0.5, 1.0, 12);
        validate_mesh("capsule", &v, &i);
    }

    #[test]
    fn merge_meshes_offsets_indices() {
        let a = sphere_mesh(4, 4, 0.5, 0.0, 0.0, 0.0);
        let b = sphere_mesh(4, 4, 0.5, 2.0, 0.0, 0.0);
        let (v, i) = merge_meshes(&[a, b]);
        let max_v = v.len() as u32 / 8;
        for &idx in &i {
            assert!(idx < max_v, "merged index out of range");
        }
    }

    // ── Evolution mesh tests ──

    #[test]
    fn brainrot_character_max_stages() {
        assert_eq!(BrainrotCharacter::Skibidi.max_stage(), 3);
        assert_eq!(BrainrotCharacter::Sigma.max_stage(), 4);
        assert_eq!(BrainrotCharacter::Ohio.max_stage(), 2);
        assert_eq!(BrainrotCharacter::Grimace.max_stage(), 3);
        assert_eq!(BrainrotCharacter::Rizz.max_stage(), 2);
        assert_eq!(BrainrotCharacter::Fanum.max_stage(), 3);
    }

    #[test]
    fn brainrot_stage_names() {
        assert_eq!(BrainrotCharacter::Skibidi.stage_name(0), "Mini Toilet");
        assert_eq!(BrainrotCharacter::Skibidi.stage_name(3), "Skibidi Titan");
        assert_eq!(BrainrotCharacter::Sigma.stage_name(4), "Sigma Ascended");
        assert_eq!(BrainrotCharacter::Ohio.stage_name(2), "Ohio Eldritch");
        assert_eq!(
            BrainrotCharacter::Grimace.stage_name(3),
            "Grimace Singularity"
        );
        assert_eq!(BrainrotCharacter::Rizz.stage_name(2), "Rizz Sensei");
        assert_eq!(BrainrotCharacter::Fanum.stage_name(3), "Fanum Mogul");
    }

    #[test]
    fn skibidi_all_stages_valid() {
        for stage in 0..=3u8 {
            let (v, i) = brainrot_evolution_mesh(BrainrotCharacter::Skibidi, stage, 0.0);
            validate_mesh(&format!("skibidi_stage{stage}"), &v, &i);
        }
    }

    #[test]
    fn sigma_all_stages_valid() {
        for stage in 0..=4u8 {
            let (v, i) = brainrot_evolution_mesh(BrainrotCharacter::Sigma, stage, 0.0);
            validate_mesh(&format!("sigma_stage{stage}"), &v, &i);
        }
    }

    #[test]
    fn ohio_all_stages_valid() {
        for stage in 0..=2u8 {
            let (v, i) = brainrot_evolution_mesh(BrainrotCharacter::Ohio, stage, 0.5);
            validate_mesh(&format!("ohio_stage{stage}"), &v, &i);
        }
    }

    #[test]
    fn grimace_all_stages_valid() {
        for stage in 0..=3u8 {
            let (v, i) = brainrot_evolution_mesh(BrainrotCharacter::Grimace, stage, 0.3);
            validate_mesh(&format!("grimace_stage{stage}"), &v, &i);
        }
    }

    #[test]
    fn rizz_all_stages_valid() {
        for stage in 0..=2u8 {
            let (v, i) = brainrot_evolution_mesh(BrainrotCharacter::Rizz, stage, 0.0);
            validate_mesh(&format!("rizz_stage{stage}"), &v, &i);
        }
    }

    #[test]
    fn fanum_all_stages_valid() {
        for stage in 0..=3u8 {
            let (v, i) = brainrot_evolution_mesh(BrainrotCharacter::Fanum, stage, 0.0);
            validate_mesh(&format!("fanum_stage{stage}"), &v, &i);
        }
    }

    #[test]
    fn evolution_mesh_grows_with_stage() {
        // Higher stages should generally produce larger meshes (more vertices)
        let (v0, _) = brainrot_evolution_mesh(BrainrotCharacter::Skibidi, 0, 0.0);
        let (v3, _) = brainrot_evolution_mesh(BrainrotCharacter::Skibidi, 3, 0.0);
        assert!(
            v3.len() > v0.len(),
            "Skibidi Titan should have more vertices than Mini Toilet"
        );

        let (v0, _) = brainrot_evolution_mesh(BrainrotCharacter::Grimace, 0, 0.0);
        let (v3, _) = brainrot_evolution_mesh(BrainrotCharacter::Grimace, 3, 0.0);
        assert!(
            v3.len() > v0.len(),
            "Grimace Singularity should have more vertices than Purple Puddle"
        );
    }

    #[test]
    fn evolution_stage_clamped() {
        // Stage beyond max should clamp to max
        let (v_max, _) = brainrot_evolution_mesh(BrainrotCharacter::Skibidi, 3, 0.0);
        let (v_over, _) = brainrot_evolution_mesh(BrainrotCharacter::Skibidi, 99, 0.0);
        assert_eq!(
            v_max.len(),
            v_over.len(),
            "stage 99 should clamp to max stage 3"
        );
    }

    #[test]
    fn gorilla_mesh_valid() {
        for phase in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (v, i) = gorilla_mesh(phase);
            validate_mesh(&format!("gorilla({phase})"), &v, &i);
        }
    }

    #[test]
    fn gorilla_mesh_anger_changes_shape() {
        let (v1, _) = gorilla_mesh(0.0);
        let (v2, _) = gorilla_mesh(1.0);
        let diff: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 0.01, "gorilla anger should change mesh shape");
    }

    #[test]
    fn baby_gorilla_mesh_valid() {
        let (v, i) = baby_gorilla_mesh();
        validate_mesh("baby_gorilla", &v, &i);
    }

    #[test]
    fn banana_mesh_valid() {
        let (v, i) = banana_mesh();
        validate_mesh("banana", &v, &i);
    }

    #[test]
    fn gorilla_butt_is_prominent() {
        let (v, _) = gorilla_mesh(0.5);
        // Check that vertices extend significantly in the -Z direction (butt area)
        let min_z = v.chunks(8).map(|c| c[2]).fold(f32::INFINITY, f32::min);
        assert!(
            min_z < -0.5,
            "gorilla butt should extend behind the body (min_z={min_z})"
        );
    }

    #[test]
    fn evolution_scale_increases_with_stage() {
        for character in [
            BrainrotCharacter::Skibidi,
            BrainrotCharacter::Sigma,
            BrainrotCharacter::Ohio,
            BrainrotCharacter::Grimace,
            BrainrotCharacter::Rizz,
            BrainrotCharacter::Fanum,
        ] {
            let s0 = character.stage_scale(0);
            let s_max = character.stage_scale(character.max_stage());
            assert!(
                s_max >= s0,
                "{:?} final scale should be >= initial scale",
                character
            );
        }
    }
}
