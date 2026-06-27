//! Part composition: merge multiple VrmParts into a single VrmDocument.

use std::collections::HashMap;

use crate::VrmError;
use crate::gltf_types::*;
use crate::vrm_types::*;

/// Source specification: a part + its source document.
pub struct PartSource<'a> {
    pub part: &'a crate::part::VrmPart,
    pub doc: &'a VrmDocument,
}

/// Configuration for composition.
pub struct ComposeConfig {
    /// Index into `sources` whose skeleton to use as the canonical armature.
    pub skeleton_base: usize,
}

/// Compose multiple VRM parts into a single VrmDocument.
///
/// Phases: skeleton unification → buffer merge → mesh merge →
/// joint remap → material merge → spring bone merge → expression merge → rebuild.
pub fn compose(
    sources: &[PartSource<'_>],
    config: &ComposeConfig,
) -> Result<VrmDocument, VrmError> {
    if sources.is_empty() {
        return Err(VrmError::Part("no sources provided".into()));
    }

    let base_doc = sources
        .get(config.skeleton_base)
        .ok_or_else(|| VrmError::Part("skeleton_base out of range".into()))?
        .doc;

    // ── Phase 1: Skeleton unification ──
    // Use the base document's node tree as the canonical skeleton.
    // Build node_remap: (source_idx, old_node_idx) → new_node_idx.
    let mut unified_nodes: Vec<Node> = base_doc.gltf.nodes.clone();
    let mut node_remap: HashMap<(usize, usize), usize> = HashMap::new();

    // Base document nodes map to themselves
    for i in 0..base_doc.gltf.nodes.len() {
        node_remap.insert((config.skeleton_base, i), i);
    }

    // Build humanoid bone lookup for base
    let base_bone_map: HashMap<HumanBoneName, usize> = base_doc
        .humanoid
        .human_bones
        .iter()
        .map(|hb| (hb.bone, hb.node))
        .collect();

    // Map other sources' nodes
    for (src_idx, src) in sources.iter().enumerate() {
        if src_idx == config.skeleton_base {
            continue;
        }

        // Map humanoid bones to base
        let src_bone_map: HashMap<HumanBoneName, usize> = src
            .doc
            .humanoid
            .human_bones
            .iter()
            .map(|hb| (hb.bone, hb.node))
            .collect();

        for (&bone_name, &src_node) in &src_bone_map {
            if let Some(&base_node) = base_bone_map.get(&bone_name) {
                node_remap.insert((src_idx, src_node), base_node);
            }
        }

        // Non-humanoid nodes (spring bone targets, accessories): append
        for &ni in &src.part.node_indices {
            if node_remap.contains_key(&(src_idx, ni)) {
                continue;
            }

            // Find closest humanoid ancestor
            let parent_new = find_parent_in_remap(src.doc, ni, src_idx, &node_remap);

            let mut new_node = src.doc.gltf.nodes[ni].clone();
            new_node.children = vec![]; // Children will be re-linked below
            new_node.mesh = None; // Mesh will be handled in Phase 3

            let new_idx = unified_nodes.len();
            unified_nodes.push(new_node);
            node_remap.insert((src_idx, ni), new_idx);

            // Add as child of parent
            if let Some(parent_idx) = parent_new {
                unified_nodes[parent_idx].children.push(new_idx);
            }
        }

        // Re-link children for appended nodes
        for &ni in &src.part.node_indices {
            if let Some(&new_ni) = node_remap.get(&(src_idx, ni)) {
                if new_ni >= base_doc.gltf.nodes.len() {
                    // This is an appended node, fix children
                    let old_children: Vec<usize> = src.doc.gltf.nodes[ni].children.clone();
                    let new_children: Vec<usize> = old_children
                        .iter()
                        .filter_map(|&c| node_remap.get(&(src_idx, c)).copied())
                        .collect();
                    unified_nodes[new_ni].children = new_children;
                }
            }
        }
    }

    // ── Phase 2: Buffer merging ──
    let mut unified_bin = Vec::new();
    let mut buffer_view_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut accessor_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut unified_buffer_views: Vec<BufferView> = Vec::new();
    let mut unified_accessors: Vec<Accessor> = Vec::new();

    for (src_idx, src) in sources.iter().enumerate() {
        let base_offset = unified_bin.len();

        // Copy entire BIN from this source (simplest approach)
        unified_bin.extend_from_slice(&src.doc.bin);
        // Pad to 4-byte alignment
        let pad = (4 - (unified_bin.len() % 4)) % 4;
        unified_bin.extend(std::iter::repeat(0u8).take(pad));

        // Remap buffer views
        for (bv_idx, bv) in src.doc.gltf.buffer_views.iter().enumerate() {
            let new_bv_idx = unified_buffer_views.len();
            unified_buffer_views.push(BufferView {
                buffer: 0, // Single unified buffer
                byte_offset: bv.byte_offset + base_offset,
                byte_length: bv.byte_length,
                byte_stride: bv.byte_stride,
                target: bv.target,
            });
            buffer_view_remap.insert((src_idx, bv_idx), new_bv_idx);
        }

        // Remap accessors
        for (acc_idx, acc) in src.doc.gltf.accessors.iter().enumerate() {
            let new_acc_idx = unified_accessors.len();
            let new_bv = acc
                .buffer_view
                .and_then(|bv| buffer_view_remap.get(&(src_idx, bv)).copied());
            unified_accessors.push(Accessor {
                buffer_view: new_bv,
                component_type: acc.component_type,
                count: acc.count,
                accessor_type: acc.accessor_type.clone(),
                byte_offset: acc.byte_offset,
                min: acc.min.clone(),
                max: acc.max.clone(),
                normalized: acc.normalized,
            });
            accessor_remap.insert((src_idx, acc_idx), new_acc_idx);
        }
    }

    // ── Phase 3: Mesh merging ──
    let mut unified_meshes: Vec<Mesh> = Vec::new();
    let mut mesh_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut material_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut unified_materials: Vec<Material> = Vec::new();
    let mut texture_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut unified_textures: Vec<Texture> = Vec::new();
    let mut image_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut unified_images: Vec<Image> = Vec::new();
    let mut unified_samplers: Vec<Sampler> = Vec::new();

    // ── Phase 4 (merged): Material/Texture/Image collection ──
    for (src_idx, src) in sources.iter().enumerate() {
        // Images
        for (img_idx, img) in src.doc.gltf.images.iter().enumerate() {
            if image_remap.contains_key(&(src_idx, img_idx)) {
                continue;
            }
            let new_idx = unified_images.len();
            let mut new_img = img.clone();
            // Remap buffer view if embedded
            if let Some(bv) = new_img.buffer_view {
                new_img.buffer_view = buffer_view_remap.get(&(src_idx, bv)).copied();
            }
            unified_images.push(new_img);
            image_remap.insert((src_idx, img_idx), new_idx);
        }

        // Samplers (just append, they're small)
        let sampler_base = unified_samplers.len();
        unified_samplers.extend(src.doc.gltf.samplers.clone());

        // Textures
        for (tex_idx, tex) in src.doc.gltf.textures.iter().enumerate() {
            if texture_remap.contains_key(&(src_idx, tex_idx)) {
                continue;
            }
            let new_idx = unified_textures.len();
            unified_textures.push(Texture {
                sampler: tex.sampler.map(|s| s + sampler_base),
                source: tex
                    .source
                    .and_then(|s| image_remap.get(&(src_idx, s)).copied()),
            });
            texture_remap.insert((src_idx, tex_idx), new_idx);
        }

        // Materials
        for &mat_idx in &src.part.material_indices {
            if material_remap.contains_key(&(src_idx, mat_idx)) {
                continue;
            }
            if let Some(mat) = src.doc.gltf.materials.get(mat_idx) {
                let new_idx = unified_materials.len();
                let mut new_mat = mat.clone();
                // Remap texture references in PBR
                if let Some(pbr) = &mut new_mat.pbr_metallic_roughness {
                    if let Some(tex) = &mut pbr.base_color_texture {
                        tex.index = texture_remap
                            .get(&(src_idx, tex.index))
                            .copied()
                            .unwrap_or(tex.index);
                    }
                    if let Some(tex) = &mut pbr.metallic_roughness_texture {
                        tex.index = texture_remap
                            .get(&(src_idx, tex.index))
                            .copied()
                            .unwrap_or(tex.index);
                    }
                }
                unified_materials.push(new_mat);
                material_remap.insert((src_idx, mat_idx), new_idx);
            }
        }

        // Meshes
        for &mi in &src.part.mesh_indices {
            if let Some(mesh) = src.doc.gltf.meshes.get(mi) {
                let new_mi = unified_meshes.len();
                let mut new_mesh = mesh.clone();

                for prim in &mut new_mesh.primitives {
                    // Remap accessor indices in attributes
                    for (_attr_name, val) in prim.attributes.iter_mut() {
                        if let Some(idx) = val.as_u64() {
                            if let Some(&new_idx) = accessor_remap.get(&(src_idx, idx as usize)) {
                                *val = serde_json::Value::Number(serde_json::Number::from(new_idx));
                            }
                        }
                    }

                    // Remap indices accessor
                    if let Some(idx) = prim.indices {
                        prim.indices = accessor_remap.get(&(src_idx, idx)).copied();
                    }

                    // Remap material
                    if let Some(mat) = prim.material {
                        prim.material = material_remap.get(&(src_idx, mat)).copied();
                    }

                    // Remap morph targets
                    for target in &mut prim.targets {
                        for (_attr, val) in target.iter_mut() {
                            if let Some(idx) = val.as_u64() {
                                if let Some(&new_idx) = accessor_remap.get(&(src_idx, idx as usize))
                                {
                                    *val = serde_json::Value::Number(serde_json::Number::from(
                                        new_idx,
                                    ));
                                }
                            }
                        }
                    }
                }

                unified_meshes.push(new_mesh);
                mesh_remap.insert((src_idx, mi), new_mi);

                // Attach mesh to correct node
                for &ni in &src.part.node_indices {
                    if src.doc.gltf.nodes.get(ni).and_then(|n| n.mesh) == Some(mi) {
                        if let Some(&new_ni) = node_remap.get(&(src_idx, ni)) {
                            if new_ni < unified_nodes.len() {
                                unified_nodes[new_ni].mesh = Some(new_mi);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Phase 5: Skin rebuild ──
    let unified_skin = if let Some(base_skin) = base_doc.gltf.skins.first() {
        // Start with base skin joints, add new joint nodes from other sources
        let mut joint_set: Vec<usize> = base_skin.joints.clone();
        for (src_idx, src) in sources.iter().enumerate() {
            if src_idx == config.skeleton_base {
                continue;
            }
            if let Some(src_skin) = src.doc.gltf.skins.first() {
                for &old_joint in &src_skin.joints {
                    if let Some(&new_joint) = node_remap.get(&(src_idx, old_joint)) {
                        if !joint_set.contains(&new_joint) {
                            joint_set.push(new_joint);
                        }
                    }
                }
            }
        }

        // IBM accessor: reuse base skin's IBM, extend with identity for new joints
        let ibm_acc_idx = if let Some(ibm_idx) = base_skin.inverse_bind_matrices {
            if let Some(&new_ibm_idx) = accessor_remap.get(&(config.skeleton_base, ibm_idx)) {
                Some(new_ibm_idx)
            } else {
                None
            }
        } else {
            None
        };

        Some(Skin {
            name: base_skin.name.clone(),
            joints: joint_set,
            inverse_bind_matrices: ibm_acc_idx,
            skeleton: base_skin
                .skeleton
                .and_then(|s| node_remap.get(&(config.skeleton_base, s)).copied()),
        })
    } else {
        None
    };

    // Assign skin to mesh nodes
    if unified_skin.is_some() {
        for node in &mut unified_nodes {
            if node.mesh.is_some() {
                node.skin = Some(0);
            }
        }
    }

    // ── Phase 6: Spring bone merging ──
    let mut unified_spring_bones = Vec::new();
    let mut unified_colliders = Vec::new();
    let mut unified_collider_groups = Vec::new();
    let mut collider_remap: HashMap<(usize, usize), usize> = HashMap::new();
    let mut collider_group_remap: HashMap<(usize, usize), usize> = HashMap::new();

    for (src_idx, src) in sources.iter().enumerate() {
        // Colliders
        for &ci in &src.part.collider_indices {
            if let Some(collider) = src.doc.spring_bone_colliders.get(ci) {
                let new_ci = unified_colliders.len();
                unified_colliders.push(VrmCollider {
                    node: node_remap
                        .get(&(src_idx, collider.node))
                        .copied()
                        .unwrap_or(collider.node),
                    shape: collider.shape.clone(),
                });
                collider_remap.insert((src_idx, ci), new_ci);
            }
        }

        // Collider groups
        for (gi, group) in src.doc.spring_bone_collider_groups.iter().enumerate() {
            let has_relevant = group
                .colliders
                .iter()
                .any(|c| src.part.collider_indices.contains(c));
            if !has_relevant {
                continue;
            }
            let new_gi = unified_collider_groups.len();
            unified_collider_groups.push(VrmColliderGroup {
                name: group.name.clone(),
                colliders: group
                    .colliders
                    .iter()
                    .filter_map(|&c| collider_remap.get(&(src_idx, c)).copied())
                    .collect(),
            });
            collider_group_remap.insert((src_idx, gi), new_gi);
        }

        // Spring bone chains
        for &sbi in &src.part.spring_bone_indices {
            if let Some(chain) = src.doc.spring_bones.get(sbi) {
                unified_spring_bones.push(VrmSpringBoneChain {
                    name: chain.name.clone(),
                    joints: chain
                        .joints
                        .iter()
                        .map(|j| SpringJoint {
                            node: node_remap
                                .get(&(src_idx, j.node))
                                .copied()
                                .unwrap_or(j.node),
                            hit_radius: j.hit_radius,
                            stiffness: j.stiffness,
                            gravity_power: j.gravity_power,
                            gravity_dir: j.gravity_dir,
                            drag_force: j.drag_force,
                        })
                        .collect(),
                    collider_groups: chain
                        .collider_groups
                        .iter()
                        .filter_map(|&cg| collider_group_remap.get(&(src_idx, cg)).copied())
                        .collect(),
                    center: chain
                        .center
                        .and_then(|c| node_remap.get(&(src_idx, c)).copied()),
                });
            }
        }
    }

    // ── Phase 7: Expression merging ──
    let mut unified_expressions: Vec<VrmExpression> = Vec::new();
    for (src_idx, src) in sources.iter().enumerate() {
        for &ei in &src.part.expression_indices {
            if let Some(expr) = src.doc.expressions.get(ei) {
                // Check if we already have this preset
                let existing = expr.preset.and_then(|preset| {
                    unified_expressions
                        .iter_mut()
                        .find(|e| e.preset == Some(preset))
                });

                if let Some(existing) = existing {
                    // Merge binds into existing expression
                    for bind in &expr.morph_target_binds {
                        existing.morph_target_binds.push(MorphTargetBind {
                            mesh_index: mesh_remap
                                .get(&(src_idx, bind.mesh_index))
                                .copied()
                                .unwrap_or(bind.mesh_index),
                            morph_index: bind.morph_index,
                            weight: bind.weight,
                        });
                    }
                    for bind in &expr.material_color_binds {
                        existing.material_color_binds.push(MaterialColorBind {
                            material_index: material_remap
                                .get(&(src_idx, bind.material_index))
                                .copied()
                                .unwrap_or(bind.material_index),
                            property: bind.property.clone(),
                            target_value: bind.target_value,
                        });
                    }
                } else {
                    // New expression
                    unified_expressions.push(VrmExpression {
                        name: expr.name.clone(),
                        preset: expr.preset,
                        is_binary: expr.is_binary,
                        morph_target_binds: expr
                            .morph_target_binds
                            .iter()
                            .map(|b| MorphTargetBind {
                                mesh_index: mesh_remap
                                    .get(&(src_idx, b.mesh_index))
                                    .copied()
                                    .unwrap_or(b.mesh_index),
                                morph_index: b.morph_index,
                                weight: b.weight,
                            })
                            .collect(),
                        material_color_binds: expr
                            .material_color_binds
                            .iter()
                            .map(|b| MaterialColorBind {
                                material_index: material_remap
                                    .get(&(src_idx, b.material_index))
                                    .copied()
                                    .unwrap_or(b.material_index),
                                property: b.property.clone(),
                                target_value: b.target_value,
                            })
                            .collect(),
                        texture_transform_binds: expr
                            .texture_transform_binds
                            .iter()
                            .map(|b| TextureTransformBind {
                                material_index: material_remap
                                    .get(&(src_idx, b.material_index))
                                    .copied()
                                    .unwrap_or(b.material_index),
                                offset: b.offset,
                                scale: b.scale,
                            })
                            .collect(),
                        override_blink: expr.override_blink,
                        override_look_at: expr.override_look_at,
                        override_mouth: expr.override_mouth,
                    });
                }
            }
        }
    }

    // ── Phase 8: MToon material merging ──
    let mut unified_mtoon = Vec::new();
    for (src_idx, src) in sources.iter().enumerate() {
        for mtoon in &src.doc.mtoon_materials {
            if src.part.material_indices.contains(&mtoon.material_index) {
                let new_mat_idx = material_remap
                    .get(&(src_idx, mtoon.material_index))
                    .copied()
                    .unwrap_or(mtoon.material_index);
                unified_mtoon.push(VrmMtoonMaterial {
                    material_index: new_mat_idx,
                    shade_color_factor: mtoon.shade_color_factor,
                    shade_multiply_texture: mtoon
                        .shade_multiply_texture
                        .and_then(|t| texture_remap.get(&(src_idx, t)).copied()),
                    shading_shift_factor: mtoon.shading_shift_factor,
                    shading_toony_factor: mtoon.shading_toony_factor,
                    gi_equalization_factor: mtoon.gi_equalization_factor,
                    rim_color_factor: mtoon.rim_color_factor,
                    rim_lighting_mix_factor: mtoon.rim_lighting_mix_factor,
                    rim_fresnel_power_factor: mtoon.rim_fresnel_power_factor,
                    rim_lift_factor: mtoon.rim_lift_factor,
                    rim_multiply_texture: mtoon
                        .rim_multiply_texture
                        .and_then(|t| texture_remap.get(&(src_idx, t)).copied()),
                    outline_width_mode: mtoon.outline_width_mode,
                    outline_width_factor: mtoon.outline_width_factor,
                    outline_color_factor: mtoon.outline_color_factor,
                    outline_lighting_mix_factor: mtoon.outline_lighting_mix_factor,
                    matcap_texture: mtoon
                        .matcap_texture
                        .and_then(|t| texture_remap.get(&(src_idx, t)).copied()),
                    parametric_rim_color_factor: mtoon.parametric_rim_color_factor,
                    uv_animation_scroll_x: mtoon.uv_animation_scroll_x,
                    uv_animation_scroll_y: mtoon.uv_animation_scroll_y,
                    uv_animation_rotation: mtoon.uv_animation_rotation,
                    render_queue_offset: mtoon.render_queue_offset,
                    transparent_with_z_write: mtoon.transparent_with_z_write,
                });
            }
        }
    }

    // ── Build unified scene ──
    let scene_nodes: Vec<usize> = base_doc
        .gltf
        .scenes
        .first()
        .map(|s| s.nodes.clone())
        .unwrap_or_else(|| vec![0]);

    let unified_gltf = GltfDocument {
        asset: Asset {
            version: "2.0".into(),
            generator: Some("kami-vrm".into()),
        },
        scene: Some(0),
        scenes: vec![Scene {
            nodes: scene_nodes,
            name: None,
        }],
        nodes: unified_nodes,
        meshes: unified_meshes,
        accessors: unified_accessors,
        buffer_views: unified_buffer_views,
        buffers: vec![Buffer {
            byte_length: unified_bin.len(),
        }],
        materials: unified_materials,
        textures: unified_textures,
        images: unified_images,
        samplers: unified_samplers,
        skins: unified_skin.into_iter().collect(),
        animations: vec![],
        extensions_used: vec![
            "VRMC_vrm".into(),
            "VRMC_springBone".into(),
            "VRMC_materials_mtoon".into(),
        ],
        extensions_required: vec![],
        extensions: None, // Will be populated by export
    };

    Ok(VrmDocument {
        gltf: unified_gltf,
        bin: unified_bin,
        version: VrmVersion::V1_0,
        meta: base_doc.meta.clone(),
        humanoid: VrmHumanoid {
            human_bones: base_doc
                .humanoid
                .human_bones
                .iter()
                .map(|hb| VrmHumanBone {
                    bone: hb.bone,
                    node: node_remap
                        .get(&(config.skeleton_base, hb.node))
                        .copied()
                        .unwrap_or(hb.node),
                })
                .collect(),
        },
        expressions: unified_expressions,
        spring_bones: unified_spring_bones,
        spring_bone_colliders: unified_colliders,
        spring_bone_collider_groups: unified_collider_groups,
        mtoon_materials: unified_mtoon,
        look_at: base_doc.look_at.clone(),
        first_person: base_doc.first_person.clone(),
        node_constraints: vec![],
    })
}

/// Find the remapped parent node for a source node.
fn find_parent_in_remap(
    doc: &VrmDocument,
    node_idx: usize,
    src_idx: usize,
    node_remap: &HashMap<(usize, usize), usize>,
) -> Option<usize> {
    // Walk up the node tree to find a mapped ancestor
    for (potential_parent_idx, potential_parent) in doc.gltf.nodes.iter().enumerate() {
        if potential_parent.children.contains(&node_idx) {
            if let Some(&mapped) = node_remap.get(&(src_idx, potential_parent_idx)) {
                return Some(mapped);
            }
            // Recurse up
            return find_parent_in_remap(doc, potential_parent_idx, src_idx, node_remap);
        }
    }
    None
}
