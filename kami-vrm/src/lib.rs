//! kami-vrm: VRM avatar part composition (parse, decompose, compose, export).
//!
//! Pipeline:
//! ```text
//! GLB bytes  → parse::parse_vrm()    → VrmDocument
//! VrmDocument → part::decompose()     → Vec<VrmPart>
//! Vec<VrmPart> → compose::compose()   → VrmDocument
//! VrmDocument → export::export_glb()  → GLB bytes
//! ```
//!
//! ## Example
//! ```ignore
//! // Parse base avatar
//! let doc = kami_vrm::parse_vrm(&base_glb)?;
//! let parts = kami_vrm::decompose(&doc)?;
//!
//! // Parse replacement hair
//! let hair_doc = kami_vrm::parse_vrm(&hair_glb)?;
//! let hair_parts = kami_vrm::decompose(&hair_doc)?;
//! let new_hair = hair_parts.iter().find(|p| p.category == PartCategory::Hair).unwrap();
//!
//! // Compose: base body + new hair
//! let body = parts.iter().find(|p| p.category == PartCategory::Body).unwrap();
//! let sources = vec![
//!     PartSource { part: body, doc: &doc },
//!     PartSource { part: new_hair, doc: &hair_doc },
//! ];
//! let composed = kami_vrm::compose(&sources, &ComposeConfig { skeleton_base: 0 })?;
//! let output = kami_vrm::export_glb(&composed)?;
//! ```

pub mod glb;
pub mod gltf_types;
pub mod vrm_types;
pub mod parse;
pub mod compat;
pub mod humanoid;
pub mod part;
pub mod compose;
pub mod export;
pub mod convert;
pub mod spring;
pub mod constraint;
pub mod expression;
pub mod firstperson;

// Re-exports for convenience.
pub use compose::{ComposeConfig, PartSource};
pub use expression::{ColorOverride, ExpressionManager, ResolvedExpression, UvOverride};
pub use firstperson::{node_visible, FirstPersonResolver, FirstPersonView};
pub use part::{decompose, PartCategory, VrmPart};
pub use parse::parse_vrm;
pub use export::export_glb;
pub use vrm_types::*;

use thiserror::Error;

/// Errors from VRM parsing, composition, or export.
#[derive(Debug, Error)]
pub enum VrmError {
    #[error("invalid GLB: {0}")]
    InvalidGlb(&'static str),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing required extension: {0}")]
    MissingExtension(String),
    #[error("accessor out of range: index {0}")]
    AccessorOutOfRange(usize),
    #[error("buffer view out of range: index {0}")]
    BufferViewOutOfRange(usize),
    #[error("incompatible skeleton: {0}")]
    IncompatibleSkeleton(String),
    #[error("unsupported VRM version: {0}")]
    UnsupportedVersion(String),
    #[error("part error: {0}")]
    Part(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal VRM 1.0 GLB for testing.
    fn make_test_vrm() -> Vec<u8> {
        let json = serde_json::json!({
            "asset": { "version": "2.0", "generator": "kami-vrm-test" },
            "extensionsUsed": ["VRMC_vrm"],
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [
                { "name": "Root", "children": [1, 2] },
                { "name": "Hips", "mesh": 0, "skin": 0, "translation": [0, 0.8, 0] },
                { "name": "Head", "mesh": 1, "translation": [0, 0.4, 0] },
            ],
            "meshes": [
                {
                    "name": "Body",
                    "primitives": [{
                        "attributes": { "POSITION": 0 },
                        "indices": 1,
                        "material": 0,
                    }],
                },
                {
                    "name": "Hair",
                    "primitives": [{
                        "attributes": { "POSITION": 2 },
                        "indices": 3,
                        "material": 1,
                    }],
                },
            ],
            "materials": [
                { "name": "skin_material", "pbrMetallicRoughness": { "baseColorFactor": [0.9, 0.7, 0.6, 1.0] } },
                { "name": "hair_material", "pbrMetallicRoughness": { "baseColorFactor": [0.2, 0.1, 0.05, 1.0] } },
            ],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [-0.5, -0.5, -0.5], "max": [0.5, 0.5, 0.5] },
                { "bufferView": 1, "componentType": 5125, "count": 3, "type": "SCALAR" },
                { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC3", "min": [-0.3, 0.0, -0.3], "max": [0.3, 0.5, 0.3] },
                { "bufferView": 3, "componentType": 5125, "count": 3, "type": "SCALAR" },
            ],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 12 },
                { "buffer": 0, "byteOffset": 48, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 84, "byteLength": 12 },
            ],
            "buffers": [{ "byteLength": 96 }],
            "skins": [{
                "joints": [0, 1, 2],
                "inverseBindMatrices": 4,
            }],
            "extensions": {
                "VRMC_vrm": {
                    "specVersion": "1.0",
                    "meta": {
                        "name": "TestAvatar",
                        "authors": ["test"],
                        "licenseUrl": "https://vrm.dev/licenses/1.0/",
                        "avatarPermission": "everyone",
                    },
                    "humanoid": {
                        "humanBones": {
                            "hips": { "node": 1 },
                            "head": { "node": 2 },
                        }
                    }
                }
            }
        });

        // Binary: 3 verts (body) + 3 indices + 3 verts (hair) + 3 indices + 3 IBM mat4
        let mut bin = Vec::new();
        // Body positions (3 verts × 3 floats × 4 bytes = 36 bytes)
        for v in &[-0.5f32, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        // Body indices (3 × 4 bytes = 12 bytes)
        for i in &[0u32, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        // Hair positions (36 bytes)
        for v in &[-0.3f32, 0.0, 0.0, 0.3, 0.0, 0.0, 0.0, 0.5, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        // Hair indices (12 bytes)
        for i in &[0u32, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        // Inverse bind matrices (3 × 64 bytes = 192 bytes)
        // Add accessor for IBM
        let mut json_val: serde_json::Value = json.clone();
        json_val["accessors"].as_array_mut().unwrap().push(serde_json::json!({
            "bufferView": 4, "componentType": 5126, "count": 3, "type": "MAT4"
        }));
        json_val["bufferViews"].as_array_mut().unwrap().push(serde_json::json!({
            "buffer": 0, "byteOffset": 96, "byteLength": 192
        }));

        let identity = glam::Mat4::IDENTITY;
        for _ in 0..3 {
            for &f in &identity.to_cols_array() {
                bin.extend_from_slice(&f.to_le_bytes());
            }
        }

        json_val["buffers"][0]["byteLength"] = serde_json::Value::Number((bin.len() as u64).into());

        let json_bytes = serde_json::to_vec(&json_val).unwrap();
        glb::write_glb(&json_bytes, &bin)
    }

    #[test]
    fn parse_test_vrm() {
        let glb = make_test_vrm();
        let doc = parse_vrm(&glb).unwrap();
        assert_eq!(doc.version, VrmVersion::V1_0);
        assert_eq!(doc.meta.name, "TestAvatar");
        assert_eq!(doc.humanoid.human_bones.len(), 2);
        assert_eq!(doc.gltf.meshes.len(), 2);
    }

    #[test]
    fn decompose_test_vrm() {
        let glb = make_test_vrm();
        let doc = parse_vrm(&glb).unwrap();
        let parts = decompose(&doc).unwrap();
        assert!(parts.len() >= 2);

        let body = parts.iter().find(|p| p.category == PartCategory::Body);
        let hair = parts.iter().find(|p| p.category == PartCategory::Hair);
        assert!(body.is_some(), "should find Body part");
        assert!(hair.is_some(), "should find Hair part");
    }

    #[test]
    fn compose_and_export_roundtrip() {
        let glb = make_test_vrm();
        let doc = parse_vrm(&glb).unwrap();
        let parts = decompose(&doc).unwrap();

        let sources: Vec<PartSource<'_>> = parts
            .iter()
            .map(|p| PartSource { part: p, doc: &doc })
            .collect();

        let composed = compose::compose(&sources, &ComposeConfig { skeleton_base: 0 }).unwrap();

        // Export back to GLB
        let output = export_glb(&composed).unwrap();

        // Verify output is valid GLB
        let chunks = glb::parse_glb(&output).unwrap();
        assert!(!chunks.json.is_empty());

        // Re-parse the exported VRM
        let reparsed = parse_vrm(&output).unwrap();
        assert_eq!(reparsed.meta.name, "TestAvatar");
        assert!(reparsed.humanoid.human_bones.len() >= 2);
    }

    #[test]
    fn skeleton_extraction() {
        let glb = make_test_vrm();
        let doc = parse_vrm(&glb).unwrap();
        let skeleton = humanoid::to_kami_skeleton(&doc).unwrap();
        assert_eq!(skeleton.bones.len(), 3); // Root, Hips, Head
    }
}
