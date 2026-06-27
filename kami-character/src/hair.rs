//! Hair mesh generation — preset-based strand quad strips.

use crate::params::{HairParams, HairPreset};
use crate::{MaterialId, MeshPart, Vertex};
use glam::Vec3;
use std::f32::consts::PI;

/// Generate hair mesh from parameters.
pub fn generate_hair(params: &HairParams) -> MeshPart {
    let (n_strands, n_segments, base_length) = preset_config(params.preset);
    let length = base_length * params.length_scale;
    let volume = params.volume;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut idx = 0u32;

    for s in 0..n_strands {
        let (start_theta, start_phi) =
            strand_origin(params.preset, s, n_strands, params.part_position);
        let sx = (0.095 + volume * 0.01) * start_phi.sin() * start_theta.cos();
        let sy = 0.125 * start_phi.cos();
        let sz = (0.085 + volume * 0.005) * start_phi.sin() * start_theta.sin();
        let w0 = 0.001 + volume * 0.002;
        let curl = hash_f32(s as u32, 0) * 0.4 - 0.2;

        for seg in 0..n_segments {
            let t = seg as f32 / (n_segments - 1) as f32;
            let w = w0 * (1.0 - t * 0.8);
            let x =
                sx + hash_f32(s as u32, seg + 100) * 0.004 * t + t * 0.008 * (curl + t * 1.5).sin();
            let y = sy - t * length;
            let z = sz + hash_f32(s as u32, seg + 200) * 0.004 * t + t * 0.005 * start_theta.sin();

            let right = Vec3::new(
                (start_theta + PI / 2.0).cos(),
                0.0,
                (start_theta + PI / 2.0).sin(),
            ) * w;
            let pos_l = Vec3::new(x - right.x, y, z - right.z);
            let pos_r = Vec3::new(x + right.x, y, z + right.z);
            // Normal faces outward from head center
            let center_dir = Vec3::new(x, 0.0, z).normalize_or_zero();
            let normal = center_dir;

            vertices.push(Vertex {
                position: pos_l,
                normal,
                uv: [0.0, t],
            });
            vertices.push(Vertex {
                position: pos_r,
                normal,
                uv: [1.0, t],
            });

            if seg > 0 {
                let i = idx + (seg as u32 - 1) * 2;
                indices.extend_from_slice(&[i, i + 2, i + 1, i + 1, i + 2, i + 3]);
            }
        }
        idx += n_segments as u32 * 2;
    }

    MeshPart {
        name: "hair".into(),
        vertices,
        indices,
        material: MaterialId::Hair,
    }
}

/// Get strand count, segments, and base length for a hair preset.
fn preset_config(preset: HairPreset) -> (u32, u32, f32) {
    match preset {
        HairPreset::Bald => (0, 0, 0.0),
        HairPreset::Buzz => (200, 3, 0.015),
        HairPreset::Pixie => (150, 6, 0.04),
        HairPreset::ShortStraight | HairPreset::ShortWavy | HairPreset::ShortCurly => {
            (150, 8, 0.07)
        }
        HairPreset::Bob => (180, 10, 0.10),
        HairPreset::MediumStraight | HairPreset::MediumWavy | HairPreset::MediumLayered => {
            (200, 10, 0.15)
        }
        HairPreset::LongStraight | HairPreset::LongWavy | HairPreset::LongCurly => (250, 12, 0.25),
        HairPreset::PonytailHigh | HairPreset::PonytailLow => (150, 12, 0.20),
        HairPreset::BunTop | HairPreset::BunLow => (120, 8, 0.06),
        HairPreset::Undercut | HairPreset::Mohawk => (100, 8, 0.07),
        HairPreset::AfroShort => (300, 5, 0.04),
        HairPreset::AfroLarge => (400, 6, 0.07),
        HairPreset::BraidsTwin | HairPreset::BraidsSingle => (120, 12, 0.22),
    }
}

/// Compute strand origin (theta, phi) based on preset and strand index.
fn strand_origin(preset: HairPreset, idx: u32, total: u32, part_pos: f32) -> (f32, f32) {
    let h = hash_f32(idx, 42);
    let h2 = hash_f32(idx, 99);

    match preset {
        HairPreset::Bald => (0.0, 0.0),
        HairPreset::Mohawk => {
            let theta = PI + (h - 0.5) * 0.3;
            let phi = h2 * PI * 0.35;
            (theta, phi)
        }
        _ => {
            // Default: distribute over scalp, biased back/sides
            let theta = if h > 0.3 {
                PI * (0.2 + 1.6 * h2) // Back and sides
            } else {
                2.0 * PI * h2 // All around (including front bangs)
            };
            let phi = h * PI * 0.45;
            (theta, phi)
        }
    }
}

/// Simple deterministic hash for pseudo-random strand variation.
fn hash_f32(a: u32, b: u32) -> f32 {
    let mut h = a
        .wrapping_mul(2654435761)
        .wrapping_add(b.wrapping_mul(2246822519));
    h ^= h >> 16;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0
}
