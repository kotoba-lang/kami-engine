//! glTF/GLB loader — behind `gltf-loader` feature.
//!
//! Parses GLB binary → extracts all primitives + textures → LoadedMesh + MaterialUniform.
//! Supports VRM models (multiple primitives per mesh, MToon materials treated as PBR).

#[cfg(feature = "gltf-loader")]
use crate::camera::MaterialUniform;
#[cfg(feature = "gltf-loader")]
use crate::mesh::{LoadedMesh, interleave};

#[cfg(feature = "gltf-loader")]
use thiserror::Error;

#[cfg(feature = "gltf-loader")]
#[derive(Debug, Error)]
pub enum GltfError {
    #[error("gltf parse error: {0}")]
    Parse(#[from] gltf::Error),
    #[error("missing accessor data for {0}")]
    MissingAccessor(&'static str),
    #[error("unsupported index type")]
    UnsupportedIndexType,
}

/// A node in the loaded glTF scene graph.
#[cfg(feature = "gltf-loader")]
pub struct GltfNode {
    pub mesh_index: usize,
    pub material_index: usize,
    pub transform: glam::Mat4,
    /// Index into `GltfScene::skins` if this node's mesh is skinned.
    pub skin_index: Option<usize>,
    /// Human-readable label for debugging / part selection.
    /// Format: `"{mesh_name}:{material_name}"`.
    pub label: String,
}

/// Full glTF node entry — includes non-mesh joint nodes. Used to reconstruct
/// the skeleton hierarchy for GPU skinning. Indexed by glTF node index.
#[cfg(feature = "gltf-loader")]
pub struct GltfNodeInfo {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// A glTF skin: ordered list of joint node indices + their inverse bind matrices.
#[cfg(feature = "gltf-loader")]
pub struct GltfSkin {
    /// glTF node indices referencing `GltfScene::node_hierarchy`.
    pub joint_node_indices: Vec<usize>,
    /// 4x4 column-major inverse bind matrices, one per joint.
    pub inverse_bind_matrices: Vec<[[f32; 4]; 4]>,
}

/// Loaded texture (RGBA8 pixels).
#[cfg(feature = "gltf-loader")]
pub struct GltfTexture {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Morph target data: per-vertex position deltas.
#[cfg(feature = "gltf-loader")]
pub struct MorphTarget {
    /// Position deltas (x,y,z per vertex, flat array).
    pub position_deltas: Vec<f32>,
}

/// Complete loaded glTF scene.
#[cfg(feature = "gltf-loader")]
pub struct GltfScene {
    pub meshes: Vec<LoadedMesh>,
    pub materials: Vec<MaterialUniform>,
    pub nodes: Vec<GltfNode>,
    pub textures: Vec<GltfTexture>,
    /// material_index → texture_index for albedo map (if any).
    pub material_texture_map: Vec<Option<usize>>,
    /// Per-mesh morph targets. morph_targets[mesh_index] = Vec of MorphTarget.
    pub morph_targets: Vec<Vec<MorphTarget>>,
    /// Morph target names (from mesh extras.targetNames).
    pub morph_target_names: Vec<String>,
    /// All glTF nodes with parent links + local TRS, indexed by glTF node index.
    pub node_hierarchy: Vec<GltfNodeInfo>,
    /// All skins in the document.
    pub skins: Vec<GltfSkin>,
    /// Per-mesh flat joint indices (u16 x 4 per vertex). Empty if mesh is not skinned.
    pub skin_joints: Vec<Vec<u16>>,
    /// Per-mesh flat joint weights (f32 x 4 per vertex). Empty if mesh is not skinned.
    pub skin_weights: Vec<Vec<f32>>,
}

/// Parse a GLB binary into meshes + materials + textures.
#[cfg(feature = "gltf-loader")]
pub fn load_glb(data: &[u8]) -> Result<GltfScene, GltfError> {
    let (document, buffers, images) = gltf::import_slice(data)?;

    // Extract textures from embedded images — convert to RGBA8 if needed
    let mut textures = Vec::new();
    for img in &images {
        let expected_rgba = (img.width * img.height * 4) as usize;
        let expected_rgb = (img.width * img.height * 3) as usize;
        let pixels = if img.pixels.len() == expected_rgba {
            img.pixels.clone()
        } else if img.pixels.len() == expected_rgb {
            // Convert RGB → RGBA
            let mut rgba = Vec::with_capacity(expected_rgba);
            for chunk in img.pixels.chunks(3) {
                rgba.push(chunk[0]);
                rgba.push(chunk[1]);
                rgba.push(chunk[2]);
                rgba.push(255);
            }
            rgba
        } else {
            // Unknown format — skip or pad
            log::warn!("Texture {}x{}: unexpected data size {} (expected {} RGBA or {} RGB), skipping",
                img.width, img.height, img.pixels.len(), expected_rgba, expected_rgb);
            vec![255u8; expected_rgba] // White fallback
        };
        textures.push(GltfTexture {
            pixels,
            width: img.width,
            height: img.height,
        });
    }

    // Extract materials
    let mut materials = Vec::new();
    let mut material_map = std::collections::HashMap::new();
    let mut material_texture_map = Vec::new();

    for mat in document.materials() {
        let pbr = mat.pbr_metallic_roughness();
        let base_color = pbr.base_color_factor();
        let metallic = pbr.metallic_factor();
        let roughness = pbr.roughness_factor();
        let idx = mat.index().unwrap_or(0);
        material_map.insert(idx, materials.len());

        // Check for albedo texture
        let albedo_tex = pbr.base_color_texture()
            .and_then(|info| {
                let tex = info.texture();
                let src = tex.source();
                Some(src.index())
            });

        let has_albedo = if albedo_tex.is_some() { 1u32 } else { 0u32 };

        // Detect MToon: check if VRM extensions exist in the document.
        // If document has VRMC_vrm extension, all materials are treated as MToon.
        let has_vrm = document.extensions_used().any(|e| e == "VRMC_vrm" || e == "VRMC_materials_mtoon");
        let is_mtoon = has_vrm;

        // Detect material type from name for appropriate PBR settings
        let name = mat.name().unwrap_or("").to_uppercase();
        let (sss_model, subsurface, clearcoat, aniso) = if is_mtoon {
            (99u32, 0.0f32, 0.0f32, 0.0f32) // MToon: magic value 99 selects MToon pipeline
        } else if name.contains("SKIN") || name.contains("BODY") || name.contains("FACE") {
            (1u32, 0.5f32, 0.0f32, 0.0f32) // Skin: SSS
        } else if name.contains("EYE") || name.contains("IRIS") {
            (0, 0.0, 0.6, 0.0) // Eye: clearcoat
        } else if name.contains("HAIR") {
            (0, 0.0, 0.0, 0.5) // Hair: anisotropic
        } else {
            (0, 0.0, 0.0, 0.0) // Default
        };

        // MToon shade parameters: shade_color = slightly darker base, shade_shift = -0.1 (more lit)
        let (sub_color, sub_radius, hair_scat) = if is_mtoon {
            // shade_color (RGB) = base_color * 0.85 (slightly darkened), shade_shift (A) = -0.1
            let sc = [base_color[0] * 0.85, base_color[1] * 0.8, base_color[2] * 0.85, -0.1];
            // shade_toony=0.5 (r0), rim_intensity=0.3 (r1), rim_fresnel=3.0 (r2)
            let sr = [0.5_f32, 0.3, 3.0];
            // rim_color (RGB) = white, rim_lift (A) = 0.0
            let hs = [1.0_f32, 1.0, 1.0, 0.0];
            (sc, sr, hs)
        } else {
            ([0.9, 0.5, 0.35, subsurface], [0.012_f32, 0.036, 0.12], [0.8_f32, 0.6, 0.4, 0.3])
        };

        materials.push(MaterialUniform {
            albedo: base_color,
            metallic,
            roughness,
            has_albedo_tex: has_albedo,
            has_normal_tex: 0,
            subsurface_color: sub_color,
            subsurface_radius: sub_radius,
            sss_model,
            aniso_tangent: [1.0, 0.0, 0.0],
            aniso_strength: aniso,
            hair_scatter: hair_scat,
            clearcoat,
            clearcoat_roughness: 0.1,
            emission: [0.0; 3],
            tex_flags: 0,
            parallax_depth: 0.0,
            _pad: 0.0,
        });

        material_texture_map.push(albedo_tex);
    }

    // Fallback material if none defined
    if materials.is_empty() {
        materials.push(MaterialUniform::default());
        material_texture_map.push(None);
    }

    // Build node hierarchy (parent links + local TRS) for skeleton reconstruction.
    // glTF node indices are preserved (index into document.nodes()).
    let node_count = document.nodes().count();
    let mut node_hierarchy: Vec<GltfNodeInfo> = (0..node_count)
        .map(|i| {
            let node = document.nodes().nth(i).unwrap();
            let (t, r, s) = node.transform().decomposed();
            GltfNodeInfo {
                name: node.name().unwrap_or("").to_string(),
                parent: None,
                translation: t,
                rotation: r,
                scale: s,
            }
        })
        .collect();
    // Second pass: fill parent from children links.
    for parent_node in document.nodes() {
        let p = parent_node.index();
        for child in parent_node.children() {
            let c = child.index();
            if c < node_hierarchy.len() {
                node_hierarchy[c].parent = Some(p);
            }
        }
    }

    // Extract skins (joint node indices + inverse bind matrices).
    let mut skins: Vec<GltfSkin> = Vec::new();
    for skin in document.skins() {
        let joint_node_indices: Vec<usize> = skin.joints().map(|j| j.index()).collect();
        let reader = skin.reader(|buffer| Some(&buffers[buffer.index()]));
        let inverse_bind_matrices: Vec<[[f32; 4]; 4]> = reader
            .read_inverse_bind_matrices()
            .map(|iter| iter.collect())
            .unwrap_or_else(|| vec![[[0.0; 4]; 4]; joint_node_indices.len()]);
        skins.push(GltfSkin {
            joint_node_indices,
            inverse_bind_matrices,
        });
    }
    log::info!("glTF: {} skins, {} nodes in hierarchy", skins.len(), node_hierarchy.len());

    // Extract ALL meshes (all primitives) + morph targets
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut all_morph_targets: Vec<Vec<MorphTarget>> = Vec::new();
    let mut morph_target_names: Vec<String> = Vec::new();
    let mut skin_joints_per_mesh: Vec<Vec<u16>> = Vec::new();
    let mut skin_weights_per_mesh: Vec<Vec<f32>> = Vec::new();

    for node in document.nodes() {
        if let Some(mesh) = node.mesh() {
            let transform = glam::Mat4::from_cols_array_2d(&node.transform().matrix());
            let skin_index = node.skin().map(|s| s.index());

            // Extract morph target names from mesh extras (VRoid stores targetNames there)
            // gltf crate's extras() is complex, parse the raw GLB JSON instead
            if morph_target_names.is_empty() {
                // Fall back: generate names from index
                let first_prim = mesh.primitives().next();
                if let Some(prim) = first_prim {
                    let target_count = prim.morph_targets().count();
                    if target_count > 0 && morph_target_names.is_empty() {
                        morph_target_names = (0..target_count).map(|i| format!("morph_{}", i)).collect();
                    }
                }
            }

            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

                let positions: Vec<f32> = match reader.read_positions() {
                    Some(iter) => iter.flat_map(|p| p.into_iter()).collect(),
                    None => continue,
                };

                let normals: Vec<f32> = reader
                    .read_normals()
                    .map(|iter| iter.flat_map(|n| n.into_iter()).collect())
                    .unwrap_or_else(|| generate_normals_from_tris(&positions, reader.read_indices().map(|i| { let v: Vec<u32> = i.into_u32().collect(); v }).as_deref()));

                let vertex_count = positions.len() / 3;
                let uvs: Vec<f32> = reader
                    .read_tex_coords(0)
                    .map(|iter| iter.into_f32().flat_map(|uv| uv.into_iter()).collect())
                    .unwrap_or_else(|| vec![0.0; vertex_count * 2]);

                let indices: Vec<u32> = match reader.read_indices() {
                    Some(iter) => iter.into_u32().collect(),
                    None => (0..vertex_count as u32).collect(),
                };

                // Parse morph targets (position deltas)
                // read_morph_targets returns iterator of (positions, normals, tangents) tuples
                let mut mesh_morphs: Vec<MorphTarget> = Vec::new();
                // Use a fresh reader for morph targets
                let morph_reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                for (pos_opt, _norm_opt, _tan_opt) in morph_reader.read_morph_targets() {
                    let deltas: Vec<f32> = match pos_opt {
                        Some(pos_iter) => {
                            let v: Vec<f32> = pos_iter.flat_map(|p: [f32; 3]| [p[0], p[1], p[2]]).collect();
                            v
                        }
                        None => Vec::new(),
                    };
                    if !deltas.is_empty() {
                        let max_d = deltas.iter().cloned().fold(0.0f32, f32::max);
                        let min_d = deltas.iter().cloned().fold(0.0f32, f32::min);
                        let non_zero = deltas.iter().filter(|d| d.abs() > 0.0001).count();
                        if mesh_morphs.len() < 6 && non_zero > 0 {
                            log::info!("morph[{}]: {} floats, {} non-zero, range [{:.6}, {:.6}]",
                                mesh_morphs.len(), deltas.len(), non_zero, min_d, max_d);
                        }
                        mesh_morphs.push(MorphTarget { position_deltas: deltas });
                    }
                }

                // Extract joints + weights (VRM is always skinned).
                let joints: Vec<u16> = reader
                    .read_joints(0)
                    .map(|j| {
                        j.into_u16()
                            .flat_map(|q: [u16; 4]| [q[0], q[1], q[2], q[3]])
                            .collect()
                    })
                    .unwrap_or_default();
                let weights: Vec<f32> = reader
                    .read_weights(0)
                    .map(|w| {
                        w.into_f32()
                            .flat_map(|q: [f32; 4]| [q[0], q[1], q[2], q[3]])
                            .collect()
                    })
                    .unwrap_or_default();

                let vertices = interleave(&positions, &normals, &uvs);
                let index_count = indices.len() as u32;
                let loaded = LoadedMesh {
                    vertices,
                    indices,
                    vertex_count: vertex_count as u32,
                    index_count,
                };

                let mesh_index = meshes.len();
                meshes.push(loaded);
                all_morph_targets.push(mesh_morphs);
                skin_joints_per_mesh.push(joints);
                skin_weights_per_mesh.push(weights);

                let material_index = primitive
                    .material()
                    .index()
                    .and_then(|idx| material_map.get(&idx).copied())
                    .unwrap_or(0);

                let mesh_name = mesh.name().unwrap_or("mesh").to_string();
                let material_name = primitive.material().name().unwrap_or("mat").to_string();
                let label = format!("{mesh_name}:{material_name}");

                nodes.push(GltfNode {
                    mesh_index,
                    material_index,
                    transform,
                    skin_index,
                    label,
                });
            }
        }
    }

    log::info!("glTF: {} morph target names, meshes with morphs: {}",
        morph_target_names.len(),
        all_morph_targets.iter().filter(|m| !m.is_empty()).count());

    Ok(GltfScene {
        meshes,
        materials,
        nodes,
        textures,
        material_texture_map,
        morph_targets: all_morph_targets,
        morph_target_names,
        node_hierarchy,
        skins,
        skin_joints: skin_joints_per_mesh,
        skin_weights: skin_weights_per_mesh,
    })
}

/// Generate per-triangle normals from positions + indices.
#[cfg(feature = "gltf-loader")]
fn generate_normals_from_tris(positions: &[f32], indices: Option<&[u32]>) -> Vec<f32> {
    let vertex_count = positions.len() / 3;
    let mut normals = vec![0.0f32; vertex_count * 3];

    let get_pos = |i: usize| -> glam::Vec3 {
        glam::Vec3::new(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2])
    };

    if let Some(idx) = indices {
        for tri in idx.chunks_exact(3) {
            let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let e1 = get_pos(i1) - get_pos(i0);
            let e2 = get_pos(i2) - get_pos(i0);
            let fn_ = e1.cross(e2);
            for i in [i0, i1, i2] {
                normals[i * 3] += fn_.x;
                normals[i * 3 + 1] += fn_.y;
                normals[i * 3 + 2] += fn_.z;
            }
        }
    }

    // Normalize
    for i in 0..vertex_count {
        let n = glam::Vec3::new(normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]);
        let n = n.normalize_or_zero();
        if n == glam::Vec3::ZERO {
            normals[i * 3 + 1] = 1.0; // Default Y-up
        } else {
            normals[i * 3] = n.x;
            normals[i * 3 + 1] = n.y;
            normals[i * 3 + 2] = n.z;
        }
    }

    normals
}

#[cfg(all(test, feature = "gltf-loader"))]
mod tests {
    use super::*;

    #[test]
    fn material_default_values() {
        let mat = MaterialUniform::default();
        assert_eq!(mat.albedo, [0.8, 0.8, 0.8, 1.0]);
        assert_eq!(mat.metallic, 0.0);
        assert_eq!(mat.roughness, 0.5);
    }
}
