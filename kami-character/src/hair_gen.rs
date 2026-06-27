//! Parametric hair generator — `HairStyle` → Strands / Hair Cards / GLB Mesh.
//!
//! Three output modes from the same `HairStyle` params:
//!   1. **Strands** (`GroomAsset`): 10K+ strand curves for compute shader rendering
//!   2. **Hair Cards** (`Vec<HairCard>`): textured quads for rasterization
//!   3. **Hair Mesh** (`HairMeshOutput`): polygon shell GLB for Three.js/WebGPU
//!
//! LLM pipeline: photo → Murakumo VL → HairStyle JSON → generate_*()

use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

use crate::groom::{GroomAsset, GroomGroup, HairCard, Strand};
use crate::{MaterialId, MeshPart, Vertex};

/// High-level hair style parameters. LLM outputs this from photo analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HairStyle {
    pub style: HairType,
    /// 0.0=buzz, 0.3=short, 0.5=medium, 0.7=long, 1.0=floor-length.
    pub length: f32,
    /// 0.0–1.0 strand/card/polygon count.
    pub density: f32,
    /// 0.0=flat, 1.0=voluminous.
    pub volume: f32,
    /// 0.0=straight, 0.5=wavy, 1.0=tight curls.
    pub curl: f32,
    /// -1.0=left, 0.0=center, 1.0=right.
    pub part_side: f32,
    /// 0.0=no bangs, 1.0=full bangs.
    pub bangs_length: f32,
    /// 0.0=narrow, 1.0=full width.
    pub bangs_width: f32,
    /// Base RGB.
    pub color: [f32; 3],
    /// Highlight RGB.
    pub highlight_color: [f32; 3],
    /// % highlighted.
    pub highlight_ratio: f32,
    /// Root darkening.
    pub root_darken: f32,
    /// Head radius (m).
    pub head_radius: f32,
    /// Head center Y (m).
    pub head_center_y: f32,
}

/// Hair type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HairType {
    Straight,
    Wavy,
    Curly,
    Afro,
    Braided,
}

/// Hair rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HairRenderMode {
    /// Strand curves (compute shader, highest quality).
    Strands,
    /// Textured quad strips (rasterization, game quality).
    Cards,
    /// Polygon shell mesh (GLB-compatible, artist quality).
    Mesh,
}

/// Output from `generate_hair_mesh()` — polygon shell hair.
#[derive(Debug, Clone)]
pub struct HairMeshOutput {
    /// Hair mesh parts (front, sides, back, bangs).
    pub parts: Vec<MeshPart>,
    /// Total vertex count.
    pub total_vertices: usize,
    /// Total triangle count.
    pub total_triangles: usize,
}

/// GLB-compatible hair mesh data (for Three.js / WebGPU upload).
#[derive(Debug, Clone)]
pub struct HairMeshData {
    /// Interleaved: position(3) + normal(3) + uv(2) = 8 floats/vert.
    pub vertices: Vec<f32>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// Vertex count.
    pub vertex_count: usize,
    /// Triangle count.
    pub triangle_count: usize,
}

impl Default for HairStyle {
    fn default() -> Self {
        Self {
            style: HairType::Straight,
            length: 0.7,
            density: 0.8,
            volume: 0.5,
            curl: 0.03,
            part_side: 0.1,
            bangs_length: 0.3,
            bangs_width: 0.5,
            color: [0.93, 0.86, 0.72],
            highlight_color: [0.97, 0.92, 0.82],
            highlight_ratio: 0.35,
            root_darken: 0.7,
            head_radius: 0.09,
            head_center_y: 1.43,
        }
    }
}

impl HairStyle {
    pub fn blonde_long() -> Self {
        Self::default()
    }

    pub fn dark_short() -> Self {
        Self {
            style: HairType::Straight,
            length: 0.2,
            density: 0.9,
            volume: 0.3,
            curl: 0.02,
            part_side: 0.0,
            bangs_length: 0.15,
            bangs_width: 0.6,
            color: [0.12, 0.08, 0.06],
            highlight_color: [0.20, 0.15, 0.12],
            highlight_ratio: 0.15,
            root_darken: 0.5,
            ..Self::default()
        }
    }

    pub fn red_wavy() -> Self {
        Self {
            style: HairType::Wavy,
            length: 0.6,
            density: 0.8,
            volume: 0.7,
            curl: 0.25,
            part_side: -0.2,
            bangs_length: 0.35,
            bangs_width: 0.4,
            color: [0.55, 0.18, 0.10],
            highlight_color: [0.70, 0.30, 0.18],
            highlight_ratio: 0.25,
            root_darken: 0.6,
            ..Self::default()
        }
    }

    pub fn brown_curly() -> Self {
        Self {
            style: HairType::Curly,
            length: 0.5,
            density: 0.9,
            volume: 0.8,
            curl: 0.6,
            part_side: 0.0,
            bangs_length: 0.2,
            bangs_width: 0.5,
            color: [0.25, 0.15, 0.08],
            highlight_color: [0.35, 0.22, 0.12],
            highlight_ratio: 0.2,
            root_darken: 0.5,
            ..Self::default()
        }
    }

    pub fn afro() -> Self {
        Self {
            style: HairType::Afro,
            length: 0.3,
            density: 1.0,
            volume: 1.0,
            curl: 0.9,
            part_side: 0.0,
            bangs_length: 0.0,
            bangs_width: 0.0,
            color: [0.08, 0.05, 0.03],
            highlight_color: [0.15, 0.10, 0.06],
            highlight_ratio: 0.1,
            root_darken: 0.4,
            ..Self::default()
        }
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

fn hash_f32(a: u32, b: u32) -> f32 {
    let mut h = a
        .wrapping_mul(2654435761)
        .wrapping_add(b.wrapping_mul(2246822519));
    h ^= h >> 16;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Compute strand root position on scalp + strand direction.
fn strand_root(si: usize, style: &HairStyle, is_bangs: bool, h1: f32, h2: f32) -> (Vec3, f32) {
    let r = style.head_radius;
    let cy = style.head_center_y;

    let theta = if is_bangs {
        PI * 0.35 + h2 * PI * 0.3
    } else {
        // Back hemisphere only: theta in [PI, 2PI]
        PI + h2 * PI
    };
    let phi = h1 * PI * 0.33;
    let part_offset = style.part_side * r * 0.5;

    let root = Vec3::new(
        part_offset + (r + 0.001) * phi.sin() * theta.cos(),
        cy + r * 1.2 * phi.cos(),
        (r * 0.95) * phi.sin() * theta.sin(),
    );
    (root, theta)
}

/// Compute a point along a strand at parameter t (0=root, 1=tip).
fn strand_point(
    root: Vec3,
    theta: f32,
    t: f32,
    strand_len: f32,
    style: &HairStyle,
    h3: f32,
    is_bangs: bool,
) -> Vec3 {
    let curl_freq = match style.style {
        HairType::Straight => 0.5,
        HairType::Wavy => 2.5,
        HairType::Curly => 5.0,
        HairType::Afro => 8.0,
        HairType::Braided => 3.0,
    };
    let curl_amp = style.curl * style.head_radius * 0.8;
    let afro_vol = if style.style == HairType::Afro {
        style.volume * 0.15
    } else {
        0.0
    };
    let grav = if style.style == HairType::Afro {
        t * 0.3
    } else {
        t * t
    };
    let curl = (t * curl_freq + h3 * 7.0).sin() * curl_amp * t;
    let outward = afro_vol * t + t * 0.2 * (1.0 - t);
    let r = style.head_radius;

    Vec3::new(
        root.x + outward * r * theta.cos() * 0.15 + curl,
        root.y
            - strand_len
                * (t * if is_bangs { 0.6 } else { 0.15 }
                    + grav * if is_bangs { 0.4 } else { 0.85 }),
        root.z
            + outward * r * theta.sin() * 0.15
            + if is_bangs { t * strand_len * 0.05 } else { 0.0 },
    )
}

// ─── Mode 1: Strands (GroomAsset) ───

/// Generate strand curves from `HairStyle`. For compute shader rendering.
///
/// Default density=0.8 → ~160 strands. For 100K strands, use `generate_groom_count()`.
pub fn generate_groom(style: &HairStyle, points_per_strand: usize) -> GroomAsset {
    let strand_count = (200.0 * style.density) as usize;
    generate_groom_inner(style, points_per_strand, strand_count)
}

/// Generate groom with explicit strand count (e.g. 100_000 for cinematic quality).
pub fn generate_groom_count(
    style: &HairStyle,
    points_per_strand: usize,
    strand_count: usize,
) -> GroomAsset {
    generate_groom_inner(style, points_per_strand, strand_count)
}

fn generate_groom_inner(
    style: &HairStyle,
    points_per_strand: usize,
    strand_count: usize,
) -> GroomAsset {
    let base_length = style.head_radius * 2.0 * (0.3 + style.length * 2.5);
    let mut strands = Vec::with_capacity(strand_count);

    for si in 0..strand_count {
        let h1 = hash_f32(si as u32, 42);
        let h2 = hash_f32(si as u32, 99);
        let h3 = hash_f32(si as u32, 77);
        let is_bangs = (si as f32) < strand_count as f32 * style.bangs_width * 0.05
            && style.bangs_length > 0.05;
        let (root, theta) = strand_root(si, style, is_bangs, h1, h2);
        let strand_len = if is_bangs {
            base_length * style.bangs_length * 0.5
        } else {
            base_length * (0.7 + h1 * 0.3)
        };

        let mut points = Vec::with_capacity(points_per_strand);
        let mut widths = Vec::with_capacity(points_per_strand);
        for pi in 0..points_per_strand {
            let t = pi as f32 / (points_per_strand - 1) as f32;
            points.push(strand_point(
                root, theta, t, strand_len, style, h3, is_bangs,
            ));
            widths.push(0.0008 * (1.0 - t * 0.7) * (1.0 + style.volume * 0.5));
        }

        strands.push(Strand {
            points,
            widths,
            root_uv: [theta / (2.0 * PI), h1],
            group: if h3 > (1.0 - style.highlight_ratio) {
                1
            } else {
                0
            },
        });
    }

    let guide_indices = (0..strands.len()).step_by(4).collect();
    let total_points = strands.iter().map(|s| s.points.len()).sum();
    GroomAsset {
        strands,
        guide_indices,
        total_points,
        groups: vec![
            GroomGroup {
                name: "base".into(),
                strand_count,
                material_slot: 0,
                clump_scale: 0.5,
                clump_noise: 0.1,
            },
            GroomGroup {
                name: "highlight".into(),
                strand_count: 0,
                material_slot: 1,
                clump_scale: 0.3,
                clump_noise: 0.05,
            },
        ],
    }
}

// ─── Mode 2: Hair Cards ───

/// Generate hair card quads from `HairStyle`. For rasterization.
pub fn generate_hair_cards(style: &HairStyle) -> Vec<HairCard> {
    let groom = generate_groom(style, 8);
    let cards_count = (groom.strands.len() / 10).max(1);
    groom.to_hair_cards(cards_count)
}

// ─── Mode 3: Hair Mesh (polygon shell) ───

/// Generate a polygon shell hair mesh from `HairStyle`.
///
/// Creates layered ribbon strips that form a volumetric hair shape.
/// Each strip is a quad-dominant mesh with proper normals and UVs.
/// Output is compatible with GLB export and Three.js BufferGeometry.
pub fn generate_hair_mesh(style: &HairStyle) -> HairMeshOutput {
    let base_length = style.head_radius * 2.0 * (0.3 + style.length * 2.5);
    // More strips for mesh mode (volumetric shell)
    let n_strips = (80.0 * style.density) as usize;
    let n_layers = 3; // inner, mid, outer shells
    let segs_per_strip = 12; // smoother than cards

    let mut parts = Vec::new();
    let mut total_verts = 0;
    let mut total_tris = 0;

    for layer in 0..n_layers {
        let layer_t = layer as f32 / (n_layers - 1).max(1) as f32;
        // Each layer is slightly further from head
        let layer_offset = layer_t * style.head_radius * 0.08 * (1.0 + style.volume);
        let layer_len_mul = 1.0 - layer_t * 0.3; // inner layers shorter

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for si in 0..n_strips {
            let h1 = hash_f32(si as u32 + layer as u32 * 1000, 42);
            let h2 = hash_f32(si as u32 + layer as u32 * 1000, 99);
            let h3 = hash_f32(si as u32 + layer as u32 * 1000, 77);
            let is_bangs = (si as f32) < n_strips as f32 * style.bangs_width * 0.05
                && style.bangs_length > 0.05;
            let (root, theta) = strand_root(si, style, is_bangs, h1, h2);
            let strand_len = if is_bangs {
                base_length * style.bangs_length * 0.5
            } else {
                base_length * (0.6 + h1 * 0.4) * layer_len_mul
            };

            // Strip width (wider than cards for mesh look)
            let strip_w = style.head_radius * (0.03 + h3 * 0.04) * (1.0 + style.volume * 0.3);
            let base_idx = vertices.len();

            for seg in 0..=segs_per_strip {
                let t = seg as f32 / segs_per_strip as f32;
                let mut p = strand_point(root, theta, t, strand_len, style, h3, is_bangs);
                // Push outward for layer offset
                let outward_dir = Vec3::new(theta.cos(), 0.0, theta.sin());
                p += outward_dir * layer_offset;

                let w = strip_w * (1.0 - t * 0.25);
                let perp = Vec3::new(-theta.sin(), 0.0, theta.cos());
                let left = p - perp * w;
                let right = p + perp * w;

                // Normal: outward from head center
                let n = Vec3::new(p.x - root.x, 0.2, p.z - root.z).normalize_or_zero();

                vertices.push(Vertex {
                    position: left,
                    normal: n,
                    uv: [0.0, t],
                });
                vertices.push(Vertex {
                    position: right,
                    normal: n,
                    uv: [1.0, t],
                });

                if seg > 0 {
                    let i = base_idx as u32 + (seg as u32 - 1) * 2;
                    indices.extend_from_slice(&[i, i + 2, i + 1, i + 1, i + 2, i + 3]);
                }
            }
        }

        total_verts += vertices.len();
        total_tris += indices.len() / 3;

        let layer_name = match layer {
            0 => "hair_outer",
            1 => "hair_mid",
            _ => "hair_inner",
        };

        parts.push(MeshPart {
            name: layer_name.into(),
            vertices,
            indices,
            material: MaterialId::Hair,
        });
    }

    HairMeshOutput {
        parts,
        total_vertices: total_verts,
        total_triangles: total_tris,
    }
}

/// Generate hair mesh as flat arrays for GPU upload (interleaved pos+norm+uv + indices).
pub fn generate_hair_mesh_data(style: &HairStyle) -> HairMeshData {
    let mesh = generate_hair_mesh(style);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut vert_offset = 0u32;

    for part in &mesh.parts {
        for v in &part.vertices {
            vertices.extend_from_slice(&[
                v.position.x,
                v.position.y,
                v.position.z,
                v.normal.x,
                v.normal.y,
                v.normal.z,
                v.uv[0],
                v.uv[1],
            ]);
        }
        for &idx in &part.indices {
            indices.push(idx + vert_offset);
        }
        vert_offset += part.vertices.len() as u32;
    }

    HairMeshData {
        vertex_count: mesh.total_vertices,
        triangle_count: mesh.total_triangles,
        vertices,
        indices,
    }
}

/// Export hair mesh as GLB binary (via kami-gltf).
pub fn generate_hair_glb(style: &HairStyle) -> Vec<u8> {
    let mesh = generate_hair_mesh(style);
    let char_mesh = crate::CharacterMesh {
        parts: mesh.parts,
        skeleton: None,
        blendshape_targets: Vec::new(),
    };
    let def = crate::params::CharacterDef::default();
    crate::export::export_glb(&char_mesh, &def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_groom() {
        let style = HairStyle::default();
        let groom = generate_groom(&style, 8);
        assert_eq!(groom.strands[0].points.len(), 8);
        let root_y = groom.strands[0].points[0].y;
        assert!(root_y > 1.3 && root_y < 1.6, "root_y={root_y}");
    }

    #[test]
    fn test_presets() {
        for preset in [
            HairStyle::blonde_long(),
            HairStyle::dark_short(),
            HairStyle::red_wavy(),
            HairStyle::brown_curly(),
            HairStyle::afro(),
        ] {
            let groom = generate_groom(&preset, 6);
            assert!(!groom.strands.is_empty());
            for s in &groom.strands {
                for p in &s.points {
                    assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
                }
            }
        }
    }

    #[test]
    fn test_hair_cards() {
        let cards = generate_hair_cards(&HairStyle::default());
        assert!(!cards.is_empty());
        for card in &cards {
            assert!(!card.positions.is_empty());
            assert!(!card.indices.is_empty());
        }
    }

    #[test]
    fn test_hair_mesh() {
        let mesh = generate_hair_mesh(&HairStyle::default());
        assert_eq!(mesh.parts.len(), 3); // 3 layers
        assert!(mesh.total_vertices > 1000);
        assert!(mesh.total_triangles > 500);
        for part in &mesh.parts {
            assert!(!part.vertices.is_empty());
            for v in &part.vertices {
                assert!(v.position.y.is_finite());
            }
        }
    }

    #[test]
    fn test_hair_mesh_data() {
        let data = generate_hair_mesh_data(&HairStyle::default());
        assert_eq!(data.vertices.len(), data.vertex_count * 8);
        assert_eq!(data.indices.len(), data.triangle_count * 3);
    }

    #[test]
    fn test_hair_glb() {
        let glb = generate_hair_glb(&HairStyle::default());
        assert_eq!(&glb[0..4], &[0x67, 0x6C, 0x54, 0x46]); // glTF magic
        assert!(glb.len() > 1000);
    }

    #[test]
    fn test_json_roundtrip() {
        let style = HairStyle::red_wavy();
        let json = style.to_json();
        let restored = HairStyle::from_json(&json).unwrap();
        assert!((restored.curl - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn test_density_affects_count() {
        let low = generate_groom(
            &HairStyle {
                density: 0.3,
                ..HairStyle::default()
            },
            4,
        );
        let high = generate_groom(
            &HairStyle {
                density: 1.0,
                ..HairStyle::default()
            },
            4,
        );
        assert!(high.strands.len() > low.strands.len() * 2);
    }

    #[test]
    fn test_render_modes() {
        let style = HairStyle::default();
        let strands = generate_groom(&style, 8);
        let cards = generate_hair_cards(&style);
        let mesh = generate_hair_mesh(&style);
        // All produce non-empty output
        assert!(!strands.strands.is_empty());
        assert!(!cards.is_empty());
        assert!(!mesh.parts.is_empty());
    }

    #[test]
    fn test_100k_strands() {
        let style = HairStyle::default();
        let t0 = std::time::Instant::now();
        let groom = generate_groom_count(&style, 8, 100_000);
        let gen_time = t0.elapsed();
        assert_eq!(groom.strands.len(), 100_000);
        assert_eq!(groom.strands[0].points.len(), 8);
        let total_pts: usize = groom.strands.iter().map(|s| s.points.len()).sum();
        assert_eq!(total_pts, 800_000);

        // Verify all points are finite
        for s in &groom.strands[..100] {
            for p in &s.points {
                assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
            }
        }

        // Benchmark strand buffer generation
        let t1 = std::time::Instant::now();
        let (buf, offsets) = groom.to_strand_buffer();
        let buf_time = t1.elapsed();

        eprintln!("100K strands generated in {:?}", gen_time);
        eprintln!(
            "Strand buffer: {} floats ({:.1} MB) in {:?}",
            buf.len(),
            buf.len() as f64 * 4.0 / 1048576.0,
            buf_time
        );
        eprintln!("Offsets: {} entries", offsets.len());
        assert_eq!(buf.len(), 800_000 * 4); // xyz + width
        assert_eq!(offsets.len(), 100_001); // strand_count + 1
    }

    #[test]
    fn test_back_hemisphere() {
        let style = HairStyle::default();
        let groom = generate_groom(&style, 4);
        // All non-bangs strand roots should have Z <= 0 (back hemisphere)
        let bangs_count = (groom.strands.len() as f32 * style.bangs_width * 0.05) as usize;
        for (i, s) in groom.strands.iter().enumerate().skip(bangs_count) {
            assert!(
                s.points[0].z <= 0.01,
                "strand {i} root z={} should be <= 0",
                s.points[0].z
            );
        }
    }
}
