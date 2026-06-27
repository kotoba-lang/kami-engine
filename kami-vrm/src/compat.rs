//! VRM 0.x → 1.0 conversion layer.

use crate::VrmError;
use crate::gltf_types::GltfDocument;
use crate::vrm_types::*;

/// Convert VRM 0.x extension JSON to VRM 1.0 types.
///
/// Key differences handled:
/// - 0.x single "VRM" extension → 1.0 separate VRMC_* extensions
/// - 0.x blendShapeMaster.blendShapeGroups → 1.0 expressions
/// - 0.x secondaryAnimation → 1.0 VRMC_springBone
/// - 0.x materialProperties → 1.0 VRMC_materials_mtoon
#[allow(clippy::type_complexity)]
pub fn convert_v0x_to_v1(
    vrm_ext: &serde_json::Value,
    _gltf: &GltfDocument,
) -> Result<
    (
        VrmMeta,
        VrmHumanoid,
        Vec<VrmExpression>,
        Vec<VrmSpringBoneChain>,
        Vec<VrmCollider>,
        Vec<VrmColliderGroup>,
        Vec<VrmMtoonMaterial>,
        Option<VrmLookAt>,
        Option<VrmFirstPerson>,
    ),
    VrmError,
> {
    // Meta
    let meta = convert_meta_v0x(vrm_ext)?;

    // Humanoid
    let humanoid = convert_humanoid_v0x(vrm_ext)?;

    // Expressions (from blendShapeMaster)
    let expressions = convert_expressions_v0x(vrm_ext);

    // Spring bones (from secondaryAnimation)
    let (spring_bones, colliders, collider_groups) = convert_spring_bones_v0x(vrm_ext);

    // MToon materials (from materialProperties)
    let mtoon_materials = convert_mtoon_v0x(vrm_ext);

    // LookAt
    let look_at = convert_look_at_v0x(vrm_ext);

    // FirstPerson
    let first_person = convert_first_person_v0x(vrm_ext);

    Ok((
        meta,
        humanoid,
        expressions,
        spring_bones,
        colliders,
        collider_groups,
        mtoon_materials,
        look_at,
        first_person,
    ))
}

fn convert_meta_v0x(vrm_ext: &serde_json::Value) -> Result<VrmMeta, VrmError> {
    let meta = vrm_ext
        .get("meta")
        .ok_or_else(|| VrmError::MissingExtension("VRM.meta".into()))?;
    Ok(VrmMeta {
        name: meta
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        version: meta
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
        authors: meta
            .get("author")
            .and_then(|v| v.as_str())
            .map(|a| vec![a.to_string()])
            .unwrap_or_default(),
        license_url: meta
            .get("otherLicenseUrl")
            .and_then(|v| v.as_str())
            .map(String::from),
        allow_redistribution: meta
            .get("allowedUserName")
            .and_then(|v| v.as_str())
            .map(|s| s == "Everyone"),
        thumbnail_image: meta
            .get("texture")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        avatar_permission: meta
            .get("allowedUserName")
            .and_then(|v| v.as_str())
            .map(String::from),
        commercial_usage: meta
            .get("commercialUssageName")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

fn convert_humanoid_v0x(vrm_ext: &serde_json::Value) -> Result<VrmHumanoid, VrmError> {
    let humanoid = vrm_ext
        .get("humanoid")
        .ok_or_else(|| VrmError::MissingExtension("VRM.humanoid".into()))?;

    let bones = humanoid
        .get("humanBones")
        .and_then(|v| v.as_array())
        .ok_or_else(|| VrmError::MissingExtension("VRM.humanoid.humanBones".into()))?;

    let human_bones = bones
        .iter()
        .filter_map(|b| {
            let name = b.get("bone")?.as_str()?;
            let node = b.get("node")?.as_u64()? as usize;
            let bone_name = HumanBoneName::from_str(name)?;
            Some(VrmHumanBone {
                bone: bone_name,
                node,
            })
        })
        .collect();

    Ok(VrmHumanoid { human_bones })
}

fn convert_expressions_v0x(vrm_ext: &serde_json::Value) -> Vec<VrmExpression> {
    let Some(groups) = vrm_ext
        .get("blendShapeMaster")
        .and_then(|v| v.get("blendShapeGroups"))
        .and_then(|v| v.as_array())
    else {
        return vec![];
    };

    groups
        .iter()
        .filter_map(|g| {
            let name = g.get("name")?.as_str()?.to_string();
            let preset_name = g.get("presetName").and_then(|v| v.as_str()).unwrap_or("");
            let preset = match preset_name.to_lowercase().as_str() {
                "joy" | "happy" => Some(ExpressionPreset::Happy),
                "angry" => Some(ExpressionPreset::Angry),
                "sorrow" | "sad" => Some(ExpressionPreset::Sad),
                "fun" | "relaxed" => Some(ExpressionPreset::Relaxed),
                "surprised" => Some(ExpressionPreset::Surprised),
                "a" | "aa" => Some(ExpressionPreset::Aa),
                "i" | "ih" => Some(ExpressionPreset::Ih),
                "u" | "ou" => Some(ExpressionPreset::Ou),
                "e" | "ee" => Some(ExpressionPreset::Ee),
                "o" | "oh" => Some(ExpressionPreset::Oh),
                "blink" => Some(ExpressionPreset::Blink),
                "blink_l" | "blinkleft" => Some(ExpressionPreset::BlinkLeft),
                "blink_r" | "blinkright" => Some(ExpressionPreset::BlinkRight),
                "lookup" => Some(ExpressionPreset::LookUp),
                "lookdown" => Some(ExpressionPreset::LookDown),
                "lookleft" => Some(ExpressionPreset::LookLeft),
                "lookright" => Some(ExpressionPreset::LookRight),
                "neutral" => Some(ExpressionPreset::Neutral),
                _ => None,
            };

            let morph_target_binds = g
                .get("binds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| {
                            Some(MorphTargetBind {
                                mesh_index: b.get("mesh")?.as_u64()? as usize,
                                morph_index: b.get("index")?.as_u64()? as usize,
                                // VRM 0.x uses 0-100 weight range
                                weight: b.get("weight").and_then(|v| v.as_f64()).unwrap_or(100.0)
                                    as f32
                                    / 100.0,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let material_color_binds = g
                .get("materialValues")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| {
                            let tv = b.get("targetValue")?.as_array()?;
                            Some(MaterialColorBind {
                                material_index: b
                                    .get("materialName")
                                    .and_then(|_v| {
                                        // 0.x uses material name, not index. We map it to 0 as placeholder.
                                        Some(0usize)
                                    })
                                    .unwrap_or(0),
                                property: b.get("propertyName")?.as_str()?.to_string(),
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

            Some(VrmExpression {
                name,
                preset,
                is_binary: g.get("isBinary").and_then(|v| v.as_bool()).unwrap_or(false),
                morph_target_binds,
                material_color_binds,
                texture_transform_binds: vec![],
                override_blink: None,
                override_look_at: None,
                override_mouth: None,
            })
        })
        .collect()
}

fn convert_spring_bones_v0x(
    vrm_ext: &serde_json::Value,
) -> (
    Vec<VrmSpringBoneChain>,
    Vec<VrmCollider>,
    Vec<VrmColliderGroup>,
) {
    let Some(sa) = vrm_ext.get("secondaryAnimation") else {
        return (vec![], vec![], vec![]);
    };

    // Collider groups (0.x has colliderGroups at top level of secondaryAnimation)
    let mut all_colliders = Vec::new();
    let collider_groups: Vec<VrmColliderGroup> = sa
        .get("colliderGroups")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|g| {
                    let node = g.get("node").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let colliders_in_group: Vec<usize> = g
                        .get("colliders")
                        .and_then(|v| v.as_array())
                        .map(|ca| {
                            ca.iter()
                                .filter_map(|c| {
                                    let offset = c.get("offset")?;
                                    let ox = offset.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as f32;
                                    let oy = offset.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as f32;
                                    let oz = offset.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                        as f32;
                                    let radius =
                                        c.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                            as f32;
                                    let idx = all_colliders.len();
                                    all_colliders.push(VrmCollider {
                                        node,
                                        shape: ColliderShape::Sphere {
                                            offset: [ox, oy, oz],
                                            radius,
                                        },
                                    });
                                    Some(idx)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    VrmColliderGroup {
                        name: None,
                        colliders: colliders_in_group,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Bone groups (springs)
    let springs: Vec<VrmSpringBoneChain> = sa
        .get("boneGroups")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|bg| {
                    let stiffness =
                        bg.get("stiffiness").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    let gravity_power = bg
                        .get("gravityPower")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    let gravity_dir_obj = bg.get("gravityDir");
                    let gravity_dir = gravity_dir_obj
                        .and_then(|d| {
                            Some([
                                d.get("x")?.as_f64()? as f32,
                                d.get("y")?.as_f64()? as f32,
                                d.get("z")?.as_f64()? as f32,
                            ])
                        })
                        .unwrap_or([0.0, -1.0, 0.0]);
                    let drag_force =
                        bg.get("dragForce").and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
                    let hit_radius =
                        bg.get("hitRadius").and_then(|v| v.as_f64()).unwrap_or(0.02) as f32;

                    let bones = bg.get("bones")?.as_array()?;
                    let joints: Vec<SpringJoint> = bones
                        .iter()
                        .filter_map(|b| {
                            Some(SpringJoint {
                                node: b.as_u64()? as usize,
                                hit_radius,
                                stiffness,
                                gravity_power,
                                gravity_dir,
                                drag_force,
                            })
                        })
                        .collect();

                    let cg = bg
                        .get("colliderGroups")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect()
                        })
                        .unwrap_or_default();

                    Some(VrmSpringBoneChain {
                        name: bg.get("comment").and_then(|v| v.as_str()).map(String::from),
                        joints,
                        collider_groups: cg,
                        center: bg
                            .get("center")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as usize),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    (springs, all_colliders, collider_groups)
}

fn convert_mtoon_v0x(vrm_ext: &serde_json::Value) -> Vec<VrmMtoonMaterial> {
    let Some(props) = vrm_ext.get("materialProperties").and_then(|v| v.as_array()) else {
        return vec![];
    };

    props
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let shader = p.get("shader")?.as_str()?;
            if !shader.contains("MToon") {
                return None;
            }

            let fv = |key: &str| -> f32 {
                p.get("floatProperties")
                    .and_then(|fp| fp.get(key))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32
            };

            let cv = |key: &str| -> [f32; 3] {
                p.get("vectorProperties")
                    .and_then(|vp| vp.get(key))
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        [
                            a.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                            a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                            a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                        ]
                    })
                    .unwrap_or([0.0; 3])
            };

            Some(VrmMtoonMaterial {
                material_index: i,
                shade_color_factor: cv("_ShadeColor"),
                shade_multiply_texture: None,
                shading_shift_factor: fv("_ShadeShift"),
                shading_toony_factor: fv("_ShadeToony"),
                gi_equalization_factor: fv("_IndirectLightIntensity"),
                rim_color_factor: cv("_RimColor"),
                rim_lighting_mix_factor: fv("_RimLightingMix"),
                rim_fresnel_power_factor: fv("_RimFresnelPower"),
                rim_lift_factor: fv("_RimLift"),
                rim_multiply_texture: None,
                outline_width_mode: match fv("_OutlineWidthMode") as u32 {
                    1 => OutlineWidthMode::WorldCoordinates,
                    2 => OutlineWidthMode::ScreenCoordinates,
                    _ => OutlineWidthMode::None,
                },
                outline_width_factor: fv("_OutlineWidth"),
                outline_color_factor: cv("_OutlineColor"),
                outline_lighting_mix_factor: fv("_OutlineLightingMix"),
                matcap_texture: None,
                parametric_rim_color_factor: cv("_RimColor"),
                uv_animation_scroll_x: fv("_UvAnimScrollX"),
                uv_animation_scroll_y: fv("_UvAnimScrollY"),
                uv_animation_rotation: fv("_UvAnimRotation"),
                render_queue_offset: p.get("renderQueue").and_then(|v| v.as_i64()).unwrap_or(0)
                    as i32,
                transparent_with_z_write: fv("_ZWrite") > 0.5,
            })
        })
        .collect()
}

fn convert_look_at_v0x(vrm_ext: &serde_json::Value) -> Option<VrmLookAt> {
    let fp = vrm_ext.get("firstPerson")?;
    let lat = fp.get("lookAtTypeName")?.as_str()?;
    Some(VrmLookAt {
        look_at_type: if lat == "Bone" {
            LookAtType::Bone
        } else {
            LookAtType::Expression
        },
        offset_from_head_bone: fp
            .get("lookAtHorizontalInner")
            .and_then(|_| {
                let offset = fp.get("firstPersonBoneOffset")?;
                Some([
                    offset.get("x")?.as_f64()? as f32,
                    offset.get("y")?.as_f64()? as f32,
                    offset.get("z")?.as_f64()? as f32,
                ])
            })
            .unwrap_or([0.0; 3]),
        range_map_horizontal_inner: convert_range_map_v0x(fp.get("lookAtHorizontalInner")),
        range_map_horizontal_outer: convert_range_map_v0x(fp.get("lookAtHorizontalOuter")),
        range_map_vertical_down: convert_range_map_v0x(fp.get("lookAtVerticalDown")),
        range_map_vertical_up: convert_range_map_v0x(fp.get("lookAtVerticalUp")),
    })
}

fn convert_range_map_v0x(v: Option<&serde_json::Value>) -> RangeMap {
    let Some(v) = v else {
        return RangeMap {
            input_max_value: 90.0,
            output_scale: 10.0,
        };
    };
    RangeMap {
        input_max_value: v.get("xRange").and_then(|v| v.as_f64()).unwrap_or(90.0) as f32,
        output_scale: v.get("yRange").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32,
    }
}

fn convert_first_person_v0x(vrm_ext: &serde_json::Value) -> Option<VrmFirstPerson> {
    let fp = vrm_ext.get("firstPerson")?;
    let annotations = fp.get("meshAnnotations")?.as_array()?;
    let mesh_annotations = annotations
        .iter()
        .filter_map(|a| {
            Some(MeshAnnotation {
                node: a.get("mesh")?.as_u64()? as usize,
                annotation_type: match a.get("firstPersonFlag")?.as_str()? {
                    "Both" => FirstPersonFlag::Both,
                    "ThirdPersonOnly" => FirstPersonFlag::ThirdPersonOnly,
                    "FirstPersonOnly" => FirstPersonFlag::FirstPersonOnly,
                    _ => FirstPersonFlag::Auto,
                },
            })
        })
        .collect();
    Some(VrmFirstPerson { mesh_annotations })
}
