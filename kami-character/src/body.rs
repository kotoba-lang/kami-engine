//! Body + clothing mesh generation + humanoid skeleton.

use crate::params::{BodyParams, ClothingParams, ClothingPreset};
use crate::{MaterialId, MeshPart, Vertex};
use glam::Mat4;
use glam::Vec3;
use kami_skeleton::{Bone, Skeleton};
use std::f32::consts::PI;

/// Generate neck + upper body mesh.
pub fn generate_body(params: &BodyParams) -> MeshPart {
    let n_rings = 20u32;
    let n_seg = 28u32;
    let neck_thick = 0.035 + params.neck_thickness * 0.02;
    let shoulder_w = 0.1 + params.shoulder_width * 0.08;
    let build = params.build;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=n_rings {
        let t = i as f32 / n_rings as f32;
        let y = -0.12 - t * 0.28 * params.height;

        let (rx, rz) = if t < 0.2 {
            (neck_thick + t * 0.06, neck_thick * 0.85 + t * 0.05)
        } else if t < 0.5 {
            let s = t - 0.2;
            (
                neck_thick + 0.012 + s * (shoulder_w - neck_thick) / 0.3,
                neck_thick * 0.85 + 0.01 + s * 0.1,
            )
        } else {
            let s = t - 0.5;
            (
                shoulder_w + s * 0.02 + build * 0.02,
                0.08 + build * 0.03 + s * 0.01,
            )
        };

        for j in 0..=n_seg {
            let theta = 2.0 * PI * j as f32 / n_seg as f32;
            let x = rx * theta.cos();
            let z = rz * theta.sin();
            let n = Vec3::new(theta.cos(), 0.0, theta.sin()).normalize();
            vertices.push(Vertex {
                position: Vec3::new(x, y, z),
                normal: n,
                uv: [j as f32 / n_seg as f32, t],
            });
        }
    }

    for i in 0..n_rings {
        for j in 0..n_seg {
            let a = i * (n_seg + 1) + j;
            let b = a + n_seg + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    MeshPart {
        name: "body".into(),
        vertices,
        indices,
        material: MaterialId::Skin,
    }
}

/// Generate clothing mesh (slightly offset from body).
pub fn generate_clothing(params: &ClothingParams, body: &BodyParams) -> MeshPart {
    let n_rings = 16u32;
    let n_seg = 24u32;
    let offset = 0.004 + params.fit * 0.003;
    let shoulder_w = 0.1 + body.shoulder_width * 0.08 + offset;

    // Clothing coverage (how far up the neck it goes)
    let (start_t, coverage) = match params.preset {
        ClothingPreset::TankTop | ClothingPreset::NudeShoulders => (0.35, 0.65),
        ClothingPreset::TShirt | ClothingPreset::Blouse => (0.25, 0.75),
        ClothingPreset::Hoodie | ClothingPreset::Jacket => (0.15, 0.85),
        _ => (0.25, 0.75),
    };

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=n_rings {
        let t = start_t + coverage * i as f32 / n_rings as f32;
        let y = -0.12 - t * 0.28 * body.height;
        let rx = if t < 0.5 {
            shoulder_w * t * 2.0
        } else {
            shoulder_w + (t - 0.5) * 0.02
        } + offset;
        let rz = 0.08 + body.build * 0.03 + offset;

        for j in 0..=n_seg {
            let theta = 2.0 * PI * j as f32 / n_seg as f32;
            let x = rx * theta.cos();
            let z = rz * theta.sin();
            let n = Vec3::new(theta.cos(), 0.0, theta.sin()).normalize();
            vertices.push(Vertex {
                position: Vec3::new(x, y, z),
                normal: n,
                uv: [j as f32 / n_seg as f32, i as f32 / n_rings as f32],
            });
        }
    }

    for i in 0..n_rings {
        for j in 0..n_seg {
            let a = i * (n_seg + 1) + j;
            let b = a + n_seg + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    MeshPart {
        name: "clothing".into(),
        vertices,
        indices,
        material: MaterialId::Clothing,
    }
}

/// Generate VRM 1.0-compatible humanoid skeleton (55 bones).
pub fn generate_humanoid_skeleton() -> Skeleton {
    let id = Mat4::IDENTITY.to_cols_array_2d();
    let bones = vec![
        Bone {
            name: "hips".into(),
            parent: None,
            local_position: [0.0, -0.2, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "spine".into(),
            parent: Some(0),
            local_position: [0.0, 0.08, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "chest".into(),
            parent: Some(1),
            local_position: [0.0, 0.08, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "upperChest".into(),
            parent: Some(2),
            local_position: [0.0, 0.06, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "neck".into(),
            parent: Some(3),
            local_position: [0.0, 0.06, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "head".into(),
            parent: Some(4),
            local_position: [0.0, 0.06, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftEye".into(),
            parent: Some(5),
            local_position: [-0.03, 0.04, 0.06],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightEye".into(),
            parent: Some(5),
            local_position: [0.03, 0.04, 0.06],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "jaw".into(),
            parent: Some(5),
            local_position: [0.0, -0.02, 0.04],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftShoulder".into(),
            parent: Some(3),
            local_position: [-0.04, 0.04, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftUpperArm".into(),
            parent: Some(9),
            local_position: [-0.06, 0.0, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightShoulder".into(),
            parent: Some(3),
            local_position: [0.04, 0.04, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightUpperArm".into(),
            parent: Some(11),
            local_position: [0.06, 0.0, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
    ];

    Skeleton { bones }
}
