//! Conversions between kami-vrm types and kami-skeleton types.

use crate::VrmError;
use crate::gltf_types::component_type;
use crate::vrm_types::VrmDocument;

/// Read typed data from a glTF accessor in the BIN chunk.
///
/// Supports FLOAT, UNSIGNED_SHORT, UNSIGNED_INT, UNSIGNED_BYTE component types.
pub fn read_accessor_f32(doc: &VrmDocument, accessor_idx: usize) -> Result<Vec<f32>, VrmError> {
    let acc = doc
        .gltf
        .accessors
        .get(accessor_idx)
        .ok_or(VrmError::AccessorOutOfRange(accessor_idx))?;

    let bv_idx = acc
        .buffer_view
        .ok_or(VrmError::AccessorOutOfRange(accessor_idx))?;
    let bv = doc
        .gltf
        .buffer_views
        .get(bv_idx)
        .ok_or(VrmError::BufferViewOutOfRange(bv_idx))?;

    let components = match acc.accessor_type.as_str() {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        _ => 1,
    };

    let elem_size = match acc.component_type {
        component_type::FLOAT => 4,
        component_type::UNSIGNED_SHORT => 2,
        component_type::UNSIGNED_BYTE => 1,
        component_type::UNSIGNED_INT => 4,
        _ => 4,
    };
    let default_stride = components * elem_size;
    let stride = bv.byte_stride.unwrap_or(default_stride);
    let base = bv.byte_offset + acc.byte_offset;

    let mut result = Vec::with_capacity(acc.count * components);
    for i in 0..acc.count {
        let offset = base + i * stride;
        for c in 0..components {
            let o = offset + c * elem_size;
            let val = match acc.component_type {
                component_type::FLOAT => {
                    if o + 4 > doc.bin.len() {
                        return Err(VrmError::InvalidGlb("accessor data truncated"));
                    }
                    f32::from_le_bytes([doc.bin[o], doc.bin[o + 1], doc.bin[o + 2], doc.bin[o + 3]])
                }
                component_type::UNSIGNED_SHORT => {
                    if o + 2 > doc.bin.len() {
                        return Err(VrmError::InvalidGlb("accessor data truncated"));
                    }
                    u16::from_le_bytes([doc.bin[o], doc.bin[o + 1]]) as f32
                }
                component_type::UNSIGNED_BYTE => {
                    if o + 1 > doc.bin.len() {
                        return Err(VrmError::InvalidGlb("accessor data truncated"));
                    }
                    doc.bin[o] as f32
                }
                component_type::UNSIGNED_INT => {
                    if o + 4 > doc.bin.len() {
                        return Err(VrmError::InvalidGlb("accessor data truncated"));
                    }
                    u32::from_le_bytes([doc.bin[o], doc.bin[o + 1], doc.bin[o + 2], doc.bin[o + 3]])
                        as f32
                }
                _ => 0.0,
            };
            result.push(val);
        }
    }

    Ok(result)
}

/// Extract interleaved vertex data (pos3+norm3+uv2 = 32B/vertex) from a VRM mesh primitive.
///
/// Returns (vertices: Vec<f32>, indices: Vec<u32>) ready for kami-render upload.
pub fn extract_primitive_mesh(
    doc: &VrmDocument,
    mesh_idx: usize,
    prim_idx: usize,
) -> Result<(Vec<f32>, Vec<u32>), VrmError> {
    let mesh = doc
        .gltf
        .meshes
        .get(mesh_idx)
        .ok_or_else(|| VrmError::Part(format!("mesh {mesh_idx} not found")))?;
    let prim = mesh
        .primitives
        .get(prim_idx)
        .ok_or_else(|| VrmError::Part(format!("primitive {prim_idx} not found")))?;

    // Read POSITION
    let pos_acc =
        prim.attributes
            .get("POSITION")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| VrmError::Part("missing POSITION attribute".into()))? as usize;
    let positions = read_accessor_f32(doc, pos_acc)?;

    // Read NORMAL (optional, generate flat normals if missing)
    let normals = if let Some(norm_acc) = prim.attributes.get("NORMAL").and_then(|v| v.as_u64()) {
        read_accessor_f32(doc, norm_acc as usize)?
    } else {
        vec![0.0, 1.0, 0.0].repeat(positions.len() / 3)
    };

    // Read TEXCOORD_0 (optional)
    let uvs = if let Some(uv_acc) = prim.attributes.get("TEXCOORD_0").and_then(|v| v.as_u64()) {
        read_accessor_f32(doc, uv_acc as usize)?
    } else {
        vec![0.0; (positions.len() / 3) * 2]
    };

    let vertex_count = positions.len() / 3;

    // Interleave: [pos3, norm3, uv2] × N
    let mut vertices = Vec::with_capacity(vertex_count * 8);
    for i in 0..vertex_count {
        vertices.push(positions[i * 3]);
        vertices.push(positions[i * 3 + 1]);
        vertices.push(positions[i * 3 + 2]);
        vertices.push(normals[i * 3]);
        vertices.push(normals[i * 3 + 1]);
        vertices.push(normals[i * 3 + 2]);
        vertices.push(uvs[i * 2]);
        vertices.push(uvs[i * 2 + 1]);
    }

    // Read indices
    let indices = if let Some(idx_acc) = prim.indices {
        let raw = read_accessor_f32(doc, idx_acc)?;
        raw.iter().map(|&v| v as u32).collect()
    } else {
        (0..vertex_count as u32).collect()
    };

    Ok((vertices, indices))
}
