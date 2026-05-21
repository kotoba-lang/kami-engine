//! Blendshape system — shape deformations + 52 ARKit expression targets.

use glam::Vec3;
use crate::BlendshapeTarget;
use crate::params::{FaceShapeParams, EyeParams, NoseParams, MouthParams};

/// Apply face shape blendshapes to base mesh vertices.
pub fn apply_face_shape(verts: &mut [Vec3], params: &FaceShapeParams) {
    for v in verts.iter_mut() {
        let y = v.y;
        let x_abs = v.x.abs();

        // Jaw width: scale x at lower face
        if y < 0.0 {
            let t = (-y / 0.12).min(1.0);
            let scale = 1.0 + (params.jaw_width - 0.5) * 0.3 * t;
            v.x *= scale;
        }

        // Jaw length: shift chin down
        if y < -0.06 {
            let t = ((-y - 0.06) / 0.06).min(1.0);
            v.y -= (params.jaw_length - 0.5) * 0.02 * t;
        }

        // Chin shape: pointed vs square
        if y < -0.08 {
            let t = ((-y - 0.08) / 0.04).min(1.0);
            let narrowing = (1.0 - params.chin_shape) * 0.3 * t;
            v.x *= 1.0 - narrowing * (1.0 - x_abs / 0.05).max(0.0);
        }

        // Cheekbone width
        let cheek_y = (1.0 - ((y - 0.01) / 0.03).powi(2)).max(0.0);
        if cheek_y > 0.0 {
            v.x += v.x.signum() * (params.cheekbone_width - 0.5) * 0.01 * cheek_y;
        }

        // Forehead height
        if y > 0.08 {
            let t = ((y - 0.08) / 0.04).min(1.0);
            v.y += (params.forehead_height - 0.5) * 0.015 * t;
        }

        // Face length (overall vertical scale)
        let scale_y = 1.0 + (params.face_length - 0.5) * 0.15;
        v.y *= scale_y;
    }
}

/// Apply eye shape deformations.
pub fn apply_eye_shape(verts: &mut [Vec3], params: &EyeParams) {
    for v in verts.iter_mut() {
        // Eye socket depth
        for eye_x in [-0.032_f32, 0.032] {
            let ed = ((v.x - eye_x).powi(2) + (v.y - 0.045).powi(2)).sqrt();
            if ed < 0.025 && v.z > 0.0 {
                let depth_mod = (params.depth - 0.5) * 0.005;
                v.z -= depth_mod * (1.0 - (ed / 0.025).powi(2));
            }
        }
    }
}

/// Apply nose shape deformations.
pub fn apply_nose_shape(verts: &mut [Vec3], params: &NoseParams) {
    for v in verts.iter_mut() {
        let front = (v.z / 0.1).max(0.0).min(1.0);
        // Nose length
        let nose_region = (1.0 - ((v.y - 0.01) / 0.04).powi(2)).max(0.0);
        let center = (1.0 - (v.x / 0.02).powi(2)).max(0.0);
        if nose_region > 0.0 && center > 0.0 {
            v.z += (params.bridge_height - 0.5) * 0.01 * nose_region * center * front;
        }
        // Nose width
        let tip_y = (1.0 - ((v.y + 0.005) / 0.015).powi(2)).max(0.0);
        if tip_y > 0.0 {
            v.x += v.x.signum() * (params.width - 0.5) * 0.005 * tip_y * front;
        }
    }
}

/// Apply mouth shape deformations.
pub fn apply_mouth_shape(verts: &mut [Vec3], params: &MouthParams) {
    for v in verts.iter_mut() {
        let front = (v.z / 0.08).max(0.0).min(1.0);
        let lip_y = (1.0 - ((v.y + 0.035) / 0.015).powi(2)).max(0.0);
        let lip_x = (1.0 - (v.x / 0.03).powi(2)).max(0.0);
        if lip_y > 0.0 && lip_x > 0.0 && front > 0.0 {
            // Width
            v.x += v.x.signum() * (params.width - 0.5) * 0.005 * lip_y * front;
            // Lip thickness (push forward)
            let thickness = (params.upper_lip_thickness + params.lower_lip_thickness) * 0.5;
            v.z += (thickness - 0.5) * 0.004 * lip_y * lip_x * front;
        }
    }
}

/// Generate 52 ARKit expression blendshape targets.
/// Each target contains per-vertex position deltas.
pub fn generate_arkit_targets(n_verts: usize) -> Vec<BlendshapeTarget> {
    let arkit_names = [
        "eyeBlinkLeft", "eyeBlinkRight",
        "eyeLookDownLeft", "eyeLookDownRight",
        "eyeLookInLeft", "eyeLookInRight",
        "eyeLookOutLeft", "eyeLookOutRight",
        "eyeLookUpLeft", "eyeLookUpRight",
        "eyeSquintLeft", "eyeSquintRight",
        "eyeWideLeft", "eyeWideRight",
        "jawForward", "jawLeft", "jawRight", "jawOpen",
        "mouthClose", "mouthFunnel", "mouthPucker",
        "mouthLeft", "mouthRight",
        "mouthSmileLeft", "mouthSmileRight",
        "mouthFrownLeft", "mouthFrownRight",
        "mouthDimpleLeft", "mouthDimpleRight",
        "mouthStretchLeft", "mouthStretchRight",
        "mouthRollLower", "mouthRollUpper",
        "mouthShrugLower", "mouthShrugUpper",
        "mouthPressLeft", "mouthPressRight",
        "mouthLowerDownLeft", "mouthLowerDownRight",
        "mouthUpperUpLeft", "mouthUpperUpRight",
        "browDownLeft", "browDownRight",
        "browInnerUp",
        "browOuterUpLeft", "browOuterUpRight",
        "cheekPuff", "cheekSquintLeft", "cheekSquintRight",
        "noseSneerLeft", "noseSneerRight",
        "tongueOut",
    ];

    arkit_names.iter().map(|name| {
        // Placeholder deltas (zero) — populated from captured data or procedural rules
        BlendshapeTarget {
            name: name.to_string(),
            deltas: vec![Vec3::ZERO; n_verts],
        }
    }).collect()
}
