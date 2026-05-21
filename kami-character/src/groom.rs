//! Groom: Alembic-based strand hair system for MetaHuman.
//!
//! Alembic (.abc) stores hair as ICurves — ordered point arrays per strand.
//! This module provides:
//!   - `GroomAsset`: parsed strand data (positions, widths, UVs)
//!   - `GroomLod`: LOD decimation (strand count reduction)
//!   - `GroomCards`: strand → hair card conversion for WebGPU rasterization
//!   - `GroomInstance`: per-frame interpolation for wind/physics
//!
//! Wire format: KAMI Groom Binary (.kgr) — compact strand storage.
//! Import path: .abc (Alembic ICurves) → GroomAsset → .kgr

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// A single hair strand: ordered control points from root to tip.
#[derive(Debug, Clone)]
pub struct Strand {
    /// Control point positions (world space, root-first).
    pub points: Vec<Vec3>,
    /// Per-point radius (root thicker, tip thinner).
    pub widths: Vec<f32>,
    /// Root UV on scalp surface (for texture lookup).
    pub root_uv: [f32; 2],
    /// Strand group index (for material/color variation).
    pub group: u32,
}

/// Groom asset: collection of strands parsed from Alembic ICurves.
#[derive(Debug, Clone)]
pub struct GroomAsset {
    /// All strands in the groom.
    pub strands: Vec<Strand>,
    /// Strand groups (e.g. "scalp_hair", "eyebrows", "eyelashes", "beard").
    pub groups: Vec<GroomGroup>,
    /// Guide strand indices (subset used for interpolation).
    pub guide_indices: Vec<usize>,
    /// Total point count across all strands.
    pub total_points: usize,
}

/// Named strand group with material binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroomGroup {
    pub name: String,
    pub strand_count: usize,
    /// Material slot index for this group.
    pub material_slot: u32,
    /// Clump noise parameters.
    pub clump_scale: f32,
    pub clump_noise: f32,
}

/// Groom LOD level — controls strand decimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroomLod {
    /// Full strand count (100%).
    Full,
    /// 50% strand count.
    Half,
    /// 25% strand count.
    Quarter,
    /// Hair cards only (no individual strands).
    Cards,
}

/// Hair card: textured quad strip generated from strand clusters.
#[derive(Debug, Clone)]
pub struct HairCard {
    /// Quad strip vertices (position + normal + UV).
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices for the card strip.
    pub indices: Vec<u32>,
    /// Width of the card at root.
    pub root_width: f32,
    /// Material/texture atlas index.
    pub atlas_index: u32,
}

/// Per-frame groom instance (interpolated strand positions for animation).
#[derive(Debug, Clone)]
pub struct GroomInstance {
    /// Interpolated point positions (flat: strand0_p0, strand0_p1, ..., strand1_p0, ...).
    pub positions: Vec<Vec3>,
    /// Strand start indices into positions array.
    pub strand_offsets: Vec<u32>,
}

impl GroomAsset {
    /// Parse from KAMI Groom Binary (.kgr) format.
    ///
    /// Format: header(16B) + groups(N*40B) + strands(M*var) + points(P*12B) + widths(P*4B).
    pub fn from_kgr(data: &[u8]) -> Result<Self, String> {
        if data.len() < 16 {
            return Err("KGR too small".into());
        }
        // Magic: "KGR1"
        if &data[0..4] != b"KGR1" {
            return Err("Invalid KGR magic".into());
        }
        let num_groups = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let num_strands = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let num_points = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

        let mut offset = 16;

        // Parse groups
        let mut groups = Vec::with_capacity(num_groups);
        for _ in 0..num_groups {
            if offset + 20 > data.len() {
                return Err("KGR truncated at groups".into());
            }
            let name_len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;
            let name = String::from_utf8_lossy(&data[offset..offset + name_len]).to_string();
            offset += name_len;
            let strand_count = read_u32_le(data, &mut offset) as usize;
            let material_slot = read_u32_le(data, &mut offset);
            let clump_scale = read_f32_le(data, &mut offset);
            let clump_noise = read_f32_le(data, &mut offset);
            groups.push(GroomGroup {
                name,
                strand_count,
                material_slot,
                clump_scale,
                clump_noise,
            });
        }

        // Parse strand headers (point_count + root_uv + group)
        let mut strands = Vec::with_capacity(num_strands);
        let mut point_counts = Vec::with_capacity(num_strands);
        for _ in 0..num_strands {
            let pc = read_u32_le(data, &mut offset) as usize;
            let ru = read_f32_le(data, &mut offset);
            let rv = read_f32_le(data, &mut offset);
            let grp = read_u32_le(data, &mut offset);
            point_counts.push(pc);
            strands.push(Strand {
                points: Vec::with_capacity(pc),
                widths: Vec::with_capacity(pc),
                root_uv: [ru, rv],
                group: grp,
            });
        }

        // Parse points (Vec3 × num_points)
        let mut all_points = Vec::with_capacity(num_points);
        for _ in 0..num_points {
            let x = read_f32_le(data, &mut offset);
            let y = read_f32_le(data, &mut offset);
            let z = read_f32_le(data, &mut offset);
            all_points.push(Vec3::new(x, y, z));
        }

        // Parse widths (f32 × num_points)
        let mut all_widths = Vec::with_capacity(num_points);
        for _ in 0..num_points {
            all_widths.push(read_f32_le(data, &mut offset));
        }

        // Distribute points/widths to strands
        let mut pi = 0;
        for (i, strand) in strands.iter_mut().enumerate() {
            let pc = point_counts[i];
            strand.points = all_points[pi..pi + pc].to_vec();
            strand.widths = all_widths[pi..pi + pc].to_vec();
            pi += pc;
        }

        // Guide strands: every 4th strand
        let guide_indices: Vec<usize> = (0..num_strands).step_by(4).collect();

        Ok(Self {
            strands,
            groups,
            guide_indices,
            total_points: num_points,
        })
    }

    /// Serialize to KAMI Groom Binary (.kgr) format.
    pub fn to_kgr(&self) -> Vec<u8> {
        let num_points: usize = self.strands.iter().map(|s| s.points.len()).sum();
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(b"KGR1");
        buf.extend_from_slice(&(self.groups.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.strands.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(num_points as u32).to_le_bytes());

        // Groups
        for g in &self.groups {
            let name_bytes = g.name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(name_bytes);
            buf.extend_from_slice(&(g.strand_count as u32).to_le_bytes());
            buf.extend_from_slice(&g.material_slot.to_le_bytes());
            buf.extend_from_slice(&g.clump_scale.to_le_bytes());
            buf.extend_from_slice(&g.clump_noise.to_le_bytes());
        }

        // Strand headers
        for s in &self.strands {
            buf.extend_from_slice(&(s.points.len() as u32).to_le_bytes());
            buf.extend_from_slice(&s.root_uv[0].to_le_bytes());
            buf.extend_from_slice(&s.root_uv[1].to_le_bytes());
            buf.extend_from_slice(&s.group.to_le_bytes());
        }

        // Points
        for s in &self.strands {
            for p in &s.points {
                buf.extend_from_slice(&p.x.to_le_bytes());
                buf.extend_from_slice(&p.y.to_le_bytes());
                buf.extend_from_slice(&p.z.to_le_bytes());
            }
        }

        // Widths
        for s in &self.strands {
            for w in &s.widths {
                buf.extend_from_slice(&w.to_le_bytes());
            }
        }

        buf
    }

    /// Decimate strands to target LOD.
    pub fn decimate(&self, lod: GroomLod) -> Self {
        let ratio = match lod {
            GroomLod::Full => 1.0,
            GroomLod::Half => 0.5,
            GroomLod::Quarter => 0.25,
            GroomLod::Cards => 0.1,
        };
        let target = (self.strands.len() as f32 * ratio).max(1.0) as usize;
        let step = (self.strands.len() as f32 / target as f32).ceil() as usize;
        let strands: Vec<Strand> = self
            .strands
            .iter()
            .step_by(step.max(1))
            .cloned()
            .collect();
        let total_points = strands.iter().map(|s| s.points.len()).sum();
        let guide_indices = (0..strands.len()).step_by(4).collect();
        Self {
            strands,
            groups: self.groups.clone(),
            guide_indices,
            total_points,
        }
    }

    /// Convert strands to hair cards for rasterization.
    ///
    /// Each card is a quad strip following a cluster centroid path.
    pub fn to_hair_cards(&self, cards_per_cluster: usize) -> Vec<HairCard> {
        let cluster_size = (self.strands.len() / cards_per_cluster.max(1)).max(1);
        let mut cards = Vec::new();

        for chunk in self.strands.chunks(cluster_size) {
            if chunk.is_empty() {
                continue;
            }
            // Compute centroid path from cluster
            let max_points = chunk.iter().map(|s| s.points.len()).max().unwrap_or(0);
            if max_points < 2 {
                continue;
            }
            let mut centroid_path = Vec::with_capacity(max_points);
            let mut avg_widths = Vec::with_capacity(max_points);

            for pi in 0..max_points {
                let mut sum = Vec3::ZERO;
                let mut w_sum = 0.0_f32;
                let mut count = 0;
                for s in chunk {
                    if pi < s.points.len() {
                        sum += s.points[pi];
                        w_sum += s.widths.get(pi).copied().unwrap_or(0.001);
                        count += 1;
                    }
                }
                if count > 0 {
                    centroid_path.push(sum / count as f32);
                    avg_widths.push(w_sum / count as f32);
                }
            }

            // Generate quad strip along centroid path
            let n = centroid_path.len();
            let mut positions = Vec::with_capacity(n * 2);
            let mut normals = Vec::with_capacity(n * 2);
            let mut uvs = Vec::with_capacity(n * 2);
            let mut indices = Vec::new();

            for i in 0..n {
                let t = i as f32 / (n - 1) as f32;
                let tangent = if i + 1 < n {
                    (centroid_path[i + 1] - centroid_path[i]).normalize_or_zero()
                } else if i > 0 {
                    (centroid_path[i] - centroid_path[i - 1]).normalize_or_zero()
                } else {
                    Vec3::Y
                };
                let normal = tangent.cross(Vec3::Z).normalize_or_zero();
                let w = avg_widths[i] * 2.0;
                let p = centroid_path[i];

                positions.push(p - normal * w);
                positions.push(p + normal * w);
                normals.push(tangent.cross(normal).normalize_or_zero());
                normals.push(tangent.cross(normal).normalize_or_zero());
                uvs.push([0.0, t]);
                uvs.push([1.0, t]);

                if i > 0 {
                    let base = (i as u32 - 1) * 2;
                    indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
                }
            }

            cards.push(HairCard {
                root_width: avg_widths.first().copied().unwrap_or(0.001) * 2.0,
                atlas_index: chunk.first().map(|s| s.group).unwrap_or(0),
                positions,
                normals,
                uvs,
                indices,
            });
        }

        cards
    }

    /// Build GPU-ready strand buffer for compute shader rendering.
    ///
    /// Returns flat arrays for SSBO upload.
    pub fn to_strand_buffer(&self) -> (Vec<f32>, Vec<u32>) {
        let mut points_flat = Vec::with_capacity(self.total_points * 4); // xyz + width
        let mut offsets = Vec::with_capacity(self.strands.len() + 1);
        let mut offset = 0u32;

        for strand in &self.strands {
            offsets.push(offset);
            for (i, p) in strand.points.iter().enumerate() {
                points_flat.push(p.x);
                points_flat.push(p.y);
                points_flat.push(p.z);
                points_flat.push(strand.widths.get(i).copied().unwrap_or(0.001));
            }
            offset += strand.points.len() as u32;
        }
        offsets.push(offset);

        (points_flat, offsets)
    }
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> u32 {
    let v = u32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    v
}

fn read_f32_le(data: &[u8], offset: &mut usize) -> f32 {
    let v = f32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_groom() -> GroomAsset {
        let strands = (0..100)
            .map(|i| {
                let n = 8;
                let points: Vec<Vec3> = (0..n)
                    .map(|j| Vec3::new(i as f32 * 0.01, -(j as f32 * 0.02), 0.0))
                    .collect();
                let widths = (0..n).map(|j| 0.002 * (1.0 - j as f32 / n as f32)).collect();
                Strand {
                    points,
                    widths,
                    root_uv: [i as f32 / 100.0, 0.0],
                    group: 0,
                }
            })
            .collect();
        GroomAsset {
            total_points: 800,
            guide_indices: (0..100).step_by(4).collect(),
            groups: vec![GroomGroup {
                name: "scalp".into(),
                strand_count: 100,
                material_slot: 0,
                clump_scale: 0.5,
                clump_noise: 0.1,
            }],
            strands,
        }
    }

    #[test]
    fn test_kgr_roundtrip() {
        let groom = test_groom();
        let kgr = groom.to_kgr();
        let restored = GroomAsset::from_kgr(&kgr).unwrap();
        assert_eq!(restored.strands.len(), 100);
        assert_eq!(restored.groups.len(), 1);
        assert_eq!(restored.total_points, 800);
        assert!((restored.strands[0].points[0].x).abs() < 0.001);
    }

    #[test]
    fn test_decimate() {
        let groom = test_groom();
        let half = groom.decimate(GroomLod::Half);
        assert!(half.strands.len() <= 55 && half.strands.len() >= 45);
        let quarter = groom.decimate(GroomLod::Quarter);
        assert!(quarter.strands.len() <= 30);
    }

    #[test]
    fn test_hair_cards() {
        let groom = test_groom();
        let cards = groom.to_hair_cards(10);
        assert_eq!(cards.len(), 10);
        for card in &cards {
            assert!(!card.positions.is_empty());
            assert!(!card.indices.is_empty());
        }
    }

    #[test]
    fn test_strand_buffer() {
        let groom = test_groom();
        let (points, offsets) = groom.to_strand_buffer();
        assert_eq!(points.len(), 800 * 4); // 800 points × 4 floats
        assert_eq!(offsets.len(), 101); // 100 strands + sentinel
    }
}
