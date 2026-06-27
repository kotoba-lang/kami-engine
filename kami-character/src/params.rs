//! Character definition parameters — continuous parametric values.
//! Maps 1:1 to WIT `gftd:kami/character-maker` types.

use serde::{Deserialize, Serialize};

/// Face shape parameters (0.0–1.0 normalized).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceShapeParams {
    pub jaw_width: f32,
    pub jaw_length: f32,
    pub chin_shape: f32,
    pub cheekbone_width: f32,
    pub cheekbone_height: f32,
    pub forehead_height: f32,
    pub forehead_width: f32,
    pub temple_width: f32,
    pub face_length: f32,
}

/// Eye parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EyeParams {
    pub size: f32,
    pub width: f32,
    pub height: f32,
    pub spacing: f32,
    pub tilt: f32,
    pub depth: f32,
    pub iris_size: f32,
    pub iris_color: [f32; 3],
}

/// Nose parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoseParams {
    pub length: f32,
    pub width: f32,
    pub bridge_height: f32,
    pub tip_shape: f32,
    pub tip_angle: f32,
    pub nostril_width: f32,
}

/// Mouth/lip parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouthParams {
    pub width: f32,
    pub upper_lip_thickness: f32,
    pub lower_lip_thickness: f32,
    pub corner_angle: f32,
    pub philtrum_depth: f32,
    pub lip_color: [f32; 3],
}

/// Eyebrow parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowParams {
    pub thickness: f32,
    pub arch_height: f32,
    pub spacing: f32,
    pub angle: f32,
    pub color: [f32; 3],
}

/// Skin parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinParams {
    pub tone: [f32; 3],
    pub roughness: f32,
    pub subsurface: f32,
    pub freckles: f32,
    pub blemishes: f32,
}

/// Hair preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HairPreset {
    ShortStraight,
    ShortWavy,
    ShortCurly,
    MediumStraight,
    MediumWavy,
    MediumLayered,
    LongStraight,
    LongWavy,
    LongCurly,
    PonytailHigh,
    PonytailLow,
    BunTop,
    BunLow,
    Bob,
    Pixie,
    Buzz,
    Undercut,
    Mohawk,
    AfroShort,
    AfroLarge,
    BraidsTwin,
    BraidsSingle,
    Bald,
}

/// Hair parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HairParams {
    pub preset: HairPreset,
    pub color: [f32; 3],
    pub highlight_color: Option<[f32; 3]>,
    pub length_scale: f32,
    pub volume: f32,
    pub part_position: f32,
    pub shininess: f32,
}

/// Clothing preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClothingPreset {
    TankTop,
    TShirt,
    Blouse,
    Hoodie,
    Jacket,
    DressCasual,
    DressFormal,
    SuitCasual,
    SuitFormal,
    UniformSchool,
    UniformMilitary,
    NudeShoulders,
}

/// Clothing parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClothingParams {
    pub preset: ClothingPreset,
    pub color: [f32; 3],
    pub secondary_color: Option<[f32; 3]>,
    pub fit: f32,
}

/// Body parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyParams {
    pub height: f32,
    pub shoulder_width: f32,
    pub build: f32,
    pub neck_thickness: f32,
}

/// Complete character definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDef {
    pub face: FaceShapeParams,
    pub eyes: EyeParams,
    pub nose: NoseParams,
    pub mouth: MouthParams,
    pub brows: BrowParams,
    pub skin: SkinParams,
    pub hair: HairParams,
    pub clothing: ClothingParams,
    pub body: BodyParams,
}

impl Default for CharacterDef {
    /// Default: young feminine character matching the reference photo.
    fn default() -> Self {
        Self {
            face: FaceShapeParams {
                jaw_width: 0.4,
                jaw_length: 0.5,
                chin_shape: 0.3,
                cheekbone_width: 0.55,
                cheekbone_height: 0.6,
                forehead_height: 0.5,
                forehead_width: 0.5,
                temple_width: 0.5,
                face_length: 0.6,
            },
            eyes: EyeParams {
                size: 0.7,
                width: 0.6,
                height: 0.55,
                spacing: 0.5,
                tilt: 0.1,
                depth: 0.5,
                iris_size: 0.8,
                iris_color: [0.45, 0.65, 0.85],
            },
            nose: NoseParams {
                length: 0.4,
                width: 0.35,
                bridge_height: 0.45,
                tip_shape: 0.6,
                tip_angle: 0.55,
                nostril_width: 0.35,
            },
            mouth: MouthParams {
                width: 0.55,
                upper_lip_thickness: 0.5,
                lower_lip_thickness: 0.55,
                corner_angle: 0.15,
                philtrum_depth: 0.5,
                lip_color: [0.85, 0.62, 0.62],
            },
            brows: BrowParams {
                thickness: 0.35,
                arch_height: 0.5,
                spacing: 0.5,
                angle: 0.1,
                color: [0.65, 0.55, 0.42],
            },
            skin: SkinParams {
                tone: [0.94, 0.87, 0.82],
                roughness: 0.45,
                subsurface: 0.6,
                freckles: 0.0,
                blemishes: 0.0,
            },
            hair: HairParams {
                preset: HairPreset::LongStraight,
                color: [0.92, 0.85, 0.70],
                highlight_color: Some([0.95, 0.90, 0.78]),
                length_scale: 1.0,
                volume: 0.6,
                part_position: 0.5,
                shininess: 0.5,
            },
            clothing: ClothingParams {
                preset: ClothingPreset::TankTop,
                color: [0.95, 0.95, 0.95],
                secondary_color: None,
                fit: 0.5,
            },
            body: BodyParams {
                height: 1.0,
                shoulder_width: 0.4,
                build: 0.3,
                neck_thickness: 0.35,
            },
        }
    }
}

impl CharacterDef {
    /// Create from VL analysis JSON.
    pub fn from_vl_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}
