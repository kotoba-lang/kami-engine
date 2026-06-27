//! VRM humanoid bone mapping ↔ kami-skeleton conversion.

use glam::Mat4;
use kami_skeleton::{Bone, Skeleton};

use crate::VrmError;
use crate::vrm_types::{HumanBoneName, VrmDocument};

/// Convert VRM humanoid bone mapping to kami-skeleton::Skeleton.
///
/// Walks the VRM humanoid bones, resolves each to a glTF node,
/// extracts local TRS from the node, and reads inverse bind matrices
/// from the skin accessor.
pub fn to_kami_skeleton(doc: &VrmDocument) -> Result<Skeleton, VrmError> {
    let skin = doc
        .gltf
        .skins
        .first()
        .ok_or_else(|| VrmError::IncompatibleSkeleton("no skin found".into()))?;

    // Read inverse bind matrices from accessor
    let ibm_accessor_idx = skin
        .inverse_bind_matrices
        .ok_or_else(|| VrmError::IncompatibleSkeleton("no inverseBindMatrices".into()))?;

    let inverse_binds = read_mat4_accessor(doc, ibm_accessor_idx)?;

    let mut bones = Vec::with_capacity(skin.joints.len());

    for (joint_idx, &node_idx) in skin.joints.iter().enumerate() {
        let node = doc
            .gltf
            .nodes
            .get(node_idx)
            .ok_or_else(|| VrmError::AccessorOutOfRange(node_idx))?;

        let name = node
            .name
            .clone()
            .unwrap_or_else(|| format!("bone_{joint_idx}"));

        // Find parent: look for a node in the joints list that has this node as a child
        let parent = skin.joints.iter().position(|&parent_node_idx| {
            doc.gltf
                .nodes
                .get(parent_node_idx)
                .map(|pn| pn.children.contains(&node_idx))
                .unwrap_or(false)
        });

        let local_position = node.translation.unwrap_or([0.0; 3]);
        let local_rotation = node.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let local_scale = node.scale.unwrap_or([1.0; 3]);

        let inverse_bind = inverse_binds
            .get(joint_idx)
            .copied()
            .unwrap_or(Mat4::IDENTITY)
            .to_cols_array_2d();

        bones.push(Bone {
            name,
            parent,
            local_position,
            local_rotation,
            local_scale,
            inverse_bind,
        });
    }

    Ok(Skeleton { bones })
}

/// Find the glTF node index for a given HumanBoneName.
pub fn find_bone_node(doc: &VrmDocument, bone: HumanBoneName) -> Option<usize> {
    doc.humanoid
        .human_bones
        .iter()
        .find(|hb| hb.bone == bone)
        .map(|hb| hb.node)
}

/// Map HumanBoneName to the corresponding kami-skeleton bone name.
pub fn human_bone_to_kami_name(bone: HumanBoneName) -> &'static str {
    bone.as_str()
}

/// Read Mat4 values from a glTF accessor (component type FLOAT, type MAT4).
fn read_mat4_accessor(doc: &VrmDocument, accessor_idx: usize) -> Result<Vec<Mat4>, VrmError> {
    let accessor = doc
        .gltf
        .accessors
        .get(accessor_idx)
        .ok_or(VrmError::AccessorOutOfRange(accessor_idx))?;

    let bv_idx = accessor
        .buffer_view
        .ok_or_else(|| VrmError::AccessorOutOfRange(accessor_idx))?;
    let bv = doc
        .gltf
        .buffer_views
        .get(bv_idx)
        .ok_or(VrmError::BufferViewOutOfRange(bv_idx))?;

    let byte_offset = bv.byte_offset + accessor.byte_offset;
    let mat4_size = 64; // 16 floats × 4 bytes
    let stride = bv.byte_stride.unwrap_or(mat4_size);

    let mut matrices = Vec::with_capacity(accessor.count);
    for i in 0..accessor.count {
        let start = byte_offset + i * stride;
        if start + mat4_size > doc.bin.len() {
            return Err(VrmError::InvalidGlb("inverse bind matrix data truncated"));
        }
        let mut floats = [0.0f32; 16];
        for (j, float) in floats.iter_mut().enumerate() {
            let offset = start + j * 4;
            *float = f32::from_le_bytes([
                doc.bin[offset],
                doc.bin[offset + 1],
                doc.bin[offset + 2],
                doc.bin[offset + 3],
            ]);
        }
        matrices.push(Mat4::from_cols_array(&floats));
    }

    Ok(matrices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bone_name_roundtrip() {
        for &bone in HumanBoneName::ALL {
            let name = bone.as_str();
            let parsed = HumanBoneName::from_str(name).unwrap();
            assert_eq!(bone, parsed);
        }
    }
}
