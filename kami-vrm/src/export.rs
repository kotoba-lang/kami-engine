//! VRM GLB export: VrmDocument → GLB bytes.

use crate::VrmError;
use crate::vrm_types::*;

/// Export a VrmDocument to GLB bytes with VRM 1.0 extensions.
pub fn export_glb(doc: &VrmDocument) -> Result<Vec<u8>, VrmError> {
    let mut gltf = doc.gltf.clone();

    // Ensure extensions_used
    let required_exts = ["VRMC_vrm", "VRMC_springBone", "VRMC_materials_mtoon"];
    for ext in &required_exts {
        if !gltf.extensions_used.iter().any(|e| e == ext) {
            gltf.extensions_used.push(ext.to_string());
        }
    }

    // Build root extensions
    let mut root_ext = serde_json::Map::new();
    root_ext.insert("VRMC_vrm".into(), build_vrmc_vrm(doc));
    if !doc.spring_bones.is_empty() || !doc.spring_bone_colliders.is_empty() {
        root_ext.insert("VRMC_springBone".into(), build_vrmc_spring_bone(doc));
    }
    gltf.extensions = Some(serde_json::Value::Object(root_ext));

    // Inject MToon extensions into materials
    for mtoon in &doc.mtoon_materials {
        if let Some(mat) = gltf.materials.get_mut(mtoon.material_index) {
            let mtoon_ext = build_mtoon_extension(mtoon);
            let ext = mat
                .extensions
                .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = ext.as_object_mut() {
                obj.insert("VRMC_materials_mtoon".into(), mtoon_ext);
            }
        }
    }

    // Inject node constraints
    for nc in &doc.node_constraints {
        if let Some(node) = gltf.nodes.get_mut(nc.node) {
            let nc_ext = build_node_constraint(nc);
            let ext = node
                .extensions
                .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = ext.as_object_mut() {
                obj.insert("VRMC_node_constraint".into(), nc_ext);
            }
        }
    }

    let json_str = serde_json::to_string(&gltf).map_err(VrmError::Json)?;
    Ok(crate::glb::write_glb(json_str.as_bytes(), &doc.bin))
}

/// Build VRMC_vrm extension JSON.
fn build_vrmc_vrm(doc: &VrmDocument) -> serde_json::Value {
    let mut vrm = serde_json::Map::new();
    vrm.insert("specVersion".into(), "1.0".into());

    // Meta
    let mut meta = serde_json::Map::new();
    meta.insert("name".into(), doc.meta.name.clone().into());
    if let Some(ref v) = doc.meta.version {
        meta.insert("version".into(), v.clone().into());
    }
    if !doc.meta.authors.is_empty() {
        meta.insert(
            "authors".into(),
            doc.meta
                .authors
                .iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect::<Vec<_>>()
                .into(),
        );
    }
    if let Some(ref url) = doc.meta.license_url {
        meta.insert("licenseUrl".into(), url.clone().into());
    }
    if let Some(ref perm) = doc.meta.avatar_permission {
        meta.insert("avatarPermission".into(), perm.clone().into());
    }
    if let Some(ref usage) = doc.meta.commercial_usage {
        meta.insert("commercialUsage".into(), usage.clone().into());
    }
    vrm.insert("meta".into(), serde_json::Value::Object(meta));

    // Humanoid
    let mut human_bones = serde_json::Map::new();
    for hb in &doc.humanoid.human_bones {
        let mut bone_obj = serde_json::Map::new();
        bone_obj.insert("node".into(), serde_json::Value::Number(hb.node.into()));
        human_bones.insert(
            hb.bone.as_str().to_string(),
            serde_json::Value::Object(bone_obj),
        );
    }
    vrm.insert(
        "humanoid".into(),
        serde_json::json!({ "humanBones": serde_json::Value::Object(human_bones) }),
    );

    // Expressions
    if !doc.expressions.is_empty() {
        let mut preset_map = serde_json::Map::new();
        let mut custom_map = serde_json::Map::new();

        for expr in &doc.expressions {
            let expr_obj = build_expression(expr);
            if expr.preset.is_some() {
                preset_map.insert(expr.name.clone(), expr_obj);
            } else {
                custom_map.insert(expr.name.clone(), expr_obj);
            }
        }

        let mut expressions = serde_json::Map::new();
        if !preset_map.is_empty() {
            expressions.insert("preset".into(), serde_json::Value::Object(preset_map));
        }
        if !custom_map.is_empty() {
            expressions.insert("custom".into(), serde_json::Value::Object(custom_map));
        }
        vrm.insert("expressions".into(), serde_json::Value::Object(expressions));
    }

    // LookAt
    if let Some(ref la) = doc.look_at {
        vrm.insert("lookAt".into(), serde_json::json!({
            "type": match la.look_at_type { LookAtType::Bone => "bone", LookAtType::Expression => "expression" },
            "offsetFromHeadBone": la.offset_from_head_bone,
            "rangeMapHorizontalInner": { "inputMaxValue": la.range_map_horizontal_inner.input_max_value, "outputScale": la.range_map_horizontal_inner.output_scale },
            "rangeMapHorizontalOuter": { "inputMaxValue": la.range_map_horizontal_outer.input_max_value, "outputScale": la.range_map_horizontal_outer.output_scale },
            "rangeMapVerticalDown": { "inputMaxValue": la.range_map_vertical_down.input_max_value, "outputScale": la.range_map_vertical_down.output_scale },
            "rangeMapVerticalUp": { "inputMaxValue": la.range_map_vertical_up.input_max_value, "outputScale": la.range_map_vertical_up.output_scale },
        }));
    }

    // FirstPerson
    if let Some(ref fp) = doc.first_person {
        let annotations: Vec<serde_json::Value> = fp
            .mesh_annotations
            .iter()
            .map(|a| {
                serde_json::json!({
                    "node": a.node,
                    "type": match a.annotation_type {
                        FirstPersonFlag::Auto => "auto",
                        FirstPersonFlag::Both => "both",
                        FirstPersonFlag::ThirdPersonOnly => "thirdPersonOnly",
                        FirstPersonFlag::FirstPersonOnly => "firstPersonOnly",
                    },
                })
            })
            .collect();
        vrm.insert(
            "firstPerson".into(),
            serde_json::json!({ "meshAnnotations": annotations }),
        );
    }

    serde_json::Value::Object(vrm)
}

/// Build a single expression object.
fn build_expression(expr: &VrmExpression) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    if expr.is_binary {
        obj.insert("isBinary".into(), true.into());
    }

    if !expr.morph_target_binds.is_empty() {
        let binds: Vec<serde_json::Value> = expr
            .morph_target_binds
            .iter()
            .map(|b| {
                serde_json::json!({
                    "mesh": b.mesh_index,
                    "index": b.morph_index,
                    "weight": b.weight,
                })
            })
            .collect();
        obj.insert("morphTargetBinds".into(), binds.into());
    }

    if !expr.material_color_binds.is_empty() {
        let binds: Vec<serde_json::Value> = expr
            .material_color_binds
            .iter()
            .map(|b| {
                serde_json::json!({
                    "material": b.material_index,
                    "type": b.property,
                    "targetValue": b.target_value,
                })
            })
            .collect();
        obj.insert("materialColorBinds".into(), binds.into());
    }

    if !expr.texture_transform_binds.is_empty() {
        let binds: Vec<serde_json::Value> = expr
            .texture_transform_binds
            .iter()
            .map(|b| {
                serde_json::json!({
                    "material": b.material_index,
                    "offset": b.offset,
                    "scale": b.scale,
                })
            })
            .collect();
        obj.insert("textureTransformBinds".into(), binds.into());
    }

    let override_str = |o: Option<OverrideType>| -> Option<&'static str> {
        match o? {
            OverrideType::None => Some("none"),
            OverrideType::Block => Some("block"),
            OverrideType::Blend => Some("blend"),
        }
    };
    if let Some(s) = override_str(expr.override_blink) {
        obj.insert("overrideBlink".into(), s.into());
    }
    if let Some(s) = override_str(expr.override_look_at) {
        obj.insert("overrideLookAt".into(), s.into());
    }
    if let Some(s) = override_str(expr.override_mouth) {
        obj.insert("overrideMouth".into(), s.into());
    }

    serde_json::Value::Object(obj)
}

/// Build VRMC_springBone extension JSON.
fn build_vrmc_spring_bone(doc: &VrmDocument) -> serde_json::Value {
    let mut sb = serde_json::Map::new();

    // Colliders
    if !doc.spring_bone_colliders.is_empty() {
        let colliders: Vec<serde_json::Value> = doc.spring_bone_colliders.iter().map(|c| {
            let shape = match &c.shape {
                ColliderShape::Sphere { offset, radius } => {
                    serde_json::json!({ "sphere": { "offset": offset, "radius": radius } })
                }
                ColliderShape::Capsule { offset, tail, radius } => {
                    serde_json::json!({ "capsule": { "offset": offset, "tail": tail, "radius": radius } })
                }
            };
            serde_json::json!({ "node": c.node, "shape": shape })
        }).collect();
        sb.insert("colliders".into(), colliders.into());
    }

    // Collider groups
    if !doc.spring_bone_collider_groups.is_empty() {
        let groups: Vec<serde_json::Value> = doc
            .spring_bone_collider_groups
            .iter()
            .map(|g| {
                let mut obj = serde_json::Map::new();
                if let Some(ref name) = g.name {
                    obj.insert("name".into(), name.clone().into());
                }
                obj.insert(
                    "colliders".into(),
                    g.colliders
                        .iter()
                        .map(|&c| serde_json::Value::Number(c.into()))
                        .collect::<Vec<_>>()
                        .into(),
                );
                serde_json::Value::Object(obj)
            })
            .collect();
        sb.insert("colliderGroups".into(), groups.into());
    }

    // Springs
    if !doc.spring_bones.is_empty() {
        let springs: Vec<serde_json::Value> = doc.spring_bones.iter().map(|chain| {
            let mut obj = serde_json::Map::new();
            if let Some(ref name) = chain.name {
                obj.insert("name".into(), name.clone().into());
            }
            let joints: Vec<serde_json::Value> = chain.joints.iter().map(|j| {
                serde_json::json!({
                    "node": j.node,
                    "hitRadius": j.hit_radius,
                    "stiffness": j.stiffness,
                    "gravityPower": j.gravity_power,
                    "gravityDir": { "x": j.gravity_dir[0], "y": j.gravity_dir[1], "z": j.gravity_dir[2] },
                    "dragForce": j.drag_force,
                })
            }).collect();
            obj.insert("joints".into(), joints.into());
            if !chain.collider_groups.is_empty() {
                obj.insert("colliderGroups".into(), chain.collider_groups.iter().map(|&cg| serde_json::Value::Number(cg.into())).collect::<Vec<_>>().into());
            }
            if let Some(center) = chain.center {
                obj.insert("center".into(), serde_json::Value::Number(center.into()));
            }
            serde_json::Value::Object(obj)
        }).collect();
        sb.insert("springs".into(), springs.into());
    }

    serde_json::Value::Object(sb)
}

/// Build per-material VRMC_materials_mtoon extension.
fn build_mtoon_extension(mtoon: &VrmMtoonMaterial) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("specVersion".into(), "1.0".into());
    obj.insert(
        "shadeColorFactor".into(),
        mtoon.shade_color_factor.to_vec().into(),
    );
    obj.insert(
        "shadingShiftFactor".into(),
        mtoon.shading_shift_factor.into(),
    );
    obj.insert(
        "shadingToonyFactor".into(),
        mtoon.shading_toony_factor.into(),
    );
    obj.insert(
        "giEqualizationFactor".into(),
        mtoon.gi_equalization_factor.into(),
    );
    obj.insert(
        "parametricRimColorFactor".into(),
        mtoon.parametric_rim_color_factor.to_vec().into(),
    );
    obj.insert(
        "rimLightingMixFactor".into(),
        mtoon.rim_lighting_mix_factor.into(),
    );
    obj.insert(
        "parametricRimFresnelPowerFactor".into(),
        mtoon.rim_fresnel_power_factor.into(),
    );
    obj.insert(
        "parametricRimLiftFactor".into(),
        mtoon.rim_lift_factor.into(),
    );

    let owm = match mtoon.outline_width_mode {
        OutlineWidthMode::None => "none",
        OutlineWidthMode::WorldCoordinates => "worldCoordinates",
        OutlineWidthMode::ScreenCoordinates => "screenCoordinates",
    };
    obj.insert("outlineWidthMode".into(), owm.into());
    obj.insert(
        "outlineWidthFactor".into(),
        mtoon.outline_width_factor.into(),
    );
    obj.insert(
        "outlineColorFactor".into(),
        mtoon.outline_color_factor.to_vec().into(),
    );
    obj.insert(
        "outlineLightingMixFactor".into(),
        mtoon.outline_lighting_mix_factor.into(),
    );
    obj.insert(
        "renderQueueOffsetNumber".into(),
        mtoon.render_queue_offset.into(),
    );
    obj.insert(
        "transparentWithZWrite".into(),
        mtoon.transparent_with_z_write.into(),
    );

    if let Some(tex) = mtoon.shade_multiply_texture {
        obj.insert(
            "shadeMultiplyTexture".into(),
            serde_json::json!({ "index": tex }),
        );
    }
    if let Some(tex) = mtoon.rim_multiply_texture {
        obj.insert(
            "rimMultiplyTexture".into(),
            serde_json::json!({ "index": tex }),
        );
    }
    if let Some(tex) = mtoon.matcap_texture {
        obj.insert("matcapTexture".into(), serde_json::json!({ "index": tex }));
    }

    serde_json::Value::Object(obj)
}

/// Build per-node VRMC_node_constraint extension.
fn build_node_constraint(nc: &VrmNodeConstraint) -> serde_json::Value {
    let constraint = match &nc.constraint {
        ConstraintType::Aim {
            source,
            aim_axis,
            weight,
        } => {
            serde_json::json!({ "aim": { "source": source, "aimAxis": aim_axis, "weight": weight } })
        }
        ConstraintType::Rotation { source, weight } => {
            serde_json::json!({ "rotation": { "source": source, "weight": weight } })
        }
        ConstraintType::Roll {
            source,
            roll_axis,
            weight,
        } => {
            serde_json::json!({ "roll": { "source": source, "rollAxis": roll_axis, "weight": weight } })
        }
    };
    serde_json::json!({ "constraint": constraint })
}
