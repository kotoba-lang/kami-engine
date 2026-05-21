// Pose lexicon — semantic labels ("action.dash", "expression.angry") mapped to
// VRM standard bone rotations and ARKit-style expression weights. LLM nodes in
// `lg_mangaka.compose_scene_3d` emit these labels; resolution happens here.
//
// Rotations are deliberately coarse — manga storytelling needs readable
// silhouettes, not biomechanical fidelity. Refined animation comes from the
// caller via explicit `BoneRotation` overrides in `PoseSpec.bones`.

use glam::{EulerRot, Quat};
use kami_vrm::vrm_types::HumanBoneName;

use crate::pose::{BoneRotation, Expression};

/// Resolve a semantic pose label into a coarse VRM bone rotation set.
///
/// Returns `None` when the label is unknown so the caller can fall back to
/// `PoseSpec::rest` or an explicit override list.
pub fn pose_preset(label: &str) -> Option<Vec<BoneRotation>> {
    let bones: &[(HumanBoneName, [f32; 3])] = match label {
        // Idle / rest baselines.
        "action.rest" | "rest" => &[],
        "action.idle" => &[
            (HumanBoneName::Spine, [0.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [0.0, 0.0, 70.0]),
            (HumanBoneName::RightUpperArm, [0.0, 0.0, -70.0]),
        ],

        // Locomotion.
        "action.dash" | "action.run" => &[
            (HumanBoneName::Spine, [10.0, 0.0, 0.0]),
            (HumanBoneName::Chest, [8.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [-45.0, 0.0, 75.0]),
            (HumanBoneName::LeftLowerArm, [-65.0, 0.0, 0.0]),
            (HumanBoneName::RightUpperArm, [45.0, 0.0, -75.0]),
            (HumanBoneName::RightLowerArm, [-65.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperLeg, [-30.0, 0.0, 0.0]),
            (HumanBoneName::LeftLowerLeg, [55.0, 0.0, 0.0]),
            (HumanBoneName::RightUpperLeg, [30.0, 0.0, 0.0]),
            (HumanBoneName::RightLowerLeg, [10.0, 0.0, 0.0]),
        ],
        "action.walk" => &[
            (HumanBoneName::LeftUpperArm, [-12.0, 0.0, 72.0]),
            (HumanBoneName::RightUpperArm, [12.0, 0.0, -72.0]),
            (HumanBoneName::LeftUpperLeg, [-10.0, 0.0, 0.0]),
            (HumanBoneName::RightUpperLeg, [10.0, 0.0, 0.0]),
        ],

        // Combat / action beats.
        "action.swing" | "action.attack" => &[
            (HumanBoneName::Spine, [0.0, -20.0, 0.0]),
            (HumanBoneName::Chest, [0.0, -10.0, 0.0]),
            (HumanBoneName::RightShoulder, [0.0, 0.0, -20.0]),
            (HumanBoneName::RightUpperArm, [-90.0, 0.0, -45.0]),
            (HumanBoneName::RightLowerArm, [-45.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [0.0, 0.0, 60.0]),
            (HumanBoneName::LeftLowerArm, [-30.0, 0.0, 0.0]),
        ],
        "action.hit" | "action.impact" => &[
            (HumanBoneName::Spine, [-15.0, 0.0, 0.0]),
            (HumanBoneName::Chest, [-10.0, 0.0, 0.0]),
            (HumanBoneName::Neck, [-10.0, 0.0, 0.0]),
            (HumanBoneName::Head, [-15.0, 10.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [-30.0, 0.0, 95.0]),
            (HumanBoneName::RightUpperArm, [-30.0, 0.0, -95.0]),
        ],

        // Reactions / emotion staging.
        "action.fall" => &[
            (HumanBoneName::Spine, [-30.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperLeg, [-70.0, 0.0, 0.0]),
            (HumanBoneName::RightUpperLeg, [-70.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [-60.0, 0.0, 110.0]),
            (HumanBoneName::RightUpperArm, [-60.0, 0.0, -110.0]),
        ],
        "action.cower" | "action.flinch" => &[
            (HumanBoneName::Spine, [20.0, 0.0, 0.0]),
            (HumanBoneName::Chest, [15.0, 0.0, 0.0]),
            (HumanBoneName::Neck, [12.0, 0.0, 0.0]),
            (HumanBoneName::Head, [12.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [-30.0, 0.0, 110.0]),
            (HumanBoneName::LeftLowerArm, [-95.0, 0.0, 0.0]),
            (HumanBoneName::RightUpperArm, [-30.0, 0.0, -110.0]),
            (HumanBoneName::RightLowerArm, [-95.0, 0.0, 0.0]),
        ],
        "action.shout" | "action.yell" => &[
            (HumanBoneName::Spine, [-5.0, 0.0, 0.0]),
            (HumanBoneName::Neck, [-15.0, 0.0, 0.0]),
            (HumanBoneName::Head, [-20.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [-10.0, 0.0, 110.0]),
            (HumanBoneName::RightUpperArm, [-10.0, 0.0, -110.0]),
        ],
        "action.point" => &[
            (HumanBoneName::RightShoulder, [0.0, -10.0, -5.0]),
            (HumanBoneName::RightUpperArm, [0.0, 0.0, -95.0]),
            (HumanBoneName::RightLowerArm, [0.0, 0.0, 0.0]),
            (HumanBoneName::RightIndexProximal, [0.0, 0.0, 0.0]),
        ],
        "action.reach" => &[
            (HumanBoneName::Spine, [-5.0, 0.0, 0.0]),
            (HumanBoneName::RightUpperArm, [0.0, 0.0, -110.0]),
            (HumanBoneName::RightLowerArm, [0.0, 0.0, 5.0]),
        ],
        "action.stand_proud" | "action.heroic" => &[
            (HumanBoneName::Spine, [-2.0, 0.0, 0.0]),
            (HumanBoneName::Chest, [-3.0, 0.0, 0.0]),
            (HumanBoneName::Head, [-5.0, 0.0, 0.0]),
            (HumanBoneName::LeftUpperArm, [0.0, 0.0, 78.0]),
            (HumanBoneName::RightUpperArm, [0.0, 0.0, -78.0]),
        ],

        _ => return None,
    };

    Some(
        bones
            .iter()
            .map(|(bone, euler_deg)| BoneRotation {
                bone: bone.as_str().into(),
                rotation: euler_quat(*euler_deg),
            })
            .collect(),
    )
}

/// Resolve an ARKit-style expression preset name to an [`Expression`] tag.
/// Unknown names fall back to `Expression::Neutral`.
pub fn expression_preset(name: &str) -> Expression {
    match name.to_ascii_lowercase().as_str() {
        "happy" | "joy" | "smile" => Expression::Happy,
        "angry" | "rage" => Expression::Angry,
        "sad" | "sorrow" | "grief" => Expression::Sad,
        "surprised" | "surprise" | "shock" => Expression::Surprised,
        "determined" | "resolve" | "focus" => Expression::Determined,
        "pained" | "pain" | "hurt" => Expression::Pained,
        "smirk" | "smug" => Expression::Smirk,
        _ => Expression::Neutral,
    }
}

fn euler_quat(deg: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        deg[0].to_radians(),
        deg[1].to_radians(),
        deg[2].to_radians(),
    )
}
