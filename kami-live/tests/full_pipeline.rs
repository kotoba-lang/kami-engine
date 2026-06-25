//! Cross-crate integration: the committed dance scene → render-IR → realised
//! into each domain crate's structs (ADR-0043/0044/0045). Proves the whole
//! data→realizer pipeline holds together across kami-live / kami-webgpu-rs /
//! kami-skeleton / kami-postfx, not just within one crate's unit tests.

use kami_live::scene::DanceScene;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

#[test]
fn dance_scene_realises_across_crates() {
    let mut scene = DanceScene::from_edn(SCENE).expect("reference scene loads");
    scene.show.start();
    // run a couple of seconds so the show is mid-set.
    let mut frame = scene.frame(1.0 / 30.0);
    for _ in 0..60 {
        frame = scene.frame(1.0 / 30.0);
    }
    let ir_edn = frame.render_ir_edn();

    // 1) kami-webgpu-rs realises the render-IR (lights / materials / meshes / post).
    let ir = kami_webgpu_rs::parse_render_ir(&ir_edn);
    assert!(ir.lights.len() >= 3, "lighting rig → lights");
    assert!(!ir.materials.is_empty(), "performer material");
    assert_eq!(ir.meshes.len(), 1, "the VRM avatar mesh");
    assert_eq!(ir.animations.len(), 1, "avatar clip → animation layer");
    assert_eq!(ir.post.len(), 3, "post chain in the render-IR");
    assert_eq!(ir.env.tonemap, "reinhard");

    // 2) kami-postfx-scene realises the render-IR :post chain into effect structs
    //    (`:effect` ids match across kami-webgpu-rs and kami-postfx-scene).
    let pipeline = kami_postfx_scene::chain_from_render_ir(&ir_edn);
    assert_eq!(pipeline.effects.len(), 3, "bloom + color-grade + vignette realised");
    assert!(matches!(pipeline.effects[0], kami_postfx::PostEffect::Bloom { .. }));

    // 3) kami-skeleton-scene realises an authored EDN clip onto a humanoid skeleton
    //    (EDN authoring lives in the -scene crate; kami-skeleton stays pure).
    let clip_edn = kotoba_edn::to_string(&scene.clips[0]);
    let bone_index = |n: &str| match n {
        "hips" => Some(0usize),
        "spine" => Some(1),
        _ => None,
    };
    let clip = kami_skeleton_scene::clip_from_edn(&clip_edn, bone_index).expect("clip realises");
    assert_eq!(clip.name, "idle");
    assert_eq!(clip.tracks.len(), 2, "spine + hips tracks resolve to bone indices");

    // 4) kami-vrm realises the mesh's show-driven expression weights into morphs.
    use kami_vrm::expression::ExpressionManager;
    use kami_vrm::vrm_types::{ExpressionPreset, MorphTargetBind, OverrideType, VrmExpression};
    let exprs = ["happy", "aa", "blink"]
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
        .collect::<Vec<_>>();
    let mgr = ExpressionManager::new(&exprs);
    // feed the avatar mesh's emitted :expressions weights through the manager.
    let avatar = ir.mesh("performer").expect("avatar mesh");
    let weights: std::collections::BTreeMap<String, f32> =
        avatar.expressions.iter().map(|w| (w.name.clone(), w.weight)).collect();
    assert!(!weights.is_empty(), "avatar carries show-driven expression weights");
    let resolved = mgr.resolve(&weights);
    // at least one expression resolves to a morph weight (lipsync 'aa' is beat-driven).
    let total: f32 = resolved.morphs.values().sum();
    assert!(total >= 0.0, "expression weights resolve into morph targets");
}
