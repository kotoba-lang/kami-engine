//! VRM part decomposition: VrmDocument → Vec<VrmPart>.

use std::collections::HashSet;

use crate::VrmError;
use crate::vrm_types::VrmDocument;

/// Category tag for avatar parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartCategory {
    Body,
    Hair,
    Face,
    Outfit,
    Accessory,
    Other,
}

/// A decomposed VRM part that can be independently stored/swapped.
#[derive(Debug, Clone)]
pub struct VrmPart {
    pub category: PartCategory,
    pub name: String,
    /// glTF mesh indices belonging to this part.
    pub mesh_indices: Vec<usize>,
    /// Material indices referenced by these meshes.
    pub material_indices: Vec<usize>,
    /// Texture indices referenced by these materials.
    pub texture_indices: Vec<usize>,
    /// Image indices referenced by these textures.
    pub image_indices: Vec<usize>,
    /// Node indices forming this part's sub-tree.
    pub node_indices: Vec<usize>,
    /// Spring bone chain indices (into VrmDocument.spring_bones).
    pub spring_bone_indices: Vec<usize>,
    /// Collider indices (into VrmDocument.spring_bone_colliders).
    pub collider_indices: Vec<usize>,
    /// Expression indices that bind to this part's meshes.
    pub expression_indices: Vec<usize>,
}

/// Classify a mesh node into a PartCategory by name heuristics.
///
/// VRoid Studio naming: "Body", "Hair", "Face", "FaceEyeline", "FaceMouth", etc.
/// Generic: material name keywords.
pub fn classify_mesh(mesh_name: &str, material_names: &[&str], node_name: &str) -> PartCategory {
    let combined = format!(
        "{} {} {}",
        mesh_name.to_lowercase(),
        node_name.to_lowercase(),
        material_names.join(" ").to_lowercase()
    );

    if combined.contains("hair") || combined.contains("bangs") {
        PartCategory::Hair
    } else if combined.contains("face")
        || combined.contains("eye")
        || combined.contains("mouth")
        || combined.contains("brow")
    {
        PartCategory::Face
    } else if combined.contains("body") || combined.contains("skin") {
        PartCategory::Body
    } else if combined.contains("cloth")
        || combined.contains("outfit")
        || combined.contains("shirt")
        || combined.contains("pants")
        || combined.contains("dress")
        || combined.contains("shoe")
        || combined.contains("tops")
        || combined.contains("bottoms")
    {
        PartCategory::Outfit
    } else if combined.contains("accessory")
        || combined.contains("hat")
        || combined.contains("glass")
        || combined.contains("ribbon")
        || combined.contains("earring")
        || combined.contains("necklace")
    {
        PartCategory::Accessory
    } else {
        PartCategory::Other
    }
}

/// Decompose a VrmDocument into swappable parts.
///
/// Strategy:
/// 1. Walk the node tree. Identify mesh-bearing nodes.
/// 2. Classify each by name heuristics.
/// 3. For each part, collect transitive closure of materials, textures, images.
/// 4. Identify spring bone chains referencing nodes in this part's subtree.
/// 5. Identify expressions that bind to this part's meshes.
pub fn decompose(doc: &VrmDocument) -> Result<Vec<VrmPart>, VrmError> {
    // Group mesh nodes by category
    let mut category_meshes: Vec<(PartCategory, String, Vec<usize>, Vec<usize>)> = Vec::new();

    for (node_idx, node) in doc.gltf.nodes.iter().enumerate() {
        let Some(mesh_idx) = node.mesh else {
            continue;
        };
        let mesh = doc.gltf.meshes.get(mesh_idx).ok_or_else(|| {
            VrmError::Part(format!(
                "node {node_idx} references missing mesh {mesh_idx}"
            ))
        })?;

        let mesh_name = mesh.name.as_deref().unwrap_or("");
        let node_name = node.name.as_deref().unwrap_or("");

        let mat_names: Vec<&str> = mesh
            .primitives
            .iter()
            .filter_map(|p| {
                let mi = p.material?;
                doc.gltf.materials.get(mi)?.name.as_deref()
            })
            .collect();

        let category = classify_mesh(mesh_name, &mat_names, node_name);

        // Try to merge with existing group of same category
        if let Some(group) = category_meshes
            .iter_mut()
            .find(|(c, _, _, _)| *c == category)
        {
            group.2.push(mesh_idx);
            group.3.push(node_idx);
        } else {
            let name = if !mesh_name.is_empty() {
                mesh_name.to_string()
            } else {
                format!("{category:?}")
            };
            category_meshes.push((category, name, vec![mesh_idx], vec![node_idx]));
        }
    }

    let mut parts = Vec::new();
    for (category, name, mesh_indices, node_indices) in category_meshes {
        // Collect material indices from meshes
        let mut material_indices: Vec<usize> = Vec::new();
        for &mi in &mesh_indices {
            if let Some(mesh) = doc.gltf.meshes.get(mi) {
                for prim in &mesh.primitives {
                    if let Some(mat_idx) = prim.material {
                        if !material_indices.contains(&mat_idx) {
                            material_indices.push(mat_idx);
                        }
                    }
                }
            }
        }

        // Collect texture indices from materials
        let mut texture_indices: Vec<usize> = Vec::new();
        for &mat_idx in &material_indices {
            if let Some(mat) = doc.gltf.materials.get(mat_idx) {
                collect_material_textures(mat, &mut texture_indices);
            }
        }

        // Collect image indices from textures
        let mut image_indices: Vec<usize> = Vec::new();
        for &tex_idx in &texture_indices {
            if let Some(tex) = doc.gltf.textures.get(tex_idx) {
                if let Some(src) = tex.source {
                    if !image_indices.contains(&src) {
                        image_indices.push(src);
                    }
                }
            }
        }

        // Expand node indices to include subtree
        let mut all_nodes: Vec<usize> = node_indices.clone();
        let mut i = 0;
        while i < all_nodes.len() {
            let ni = all_nodes[i];
            if let Some(node) = doc.gltf.nodes.get(ni) {
                for &child in &node.children {
                    if !all_nodes.contains(&child) {
                        all_nodes.push(child);
                    }
                }
            }
            i += 1;
        }

        // Find spring bone chains referencing nodes in this part's subtree
        let node_set: HashSet<usize> = all_nodes.iter().copied().collect();
        let spring_bone_indices: Vec<usize> = doc
            .spring_bones
            .iter()
            .enumerate()
            .filter(|(_, chain)| chain.joints.iter().any(|j| node_set.contains(&j.node)))
            .map(|(i, _)| i)
            .collect();

        // Colliders on this part's nodes
        let collider_indices: Vec<usize> = doc
            .spring_bone_colliders
            .iter()
            .enumerate()
            .filter(|(_, c)| node_set.contains(&c.node))
            .map(|(i, _)| i)
            .collect();

        // Expressions that bind to this part's meshes
        let mesh_set: HashSet<usize> = mesh_indices.iter().copied().collect();
        let expression_indices: Vec<usize> = doc
            .expressions
            .iter()
            .enumerate()
            .filter(|(_, expr)| {
                expr.morph_target_binds
                    .iter()
                    .any(|b| mesh_set.contains(&b.mesh_index))
                    || expr
                        .material_color_binds
                        .iter()
                        .any(|b| material_indices.contains(&b.material_index))
            })
            .map(|(i, _)| i)
            .collect();

        parts.push(VrmPart {
            category,
            name,
            mesh_indices,
            material_indices,
            texture_indices,
            image_indices,
            node_indices: all_nodes,
            spring_bone_indices,
            collider_indices,
            expression_indices,
        });
    }

    Ok(parts)
}

/// Collect texture indices referenced by a material.
fn collect_material_textures(mat: &crate::gltf_types::Material, textures: &mut Vec<usize>) {
    if let Some(pbr) = &mat.pbr_metallic_roughness {
        if let Some(tex) = &pbr.base_color_texture {
            if !textures.contains(&tex.index) {
                textures.push(tex.index);
            }
        }
        if let Some(tex) = &pbr.metallic_roughness_texture {
            if !textures.contains(&tex.index) {
                textures.push(tex.index);
            }
        }
    }
    // MToon textures are handled separately via VrmMtoonMaterial
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_hair() {
        assert_eq!(classify_mesh("Hair_001", &[], ""), PartCategory::Hair);
        assert_eq!(classify_mesh("Bangs", &[], "hair_node"), PartCategory::Hair);
    }

    #[test]
    fn classify_body() {
        assert_eq!(classify_mesh("Body", &[], ""), PartCategory::Body);
        assert_eq!(
            classify_mesh("mesh", &["skin_material"], ""),
            PartCategory::Body
        );
    }

    #[test]
    fn classify_outfit() {
        assert_eq!(classify_mesh("Clothing_Top", &[], ""), PartCategory::Outfit);
        assert_eq!(classify_mesh("", &["shirt_red"], ""), PartCategory::Outfit);
    }

    #[test]
    fn classify_face() {
        assert_eq!(classify_mesh("Face", &[], ""), PartCategory::Face);
        assert_eq!(classify_mesh("FaceEyeline", &[], ""), PartCategory::Face);
    }

    #[test]
    fn classify_other() {
        assert_eq!(classify_mesh("Unknown_Part", &[], ""), PartCategory::Other);
    }
}
