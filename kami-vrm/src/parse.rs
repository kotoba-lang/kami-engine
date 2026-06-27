//! VRM document parser: GLB bytes → VrmDocument.

use crate::VrmError;
use crate::compat::convert_v0x_to_v1;
use crate::gltf_types::GltfDocument;
use crate::vrm_types::*;

/// Parse VRM GLB bytes into a VrmDocument.
///
/// Detects VRM version automatically:
/// - extensionsUsed contains "VRMC_vrm" → VRM 1.0
/// - root extensions has "VRM" key → VRM 0.x (converted to 1.0 types)
pub fn parse_vrm(data: &[u8]) -> Result<VrmDocument, VrmError> {
    let chunks = crate::glb::parse_glb(data)?;
    let gltf: GltfDocument = serde_json::from_slice(chunks.json)?;
    let bin = chunks.bin.unwrap_or(&[]).to_vec();

    let is_v1 = gltf.extensions_used.iter().any(|e| e == "VRMC_vrm");
    let is_v0 = !is_v1
        && gltf
            .extensions
            .as_ref()
            .and_then(|e| e.get("VRM"))
            .is_some();

    if is_v1 {
        parse_vrm_1_0(gltf, bin)
    } else if is_v0 {
        parse_vrm_0x(gltf, bin)
    } else {
        Err(VrmError::MissingExtension("VRMC_vrm or VRM".into()))
    }
}

/// Parse VRM 1.0 from glTF document.
fn parse_vrm_1_0(gltf: GltfDocument, bin: Vec<u8>) -> Result<VrmDocument, VrmError> {
    let root_ext = gltf
        .extensions
        .as_ref()
        .ok_or_else(|| VrmError::MissingExtension("VRMC_vrm".into()))?;

    let vrmc_vrm = root_ext
        .get("VRMC_vrm")
        .ok_or_else(|| VrmError::MissingExtension("VRMC_vrm".into()))?;

    // Meta
    let meta = parse_meta_v1(vrmc_vrm)?;

    // Humanoid
    let humanoid = parse_humanoid_v1(vrmc_vrm)?;

    // Expressions
    let expressions = parse_expressions_v1(vrmc_vrm);

    // LookAt
    let look_at = parse_look_at_v1(vrmc_vrm);

    // FirstPerson
    let first_person = parse_first_person_v1(vrmc_vrm);

    // Spring bones
    let (spring_bones, spring_bone_colliders, spring_bone_collider_groups) =
        if let Some(sb_ext) = root_ext.get("VRMC_springBone") {
            parse_spring_bone_v1(sb_ext)
        } else {
            (vec![], vec![], vec![])
        };

    // MToon materials (per-material extension)
    let mtoon_materials = parse_mtoon_materials(&gltf);

    // Node constraints
    let node_constraints = parse_node_constraints(&gltf);

    Ok(VrmDocument {
        gltf,
        bin,
        version: VrmVersion::V1_0,
        meta,
        humanoid,
        expressions,
        spring_bones,
        spring_bone_colliders,
        spring_bone_collider_groups,
        mtoon_materials,
        look_at,
        first_person,
        node_constraints,
    })
}

/// Parse VRM 0.x, converting to 1.0 types via compat layer.
fn parse_vrm_0x(gltf: GltfDocument, bin: Vec<u8>) -> Result<VrmDocument, VrmError> {
    let vrm_ext = gltf
        .extensions
        .as_ref()
        .and_then(|e| e.get("VRM"))
        .ok_or_else(|| VrmError::MissingExtension("VRM".into()))?;

    let (
        meta,
        humanoid,
        expressions,
        spring_bones,
        colliders,
        collider_groups,
        mtoon_materials,
        look_at,
        first_person,
    ) = convert_v0x_to_v1(vrm_ext, &gltf)?;

    Ok(VrmDocument {
        gltf,
        bin,
        version: VrmVersion::V0x,
        meta,
        humanoid,
        expressions,
        spring_bones,
        spring_bone_colliders: colliders,
        spring_bone_collider_groups: collider_groups,
        mtoon_materials,
        look_at,
        first_person,
        node_constraints: vec![],
    })
}

// ── VRM 1.0 parsers ──

fn parse_meta_v1(vrmc: &serde_json::Value) -> Result<VrmMeta, VrmError> {
    let meta = vrmc
        .get("meta")
        .ok_or_else(|| VrmError::MissingExtension("VRMC_vrm.meta".into()))?;
    Ok(VrmMeta {
        name: meta
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        version: meta
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
        authors: meta
            .get("authors")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        license_url: meta
            .get("licenseUrl")
            .and_then(|v| v.as_str())
            .map(String::from),
        allow_redistribution: meta
            .get("allowRedistribution")
            .and_then(|v| v.as_str())
            .map(|s| s == "allow"),
        thumbnail_image: meta
            .get("thumbnailImage")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize),
        avatar_permission: meta
            .get("avatarPermission")
            .and_then(|v| v.as_str())
            .map(String::from),
        commercial_usage: meta
            .get("commercialUsage")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

fn parse_humanoid_v1(vrmc: &serde_json::Value) -> Result<VrmHumanoid, VrmError> {
    let humanoid = vrmc
        .get("humanoid")
        .ok_or_else(|| VrmError::MissingExtension("VRMC_vrm.humanoid".into()))?;

    let human_bones_obj = humanoid
        .get("humanBones")
        .and_then(|v| v.as_object())
        .ok_or_else(|| VrmError::MissingExtension("VRMC_vrm.humanoid.humanBones".into()))?;

    let mut human_bones = Vec::new();
    for (name, val) in human_bones_obj {
        if let Some(bone_name) = HumanBoneName::from_str(name) {
            if let Some(node) = val.get("node").and_then(|v| v.as_u64()) {
                human_bones.push(VrmHumanBone {
                    bone: bone_name,
                    node: node as usize,
                });
            }
        }
    }

    Ok(VrmHumanoid { human_bones })
}

fn parse_expressions_v1(vrmc: &serde_json::Value) -> Vec<VrmExpression> {
    let Some(expressions) = vrmc.get("expressions") else {
        return vec![];
    };

    let mut result = Vec::new();

    // Preset expressions
    if let Some(preset) = expressions.get("preset").and_then(|v| v.as_object()) {
        for (name, val) in preset {
            if let Some(expr) = parse_single_expression(name, val, ExpressionPreset::from_str(name))
            {
                result.push(expr);
            }
        }
    }

    // Custom expressions
    if let Some(custom) = expressions.get("custom").and_then(|v| v.as_object()) {
        for (name, val) in custom {
            if let Some(expr) = parse_single_expression(name, val, None) {
                result.push(expr);
            }
        }
    }

    result
}

fn parse_single_expression(
    name: &str,
    val: &serde_json::Value,
    preset: Option<ExpressionPreset>,
) -> Option<VrmExpression> {
    let morph_target_binds = val
        .get("morphTargetBinds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| {
                    Some(MorphTargetBind {
                        mesh_index: b.get("mesh")?.as_u64()? as usize,
                        morph_index: b.get("index")?.as_u64()? as usize,
                        weight: b.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let material_color_binds = val
        .get("materialColorBinds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| {
                    let tv = b.get("targetValue")?.as_array()?;
                    Some(MaterialColorBind {
                        material_index: b.get("material")?.as_u64()? as usize,
                        property: b.get("type")?.as_str()?.to_string(),
                        target_value: [
                            tv.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                            tv.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                            tv.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                            tv.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                        ],
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let texture_transform_binds = val
        .get("textureTransformBinds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| {
                    Some(TextureTransformBind {
                        material_index: b.get("material")?.as_u64()? as usize,
                        offset: parse_f32_2(b.get("offset")?).unwrap_or([0.0; 2]),
                        scale: parse_f32_2(b.get("scale")?).unwrap_or([1.0; 2]),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let parse_override = |key: &str| -> Option<OverrideType> {
        match val.get(key)?.as_str()? {
            "block" => Some(OverrideType::Block),
            "blend" => Some(OverrideType::Blend),
            "none" => Some(OverrideType::None),
            _ => None,
        }
    };

    Some(VrmExpression {
        name: name.to_string(),
        preset,
        is_binary: val
            .get("isBinary")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        morph_target_binds,
        material_color_binds,
        texture_transform_binds,
        override_blink: parse_override("overrideBlink"),
        override_look_at: parse_override("overrideLookAt"),
        override_mouth: parse_override("overrideMouth"),
    })
}

fn parse_look_at_v1(vrmc: &serde_json::Value) -> Option<VrmLookAt> {
    let la = vrmc.get("lookAt")?;
    Some(VrmLookAt {
        look_at_type: match la.get("type")?.as_str()? {
            "bone" => LookAtType::Bone,
            _ => LookAtType::Expression,
        },
        offset_from_head_bone: parse_f32_3(la.get("offsetFromHeadBone")?).unwrap_or([0.0; 3]),
        range_map_horizontal_inner: parse_range_map(la.get("rangeMapHorizontalInner")?),
        range_map_horizontal_outer: parse_range_map(la.get("rangeMapHorizontalOuter")?),
        range_map_vertical_down: parse_range_map(la.get("rangeMapVerticalDown")?),
        range_map_vertical_up: parse_range_map(la.get("rangeMapVerticalUp")?),
    })
}

fn parse_first_person_v1(vrmc: &serde_json::Value) -> Option<VrmFirstPerson> {
    let fp = vrmc.get("firstPerson")?;
    let annotations = fp.get("meshAnnotations")?.as_array()?;
    let mesh_annotations = annotations
        .iter()
        .filter_map(|a| {
            Some(MeshAnnotation {
                node: a.get("node")?.as_u64()? as usize,
                annotation_type: match a.get("type")?.as_str()? {
                    "both" => FirstPersonFlag::Both,
                    "thirdPersonOnly" => FirstPersonFlag::ThirdPersonOnly,
                    "firstPersonOnly" => FirstPersonFlag::FirstPersonOnly,
                    _ => FirstPersonFlag::Auto,
                },
            })
        })
        .collect();
    Some(VrmFirstPerson { mesh_annotations })
}

fn parse_spring_bone_v1(
    sb_ext: &serde_json::Value,
) -> (
    Vec<VrmSpringBoneChain>,
    Vec<VrmCollider>,
    Vec<VrmColliderGroup>,
) {
    // Colliders
    let colliders: Vec<VrmCollider> = sb_ext
        .get("colliders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let node = c.get("node")?.as_u64()? as usize;
                    let shape = c.get("shape")?;
                    let collider_shape = if let Some(sphere) = shape.get("sphere") {
                        ColliderShape::Sphere {
                            offset: parse_f32_3(sphere.get("offset")?).unwrap_or([0.0; 3]),
                            radius: sphere.get("radius")?.as_f64()? as f32,
                        }
                    } else if let Some(capsule) = shape.get("capsule") {
                        ColliderShape::Capsule {
                            offset: parse_f32_3(capsule.get("offset")?).unwrap_or([0.0; 3]),
                            tail: parse_f32_3(capsule.get("tail")?).unwrap_or([0.0; 3]),
                            radius: capsule.get("radius")?.as_f64()? as f32,
                        }
                    } else {
                        return None;
                    };
                    Some(VrmCollider {
                        node,
                        shape: collider_shape,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Collider groups
    let collider_groups: Vec<VrmColliderGroup> = sb_ext
        .get("colliderGroups")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|g| VrmColliderGroup {
                    name: g.get("name").and_then(|v| v.as_str()).map(String::from),
                    colliders: g
                        .get("colliders")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Springs (chains)
    let springs: Vec<VrmSpringBoneChain> = sb_ext
        .get("springs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|s| {
                    let joints = s
                        .get("joints")
                        .and_then(|v| v.as_array())
                        .map(|ja| {
                            ja.iter()
                                .filter_map(|j| {
                                    Some(SpringJoint {
                                        node: j.get("node")?.as_u64()? as usize,
                                        hit_radius: j
                                            .get("hitRadius")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0)
                                            as f32,
                                        stiffness: j
                                            .get("stiffness")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(1.0)
                                            as f32,
                                        gravity_power: j
                                            .get("gravityPower")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0)
                                            as f32,
                                        gravity_dir: j
                                            .get("gravityDir")
                                            .and_then(parse_f32_3)
                                            .unwrap_or([0.0, -1.0, 0.0]),
                                        drag_force: j
                                            .get("dragForce")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.4)
                                            as f32,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    VrmSpringBoneChain {
                        name: s.get("name").and_then(|v| v.as_str()).map(String::from),
                        joints,
                        collider_groups: s
                            .get("colliderGroups")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        center: s.get("center").and_then(|v| v.as_u64()).map(|n| n as usize),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    (springs, colliders, collider_groups)
}

fn parse_mtoon_materials(gltf: &GltfDocument) -> Vec<VrmMtoonMaterial> {
    gltf.materials
        .iter()
        .enumerate()
        .filter_map(|(i, mat)| {
            let ext = mat.extensions.as_ref()?.get("VRMC_materials_mtoon")?;
            Some(VrmMtoonMaterial {
                material_index: i,
                shade_color_factor: parse_f32_3(ext.get("shadeColorFactor")?).unwrap_or([0.0; 3]),
                shade_multiply_texture: ext
                    .get("shadeMultiplyTexture")
                    .and_then(|v| v.get("index"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                shading_shift_factor: ext
                    .get("shadingShiftFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                shading_toony_factor: ext
                    .get("shadingToonyFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.9) as f32,
                gi_equalization_factor: ext
                    .get("giEqualizationFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.9) as f32,
                rim_color_factor: ext
                    .get("parametricRimColorFactor")
                    .and_then(parse_f32_3)
                    .unwrap_or([0.0; 3]),
                rim_lighting_mix_factor: ext
                    .get("rimLightingMixFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0) as f32,
                rim_fresnel_power_factor: ext
                    .get("parametricRimFresnelPowerFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(5.0) as f32,
                rim_lift_factor: ext
                    .get("parametricRimLiftFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                rim_multiply_texture: ext
                    .get("rimMultiplyTexture")
                    .and_then(|v| v.get("index"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                outline_width_mode: match ext.get("outlineWidthMode").and_then(|v| v.as_str()) {
                    Some("worldCoordinates") => OutlineWidthMode::WorldCoordinates,
                    Some("screenCoordinates") => OutlineWidthMode::ScreenCoordinates,
                    _ => OutlineWidthMode::None,
                },
                outline_width_factor: ext
                    .get("outlineWidthFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                outline_color_factor: ext
                    .get("outlineColorFactor")
                    .and_then(parse_f32_3)
                    .unwrap_or([0.0; 3]),
                outline_lighting_mix_factor: ext
                    .get("outlineLightingMixFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0) as f32,
                matcap_texture: ext
                    .get("matcapTexture")
                    .and_then(|v| v.get("index"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
                parametric_rim_color_factor: ext
                    .get("parametricRimColorFactor")
                    .and_then(parse_f32_3)
                    .unwrap_or([0.0; 3]),
                uv_animation_scroll_x: ext
                    .get("uvAnimationScrollXSpeedFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                uv_animation_scroll_y: ext
                    .get("uvAnimationScrollYSpeedFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                uv_animation_rotation: ext
                    .get("uvAnimationRotationSpeedFactor")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                render_queue_offset: ext
                    .get("renderQueueOffsetNumber")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32,
                transparent_with_z_write: ext
                    .get("transparentWithZWrite")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn parse_node_constraints(gltf: &GltfDocument) -> Vec<VrmNodeConstraint> {
    gltf.nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            let ext = node.extensions.as_ref()?.get("VRMC_node_constraint")?;
            let constraint = ext.get("constraint")?;
            let ct = if let Some(aim) = constraint.get("aim") {
                ConstraintType::Aim {
                    source: aim.get("source")?.as_u64()? as usize,
                    aim_axis: parse_f32_3(aim.get("aimAxis")?).unwrap_or([0.0, 0.0, 1.0]),
                    weight: aim.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                }
            } else if let Some(rot) = constraint.get("rotation") {
                ConstraintType::Rotation {
                    source: rot.get("source")?.as_u64()? as usize,
                    weight: rot.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                }
            } else if let Some(roll) = constraint.get("roll") {
                ConstraintType::Roll {
                    source: roll.get("source")?.as_u64()? as usize,
                    roll_axis: parse_f32_3(roll.get("rollAxis")?).unwrap_or([0.0, 1.0, 0.0]),
                    weight: roll.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                }
            } else {
                return None;
            };
            Some(VrmNodeConstraint {
                node: i,
                constraint: ct,
            })
        })
        .collect()
}

// ── Helpers ──

fn parse_f32_3(v: &serde_json::Value) -> Option<[f32; 3]> {
    if let Some(obj) = v.as_object() {
        // {"x": 0, "y": -1, "z": 0} form
        let x = obj.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let y = obj.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let z = obj.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        return Some([x, y, z]);
    }
    let arr = v.as_array()?;
    Some([
        arr.first()?.as_f64()? as f32,
        arr.get(1)?.as_f64()? as f32,
        arr.get(2)?.as_f64()? as f32,
    ])
}

fn parse_f32_2(v: &serde_json::Value) -> Option<[f32; 2]> {
    let arr = v.as_array()?;
    Some([arr.first()?.as_f64()? as f32, arr.get(1)?.as_f64()? as f32])
}

fn parse_range_map(v: &serde_json::Value) -> RangeMap {
    RangeMap {
        input_max_value: v
            .get("inputMaxValue")
            .and_then(|v| v.as_f64())
            .unwrap_or(90.0) as f32,
        output_scale: v
            .get("outputScale")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as f32,
    }
}
