//! Base head mesh generation — FLAME-compatible topology with facial features.

use glam::Vec3;
use std::f32::consts::PI;
use crate::{Vertex, MeshPart, MaterialId};
use crate::params::EyeParams;

/// Generate base head mesh with facial feature deformations.
/// Returns (positions, indices). Normals/UVs computed separately.
pub fn generate_head(n_lat: u32, n_lon: u32) -> (Vec<Vec3>, Vec<u32>) {
    let mut verts = Vec::with_capacity(((n_lat + 1) * (n_lon + 1)) as usize);
    let nl = n_lat as f32;
    let nln = n_lon as f32;

    for i in 0..=n_lat {
        let phi = PI * i as f32 / nl;
        let y = phi.cos();
        let sin_phi = phi.sin();

        for j in 0..=n_lon {
            let theta = 2.0 * PI * j as f32 / nln;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            let mut rx: f32 = 0.09;
            let ry: f32 = 0.12;
            let mut rz: f32 = 0.08;
            let y_pos = ry * y;

            // Chin narrowing
            if y < -0.3 {
                let chin = (y + 0.3) / -0.7;
                rx *= 1.0 - 0.35 * chin;
                rz *= 1.0 - 0.25 * chin;
            }
            // Cheekbones
            let cheek = (1.0 - ((y - 0.0) / 0.25).powi(2)).max(0.0) * cos_t.max(0.0);
            rx += 0.008 * cheek;

            let mut x = rx * sin_phi * cos_t;
            let mut z = rz * sin_phi * sin_t;
            let front = sin_t.max(0.0);

            // Nose
            let nose_y = (1.0 - ((y_pos - 0.02) / 0.03).powi(2)).max(0.0);
            let nose_x = (1.0 - (x / 0.015).powi(2)).max(0.0);
            z += nose_y * nose_x * front * 0.025;
            let nose_tip_y = (1.0 - ((y_pos + 0.005) / 0.012).powi(2)).max(0.0);
            let nose_tip_x = (1.0 - (x / 0.012).powi(2)).max(0.0);
            z += nose_tip_y * nose_tip_x * front * 0.018;

            // Nostrils
            for nx_off in [-0.008_f32, 0.008] {
                let nd = ((x - nx_off).powi(2) + (y_pos + 0.012).powi(2)).sqrt();
                if nd < 0.006 {
                    z -= (1.0 - (nd / 0.006).powi(2)) * 0.003 * front;
                }
            }

            // Eye sockets
            for ex in [-0.032_f32, 0.032] {
                let ed = ((x - ex).powi(2) + (y_pos - 0.045).powi(2)).sqrt();
                if ed < 0.02 {
                    z -= (1.0 - (ed / 0.02).powi(2)) * 0.01 * front;
                }
            }

            // Brow ridge
            let brow = (1.0 - ((y_pos - 0.07) / 0.012).powi(2)).max(0.0);
            z += brow * front * 0.007;

            // Lips
            let lip_y = (1.0 - ((y_pos + 0.035) / 0.012).powi(2)).max(0.0);
            let lip_x = (1.0 - (x / 0.025).powi(2)).max(0.0);
            z += lip_y * lip_x * front * 0.01;

            // Philtrum
            let ph_y = (1.0 - ((y_pos + 0.015) / 0.008).powi(2)).max(0.0);
            let ph_x = (1.0 - (x / 0.008).powi(2)).max(0.0);
            z -= ph_y * ph_x * front * 0.005;

            // Lower lip fold
            let ll = (1.0 - ((y_pos + 0.05) / 0.008).powi(2)).max(0.0);
            z -= ll * (1.0 - (x / 0.02).powi(2)).max(0.0) * front * 0.004;

            // Chin protrusion
            let ct = (1.0 - ((y_pos + 0.10) / 0.02).powi(2)).max(0.0);
            z += ct * (1.0 - (x / 0.02).powi(2)).max(0.0) * front * 0.008;

            // Forehead
            if y_pos > 0.06 {
                z += ((y_pos - 0.06) / 0.06) * front * 0.006;
            }

            // Ears
            for ear_side in [-1.0_f32, 1.0] {
                let ear_theta = if ear_side > 0.0 { PI / 2.0 } else { 3.0 * PI / 2.0 };
                let mut angle_diff = (theta - ear_theta).abs();
                if angle_diff > PI { angle_diff = 2.0 * PI - angle_diff; }
                if angle_diff < 0.35 {
                    let ear_y = ((y_pos - 0.035) / 0.03).abs();
                    if ear_y < 1.0 {
                        let bulge = (1.0 - (angle_diff / 0.35).powi(2)) * (1.0 - ear_y.powi(2));
                        x += ear_side * bulge * 0.015;
                        z -= bulge * 0.005;
                    }
                }
            }

            verts.push(Vec3::new(x, y_pos, z));
        }
    }

    let mut indices = Vec::new();
    for i in 0..n_lat {
        for j in 0..n_lon {
            let a = i * (n_lon + 1) + j;
            let b = a + n_lon + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    (verts, indices)
}

/// Compute smooth vertex normals from triangle faces.
pub fn compute_normals(verts: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut norms = vec![Vec3::ZERO; verts.len()];
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let e1 = verts[i1] - verts[i0];
        let e2 = verts[i2] - verts[i0];
        let fn_ = e1.cross(e2);
        norms[i0] += fn_;
        norms[i1] += fn_;
        norms[i2] += fn_;
    }
    norms.iter().map(|n| n.normalize_or_zero()).collect()
}

/// Laplacian smoothing.
pub fn laplacian_smooth(verts: &mut [Vec3], indices: &[u32], iterations: u32, factor: f32) {
    let n = verts.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for tri in indices.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        adj[a].push(b); adj[a].push(c);
        adj[b].push(a); adj[b].push(c);
        adj[c].push(a); adj[c].push(b);
    }

    for _ in 0..iterations {
        let prev = verts.to_vec();
        for i in 0..n {
            if adj[i].is_empty() { continue; }
            let avg: Vec3 = adj[i].iter().map(|&j| prev[j]).sum::<Vec3>() / adj[i].len() as f32;
            verts[i] = prev[i] + factor * (avg - prev[i]);
        }
    }
}

/// Frontal projection UV mapping.
pub fn frontal_uv(verts: &[Vec3]) -> Vec<[f32; 2]> {
    verts.iter().map(|v| {
        let u = ((v.x + 0.1) / 0.2 * 0.7 + 0.15).clamp(0.0, 1.0);
        let vv = (1.0 - (v.y + 0.14) / 0.28 * 0.7 - 0.15).clamp(0.0, 1.0);
        [u, vv]
    }).collect()
}

/// Generate eye meshes (white + iris + pupil per side).
pub fn generate_eyes(params: &EyeParams) -> Vec<MeshPart> {
    let mut parts = Vec::new();
    let size = params.size;
    let spacing = 0.025 + params.spacing * 0.015;

    for side in [-1.0_f32, 1.0] {
        let cx = side * spacing;
        let cy = 0.035 + params.height * 0.02;
        let cz = 0.065 + (1.0 - params.depth) * 0.015;
        let r_eye = 0.01 * size;

        // Eye white
        let (wv, wi) = generate_eye_sphere(cx, cy, cz, r_eye, 10, 14);
        parts.push(MeshPart {
            name: format!("eye_white_{}", if side < 0.0 { "l" } else { "r" }),
            vertices: wv, indices: wi, material: MaterialId::EyeWhite,
        });

        // Iris
        let r_iris = r_eye * 0.5 * params.iris_size;
        let (iv, ii) = generate_disc(cx, cy, cz + r_eye * 1.02, r_iris, 8, 12);
        parts.push(MeshPart {
            name: format!("iris_{}", if side < 0.0 { "l" } else { "r" }),
            vertices: iv, indices: ii, material: MaterialId::Iris,
        });

        // Pupil
        let r_pupil = r_iris * 0.4;
        let (pv, pi) = generate_disc(cx, cy, cz + r_eye * 1.04, r_pupil, 6, 10);
        parts.push(MeshPart {
            name: format!("pupil_{}", if side < 0.0 { "l" } else { "r" }),
            vertices: pv, indices: pi, material: MaterialId::Pupil,
        });
    }
    parts
}

fn generate_eye_sphere(cx: f32, cy: f32, cz: f32, r: f32, n_lat: u32, n_lon: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::new();
    for i in 0..=n_lat {
        let phi = PI * 0.25 + PI * 0.5 * i as f32 / n_lat as f32;
        for j in 0..=n_lon {
            let theta = -PI * 0.45 + PI * 0.9 * j as f32 / n_lon as f32;
            let x = cx + r * phi.sin() * theta.cos();
            let y = cy + r * phi.cos();
            let z = cz + r * phi.sin() * theta.sin();
            let n = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()).normalize();
            verts.push(Vertex { position: Vec3::new(x, y, z), normal: n, uv: [0.0, 0.0] });
        }
    }
    let mut indices = Vec::new();
    for i in 0..n_lat {
        for j in 0..n_lon {
            let a = i * (n_lon + 1) + j;
            let b = a + n_lon + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (verts, indices)
}

fn generate_disc(cx: f32, cy: f32, cz: f32, r: f32, n_rings: u32, n_seg: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::new();
    let n = Vec3::Z;
    for i in 0..=n_rings {
        let ri = r * i as f32 / n_rings as f32;
        for j in 0..n_seg {
            let theta = 2.0 * PI * j as f32 / n_seg as f32;
            verts.push(Vertex {
                position: Vec3::new(cx + ri * theta.cos(), cy + ri * theta.sin(), cz),
                normal: n, uv: [0.0, 0.0],
            });
        }
    }
    let mut indices = Vec::new();
    for i in 0..n_rings {
        for j in 0..n_seg {
            let a = i * n_seg + j;
            let b = a + n_seg;
            let c = if j + 1 < n_seg { a + 1 } else { i * n_seg };
            let d = if j + 1 < n_seg { b + 1 } else { (i + 1) * n_seg };
            indices.extend_from_slice(&[a, b, c, c, b, d]);
        }
    }
    (verts, indices)
}
