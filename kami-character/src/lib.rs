//! kami-character: MetaHuman-compatible character SDK for KAMI Engine.
//!
//! ## SDK Overview
//!
//! ### Character Pipeline
//! ```text
//! Photo → Murakumo VL → CharacterDef + HairStyle + MetaHumanDna
//!   → generate_character() / generate_metahuman() / DnaFile::from_bytes()
//!   → CharacterMesh / MetaHumanMesh / TriangulatedMesh
//!   → export_glb() / SkinnedVertex GPU upload
//! ```
//!
//! ### MetaHuman DNA Pipeline
//! ```text
//! .dna binary (Epic Games) → DnaFile::from_bytes() → 50 meshes + 865 joints + 687 BS
//!   → triangulate_mesh() → positions/normals/UVs/indices (WebGPU ready)
//!   → to_skeleton() → kami_skeleton::Skeleton (bone hierarchy)
//! ```
//!
//! ### Hair Generation Pipeline
//! ```text
//! HairStyle params (JSON) → generate_groom() → GroomAsset (strand curves)
//!   → to_hair_cards() → HairCard (quad strips for rasterization)
//!   → to_kgr() / from_kgr() → .kgr binary (storage)
//!   → metahuman_hair.wgsl (Kajiya-Kay BRDF, WebGPU)
//! ```
//!
//! ### Face Animation Pipeline
//! ```text
//! FACS AU weights → ControlRig::evaluate() → bone transforms
//!   → AnimBlueprint::update() → state machine → blend tree → pose
//!   → Skeleton::joint_matrices() → GPU skinning
//! ```

// ─── Core Modules ───

pub mod base_mesh;
pub mod blendshape;
pub mod body;
pub mod export;
pub mod hair;
pub mod material;
pub mod params;

// ─── MetaHuman Modules ───

/// MetaHuman DNA/FACS/LOD system: `MetaHumanDna`, `FacsActionUnit`, `MetaHumanLod`, `generate_metahuman()`.
pub mod metahuman;

/// Epic Games .dna binary parser: `DnaFile::from_bytes()`, `triangulate_mesh()`, `to_skeleton()`.
pub mod dna;

/// GPU skinned vertex format: `SkinnedVertex` (64B), `SkeletalMeshAsset`, `.ksm` binary.
pub mod skeletal_mesh;

/// Strand-based hair: `GroomAsset`, `Strand`, `HairCard`, `.kgr` binary, LOD decimation.
pub mod groom;

/// Parametric hair generator: `HairStyle` params → `GroomAsset` / `Vec<HairCard>`.
pub mod hair_gen;

/// Control rig DAG: FACS AU → bone transforms, `ControlRig::metahuman_face_rig()`.
pub mod control_rig;

/// Animation state machine: `AnimBlueprint`, layers, blend spaces, transitions.
pub mod anim_blueprint;

// ─── Re-exports for SDK convenience ───

pub use anim_blueprint::AnimBlueprint;
pub use control_rig::ControlRig;
pub use dna::DnaFile;
pub use groom::{GroomAsset, HairCard, Strand};
pub use hair_gen::{
    generate_groom, generate_groom_count, generate_hair_cards, generate_hair_glb,
    generate_hair_mesh, generate_hair_mesh_data, HairMeshData, HairMeshOutput, HairRenderMode,
    HairStyle, HairType,
};
pub use metahuman::{generate_metahuman, FacsActionUnit, MetaHumanDna, MetaHumanLod};
pub use skeletal_mesh::{SkeletalMeshAsset, SkinnedVertex};

// ─── Core Types ───

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Vertex with position, normal, UV.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}

/// A mesh part with a material assignment.
#[derive(Debug, Clone)]
pub struct MeshPart {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: MaterialId,
}

/// Material identifier for character parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialId {
    Skin,
    EyeWhite,
    Iris,
    Pupil,
    Lip,
    Eyebrow,
    Hair,
    Clothing,
    Eyelash,
}

/// Complete character mesh ready for GLB export.
#[derive(Debug, Clone)]
pub struct CharacterMesh {
    pub parts: Vec<MeshPart>,
    pub skeleton: Option<kami_skeleton::Skeleton>,
    pub blendshape_targets: Vec<BlendshapeTarget>,
}

/// A blendshape target (morph target) — vertex position deltas.
#[derive(Debug, Clone)]
pub struct BlendshapeTarget {
    pub name: String,
    pub deltas: Vec<Vec3>,
}

/// Generate a complete character mesh from parameters.
pub fn generate_character(def: &params::CharacterDef) -> CharacterMesh {
    let (mut head_verts, head_indices) = base_mesh::generate_head(48, 64);
    blendshape::apply_face_shape(&mut head_verts, &def.face);
    blendshape::apply_eye_shape(&mut head_verts, &def.eyes);
    blendshape::apply_nose_shape(&mut head_verts, &def.nose);
    blendshape::apply_mouth_shape(&mut head_verts, &def.mouth);
    base_mesh::laplacian_smooth(&mut head_verts, &head_indices, 2, 0.2);
    let head_normals = base_mesh::compute_normals(&head_verts, &head_indices);
    let head_uvs = base_mesh::frontal_uv(&head_verts);
    let head_vertices: Vec<Vertex> = head_verts
        .iter()
        .enumerate()
        .map(|(i, &pos)| Vertex {
            position: pos,
            normal: head_normals[i],
            uv: head_uvs[i],
        })
        .collect();
    let eye_parts = base_mesh::generate_eyes(&def.eyes);
    let hair_part = hair::generate_hair(&def.hair);
    let body_part = body::generate_body(&def.body);
    let clothing_part = body::generate_clothing(&def.clothing, &def.body);
    let expression_targets = blendshape::generate_arkit_targets(head_verts.len());
    let mut parts = vec![MeshPart {
        name: "head".into(),
        vertices: head_vertices,
        indices: head_indices,
        material: MaterialId::Skin,
    }];
    parts.extend(eye_parts);
    parts.push(hair_part);
    parts.push(body_part);
    parts.push(clothing_part);
    let skeleton = body::generate_humanoid_skeleton();
    CharacterMesh {
        parts,
        skeleton: Some(skeleton),
        blendshape_targets: expression_targets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_default_character() {
        let def = params::CharacterDef::default();
        let mesh = generate_character(&def);
        assert!(!mesh.parts.is_empty());
        assert!(mesh.parts.iter().any(|p| p.name == "head"));
        assert!(mesh.parts.iter().any(|p| p.material == MaterialId::Hair));
        let total_verts: usize = mesh.parts.iter().map(|p| p.vertices.len()).sum();
        assert!(
            total_verts > 5000,
            "Expected 5K+ verts, got {}",
            total_verts
        );
    }

    #[test]
    fn test_blendshape_targets() {
        let def = params::CharacterDef::default();
        let mesh = generate_character(&def);
        assert_eq!(
            mesh.blendshape_targets.len(),
            52,
            "Expected 52 ARKit targets"
        );
    }

    #[test]
    fn test_sdk_re_exports() {
        // Verify SDK re-exports are accessible
        let _style = HairStyle::default();
        let _dna_type = MetaHumanLod::Lod0;
        let _au = FacsActionUnit::Au12LipCornerPull;
        let _rig = ControlRig::metahuman_face_rig();
        let _bp = AnimBlueprint::metahuman_default();
    }
}
