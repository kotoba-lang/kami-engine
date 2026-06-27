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

/// glTF extensions this loader understands. Files that only require these are
/// fully supported; any other entry in `extensionsRequired` is logged as a
/// best-effort load (the extension's effect is ignored).
#[cfg(feature = "gltf-loader")]
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "KHR_mesh_quantization",   // dequantized in this loader
    "EXT_meshopt_compression", // decoded by crate::meshopt before import
    "KHR_texture_basisu",      // KTX2/UASTC transcoded in this loader
    "VRMC_vrm",                // VRM 1.0 (handled here + kami-vrm)
    "VRMC_materials_mtoon",    // MToon → MToon pipeline
    "VRMC_springBone",         // kami-vrm
    "VRMC_node_constraint",    // kami-vrm
];

/// Read a vec`comps` vertex attribute from a possibly-quantized accessor,
/// returning f32 values. Handles `KHR_mesh_quantization` component types
/// (BYTE/UBYTE/SHORT/USHORT) with normalized-integer scaling. Returns `None`
/// for sparse-only accessors (no buffer view) or out-of-range slices.
#[cfg(feature = "gltf-loader")]
fn read_quantized_attr(
    accessor: &gltf::Accessor,
    buffers: &[gltf::buffer::Data],
    comps: usize,
) -> Option<Vec<f32>> {
    use gltf::accessor::DataType;
    let view = accessor.view()?;
    let buf = &buffers[view.buffer().index()];
    let comp_size = accessor.data_type().size();
    let elem_size = comp_size * comps;
    let stride = view.stride().unwrap_or(elem_size);
    let base = view.offset() + accessor.offset();
    let count = accessor.count();
    let dt = accessor.data_type();
    let norm = accessor.normalized();

    let mut out = Vec::with_capacity(count * comps);
    for i in 0..count {
        let elem = base + i * stride;
        for c in 0..comps {
            let o = elem + c * comp_size;
            let v = match dt {
                DataType::F32 => f32::from_le_bytes(buf.get(o..o + 4)?.try_into().ok()?),
                DataType::U32 => u32::from_le_bytes(buf.get(o..o + 4)?.try_into().ok()?) as f32,
                DataType::I16 => {
                    let x = i16::from_le_bytes(buf.get(o..o + 2)?.try_into().ok()?);
                    if norm {
                        (x as f32 / 32767.0).max(-1.0)
                    } else {
                        x as f32
                    }
                }
                DataType::U16 => {
                    let x = u16::from_le_bytes(buf.get(o..o + 2)?.try_into().ok()?);
                    if norm { x as f32 / 65535.0 } else { x as f32 }
                }
                DataType::I8 => {
                    let x = *buf.get(o)? as i8;
                    if norm {
                        (x as f32 / 127.0).max(-1.0)
                    } else {
                        x as f32
                    }
                }
                DataType::U8 => {
                    let x = *buf.get(o)?;
                    if norm { x as f32 / 255.0 } else { x as f32 }
                }
            };
            out.push(v);
        }
    }
    Some(out)
}

/// Decode one glTF image to RGBA8. Handles KTX2 (`KHR_texture_basisu`, UASTC)
/// via the in-crate transcoder and PNG/JPEG via the `image` crate. On any
/// failure (unsupported format, ETC1S, decode error) returns a 1×1 white
/// placeholder so the rest of the scene still loads.
#[cfg(feature = "gltf-loader")]
fn load_gltf_image(img: &gltf::Image, buffers: &[gltf::buffer::Data]) -> GltfTexture {
    use gltf::image::Source;

    let placeholder = || GltfTexture {
        pixels: vec![255, 255, 255, 255],
        width: 1,
        height: 1,
    };

    // Resolve the raw encoded bytes + optional mime type.
    let (bytes, mime): (std::borrow::Cow<[u8]>, Option<String>) = match img.source() {
        Source::View { view, mime_type } => {
            let buf = &buffers[view.buffer().index()];
            let start = view.offset();
            let end = start + view.length();
            match buf.0.get(start..end) {
                Some(s) => (std::borrow::Cow::Borrowed(s), Some(mime_type.to_string())),
                None => return placeholder(),
            }
        }
        Source::Uri { uri, mime_type } => {
            if let Some(comma) = uri.find(',') {
                if uri[..comma].starts_with("data:") {
                    match base64_decode(&uri[comma + 1..]) {
                        Some(b) => (std::borrow::Cow::Owned(b), mime_type.map(|m| m.to_string())),
                        None => return placeholder(),
                    }
                } else {
                    log::warn!("glTF external image URI unsupported: {uri}");
                    return placeholder();
                }
            } else {
                return placeholder();
            }
        }
    };

    let is_ktx2 = crate::basisu::is_ktx2(&bytes) || mime.as_deref() == Some("image/ktx2");
    if is_ktx2 {
        return match crate::basisu::decode_ktx2(&bytes) {
            Ok(d) => GltfTexture {
                pixels: d.rgba,
                width: d.width,
                height: d.height,
            },
            Err(e) => {
                log::warn!("KHR_texture_basisu: {e} — using placeholder");
                placeholder()
            }
        };
    }

    match image::load_from_memory(&bytes) {
        Ok(dyn_img) => {
            let rgba = dyn_img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            GltfTexture {
                pixels: rgba.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(e) => {
            log::warn!("image decode failed: {e} — using placeholder");
            placeholder()
        }
    }
}

/// Build `texture_index → image_index`, honouring `KHR_texture_basisu.source`
/// (which the `gltf` crate does not parse) and falling back to the standard
/// `source`. Parses the GLB JSON chunk directly. Returns an empty vec if the
/// input isn't a parseable GLB, in which case albedo textures resolve to none.
#[cfg(feature = "gltf-loader")]
fn build_texture_image_map(data: &[u8], document: &gltf::Document) -> Vec<Option<usize>> {
    let json = match glb_json_value(data) {
        Some(j) => j,
        None => return vec![None; document.textures().count()],
    };
    let textures = json.get("textures").and_then(|v| v.as_array());
    match textures {
        Some(arr) => arr
            .iter()
            .map(|t| {
                t.pointer("/extensions/KHR_texture_basisu/source")
                    .or_else(|| t.get("source"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
            })
            .collect(),
        None => vec![None; document.textures().count()],
    }
}

/// Extract and parse the JSON chunk of a GLB into a `serde_json::Value`.
#[cfg(feature = "gltf-loader")]
fn glb_json_value(data: &[u8]) -> Option<serde_json::Value> {
    if data.len() < 20 || u32::from_le_bytes(data[0..4].try_into().ok()?) != 0x4654_6C67 {
        return None;
    }
    let json_len = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;
    // chunk type at [16..20] should be "JSON"; payload follows at 20.
    let json_bytes = data.get(20..20 + json_len)?;
    serde_json::from_slice(json_bytes).ok()
}

/// Minimal standard-alphabet base64 decode (ignores padding/whitespace).
#[cfg(feature = "gltf-loader")]
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
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
    // EXT_meshopt_compression: the `gltf` crate cannot decode meshopt buffer
    // views (and mishandles the fallback-buffer layout), so transparently
    // pre-decode into a clean, extension-free GLB first. Returns None when the
    // input doesn't use the extension — then we import the original bytes.
    let decoded = crate::meshopt::decode_meshopt_glb(data)
        .map_err(|e| {
            log::warn!("EXT_meshopt_compression decode failed: {e}");
            e
        })
        .ok()
        .flatten();
    let data: &[u8] = decoded.as_deref().unwrap_or(data);

    // Use the non-validating import: the crate's validator hard-rejects any
    // extension listed in `extensionsRequired` that it doesn't itself
    // implement (e.g. KHR_mesh_quantization, KHR_texture_basisu) and also
    // rejects accessors missing min/max. We handle those extensions ourselves,
    // so skip validation and import buffers/images manually.
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice_without_validation(data)?;
    let buffers = gltf::import_buffers(&document, None, blob)?;

    // Surface any required extension we don't understand (best-effort load).
    for ext in document.extensions_required() {
        if !SUPPORTED_EXTENSIONS.contains(&ext) {
            log::warn!("glTF requires unsupported extension '{ext}' — loading best-effort");
        }
    }

    // Decode images ourselves so KHR_texture_basisu (KTX2/UASTC) images work —
    // the `gltf`/`image` crates can't decode KTX2. PNG/JPEG go through `image`.
    let textures: Vec<GltfTexture> = document
        .images()
        .map(|img| load_gltf_image(&img, &buffers))
        .collect();

    // texture_index → image_index, honouring KHR_texture_basisu.source (which
    // the gltf crate doesn't parse). Falls back to the standard `source`.
    let texture_image_map = build_texture_image_map(data, &document);

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

        // Check for albedo texture. Resolve via the basisu-aware map so
        // KHR_texture_basisu textures (whose standard `source` may be empty)
        // map to the correct KTX2 image rather than panicking.
        let albedo_tex = pbr.base_color_texture().and_then(|info| {
            let ti = info.texture().index();
            texture_image_map.get(ti).copied().flatten()
        });

        let has_albedo = if albedo_tex.is_some() { 1u32 } else { 0u32 };

        // Detect MToon: check if VRM extensions exist in the document.
        // If document has VRMC_vrm extension, all materials are treated as MToon.
        let has_vrm = document
            .extensions_used()
            .any(|e| e == "VRMC_vrm" || e == "VRMC_materials_mtoon");
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
            let sc = [
                base_color[0] * 0.85,
                base_color[1] * 0.8,
                base_color[2] * 0.85,
                -0.1,
            ];
            // shade_toony=0.5 (r0), rim_intensity=0.3 (r1), rim_fresnel=3.0 (r2)
            let sr = [0.5_f32, 0.3, 3.0];
            // rim_color (RGB) = white, rim_lift (A) = 0.0
            let hs = [1.0_f32, 1.0, 1.0, 0.0];
            (sc, sr, hs)
        } else {
            (
                [0.9, 0.5, 0.35, subsurface],
                [0.012_f32, 0.036, 0.12],
                [0.8_f32, 0.6, 0.4, 0.3],
            )
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
    log::info!(
        "glTF: {} skins, {} nodes in hierarchy",
        skins.len(),
        node_hierarchy.len()
    );

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
                        morph_target_names =
                            (0..target_count).map(|i| format!("morph_{}", i)).collect();
                    }
                }
            }

            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

                // KHR_mesh_quantization: POSITION/NORMAL may be stored as
                // BYTE/UBYTE/SHORT/USHORT. The `gltf` crate's read_positions /
                // read_normals are hardcoded to read f32 (it never inspects the
                // component type), so quantized data would be misread as garbage.
                // Detect non-f32 accessors and dequantize manually; positions
                // are dequantized geometrically by the node TRS (already applied
                // downstream), normals/tangents by normalized-integer scaling.
                use gltf::accessor::DataType;
                let positions: Vec<f32> = match primitive.get(&gltf::Semantic::Positions) {
                    Some(acc) if acc.data_type() != DataType::F32 => {
                        match read_quantized_attr(&acc, &buffers, 3) {
                            Some(v) => v,
                            None => continue,
                        }
                    }
                    Some(_) => match reader.read_positions() {
                        Some(iter) => iter.flat_map(|p| p.into_iter()).collect(),
                        None => continue,
                    },
                    None => continue,
                };

                let read_indices_u32 = || -> Option<Vec<u32>> {
                    reader.read_indices().map(|i| i.into_u32().collect())
                };
                let normals: Vec<f32> = match primitive.get(&gltf::Semantic::Normals) {
                    Some(acc) if acc.data_type() != DataType::F32 => {
                        read_quantized_attr(&acc, &buffers, 3).unwrap_or_else(|| {
                            generate_normals_from_tris(&positions, read_indices_u32().as_deref())
                        })
                    }
                    Some(_) => reader
                        .read_normals()
                        .map(|iter| iter.flat_map(|n| n.into_iter()).collect())
                        .unwrap_or_else(|| {
                            generate_normals_from_tris(&positions, read_indices_u32().as_deref())
                        }),
                    None => generate_normals_from_tris(&positions, read_indices_u32().as_deref()),
                };

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
                            let v: Vec<f32> = pos_iter
                                .flat_map(|p: [f32; 3]| [p[0], p[1], p[2]])
                                .collect();
                            v
                        }
                        None => Vec::new(),
                    };
                    if !deltas.is_empty() {
                        let max_d = deltas.iter().cloned().fold(0.0f32, f32::max);
                        let min_d = deltas.iter().cloned().fold(0.0f32, f32::min);
                        let non_zero = deltas.iter().filter(|d| d.abs() > 0.0001).count();
                        if mesh_morphs.len() < 6 && non_zero > 0 {
                            log::info!(
                                "morph[{}]: {} floats, {} non-zero, range [{:.6}, {:.6}]",
                                mesh_morphs.len(),
                                deltas.len(),
                                non_zero,
                                min_d,
                                max_d
                            );
                        }
                        mesh_morphs.push(MorphTarget {
                            position_deltas: deltas,
                        });
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

    log::info!(
        "glTF: {} morph target names, meshes with morphs: {}",
        morph_target_names.len(),
        all_morph_targets.iter().filter(|m| !m.is_empty()).count()
    );

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

    fn build_glb(json: serde_json::Value, bin: &[u8]) -> Vec<u8> {
        let mut json_bytes = serde_json::to_vec(&json).unwrap();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let mut bin = bin.to_vec();
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::new();
        glb.extend_from_slice(&0x4654_6C67u32.to_le_bytes()); // "glTF"
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // "JSON"
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // "BIN\0"
        glb.extend_from_slice(&bin);
        glb
    }

    #[test]
    fn khr_mesh_quantization_dequantizes_positions_and_normals() {
        // One triangle. POSITION = SHORT (non-normalized, dequantized by the
        // node scale 0.001). NORMAL = BYTE normalized (127 → 1.0).
        let mut bin: Vec<u8> = Vec::new();
        // positions: stride 8 (i16 x3 + 2 pad), 3 verts
        let verts: [[i16; 3]; 3] = [[0, 0, 0], [1000, 0, 0], [0, 1000, 0]];
        for v in verts {
            for c in v {
                bin.extend_from_slice(&c.to_le_bytes());
            }
            bin.extend_from_slice(&[0, 0]); // pad to stride 8
        }
        assert_eq!(bin.len(), 24);
        // normals: stride 4 (i8 x3 + 1 pad), 3 verts, all (0,0,127)
        for _ in 0..3 {
            bin.extend_from_slice(&[0u8, 0, 127, 0]);
        }
        assert_eq!(bin.len(), 36);
        // indices: u16 x3
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }

        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "extensionsUsed": ["KHR_mesh_quantization"],
            "extensionsRequired": ["KHR_mesh_quantization"],
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0,  "byteLength": 24, "byteStride": 8, "target": 34962},
                {"buffer": 0, "byteOffset": 24, "byteLength": 12, "byteStride": 4, "target": 34962},
                {"buffer": 0, "byteOffset": 36, "byteLength": 6,  "target": 34963}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5122, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5120, "normalized": true, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2}]}],
            "nodes": [{"mesh": 0, "scale": [0.001, 0.001, 0.001]}],
            "scenes": [{"nodes": [0]}],
            "scene": 0
        });
        let glb = build_glb(json, &bin);

        let scene = load_glb(&glb).expect("load quantized glb");
        assert_eq!(scene.meshes.len(), 1);
        let v = &scene.meshes[0].vertices; // 8 floats/vertex: pos3, norm3, uv2

        // Positions are read in quantized object space (raw integer values);
        // the node scale (0.001) dequantizes them downstream.
        assert_eq!(&v[0..3], &[0.0, 0.0, 0.0]);
        assert_eq!(&v[8..11], &[1000.0, 0.0, 0.0]);
        assert_eq!(&v[16..19], &[0.0, 1000.0, 0.0]);

        // Normals: BYTE 127 normalized → 1.0 on Z.
        assert!((v[3] - 0.0).abs() < 1e-6);
        assert!((v[4] - 0.0).abs() < 1e-6);
        assert!((v[5] - 1.0).abs() < 1e-4, "nz={}", v[5]);

        // The node carries the dequantization scale.
        let (scale, _, _) = scene.nodes[0].transform.to_scale_rotation_translation();
        assert!((scale.x - 0.001).abs() < 1e-6, "scale={scale:?}");
    }
}
