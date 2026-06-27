//! PBR material definitions for character parts.

use crate::params::{ClothingParams, EyeParams, HairParams, MouthParams, SkinParams};
use crate::MaterialId;

/// PBR material properties for GLB export.
#[derive(Debug, Clone)]
pub struct PbrMaterial {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    /// Subsurface scattering strength (KAMI PBR extension).
    pub subsurface: f32,
    pub subsurface_color: [f32; 3],
    /// Anisotropic strength (for hair).
    pub anisotropic: f32,
    /// Clearcoat (for eyes).
    pub clearcoat: f32,
    pub clearcoat_roughness: f32,
    /// Emission RGB.
    pub emission: [f32; 3],
}

impl PbrMaterial {
    /// Generate material for a character part from parameters.
    pub fn for_part(
        id: MaterialId,
        skin: &SkinParams,
        eyes: &EyeParams,
        mouth: &MouthParams,
        hair: &HairParams,
        clothing: &ClothingParams,
    ) -> Self {
        match id {
            MaterialId::Skin => Self {
                name: "skin".into(),
                base_color: [skin.tone[0], skin.tone[1], skin.tone[2], 1.0],
                metallic: 0.0,
                roughness: skin.roughness,
                subsurface: skin.subsurface * 0.8,
                subsurface_color: [0.9, 0.5, 0.35],
                anisotropic: 0.0,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                emission: [0.0; 3],
            },
            MaterialId::EyeWhite => Self {
                name: "eye_white".into(),
                base_color: [0.97, 0.97, 0.97, 1.0],
                metallic: 0.0,
                roughness: 0.15,
                subsurface: 0.3,
                subsurface_color: [0.95, 0.85, 0.85],
                anisotropic: 0.0,
                clearcoat: 0.4,
                clearcoat_roughness: 0.1,
                emission: [0.0; 3],
            },
            MaterialId::Iris => Self {
                name: "iris".into(),
                base_color: [
                    eyes.iris_color[0],
                    eyes.iris_color[1],
                    eyes.iris_color[2],
                    1.0,
                ],
                metallic: 0.05,
                roughness: 0.12,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.0,
                clearcoat: 0.8,
                clearcoat_roughness: 0.05,
                emission: [0.0; 3],
            },
            MaterialId::Pupil => Self {
                name: "pupil".into(),
                base_color: [0.05, 0.03, 0.02, 1.0],
                metallic: 0.0,
                roughness: 0.3,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.0,
                clearcoat: 0.9,
                clearcoat_roughness: 0.05,
                emission: [0.0; 3],
            },
            MaterialId::Lip => Self {
                name: "lip".into(),
                base_color: [
                    mouth.lip_color[0],
                    mouth.lip_color[1],
                    mouth.lip_color[2],
                    1.0,
                ],
                metallic: 0.0,
                roughness: 0.3,
                subsurface: 0.5,
                subsurface_color: [0.9, 0.4, 0.3],
                anisotropic: 0.0,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                emission: [0.0; 3],
            },
            MaterialId::Eyebrow => Self {
                name: "eyebrow".into(),
                base_color: [
                    hair.color[0] * 0.7,
                    hair.color[1] * 0.6,
                    hair.color[2] * 0.5,
                    1.0,
                ],
                metallic: 0.0,
                roughness: 0.7,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.0,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                emission: [0.0; 3],
            },
            MaterialId::Hair => Self {
                name: "hair".into(),
                base_color: [hair.color[0], hair.color[1], hair.color[2], 1.0],
                metallic: 0.05 + hair.shininess * 0.1,
                roughness: 0.25 + (1.0 - hair.shininess) * 0.25,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.6 + hair.shininess * 0.3,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                emission: [0.0; 3],
            },
            MaterialId::Clothing => Self {
                name: "clothing".into(),
                base_color: [clothing.color[0], clothing.color[1], clothing.color[2], 1.0],
                metallic: 0.0,
                roughness: 0.55,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.0,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                emission: [0.0; 3],
            },
            MaterialId::Eyelash => Self {
                name: "eyelash".into(),
                base_color: [0.08, 0.06, 0.04, 1.0],
                metallic: 0.0,
                roughness: 0.5,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.3,
                clearcoat: 0.0,
                clearcoat_roughness: 0.0,
                emission: [0.0; 3],
            },
        }
    }
}
