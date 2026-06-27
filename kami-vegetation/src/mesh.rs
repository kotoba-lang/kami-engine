//! Per-species procedural meshes driven by `TaxonomicProfile`.
//!
//! Core entry: `mesh_from_profile(profile) -> SpeciesMesh`. Adding a new
//! species means creating a new `TaxonomicProfile` (via `taxonomy.rs`) —
//! no new mesh function required.
//!
//! Vertex format: `[pos.x, pos.y, pos.z, uv.x, uv.y]` (20 bytes / vertex).

use crate::species::SpeciesId;
use crate::taxonomy::{CanopyShape, TaxonomicProfile};

/// Per-species mesh (CPU side, ready to upload as GPU vertex buffer).
pub struct SpeciesMesh {
    /// Flat f32 buffer: 5 floats per vertex (pos3 + uv2).
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub vertex_count: u32,
    pub index_count: u32,
}

impl SpeciesMesh {
    fn finalize(vertices: Vec<f32>, indices: Vec<u32>) -> Self {
        let vc = (vertices.len() / 5) as u32;
        let ic = indices.len() as u32;
        Self {
            vertices,
            indices,
            vertex_count: vc,
            index_count: ic,
        }
    }
}

// ── Vertex pushers (shared primitives) ──

fn rot_xz(p: [f32; 3], c: f32, s: f32) -> [f32; 3] {
    [c * p[0] - s * p[2], p[1], s * p[0] + c * p[2]]
}

fn push_quad(
    v: &mut Vec<f32>,
    i: &mut Vec<u32>,
    corners: [[f32; 3]; 4],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
) {
    let base = (v.len() / 5) as u32;
    let uvs = [
        [uv_min[0], uv_max[1]],
        [uv_max[0], uv_max[1]],
        [uv_min[0], uv_min[1]],
        [uv_max[0], uv_min[1]],
    ];
    for (c, uv) in corners.iter().zip(uvs.iter()) {
        v.extend_from_slice(&[c[0], c[1], c[2], uv[0], uv[1]]);
    }
    i.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
}

fn push_tapered_blade(
    v: &mut Vec<f32>,
    i: &mut Vec<u32>,
    angle: f32,
    width: f32,
    height: f32,
    curve: f32,
) {
    let c = angle.cos();
    let s = angle.sin();
    let hw = width * 0.5;
    let tip_narrow = 0.15;
    let bl = rot_xz([-hw, 0.0, 0.0], c, s);
    let br = rot_xz([hw, 0.0, 0.0], c, s);
    let tl = rot_xz([-hw * tip_narrow, height, curve], c, s);
    let tr = rot_xz([hw * tip_narrow, height, curve], c, s);
    let base = (v.len() / 5) as u32;
    v.extend_from_slice(&[bl[0], bl[1], bl[2], 0.0, 1.0]);
    v.extend_from_slice(&[br[0], br[1], br[2], 1.0, 1.0]);
    v.extend_from_slice(&[tl[0], tl[1], tl[2], 0.1, 0.0]);
    v.extend_from_slice(&[tr[0], tr[1], tr[2], 0.9, 0.0]);
    i.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
}

fn push_trunk_cross(v: &mut Vec<f32>, i: &mut Vec<u32>, r_base: f32, r_top: f32, h: f32) {
    // Two perpendicular thin quads forming a cross-section trunk.
    push_quad(
        v,
        i,
        [
            [-r_base, 0.0, 0.0],
            [r_base, 0.0, 0.0],
            [-r_top, h, 0.0],
            [r_top, h, 0.0],
        ],
        [0.3, 1.0],
        [0.5, 0.5],
    );
    push_quad(
        v,
        i,
        [
            [0.0, 0.0, -r_base],
            [0.0, 0.0, r_base],
            [0.0, h, -r_top],
            [0.0, h, r_top],
        ],
        [0.3, 1.0],
        [0.5, 0.5],
    );
}

// ── CanopyShape generators ──

fn gen_blade(p: &TaxonomicProfile) -> SpeciesMesh {
    // Grass / tussock: N tapered blades fanned at equal angles
    let mut v = Vec::new();
    let mut i = Vec::new();
    let n = p.leaf_count.max(1);
    for k in 0..n {
        let angle = (k as f32 / n as f32) * std::f32::consts::PI; // half-fan
        push_tapered_blade(&mut v, &mut i, angle, p.leaf_size, 1.0, p.leaf_size * 0.8);
    }
    SpeciesMesh::finalize(v, i)
}

fn gen_fan(p: &TaxonomicProfile) -> SpeciesMesh {
    // Fern: central stem + N leaflet pairs
    let mut v = Vec::new();
    let mut i = Vec::new();
    // Stem
    let r = p.stem_radius_base.max(0.02);
    push_quad(
        &mut v,
        &mut i,
        [[-r, 0.0, 0.0], [r, 0.0, 0.0], [-r, 1.0, 0.0], [r, 1.0, 0.0]],
        [0.45, 1.0],
        [0.55, 0.0],
    );
    // Leaflets (paired)
    let n = p.leaf_count.max(1);
    for k in 0..n {
        let y = 0.15 + (0.75 / n as f32) * k as f32;
        let size = p.leaf_size * (1.0 - k as f32 * 0.1);
        let tilt = 0.05;
        for &sign in &[-1.0f32, 1.0] {
            push_quad(
                &mut v,
                &mut i,
                [
                    [0.0, y, 0.0],
                    [sign * size, y + tilt, 0.0],
                    [0.0, y + size * 0.3, 0.0],
                    [sign * size, y + size * 0.3 + tilt, 0.0],
                ],
                if sign < 0.0 { [0.0, 0.8] } else { [0.5, 0.8] },
                if sign < 0.0 { [0.5, 0.0] } else { [1.0, 0.0] },
            );
        }
    }
    SpeciesMesh::finalize(v, i)
}

fn gen_radial(p: &TaxonomicProfile) -> SpeciesMesh {
    // Palm: trunk + N radial fronds at top
    let mut v = Vec::new();
    let mut i = Vec::new();
    let trunk_h = 0.85;
    push_trunk_cross(
        &mut v,
        &mut i,
        p.stem_radius_base,
        p.stem_radius_top,
        trunk_h,
    );
    let n = p.leaf_count.max(1);
    let frond_len = p.leaf_size;
    let droop = 0.15;
    let base_w = 0.08;
    let tip_w = 0.04;
    for k in 0..n {
        let angle = (k as f32 / n as f32) * std::f32::consts::TAU;
        let c = angle.cos();
        let s = angle.sin();
        let rot = |q: [f32; 3]| rot_xz(q, c, s);
        push_quad(
            &mut v,
            &mut i,
            [
                rot([-base_w, trunk_h, 0.0]),
                rot([base_w, trunk_h, 0.0]),
                rot([-tip_w * 0.5, trunk_h + 0.05 - droop, frond_len]),
                rot([tip_w * 0.5, trunk_h + 0.05 - droop, frond_len]),
            ],
            [0.0, 1.0],
            [1.0, 0.0],
        );
    }
    SpeciesMesh::finalize(v, i)
}

fn gen_cone(p: &TaxonomicProfile) -> SpeciesMesh {
    // Conifer: trunk + N cone layers (6-side pyramids)
    let mut v = Vec::new();
    let mut i = Vec::new();
    push_trunk_cross(&mut v, &mut i, p.stem_radius_base, p.stem_radius_top, 0.4);

    let layers = p.leaf_count.max(1);
    let top_y = 0.98;
    let base_y = 0.30;
    let step = (top_y - base_y) / layers as f32;
    for k in 0..layers {
        let t = k as f32 / layers as f32;
        let y_base = base_y + step * k as f32;
        let y_top = y_base + step * 1.2;
        // Radius narrows with height
        let radius = p.leaf_size * (1.0 - t * 0.6);
        let apex = [0.0, y_top, 0.0];
        let sides = 6;
        for s in 0..sides {
            let a0 = (s as f32 / sides as f32) * std::f32::consts::TAU;
            let a1 = ((s + 1) as f32 / sides as f32) * std::f32::consts::TAU;
            let p0 = [radius * a0.cos(), y_base, radius * a0.sin()];
            let p1 = [radius * a1.cos(), y_base, radius * a1.sin()];
            let base = (v.len() / 5) as u32;
            v.extend_from_slice(&[p0[0], p0[1], p0[2], 0.0, 1.0]);
            v.extend_from_slice(&[p1[0], p1[1], p1[2], 1.0, 1.0]);
            v.extend_from_slice(&[apex[0], apex[1], apex[2], 0.5, 0.0]);
            i.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
    SpeciesMesh::finalize(v, i)
}

fn gen_dome(p: &TaxonomicProfile) -> SpeciesMesh {
    // Bush: N overlapping rotated quads forming a sphere silhouette
    let mut v = Vec::new();
    let mut i = Vec::new();
    let n = p.leaf_count.max(1);
    let r = p.leaf_size;
    for k in 0..n {
        let t = k as f32 / n as f32;
        let angle = t * std::f32::consts::PI;
        let y_c = 0.4 + 0.3 * (t + (k as f32 * 0.37).sin() * 0.3);
        let rad = r * (0.7 + 0.3 * (k as f32 * 0.61).cos());
        let c = angle.cos();
        let s = angle.sin();
        let rot = |q: [f32; 3]| rot_xz(q, c, s);
        push_quad(
            &mut v,
            &mut i,
            [
                rot([-rad, y_c - rad * 0.5, 0.0]),
                rot([rad, y_c - rad * 0.5, 0.0]),
                rot([-rad, y_c + rad * 0.5, 0.0]),
                rot([rad, y_c + rad * 0.5, 0.0]),
            ],
            [0.0, 1.0],
            [1.0, 0.0],
        );
    }
    SpeciesMesh::finalize(v, i)
}

fn gen_column(p: &TaxonomicProfile) -> SpeciesMesh {
    // Cactus: tall fluted cylinder — 8-sided prism with vertical ribs
    let mut v = Vec::new();
    let mut i = Vec::new();
    let sides = 8;
    let r_b = p.stem_radius_base.max(0.1);
    let r_t = p.stem_radius_top.max(0.08);
    let h = 1.0;
    for s in 0..sides {
        let a0 = (s as f32 / sides as f32) * std::f32::consts::TAU;
        let a1 = ((s + 1) as f32 / sides as f32) * std::f32::consts::TAU;
        let p0_b = [r_b * a0.cos(), 0.0, r_b * a0.sin()];
        let p1_b = [r_b * a1.cos(), 0.0, r_b * a1.sin()];
        let p0_t = [r_t * a0.cos(), h, r_t * a0.sin()];
        let p1_t = [r_t * a1.cos(), h, r_t * a1.sin()];
        push_quad(
            &mut v,
            &mut i,
            [p0_b, p1_b, p0_t, p1_t],
            [s as f32 / sides as f32, 1.0],
            [(s + 1) as f32 / sides as f32, 0.0],
        );
    }
    // Top cap (flat polygon approximated as triangle fan)
    let center_top = [0.0, h, 0.0];
    let top_base = (v.len() / 5) as u32;
    v.extend_from_slice(&[center_top[0], center_top[1], center_top[2], 0.5, 0.5]);
    for s in 0..sides {
        let a = (s as f32 / sides as f32) * std::f32::consts::TAU;
        let p = [r_t * a.cos(), h, r_t * a.sin()];
        v.extend_from_slice(&[p[0], p[1], p[2], 0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()]);
    }
    for s in 0..sides {
        let a = top_base;
        let b = top_base + 1 + s as u32;
        let c = top_base + 1 + ((s + 1) % sides) as u32;
        i.extend_from_slice(&[a, b, c]);
    }
    SpeciesMesh::finalize(v, i)
}

fn gen_carpet(p: &TaxonomicProfile) -> SpeciesMesh {
    // Moss: flat multi-patch carpet — N overlapping horizontal quads at slight tilt
    let mut v = Vec::new();
    let mut i = Vec::new();
    let patches = (p.leaf_count.max(3)).max(3);
    let r = p.leaf_size.max(0.25);
    for k in 0..patches {
        let t = k as f32 / patches as f32;
        let angle = t * std::f32::consts::TAU;
        let c = angle.cos();
        let s = angle.sin();
        let cx = 0.15 * c;
        let cz = 0.15 * s;
        let y = 0.05 + 0.05 * (t * 4.0 + 0.7).sin().abs();
        let rot = |q: [f32; 3]| rot_xz([q[0] + cx, q[1], q[2] + cz], c, s);
        push_quad(
            &mut v,
            &mut i,
            [
                rot([-r * 0.5, y, -r * 0.5]),
                rot([r * 0.5, y, -r * 0.5]),
                rot([-r * 0.5, y, r * 0.5]),
                rot([r * 0.5, y, r * 0.5]),
            ],
            [0.0, 1.0],
            [1.0, 0.0],
        );
    }
    SpeciesMesh::finalize(v, i)
}

// ── Public API ──

/// Generate a mesh from a taxonomic profile.
///
/// Switches on `profile.canopy` (CanopyShape); uses other profile fields
/// (leaf_count, leaf_size, stem radii) to parameterize the generator.
pub fn mesh_from_profile(profile: &TaxonomicProfile) -> SpeciesMesh {
    match profile.canopy {
        CanopyShape::Blade => gen_blade(profile),
        CanopyShape::Fan => gen_fan(profile),
        CanopyShape::Radial => gen_radial(profile),
        CanopyShape::Cone => gen_cone(profile),
        CanopyShape::Dome => gen_dome(profile),
        CanopyShape::Column => gen_column(profile),
        CanopyShape::Carpet => gen_carpet(profile),
    }
}

/// Convenience: map `SpeciesId` → preset profile → mesh.
pub fn mesh_for(species: SpeciesId) -> SpeciesMesh {
    use crate::taxonomy;
    let profile = match species {
        SpeciesId::Grass => taxonomy::grass(),
        SpeciesId::Fern => taxonomy::fern(),
        SpeciesId::PalmTree => taxonomy::palm(),
        SpeciesId::Conifer => taxonomy::conifer(),
        SpeciesId::Bush => taxonomy::bush(),
    };
    mesh_from_profile(&profile)
}

/// Full 5-species library (kept for the existing scene_pipelines upload API).
pub fn species_mesh_library() -> Vec<(SpeciesId, SpeciesMesh)> {
    SpeciesId::all().iter().map(|&s| (s, mesh_for(s))).collect()
}

/// 7-species library including Cactus and Moss (new taxonomic extensions).
pub fn extended_mesh_library() -> Vec<(String, SpeciesMesh)> {
    crate::taxonomy::default_catalog()
        .into_iter()
        .map(|p| (p.common_name.to_string(), mesh_from_profile(&p)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::*;

    #[test]
    fn all_species_have_mesh() {
        for (species, mesh) in species_mesh_library() {
            assert!(mesh.vertex_count > 0, "{:?} empty verts", species);
            assert!(mesh.index_count > 0, "{:?} empty idx", species);
            assert!(mesh.index_count % 3 == 0, "{:?} non-triangle idx", species);
        }
    }

    #[test]
    fn conifer_wider_than_grass() {
        let g = mesh_for(SpeciesId::Grass);
        let c = mesh_for(SpeciesId::Conifer);
        let max_xz = |m: &SpeciesMesh| -> f32 {
            let mut r = 0.0f32;
            for ch in m.vertices.chunks_exact(5) {
                r = r.max((ch[0] * ch[0] + ch[2] * ch[2]).sqrt());
            }
            r
        };
        assert!(max_xz(&c) > max_xz(&g));
    }

    #[test]
    fn extended_library_has_cactus_and_moss() {
        let ext = extended_mesh_library();
        assert_eq!(ext.len(), 7);
        let names: Vec<_> = ext.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"cactus"));
        assert!(names.contains(&"moss"));
    }

    #[test]
    fn cactus_is_columnar() {
        let p = cactus();
        let m = mesh_from_profile(&p);
        // Cactus should have many vertices (8-sided prism + top cap = 8*4 + 9 = 41 verts)
        assert!(m.vertex_count >= 30);
    }

    #[test]
    fn moss_is_flat() {
        let p = moss();
        let m = mesh_from_profile(&p);
        let max_y = m
            .vertices
            .chunks_exact(5)
            .map(|c| c[1])
            .fold(0.0f32, f32::max);
        // Moss should be quite flat — max_y < 0.2 (in unit mesh space)
        assert!(max_y < 0.2, "moss should be flat, got max_y={}", max_y);
    }

    #[test]
    fn profile_drives_leaf_count() {
        // Increasing leaf_count should produce more vertices.
        let mut p = fern();
        p.leaf_count = 3;
        let small = mesh_from_profile(&p);
        p.leaf_count = 10;
        let big = mesh_from_profile(&p);
        assert!(big.vertex_count > small.vertex_count);
    }
}
