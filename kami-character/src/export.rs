//! GLB export — CharacterMesh → binary glTF 2.0 (.glb).

use crate::material::PbrMaterial;
use crate::params::CharacterDef;
use crate::{CharacterMesh, MaterialId, Vertex};
use std::collections::HashMap;

/// Export CharacterMesh to GLB binary (Vec<u8>).
pub fn export_glb(mesh: &CharacterMesh, def: &CharacterDef) -> Vec<u8> {
    let mat_order = [
        MaterialId::Skin,
        MaterialId::EyeWhite,
        MaterialId::Iris,
        MaterialId::Pupil,
        MaterialId::Lip,
        MaterialId::Eyebrow,
        MaterialId::Hair,
        MaterialId::Clothing,
        MaterialId::Eyelash,
    ];

    // Group parts by material
    let mut groups: HashMap<MaterialId, (Vec<Vertex>, Vec<u32>)> = HashMap::new();
    for part in &mesh.parts {
        let entry = groups
            .entry(part.material)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let offset = entry.0.len() as u32;
        entry.0.extend_from_slice(&part.vertices);
        for &idx in &part.indices {
            entry.1.push(idx + offset);
        }
    }

    // Build binary buffer
    let mut buf: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut primitives = Vec::new();
    let mut materials_json = Vec::new();
    let mut mat_index_map: HashMap<MaterialId, usize> = HashMap::new();

    for &mid in &mat_order {
        let (verts, indices) = match groups.get(&mid) {
            Some(g) if !g.0.is_empty() => g,
            _ => continue,
        };

        let mat_idx = materials_json.len();
        mat_index_map.insert(mid, mat_idx);

        let pbr = PbrMaterial::for_part(
            mid,
            &def.skin,
            &def.eyes,
            &def.mouth,
            &def.hair,
            &def.clothing,
        );
        materials_json.push(format!(
            r#"{{"name":"{}","pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},{}],"metallicFactor":{},"roughnessFactor":{}}},"doubleSided":true}}"#,
            pbr.name, pbr.base_color[0], pbr.base_color[1], pbr.base_color[2], pbr.base_color[3],
            pbr.metallic, pbr.roughness,
        ));

        // Vertex data (interleaved: pos3+norm3+uv2 = 32 bytes)
        let mut vdata: Vec<u8> = Vec::with_capacity(verts.len() * 32);
        let mut v_min = [f32::MAX; 3];
        let mut v_max = [f32::MIN; 3];
        for v in verts {
            let p = [v.position.x, v.position.y, v.position.z];
            for k in 0..3 {
                v_min[k] = v_min[k].min(p[k]);
                v_max[k] = v_max[k].max(p[k]);
            }
            vdata.extend_from_slice(bytemuck::bytes_of(&p));
            vdata.extend_from_slice(bytemuck::bytes_of(&[v.normal.x, v.normal.y, v.normal.z]));
            vdata.extend_from_slice(bytemuck::bytes_of(&v.uv));
        }
        pad4(&mut vdata);

        let v_bv = buffer_views.len();
        let v_off = buf.len();
        buf.extend_from_slice(&vdata);
        buffer_views.push(format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{},"byteStride":32,"target":34962}}"#,
            v_off,
            vdata.len()
        ));

        // Index data (u32)
        let idata: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let mut idata_padded = idata.clone();
        pad4(&mut idata_padded);

        let i_bv = buffer_views.len();
        let i_off = buf.len();
        buf.extend_from_slice(&idata_padded);
        buffer_views.push(format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{},"target":34963}}"#,
            i_off,
            idata.len()
        ));

        let pos_a = accessors.len();
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":5126,"count":{},"type":"VEC3","byteOffset":0,"min":[{},{},{}],"max":[{},{},{}]}}"#,
            v_bv, verts.len(), v_min[0], v_min[1], v_min[2], v_max[0], v_max[1], v_max[2]
        ));
        let norm_a = accessors.len();
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":5126,"count":{},"type":"VEC3","byteOffset":12}}"#,
            v_bv,
            verts.len()
        ));
        let uv_a = accessors.len();
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":5126,"count":{},"type":"VEC2","byteOffset":24}}"#,
            v_bv,
            verts.len()
        ));
        let idx_a = accessors.len();
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":5125,"count":{},"type":"SCALAR"}}"#,
            i_bv,
            indices.len()
        ));

        primitives.push(format!(
            r#"{{"attributes":{{"POSITION":{},"NORMAL":{},"TEXCOORD_0":{}}},"indices":{},"material":{}}}"#,
            pos_a, norm_a, uv_a, idx_a, mat_idx
        ));
    }

    // Build JSON
    let json = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"kami-character\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"nodes\":[{{\"mesh\":0,\"name\":\"character\"}}],\"meshes\":[{{\"primitives\":[{}]}}],\"accessors\":[{}],\"bufferViews\":[{}],\"materials\":[{}],\"buffers\":[{{\"byteLength\":{}}}]}}",
        primitives.join(","),
        accessors.join(","),
        buffer_views.join(","),
        materials_json.join(","),
        buf.len(),
    );

    let mut json_bytes = json.into_bytes();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    // GLB container
    let glb_len = 12 + 8 + json_bytes.len() + 8 + buf.len();
    let mut out = Vec::with_capacity(glb_len);

    // Header
    out.extend_from_slice(&0x46546C67u32.to_le_bytes()); // magic
    out.extend_from_slice(&2u32.to_le_bytes()); // version
    out.extend_from_slice(&(glb_len as u32).to_le_bytes());

    // JSON chunk
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(&json_bytes);

    // BIN chunk
    out.extend_from_slice(&(buf.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&buf);

    out
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::CharacterDef;

    #[test]
    fn test_export_glb() {
        let def = CharacterDef::default();
        let mesh = crate::generate_character(&def);
        let glb = export_glb(&mesh, &def);
        // Verify GLB magic
        assert_eq!(&glb[0..4], &[0x67, 0x6C, 0x54, 0x46]); // "glTF"
        assert!(glb.len() > 1000, "GLB too small: {} bytes", glb.len());
    }
}
