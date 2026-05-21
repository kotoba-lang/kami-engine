//! Skeletal Mesh + Skeleton: MetaHuman .uasset equivalent for KAMI engine.
//!
//! KAMI Skeletal Mesh Binary (.ksm) — GPU-ready format for WebGPU skinning.
//!
//! Contents:
//!   - Skinned vertex buffer (pos + normal + uv + tangent + joint_indices + joint_weights)
//!   - Index buffer (u32)
//!   - LOD mesh sections (per-LOD vertex/index ranges + material slots)
//!   - Morph targets (position + normal deltas, sparse)
//!   - Skeleton reference (inline or external .ksk)
//!
//! Vertex layout (64 bytes/vertex for GPU skinning):
//!   position:      vec3<f32>  (12B)
//!   normal:        vec3<f32>  (12B)
//!   uv:            vec2<f32>  (8B)
//!   tangent:       vec4<f32>  (16B) — xyz=tangent, w=bitangent sign
//!   joint_indices: vec4<u16>  (8B)  — up to 4 bone influences
//!   joint_weights: vec4<u16>  (8B)  — normalized to u16 (0–65535)

use glam::{Vec2, Vec3, Vec4};
use serde::{Deserialize, Serialize};

/// Skinned vertex for GPU upload (64 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
    pub joint_indices: [u16; 4],
    pub joint_weights: [u16; 4],
}

/// LOD section: defines a sub-range of the index buffer with a material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodSection {
    pub lod_level: u32,
    /// Start index in the index buffer.
    pub index_start: u32,
    /// Number of indices.
    pub index_count: u32,
    /// Material slot index.
    pub material_slot: u32,
    /// Vertex range (for LOD vertex stripping).
    pub vertex_start: u32,
    pub vertex_count: u32,
}

/// Morph target: sparse vertex deltas.
#[derive(Debug, Clone)]
pub struct MorphTarget {
    pub name: String,
    /// Sparse deltas: (vertex_index, position_delta, normal_delta).
    pub deltas: Vec<MorphDelta>,
}

/// Single vertex delta in a morph target.
#[derive(Debug, Clone, Copy)]
pub struct MorphDelta {
    pub vertex_index: u32,
    pub position_delta: Vec3,
    pub normal_delta: Vec3,
}

/// Material slot definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialSlot {
    pub name: String,
    pub slot_index: u32,
    /// PBR material type for this slot.
    pub material_type: SkeletalMaterialType,
}

/// Material types for MetaHuman skeletal mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkeletalMaterialType {
    /// Head skin (3-layer SSS).
    HeadSkin,
    /// Body skin.
    BodySkin,
    /// Eye (sclera + iris + cornea).
    Eye,
    /// Teeth/gum.
    Teeth,
    /// Tongue.
    Tongue,
    /// Eyelash.
    Eyelash,
    /// Eyebrow cards.
    Eyebrow,
    /// Clothing fabric.
    Clothing,
}

/// Complete skeletal mesh asset.
#[derive(Debug, Clone)]
pub struct SkeletalMeshAsset {
    /// GPU-ready skinned vertices.
    pub vertices: Vec<SkinnedVertex>,
    /// Triangle indices (u32).
    pub indices: Vec<u32>,
    /// LOD sections.
    pub lod_sections: Vec<LodSection>,
    /// Morph targets (blendshapes).
    pub morph_targets: Vec<MorphTarget>,
    /// Material slots.
    pub material_slots: Vec<MaterialSlot>,
    /// Embedded skeleton (or reference).
    pub skeleton: Option<kami_skeleton::Skeleton>,
    /// Bounding box min/max.
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl SkeletalMeshAsset {
    /// Parse from KAMI Skeletal Mesh Binary (.ksm).
    ///
    /// Format: header(32B) + vertices(N*64B) + indices(M*4B) + lod_sections + morph_targets.
    pub fn from_ksm(data: &[u8]) -> Result<Self, String> {
        if data.len() < 32 {
            return Err("KSM too small".into());
        }
        if &data[0..4] != b"KSM1" {
            return Err("Invalid KSM magic".into());
        }

        let num_verts = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let num_indices = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let num_lod_sections = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let num_morph_targets = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        let num_material_slots = u32::from_le_bytes(data[20..24].try_into().unwrap()) as usize;
        // bytes 24..32 reserved

        let mut offset = 32;

        // Parse vertices (64 bytes each)
        let vert_bytes = num_verts * 64;
        if offset + vert_bytes > data.len() {
            return Err("KSM truncated at vertices".into());
        }
        let vertices: Vec<SkinnedVertex> =
            bytemuck::cast_slice(&data[offset..offset + vert_bytes]).to_vec();
        offset += vert_bytes;

        // Parse indices (4 bytes each)
        let idx_bytes = num_indices * 4;
        if offset + idx_bytes > data.len() {
            return Err("KSM truncated at indices".into());
        }
        let indices: Vec<u32> = bytemuck::cast_slice(&data[offset..offset + idx_bytes]).to_vec();
        offset += idx_bytes;

        // Parse LOD sections
        let mut lod_sections = Vec::with_capacity(num_lod_sections);
        for _ in 0..num_lod_sections {
            let sec_bytes = &data[offset..offset + 24];
            lod_sections.push(LodSection {
                lod_level: u32::from_le_bytes(sec_bytes[0..4].try_into().unwrap()),
                index_start: u32::from_le_bytes(sec_bytes[4..8].try_into().unwrap()),
                index_count: u32::from_le_bytes(sec_bytes[8..12].try_into().unwrap()),
                material_slot: u32::from_le_bytes(sec_bytes[12..16].try_into().unwrap()),
                vertex_start: u32::from_le_bytes(sec_bytes[16..20].try_into().unwrap()),
                vertex_count: u32::from_le_bytes(sec_bytes[20..24].try_into().unwrap()),
            });
            offset += 24;
        }

        // Compute bounds
        let (mut bmin, mut bmax) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &vertices {
            let p = Vec3::from(v.position);
            bmin = bmin.min(p);
            bmax = bmax.max(p);
        }

        Ok(Self {
            vertices,
            indices,
            lod_sections,
            morph_targets: Vec::new(), // TODO: parse morph targets
            material_slots: Vec::new(),
            skeleton: None,
            bounds_min: bmin,
            bounds_max: bmax,
        })
    }

    /// Serialize to KAMI Skeletal Mesh Binary (.ksm).
    pub fn to_ksm(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(b"KSM1");
        buf.extend_from_slice(&(self.vertices.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.indices.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.lod_sections.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.morph_targets.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.material_slots.len() as u32).to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // reserved

        // Vertices
        buf.extend_from_slice(bytemuck::cast_slice(&self.vertices));

        // Indices
        buf.extend_from_slice(bytemuck::cast_slice(&self.indices));

        // LOD sections
        for sec in &self.lod_sections {
            buf.extend_from_slice(&sec.lod_level.to_le_bytes());
            buf.extend_from_slice(&sec.index_start.to_le_bytes());
            buf.extend_from_slice(&sec.index_count.to_le_bytes());
            buf.extend_from_slice(&sec.material_slot.to_le_bytes());
            buf.extend_from_slice(&sec.vertex_start.to_le_bytes());
            buf.extend_from_slice(&sec.vertex_count.to_le_bytes());
        }

        buf
    }

    /// Get indices for a specific LOD level.
    pub fn lod_indices(&self, lod: u32) -> &[u32] {
        for sec in &self.lod_sections {
            if sec.lod_level == lod {
                let start = sec.index_start as usize;
                let end = start + sec.index_count as usize;
                return &self.indices[start..end.min(self.indices.len())];
            }
        }
        &self.indices
    }

    /// Apply a morph target at given weight (0.0–1.0).
    ///
    /// Returns modified vertex positions (clone of self.vertices with deltas applied).
    pub fn apply_morph(&self, target_name: &str, weight: f32) -> Vec<SkinnedVertex> {
        let mut verts = self.vertices.clone();
        if let Some(target) = self.morph_targets.iter().find(|t| t.name == target_name) {
            for delta in &target.deltas {
                let vi = delta.vertex_index as usize;
                if vi < verts.len() {
                    verts[vi].position[0] += delta.position_delta.x * weight;
                    verts[vi].position[1] += delta.position_delta.y * weight;
                    verts[vi].position[2] += delta.position_delta.z * weight;
                    verts[vi].normal[0] += delta.normal_delta.x * weight;
                    verts[vi].normal[1] += delta.normal_delta.y * weight;
                    verts[vi].normal[2] += delta.normal_delta.z * weight;
                }
            }
        }
        verts
    }

    /// Total triangle count.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// GPU skinning vertex buffer layout descriptor (for wgpu pipeline).
///
/// 64 bytes/vertex: pos(12) + normal(12) + uv(8) + tangent(16) + joints(8) + weights(8).
pub fn skinned_vertex_buffer_layout() -> Vec<(u64, u32, &'static str)> {
    vec![
        (0, 0, "float32x3"),   // position
        (12, 1, "float32x3"),  // normal
        (24, 2, "float32x2"),  // uv
        (32, 3, "float32x4"),  // tangent
        (48, 4, "uint16x4"),   // joint_indices
        (56, 5, "uint16x4"),   // joint_weights
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skinned_vertex_size() {
        assert_eq!(std::mem::size_of::<SkinnedVertex>(), 64);
    }

    #[test]
    fn test_ksm_roundtrip() {
        let v = SkinnedVertex {
            position: [1.0, 2.0, 3.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joint_indices: [0, 1, 0, 0],
            joint_weights: [40000, 25535, 0, 0],
        };
        let asset = SkeletalMeshAsset {
            vertices: vec![v; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            lod_sections: vec![LodSection {
                lod_level: 0,
                index_start: 0,
                index_count: 6,
                material_slot: 0,
                vertex_start: 0,
                vertex_count: 4,
            }],
            morph_targets: vec![],
            material_slots: vec![],
            skeleton: None,
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ONE,
        };

        let ksm = asset.to_ksm();
        let restored = SkeletalMeshAsset::from_ksm(&ksm).unwrap();
        assert_eq!(restored.vertices.len(), 4);
        assert_eq!(restored.indices.len(), 6);
        assert_eq!(restored.lod_sections.len(), 1);
        assert!((restored.vertices[0].position[0] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_lod_indices() {
        let asset = SkeletalMeshAsset {
            vertices: vec![],
            indices: vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
            lod_sections: vec![
                LodSection { lod_level: 0, index_start: 0, index_count: 6, material_slot: 0, vertex_start: 0, vertex_count: 4 },
                LodSection { lod_level: 1, index_start: 6, index_count: 3, material_slot: 0, vertex_start: 0, vertex_count: 3 },
            ],
            morph_targets: vec![],
            material_slots: vec![],
            skeleton: None,
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ONE,
        };
        assert_eq!(asset.lod_indices(0).len(), 6);
        assert_eq!(asset.lod_indices(1).len(), 3);
    }

    #[test]
    fn test_vertex_layout() {
        let layout = skinned_vertex_buffer_layout();
        assert_eq!(layout.len(), 6);
        assert_eq!(layout[0].0, 0);  // position offset
        assert_eq!(layout[5].0, 56); // weights offset
    }
}
