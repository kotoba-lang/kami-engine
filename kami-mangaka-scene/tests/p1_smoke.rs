//! P1 smoke tests — lexicon coverage + scene JSON-LD roundtrip.
//!
//! VRM-bound paths (load_character, pose, tick, settle) need a synthetic GLB
//! fixture; those tests land in P2 alongside the headless render.

use kami_mangaka_scene::CameraSpec;
use kami_mangaka_scene::camera::{LightRole, LightSpec, ShotGrammar};
use kami_mangaka_scene::lexicon::{expression_preset, pose_preset};
use kami_mangaka_scene::pose::Expression;
use kami_mangaka_scene::scene::{Anchor, EnvironmentSpec, MangakaScene, Transform};

#[test]
fn pose_lexicon_covers_core_labels() {
    for label in [
        "action.rest",
        "action.idle",
        "action.dash",
        "action.run",
        "action.walk",
        "action.swing",
        "action.attack",
        "action.hit",
        "action.impact",
        "action.fall",
        "action.cower",
        "action.flinch",
        "action.shout",
        "action.yell",
        "action.point",
        "action.reach",
        "action.stand_proud",
        "action.heroic",
    ] {
        assert!(pose_preset(label).is_some(), "missing preset: {label}");
    }
}

#[test]
fn pose_lexicon_rejects_unknown() {
    assert!(pose_preset("action.flarble").is_none());
    assert!(pose_preset("").is_none());
}

#[test]
fn pose_lexicon_dash_has_arm_and_leg_rotations() {
    let preset = pose_preset("action.dash").unwrap();
    let bones: Vec<&str> = preset.iter().map(|b| b.bone.as_str()).collect();
    assert!(bones.contains(&"leftUpperArm"));
    assert!(bones.contains(&"rightUpperArm"));
    assert!(bones.contains(&"leftUpperLeg"));
    assert!(bones.contains(&"rightUpperLeg"));
}

#[test]
fn expression_lexicon_canonicalises_aliases() {
    assert!(matches!(expression_preset("happy"), Expression::Happy));
    assert!(matches!(expression_preset("JOY"), Expression::Happy));
    assert!(matches!(expression_preset("rage"), Expression::Angry));
    assert!(matches!(expression_preset("focus"), Expression::Determined));
    assert!(matches!(expression_preset("???"), Expression::Neutral));
}

#[test]
fn scene_jsonld_roundtrip_preserves_env_camera_lights() {
    let mut s = MangakaScene::new();
    s.set_background(EnvironmentSpec {
        biome: "Plains".into(),
        weather: Some("overcast".into()),
        seed: 42,
        ground_size_m: 64.0,
        layout_anchors: vec![Anchor {
            name: "tree_a".into(),
            xform: Transform::default(),
        }],
    });
    s.set_camera(CameraSpec {
        shot: ShotGrammar::Closeup,
        ..CameraSpec::default()
    });
    s.add_light(LightSpec::three_point_key());
    s.add_light(LightSpec::three_point_fill());
    s.add_light(LightSpec::three_point_rim());

    let j = s.to_jsonld();
    let s2 = MangakaScene::from_jsonld(&j).expect("roundtrip");
    let j2 = s2.to_jsonld();

    // Spot-check stable fields. Characters / props are scene-local handles and
    // intentionally not rehydrated; environment/camera/lights must round-trip.
    assert_eq!(j["environment"]["biome"], j2["environment"]["biome"]);
    assert_eq!(j["environment"]["seed"], j2["environment"]["seed"]);
    assert_eq!(j["camera"]["shot"], j2["camera"]["shot"]);
    assert_eq!(j["lights"].as_array().unwrap().len(), 3);
    assert_eq!(j2["lights"].as_array().unwrap().len(), 3);
    let roles: Vec<_> = j2["lights"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["role"].clone())
        .collect();
    assert!(roles.iter().any(|r| r == "Key"));
    assert!(roles.iter().any(|r| r == "Fill"));
    assert!(roles.iter().any(|r| r == "Rim"));
    // Suppress unused warning on LightRole import.
    let _ = LightRole::Key;
}
