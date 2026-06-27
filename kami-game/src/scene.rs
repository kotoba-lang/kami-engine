//! Island scene definition: JSON-LD format for LLM generation + Engine loading.
//!
//! LLM generates this JSON-LD → save-scene API → R2 → Engine loads and spawns.
//! JSON-LD fields (`@context`, `@type`, `@id`) are optional for backward compat.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Complete Island scene definition (JSON-LD compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IslandScene {
    #[serde(rename = "@context", default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(rename = "@type", default, skip_serializing_if = "Option::is_none")]
    pub ld_type: Option<String>,
    #[serde(rename = "@id", default, skip_serializing_if = "Option::is_none")]
    pub ld_id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_players: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<CharacterDef>,
    pub entities: Vec<EntityDef>,
    pub ambient_color: [f32; 3],
    pub sun_direction: [f32; 3],
    pub sun_intensity: f32,
    /// Sun color override (default: white [1,1,1]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun_color: Option<[f32; 3]>,
    /// Camera projection mode: "perspective" | "orthographic-side" | "orthographic-top".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_mode: Option<String>,
    /// Parallax layers for 2D side-scroll mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<SceneLayer>,
    /// Viewport configuration for 2D mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<SceneViewport>,
    /// Point lights for local illumination (stage spots, rim lights, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point_lights: Vec<PointLightDef>,
    /// Volumetric fog / atmospheric scattering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atmosphere: Option<AtmosphereDef>,
    /// Post-processing pipeline preset: "nintendo" | "retro" | "final_fantasy" | "baminiku_character".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postfx_preset: Option<String>,
    /// Image-Based Lighting (IBL) environment map key (R2 blob).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ibl_env_map: Option<String>,
    /// Shadow configuration override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<ShadowDef>,
}

/// Character definition for parametric Mii-style avatars (JSON-LD).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDef {
    #[serde(rename = "@type", default, skip_serializing_if = "Option::is_none")]
    pub ld_type: Option<String>,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub appearance: CharacterAppearance,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawn_points: Vec<String>,
}

/// Parametric character appearance matching `gftd:kami/character` WIT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAppearance {
    pub face: String,
    pub skin_hue: f32,
    pub skin_lightness: f32,
    pub eye: String,
    pub eye_color_hue: f32,
    pub eye_size: f32,
    pub nose: String,
    pub mouth: String,
    pub mouth_size: f32,
    pub hair: String,
    pub hair_color_hue: f32,
    pub hair_color_lightness: f32,
    pub body: String,
    pub height: f32,
    pub accessory1: String,
    pub accessory2: String,
}

/// One entity in the scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    pub id: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4], // quaternion xyzw
    pub scale: [f32; 3],
    pub mesh: MeshRef,
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    /// Layer name for 2D side-scroll parallax assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

/// Mesh reference: built-in primitive or AssetHub GLB.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MeshRef {
    #[serde(rename = "cube")]
    Cube { color: [f32; 4] },
    #[serde(rename = "sphere")]
    Sphere { color: [f32; 4], radius: f32 },
    #[serde(rename = "asset")]
    Asset { asset_id: String, blob_key: String },
    #[serde(rename = "plane")]
    Plane {
        color: [f32; 4],
        width: f32,
        depth: f32,
        subdivisions: u32,
    },
    #[serde(rename = "voxel")]
    Voxel {
        chunk_data: Vec<u8>,
        palette: Vec<[f32; 4]>,
    },
    #[serde(rename = "terrain")]
    Terrain {
        heightmap: Vec<f32>,
        width: u32,
        depth: u32,
        height_scale: f32,
    },
    #[serde(rename = "gaussian_splat")]
    GaussianSplat { splat_key: String },
    #[serde(rename = "cylinder")]
    Cylinder {
        color: [f32; 4],
        h: f32,
        r1: f32,
        r2: f32,
    },
    #[serde(rename = "scad")]
    Scad { code: String },
    /// Hexagonal prism for H3 grid visualization (maps.etzhayyim.com).
    #[serde(rename = "hex_prism")]
    HexPrism {
        color: [f32; 4],
        radius: f32,
        height: f32,
    },
    /// Hex grid: multiple hex prisms arranged in H3-style rings.
    #[serde(rename = "hex_grid")]
    HexGrid {
        color: [f32; 4],
        rings: u32,
        hex_radius: f32,
        hex_height: f32,
        spacing: f32,
    },
    /// Cylinder pipe for infrastructure rendering (water/gas/electric).
    /// `thickness` = 0 for solid, >0 for hollow pipe cross-section.
    #[serde(rename = "pipe")]
    Pipe {
        color: [f32; 4],
        radius: f32,
        thickness: f32,
        height: f32,
        segments: u32,
    },
    /// Building extrusion from 2D footprint polygon (maps.etzhayyim.com).
    #[serde(rename = "building")]
    Building {
        color: [f32; 4],
        footprint: Vec<[f32; 2]>,
        height: f32,
    },
    /// High-poly character model (GLB/VRM from R2 CDN).
    /// Uses MaterialUniform SSS/hair/eye presets per sub-mesh.
    #[serde(rename = "character_model")]
    CharacterModel {
        /// R2 blob key for GLB/VRM model file.
        blob_key: String,
        /// Material overrides per sub-mesh name.
        #[serde(default)]
        material_overrides: Vec<MaterialOverrideDef>,
    },
    /// SDF character body: smooth union of capsules/spheres for procedural generation.
    /// Marching cubes at runtime → high-poly mesh with SSS materials.
    #[serde(rename = "sdf_character")]
    SdfCharacter {
        /// SDF body parts as JSON array of {prim, transform, color, material_preset}.
        body_parts: Vec<SdfBodyPartDef>,
        /// Marching cubes resolution (32=fast, 128=high quality, 256=cinematic).
        resolution: u32,
    },
}

/// Component attached to an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ComponentDef {
    #[serde(rename = "player_spawn")]
    PlayerSpawn,
    #[serde(rename = "npc")]
    Npc {
        name: String,
        waypoints: Vec<[f32; 3]>,
    },
    #[serde(rename = "portal")]
    Portal { target_island: String },
    #[serde(rename = "item")]
    Item { item_id: String, item_name: String },
    #[serde(rename = "physics")]
    Physics { dynamic: bool },
    #[serde(rename = "trigger")]
    Trigger { kind: String, data: String },
}

/// A depth layer for parallax scrolling in 2D side-scroll mode.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneLayer {
    pub name: String,
    #[serde(default)]
    pub z: f32,
    #[serde(default = "default_parallax")]
    pub parallax: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Point light for local illumination (stage spots, rim lights, accent lights).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointLightDef {
    pub id: String,
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    /// Attenuation range in world units.
    pub range: f32,
    /// Inner/outer cone angles in radians (for spot lights). Both 0 = point light.
    #[serde(default)]
    pub inner_cone: f32,
    #[serde(default)]
    pub outer_cone: f32,
    /// Direction for spot lights (normalized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f32; 3]>,
    /// Cast shadows from this light.
    #[serde(default)]
    pub cast_shadow: bool,
}

/// Atmospheric scattering / volumetric fog configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphereDef {
    /// Fog color (RGB).
    pub fog_color: [f32; 3],
    /// Fog density (0=clear, 0.01=light haze, 0.05=thick fog).
    pub fog_density: f32,
    /// Height-based fog: fog is denser below this Y coordinate.
    #[serde(default)]
    pub fog_height: f32,
    /// Height fog falloff rate.
    #[serde(default = "default_fog_falloff")]
    pub fog_height_falloff: f32,
    /// Volumetric light scattering intensity (god rays).
    #[serde(default)]
    pub volumetric_intensity: f32,
    /// Skybox gradient top color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skybox_top: Option<[f32; 3]>,
    /// Skybox gradient bottom color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skybox_bottom: Option<[f32; 3]>,
    /// Enable star field in skybox.
    #[serde(default)]
    pub skybox_stars: bool,
}

fn default_fog_falloff() -> f32 {
    2.0
}

/// Shadow map configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowDef {
    /// Shadow map resolution (1024/2048/4096).
    pub resolution: u32,
    /// Cascade count for CSM (1/2/4).
    #[serde(default = "default_cascade_count")]
    pub cascades: u32,
    /// Shadow softness (PCF filter radius, 0=hard, 3=soft).
    #[serde(default = "default_shadow_softness")]
    pub softness: f32,
    /// Shadow bias to prevent acne.
    #[serde(default = "default_shadow_bias")]
    pub bias: f32,
}

fn default_cascade_count() -> u32 {
    2
}
fn default_shadow_softness() -> f32 {
    2.0
}
fn default_shadow_bias() -> f32 {
    0.005
}

/// Material override for a sub-mesh in a GLB/VRM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialOverrideDef {
    /// Sub-mesh name in the GLB (e.g. "Face", "Hair", "Body", "Eyes", "Clothing").
    pub mesh_name: String,
    /// Material preset: "skin", "hair", "eye", "lip", "fabric".
    pub preset: String,
    /// Preset parameters (JSON object, preset-specific).
    #[serde(default)]
    pub params: serde_json::Value,
}

/// SDF body part definition for procedural character generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdfBodyPartDef {
    /// SDF primitive type: "sphere", "capsule", "cylinder", "box".
    pub primitive: String,
    /// Transform: position [x,y,z].
    pub position: [f32; 3],
    /// Rotation as quaternion [x,y,z,w].
    #[serde(default = "default_quat")]
    pub rotation: [f32; 4],
    /// Scale [x,y,z].
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    /// Primitive-specific radius/half-extents.
    #[serde(default)]
    pub radius: f32,
    /// Height for capsule/cylinder.
    #[serde(default)]
    pub height: f32,
    /// Material preset: "skin", "hair", "eye", "lip", "fabric".
    pub material_preset: String,
    /// Material parameters (preset-specific JSON).
    #[serde(default)]
    pub material_params: serde_json::Value,
    /// Smooth union blend radius with neighboring parts (0=hard, 0.1=subtle, 0.3=smooth).
    #[serde(default = "default_blend")]
    pub blend_radius: f32,
}

fn default_quat() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_blend() -> f32 {
    0.1
}

fn default_parallax() -> f32 {
    1.0
}

/// Viewport configuration for 2D mode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneViewport {
    #[serde(default = "default_viewport_width")]
    pub width: f32,
    #[serde(default = "default_viewport_height")]
    pub height: f32,
    #[serde(default = "default_ppu")]
    pub pixels_per_unit: f32,
}

fn default_viewport_width() -> f32 {
    800.0
}
fn default_viewport_height() -> f32 {
    450.0
}
fn default_ppu() -> f32 {
    32.0
}

impl IslandScene {
    /// Demo island: ground + walls + NPCs + portal + items.
    pub fn demo() -> Self {
        Self {
            context: None,
            ld_type: None,
            ld_id: None,
            name: "Hub Island".into(),
            genre: None,
            description: None,
            max_players: None,
            characters: vec![],
            ambient_color: [0.03, 0.03, 0.05],
            sun_direction: [-1.0, -2.0, -1.0],
            sun_intensity: 3.0,
            sun_color: None,
            camera_mode: None,
            layers: vec![],
            viewport: None,
            point_lights: vec![],
            atmosphere: None,
            postfx_preset: None,
            ibl_env_map: None,
            shadow: None,
            entities: vec![
                // Ground
                EntityDef {
                    id: "ground".into(),
                    position: [0.0, -0.5, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [50.0, 1.0, 50.0],
                    mesh: MeshRef::Cube {
                        color: [0.3, 0.5, 0.3, 1.0],
                    },
                    components: vec![],
                    layer: None,
                },
                // Player spawn points
                EntityDef {
                    id: "spawn-0".into(),
                    position: [0.0, 1.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    mesh: MeshRef::Cube {
                        color: [0.2, 0.6, 1.0, 1.0],
                    },
                    components: vec![
                        ComponentDef::PlayerSpawn,
                        ComponentDef::Physics { dynamic: true },
                    ],
                    layer: None,
                },
                EntityDef {
                    id: "spawn-1".into(),
                    position: [3.0, 1.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                    mesh: MeshRef::Cube {
                        color: [1.0, 0.4, 0.2, 1.0],
                    },
                    components: vec![
                        ComponentDef::PlayerSpawn,
                        ComponentDef::Physics { dynamic: true },
                    ],
                    layer: None,
                },
                // NPCs
                EntityDef {
                    id: "guard".into(),
                    position: [8.0, 0.5, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 2.0, 1.0],
                    mesh: MeshRef::Cube {
                        color: [0.8, 0.2, 0.2, 1.0],
                    },
                    components: vec![ComponentDef::Npc {
                        name: "Guard".into(),
                        waypoints: vec![[8.0, 0.5, -5.0], [8.0, 0.5, 5.0]],
                    }],
                    layer: None,
                },
                EntityDef {
                    id: "merchant".into(),
                    position: [-5.0, 0.5, 5.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 2.0, 1.0],
                    mesh: MeshRef::Cube {
                        color: [0.9, 0.7, 0.1, 1.0],
                    },
                    components: vec![ComponentDef::Npc {
                        name: "Merchant".into(),
                        waypoints: vec![[-5.0, 0.5, 5.0], [-5.0, 0.5, -3.0]],
                    }],
                    layer: None,
                },
                // Portal
                EntityDef {
                    id: "portal-sub".into(),
                    position: [15.0, 1.5, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [2.0, 3.0, 0.5],
                    mesh: MeshRef::Cube {
                        color: [0.5, 0.0, 1.0, 0.8],
                    },
                    components: vec![ComponentDef::Portal {
                        target_island: "sub-island-001".into(),
                    }],
                    layer: None,
                },
                // Items
                EntityDef {
                    id: "item-gem".into(),
                    position: [-3.0, 0.3, -2.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.5, 0.5, 0.5],
                    mesh: MeshRef::Cube {
                        color: [0.0, 0.5, 1.0, 1.0],
                    },
                    components: vec![ComponentDef::Item {
                        item_id: "gem-blue".into(),
                        item_name: "Blue Gem".into(),
                    }],
                    layer: None,
                },
                EntityDef {
                    id: "item-sword".into(),
                    position: [2.0, 0.3, 4.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.3, 1.2, 0.3],
                    mesh: MeshRef::Cube {
                        color: [0.7, 0.7, 0.7, 1.0],
                    },
                    components: vec![ComponentDef::Item {
                        item_id: "sword-iron".into(),
                        item_name: "Iron Sword".into(),
                    }],
                    layer: None,
                },
                EntityDef {
                    id: "item-potion".into(),
                    position: [-6.0, 0.3, -4.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.4, 0.6, 0.4],
                    mesh: MeshRef::Cube {
                        color: [1.0, 0.2, 0.3, 1.0],
                    },
                    components: vec![ComponentDef::Item {
                        item_id: "potion-hp".into(),
                        item_name: "Health Potion".into(),
                    }],
                    layer: None,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_roundtrip_json() {
        let scene = IslandScene::demo();
        let json = serde_json::to_string(&scene).unwrap();
        let parsed: IslandScene = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Hub Island");
        assert_eq!(parsed.entities.len(), scene.entities.len());
    }

    #[test]
    fn scene_entity_components() {
        let scene = IslandScene::demo();
        let portal = scene
            .entities
            .iter()
            .find(|e| e.id == "portal-sub")
            .unwrap();
        assert!(
            portal
                .components
                .iter()
                .any(|c| matches!(c, ComponentDef::Portal { .. }))
        );
        let guard = scene.entities.iter().find(|e| e.id == "guard").unwrap();
        assert!(
            guard
                .components
                .iter()
                .any(|c| matches!(c, ComponentDef::Npc { .. }))
        );
    }
}
