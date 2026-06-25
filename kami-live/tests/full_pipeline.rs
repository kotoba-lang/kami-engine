//! Cross-crate integration: the committed dance scene → render-IR → realised
//! into each domain crate's structs (ADR-0043/0044). Proves the data→realizer
//! pipeline holds together across kami-live / kami-webgpu-rs / kami-vrm, and that
//! `:dance/avatar :expressions` is authored in clj/edn and resolves to morphs.

use kami_live::scene::DanceScene;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

#[test]
fn dance_scene_realises_render_ir() {
    let mut scene = DanceScene::from_edn(SCENE).expect("reference scene loads");
    scene.show.start();
    // run a couple of seconds so the show is mid-set.
    let mut frame = scene.frame(1.0 / 30.0);
    for _ in 0..60 {
        frame = scene.frame(1.0 / 30.0);
    }
    let ir = kami_webgpu_rs::parse_render_ir(&frame.render_ir_edn());
    assert!(ir.lights.len() >= 3, "lighting rig → lights");
    assert!(!ir.materials.is_empty(), "performer material realised");
    assert!(!ir.meshes.is_empty(), "the VRM avatar mesh realised");
}

#[test]
fn avatar_clip_emits_animations_layer() {
    // The avatar's `:clip` projects into a render-IR `:animations` layer driven by
    // show time (ADR-0044 §4), so a host (native or web CLJS) can evaluate the
    // authored `:dance/clips` clip without any per-frame animation authoring.
    let mut scene = DanceScene::from_edn(SCENE).expect("scene");
    scene.show.start();
    let _ = scene.frame(1.0 / 30.0);
    let edn = scene.frame(1.0 / 30.0).render_ir_edn();
    assert!(edn.contains(":animations"), "avatar :clip → render-IR :animations layer");
    assert!(edn.contains("idle"), "the named clip ('idle') is referenced");
}

#[test]
fn dance_clip_realises_onto_skeleton() {
    // `:dance/clips` is authored as EDN and realised into a kami-skeleton
    // AnimationClip via the kami-skeleton-scene tier (bone names → indices),
    // so one authored clip retargets onto any skeleton (ADR-0044 §4).
    let scene = DanceScene::from_edn(SCENE).expect("scene");
    let clip_edn = kotoba_edn::to_string(&scene.clips[0]);
    let bone_index = |n: &str| match n {
        "hips" => Some(0usize),
        "spine" => Some(1),
        _ => None,
    };
    let clip = kami_skeleton_scene::clip_from_edn(&clip_edn, bone_index).expect("clip realises");
    assert_eq!(clip.name, "idle");
    assert_eq!(clip.tracks.len(), 2, "spine + hips tracks resolve to bone indices");
}

#[test]
fn dance_stage_props_dressed_into_instances() {
    // :dance/stage props (LED wall / risers / truss / speakers) become render-IR
    // :instances alongside the performer + crowd — the venue is dressed from data.
    let scene = DanceScene::from_edn(SCENE).expect("scene");
    assert!(scene.stage.len() >= 6, "stage props parsed: {}", scene.stage.len());
    assert!(scene.stage.iter().any(|p| p.kind == "led-wall" && p.emissive > 0.0),
        "the LED wall is a self-lit prop");
}

#[test]
fn dance_post_chain_realises_into_effects() {
    // `:dance/post` is authored as EDN and realised into kami-postfx structs via
    // the kami-postfx-scene authoring tier (the `:effect` ids match across crates).
    let mut scene = DanceScene::from_edn(SCENE).expect("scene");
    scene.show.start();
    let ir_edn = scene.frame(1.0 / 30.0).render_ir_edn();
    // the render-IR `:post` chain realises into a kami-postfx pipeline in one call.
    let pipeline = kami_postfx_scene::chain_from_render_ir(&ir_edn);
    assert_eq!(pipeline.effects.len(), 3, "bloom + color-grade + vignette realised from :dance/post");
    assert!(matches!(pipeline.effects[0], kami_postfx::PostEffect::Bloom { .. }), "first fx is bloom");
}

#[test]
fn camera_rig_authored_in_edn() {
    // `:dance/camera` authors the eye/look offset + fov framing the performer,
    // and the rig flows into the render-IR `:camera` (eye follows the dancer).
    let mut scene = DanceScene::from_edn(SCENE).expect("scene");
    assert!((scene.camera.offset.z - 8.0).abs() < 1e-6, ":dance/camera :offset parsed");
    assert!((scene.camera.fov - 0.9).abs() < 1e-6, ":dance/camera :fov parsed");
    scene.show.start();
    let edn = scene.frame(1.0 / 30.0).render_ir_edn();
    assert!(edn.contains(":camera"), "render-IR carries the :camera rig");
}

#[test]
fn avatar_expressions_authored_in_edn() {
    let scene = DanceScene::from_edn(SCENE).expect("scene");
    // `:dance/avatar :expressions` declares show→VRM-expression drives.
    let names: Vec<&str> = scene.avatar.expressions.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"happy") && names.contains(&"aa") && names.contains(&"blink"),
        "happy/aa/blink drives present (authored or defaulted): {names:?}");
}

#[test]
fn expression_weights_resolve_into_morphs() {
    let scene = DanceScene::from_edn(SCENE).expect("scene");

    // Loud cheer at the start of a beat → happy (cheer) + aa (beat) lit.
    let w = scene.avatar.expression_weights(30.0, 0.5, 1.0);
    assert!(*w.get("happy").unwrap_or(&0.0) > 0.0, "cheer drives :happy from EDN");
    assert!(*w.get("aa").unwrap_or(&0.0) > 0.0, "mid-beat drives lip-sync :aa from EDN");

    // mid-pulse of the periodic blink (peaks ~0.06 s into each 3 s cycle).
    let wb = scene.avatar.expression_weights(0.0, 0.0, 0.06);
    assert!(*wb.get("blink").unwrap_or(&0.0) > 0.0, "periodic blink pulse fires");

    // The weights feed kami-vrm's ExpressionManager → morph targets.
    use kami_vrm::expression::ExpressionManager;
    let exprs = scene_test_expressions();
    let mgr = ExpressionManager::new(&exprs);
    let resolved = mgr.resolve(&w);
    let total: f32 = resolved.morphs.values().sum();
    assert!(total > 0.0, "EDN-driven weights resolve into VRM morph targets");
}

/// Minimal VRM expressions (happy/aa/blink → morph 0/1/2 on mesh 0) for the
/// resolve check — independent of any loaded asset.
fn scene_test_expressions() -> Vec<kami_vrm::vrm_types::VrmExpression> {
    use kami_vrm::vrm_types::{ExpressionPreset, MorphTargetBind, VrmExpression};
    ["happy", "aa", "blink"]
        .iter()
        .enumerate()
        .map(|(i, name)| VrmExpression {
            name: (*name).into(),
            preset: ExpressionPreset::from_str(name),
            is_binary: false,
            morph_target_binds: vec![MorphTargetBind { mesh_index: 0, morph_index: i, weight: 1.0 }],
            material_color_binds: vec![],
            texture_transform_binds: vec![],
            override_blink: None,
            override_look_at: None,
            override_mouth: None,
        })
        .collect()
}
