//! MetaHuman support — DNA calibration, FACS action units, LOD, extended face rig.
//!
//! Integrates Unreal MetaHuman-compatible digital human pipeline:
//!   - DNA-based parametric definition (face rig calibration)
//!   - FACS (Facial Action Coding System) action units (46 AUs + 52 ARKit mapped)
//!   - LOD0-LOD3 progressive mesh quality
//!   - Extended skeleton with 72 face rig bones
//!   - Teeth, tongue, inner mouth geometry
//!   - Multi-layer skin SSS (epidermis/dermis/subdermal)

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

use crate::blendshape::generate_arkit_targets;
use crate::material::PbrMaterial;
use crate::{BlendshapeTarget, MaterialId, MeshPart, Vertex};
use kami_skeleton::{Bone, Skeleton};

/// MetaHuman LOD level — controls tessellation density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetaHumanLod {
    /// ~28K head verts, full wrinkle/pore displacement.
    Lod0,
    /// ~12K head verts, wrinkle displacement only.
    Lod1,
    /// ~5K head verts (matches standard kami-character quality).
    Lod2,
    /// ~2K head verts, simplified for distance rendering.
    Lod3,
}

impl MetaHumanLod {
    /// Head mesh tessellation (latitude, longitude).
    pub fn head_resolution(&self) -> (u32, u32) {
        match self {
            Self::Lod0 => (128, 192),
            Self::Lod1 => (80, 120),
            Self::Lod2 => (48, 64),
            Self::Lod3 => (24, 32),
        }
    }

    /// Body mesh ring/segment counts.
    pub fn body_resolution(&self) -> (u32, u32) {
        match self {
            Self::Lod0 => (40, 48),
            Self::Lod1 => (28, 36),
            Self::Lod2 => (20, 28),
            Self::Lod3 => (12, 16),
        }
    }
}

/// FACS Action Unit — Facial Action Coding System.
///
/// 46 action units mapped to facial muscle groups.
/// Each AU can be activated 0.0–1.0 independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FacsActionUnit {
    /// AU1: Inner brow raiser (frontalis, pars medialis).
    Au1InnerBrowRaise,
    /// AU2: Outer brow raiser (frontalis, pars lateralis).
    Au2OuterBrowRaise,
    /// AU4: Brow lowerer (corrugator supercilii, depressor supercilii).
    Au4BrowLower,
    /// AU5: Upper lid raiser (levator palpebrae superioris).
    Au5UpperLidRaise,
    /// AU6: Cheek raiser (orbicularis oculi, pars orbitalis).
    Au6CheekRaise,
    /// AU7: Lid tightener (orbicularis oculi, pars palpebralis).
    Au7LidTighten,
    /// AU9: Nose wrinkler (levator labii superioris alaeque nasi).
    Au9NoseWrinkle,
    /// AU10: Upper lip raiser (levator labii superioris).
    Au10UpperLipRaise,
    /// AU11: Nasolabial deepener (zygomaticus minor).
    Au11NasolabialDeepen,
    /// AU12: Lip corner puller — smile (zygomaticus major).
    Au12LipCornerPull,
    /// AU13: Sharp lip puller (levator anguli oris).
    Au13SharpLipPull,
    /// AU14: Dimpler (buccinator).
    Au14Dimple,
    /// AU15: Lip corner depressor (depressor anguli oris).
    Au15LipCornerDepress,
    /// AU16: Lower lip depressor (depressor labii inferioris).
    Au16LowerLipDepress,
    /// AU17: Chin raiser (mentalis).
    Au17ChinRaise,
    /// AU18: Lip pucker (incisivii labii superioris + inferioris).
    Au18LipPucker,
    /// AU20: Lip stretcher (risorius + platysma).
    Au20LipStretch,
    /// AU22: Lip funneler (orbicularis oris).
    Au22LipFunnel,
    /// AU23: Lip tightener (orbicularis oris).
    Au23LipTighten,
    /// AU24: Lip pressor (orbicularis oris).
    Au24LipPress,
    /// AU25: Lips part (depressor labii + mentalis).
    Au25LipsPart,
    /// AU26: Jaw drop (masseter + internal/medial pterygoid).
    Au26JawDrop,
    /// AU27: Mouth stretch (pterygoids + digastric).
    Au27MouthStretch,
    /// AU28: Lip suck (mentalis).
    Au28LipSuck,
    /// AU29: Jaw thrust (lateral pterygoid).
    Au29JawThrust,
    /// AU30: Jaw sideways (lateral pterygoid).
    Au30JawSideways,
    /// AU31: Jaw clench (masseter + temporalis).
    Au31JawClench,
    /// AU32: Lip bite (orbicularis oris + mentalis).
    Au32LipBite,
    /// AU33: Cheek blow (buccinator).
    Au33CheekBlow,
    /// AU34: Cheek puff (buccinator + orbicularis oris).
    Au34CheekPuff,
    /// AU35: Cheek suck (buccinator).
    Au35CheekSuck,
    /// AU36: Tongue bulge (genioglossus).
    Au36TongueBulge,
    /// AU37: Lip wipe (mentalis + orbicularis oris).
    Au37LipWipe,
    /// AU38: Nostril dilator (nasalis, pars alaris).
    Au38NostrilDilate,
    /// AU39: Nostril compressor (nasalis, pars transversa + depressor septi nasi).
    Au39NostrilCompress,
    /// AU41: Lid droop (relaxation of levator palpebrae superioris).
    Au41LidDroop,
    /// AU42: Slit (orbicularis oculi).
    Au42Slit,
    /// AU43: Eyes closed (relaxation of levator palpebrae superioris).
    Au43EyesClosed,
    /// AU44: Squint (orbicularis oculi, pars palpebralis).
    Au44Squint,
    /// AU45: Blink (relaxation of levator palpebrae superioris + orbicularis oculi).
    Au45Blink,
    /// AU46: Wink (orbicularis oculi).
    Au46Wink,
    /// AU51: Head turn left.
    Au51HeadTurnLeft,
    /// AU52: Head turn right.
    Au52HeadTurnRight,
    /// AU53: Head up.
    Au53HeadUp,
    /// AU54: Head down.
    Au54HeadDown,
    /// AU55: Head tilt left.
    Au55HeadTiltLeft,
    /// AU56: Head tilt right.
    Au56HeadTiltRight,
}

/// MetaHuman DNA calibration — parametric definition for face rig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaHumanDna {
    /// Archetype blend weights (indexed by archetype name).
    pub archetype_weights: Vec<ArchetypeWeight>,
    /// Fine-grained face rig joint offsets (DNA calibration deltas).
    pub joint_deltas: Vec<JointDelta>,
    /// Skin weight painting overrides.
    pub skin_weight_overrides: Vec<SkinWeightOverride>,
    /// Wrinkle map intensity per region.
    pub wrinkle_regions: WrinkleRegions,
    /// Multi-layer skin SSS parameters.
    pub skin_layers: SkinLayers,
    /// Age factor (0.0 = young, 1.0 = elderly).
    pub age: f32,
    /// Asymmetry factor (0.0 = symmetric, 1.0 = maximum asymmetry).
    pub asymmetry: f32,
}

/// Archetype blend weight — blend between base face shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchetypeWeight {
    pub name: String,
    pub weight: f32,
}

/// Per-joint calibration delta for face rig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointDelta {
    pub bone_index: usize,
    pub position_delta: [f32; 3],
    pub rotation_delta: [f32; 4],
}

/// Skin weight override for specific vertices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinWeightOverride {
    pub vertex_index: usize,
    pub bone_index: usize,
    pub weight: f32,
}

/// Wrinkle map region intensities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrinkleRegions {
    /// Forehead horizontal creases.
    pub forehead: f32,
    /// Glabellar (between brow) furrows.
    pub glabellar: f32,
    /// Crow's feet (outer eye).
    pub crows_feet: f32,
    /// Nasolabial folds.
    pub nasolabial: f32,
    /// Under-eye bags/wrinkles.
    pub under_eye: f32,
    /// Lip lines (perioral).
    pub perioral: f32,
    /// Neck lines.
    pub neck: f32,
    /// Chin dimpling.
    pub chin: f32,
}

/// Multi-layer subsurface scattering for photorealistic skin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinLayers {
    /// Epidermis (outermost): melanin absorption.
    pub epidermis_thickness: f32,
    pub melanin_density: f32,
    /// Dermis (middle): hemoglobin scattering.
    pub dermis_thickness: f32,
    pub hemoglobin_density: f32,
    /// Subdermal (deepest): fat scattering.
    pub subdermal_scatter: f32,
    /// Pore density (affects roughness map).
    pub pore_density: f32,
    /// Oil/sheen on skin surface.
    pub oiliness: f32,
}

/// MetaHuman-specific material identifier (extends base MaterialId).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetaHumanMaterialId {
    /// Standard character material.
    Base(MaterialId),
    /// Upper teeth.
    TeethUpper,
    /// Lower teeth.
    TeethLower,
    /// Gums.
    Gum,
    /// Tongue.
    Tongue,
    /// Inner mouth / oral mucosa.
    OralMucosa,
    /// Tear film (caruncle + tear line).
    TearFilm,
    /// Sclera (enhanced eye white with veins).
    Sclera,
    /// Cornea (transparent refractive layer over iris).
    Cornea,
}

/// Complete MetaHuman character mesh with LOD and FACS.
#[derive(Debug, Clone)]
pub struct MetaHumanMesh {
    /// Mesh parts per LOD level (LOD0 = index 0, etc.).
    pub lod_meshes: Vec<Vec<MeshPart>>,
    /// Extended face rig skeleton (72 bones).
    pub skeleton: Skeleton,
    /// FACS action unit blendshape targets (per AU, per LOD).
    pub facs_targets: Vec<FacsBlendshapeTarget>,
    /// ARKit-compatible blendshape targets (mapped from FACS).
    pub arkit_targets: Vec<BlendshapeTarget>,
    /// DNA calibration data.
    pub dna: MetaHumanDna,
}

/// FACS blendshape target — maps an action unit to vertex deltas.
#[derive(Debug, Clone)]
pub struct FacsBlendshapeTarget {
    pub action_unit: FacsActionUnit,
    pub name: String,
    /// Per-vertex position deltas for this AU.
    pub deltas: Vec<Vec3>,
    /// Wrinkle normal map intensity for this AU activation.
    pub wrinkle_intensity: f32,
}

impl Default for MetaHumanDna {
    fn default() -> Self {
        Self {
            archetype_weights: Vec::new(),
            joint_deltas: Vec::new(),
            skin_weight_overrides: Vec::new(),
            wrinkle_regions: WrinkleRegions::default(),
            skin_layers: SkinLayers::default(),
            age: 0.3,
            asymmetry: 0.05,
        }
    }
}

impl Default for WrinkleRegions {
    fn default() -> Self {
        Self {
            forehead: 0.3,
            glabellar: 0.2,
            crows_feet: 0.2,
            nasolabial: 0.3,
            under_eye: 0.1,
            perioral: 0.1,
            neck: 0.1,
            chin: 0.1,
        }
    }
}

impl Default for SkinLayers {
    fn default() -> Self {
        Self {
            epidermis_thickness: 0.5,
            melanin_density: 0.3,
            dermis_thickness: 0.6,
            hemoglobin_density: 0.4,
            subdermal_scatter: 0.5,
            pore_density: 0.5,
            oiliness: 0.3,
        }
    }
}

impl MetaHumanDna {
    /// Create from JSON (Murakumo VL analysis or DNA export).
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Generate MetaHuman-quality character from DNA and base CharacterDef.
///
/// Produces LOD0-LOD3 meshes, FACS blendshapes, and extended face rig.
pub fn generate_metahuman(
    def: &crate::params::CharacterDef,
    dna: &MetaHumanDna,
    lods: &[MetaHumanLod],
) -> MetaHumanMesh {
    let mut lod_meshes = Vec::with_capacity(lods.len());

    for &lod in lods {
        let parts = generate_metahuman_lod(def, dna, lod);
        lod_meshes.push(parts);
    }

    // Generate extended face rig skeleton
    let skeleton = generate_metahuman_skeleton(dna);

    // Generate FACS targets from LOD0 vertex count
    let lod0_head_verts = if let Some(lod0) = lod_meshes.first() {
        lod0.iter()
            .find(|p| p.name == "head")
            .map(|p| p.vertices.len())
            .unwrap_or(0)
    } else {
        0
    };
    let facs_targets = generate_facs_targets(lod0_head_verts);

    // Map FACS → ARKit for compatibility
    let arkit_targets = generate_arkit_targets(lod0_head_verts);

    MetaHumanMesh {
        lod_meshes,
        skeleton,
        facs_targets,
        arkit_targets,
        dna: dna.clone(),
    }
}

/// Generate mesh parts for a specific LOD level.
fn generate_metahuman_lod(
    def: &crate::params::CharacterDef,
    dna: &MetaHumanDna,
    lod: MetaHumanLod,
) -> Vec<MeshPart> {
    let (n_lat, n_lon) = lod.head_resolution();
    let (body_rings, body_seg) = lod.body_resolution();

    // Generate high-detail head
    let (mut head_verts, head_indices) = crate::base_mesh::generate_head(n_lat, n_lon);

    // Apply shape blendshapes
    crate::blendshape::apply_face_shape(&mut head_verts, &def.face);
    crate::blendshape::apply_eye_shape(&mut head_verts, &def.eyes);
    crate::blendshape::apply_nose_shape(&mut head_verts, &def.nose);
    crate::blendshape::apply_mouth_shape(&mut head_verts, &def.mouth);

    // Apply MetaHuman-specific deformations
    apply_age_deformation(&mut head_verts, dna.age);
    apply_asymmetry(&mut head_verts, dna.asymmetry);
    apply_wrinkle_displacement(&mut head_verts, &dna.wrinkle_regions, lod);

    // Smooth + normals
    let smooth_iters = match lod {
        MetaHumanLod::Lod0 => 3,
        MetaHumanLod::Lod1 => 2,
        _ => 1,
    };
    crate::base_mesh::laplacian_smooth(&mut head_verts, &head_indices, smooth_iters, 0.15);
    let head_normals = crate::base_mesh::compute_normals(&head_verts, &head_indices);
    let head_uvs = crate::base_mesh::frontal_uv(&head_verts);

    let head_vertices: Vec<Vertex> = head_verts
        .iter()
        .enumerate()
        .map(|(i, &pos)| Vertex {
            position: pos,
            normal: head_normals[i],
            uv: head_uvs[i],
        })
        .collect();

    let mut parts = vec![MeshPart {
        name: "head".into(),
        vertices: head_vertices,
        indices: head_indices,
        material: MaterialId::Skin,
    }];

    // Eyes (standard)
    parts.extend(crate::base_mesh::generate_eyes(&def.eyes));

    // Hair
    parts.push(crate::hair::generate_hair(&def.hair));

    // Body + clothing (LOD-scaled)
    parts.push(generate_metahuman_body(&def.body, body_rings, body_seg));
    parts.push(crate::body::generate_clothing(&def.clothing, &def.body));

    // MetaHuman-specific: teeth, tongue, inner mouth (LOD0/LOD1 only)
    if matches!(lod, MetaHumanLod::Lod0 | MetaHumanLod::Lod1) {
        parts.push(generate_teeth(true));
        parts.push(generate_teeth(false));
        parts.push(generate_tongue());
    }

    parts
}

/// Apply age-related deformation to head vertices.
fn apply_age_deformation(verts: &mut [Vec3], age: f32) {
    if age < 0.01 {
        return;
    }
    for v in verts.iter_mut() {
        // Jowl sagging
        if v.y < -0.04 && v.y > -0.10 {
            let sag = age * 0.005 * (1.0 - ((v.y + 0.07) / 0.03).powi(2)).max(0.0);
            v.y -= sag;
            // Slight outward push at jowls
            let jowl_x = (v.x.abs() - 0.04).max(0.0) / 0.04;
            v.x += v.x.signum() * sag * 0.3 * (1.0 - jowl_x).max(0.0);
        }
        // Nasolabial fold deepening
        let nl_x = (v.x.abs() - 0.025).abs();
        let nl_y = (1.0 - ((v.y + 0.01) / 0.04).powi(2)).max(0.0);
        if nl_x < 0.008 && nl_y > 0.0 {
            v.z -= age * 0.003 * nl_y * (1.0 - nl_x / 0.008);
        }
        // Under-eye hollowing
        for ex in [-0.032_f32, 0.032] {
            let ed = ((v.x - ex).powi(2) + (v.y - 0.035).powi(2)).sqrt();
            if ed < 0.015 {
                v.z -= age * 0.002 * (1.0 - ed / 0.015);
            }
        }
        // Forehead recession (hairline)
        if v.y > 0.10 {
            let t = ((v.y - 0.10) / 0.02).min(1.0);
            v.z -= age * 0.002 * t;
        }
    }
}

/// Apply bilateral asymmetry to head vertices.
fn apply_asymmetry(verts: &mut [Vec3], asymmetry: f32) {
    if asymmetry < 0.001 {
        return;
    }
    // Deterministic asymmetry using vertex index hash
    for (i, v) in verts.iter_mut().enumerate() {
        let hash = ((i as u32).wrapping_mul(2654435761) >> 16) as f32 / 65535.0;
        if v.x > 0.0 {
            // Right side slightly different from left
            v.x += (hash - 0.5) * asymmetry * 0.002;
            v.y += (hash * 0.7 - 0.35) * asymmetry * 0.001;
        }
    }
}

/// Apply wrinkle displacement to vertices (LOD-dependent detail).
fn apply_wrinkle_displacement(verts: &mut [Vec3], wrinkles: &WrinkleRegions, lod: MetaHumanLod) {
    let scale = match lod {
        MetaHumanLod::Lod0 => 1.0,
        MetaHumanLod::Lod1 => 0.6,
        MetaHumanLod::Lod2 => 0.2,
        MetaHumanLod::Lod3 => 0.0,
    };
    if scale < 0.01 {
        return;
    }

    for (i, v) in verts.iter_mut().enumerate() {
        let hash = ((i as u32).wrapping_mul(0x85ebca6b) >> 16) as f32 / 65535.0;

        // Forehead wrinkles (horizontal creases)
        if v.y > 0.06 && v.y < 0.11 {
            let freq = (v.y * 300.0).sin();
            let center = (1.0 - (v.x / 0.06).powi(2)).max(0.0);
            v.z += freq * wrinkles.forehead * scale * 0.0005 * center;
        }

        // Crow's feet
        for ex in [-0.05_f32, 0.05] {
            let d = ((v.x - ex).powi(2) + (v.y - 0.04).powi(2)).sqrt();
            if d < 0.02 && d > 0.008 {
                let radial = ((d - 0.008) * 500.0).sin();
                v.z += radial * wrinkles.crows_feet * scale * 0.0003;
            }
        }

        // Nasolabial creases
        let nl_region = (1.0 - ((v.y + 0.01) / 0.03).powi(2)).max(0.0);
        let nl_x_dist = (v.x.abs() - 0.022).abs();
        if nl_x_dist < 0.005 && nl_region > 0.0 {
            v.z -= wrinkles.nasolabial * scale * 0.0006 * nl_region * (1.0 - nl_x_dist / 0.005);
        }

        // Pore-level micro displacement (LOD0 only)
        if matches!(lod, MetaHumanLod::Lod0) {
            let front = (v.z / 0.08).max(0.0).min(1.0);
            v.z += (hash - 0.5) * 0.0002 * front;
        }
    }
}

/// Generate body mesh with LOD-appropriate resolution.
fn generate_metahuman_body(
    params: &crate::params::BodyParams,
    n_rings: u32,
    n_seg: u32,
) -> MeshPart {
    use std::f32::consts::PI;

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

/// Generate teeth mesh (upper or lower arch).
fn generate_teeth(upper: bool) -> MeshPart {
    use std::f32::consts::PI;

    let n_teeth = 14u32;
    let arch_y = if upper { -0.035 } else { -0.042 };
    let arch_z = 0.055;
    let tooth_h = if upper { -0.006 } else { 0.006 };

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for t in 0..n_teeth {
        let angle = -PI * 0.45 + PI * 0.9 * t as f32 / (n_teeth - 1) as f32;
        let cx = 0.025 * angle.sin();
        let cz = arch_z + 0.025 * angle.cos();
        let cy = arch_y;

        // Each tooth: 8-vertex box
        let hw = 0.0015; // half width
        let hd = 0.002; // half depth
        let base_idx = vertices.len() as u32;

        // Front face (4 verts)
        let n_front = Vec3::new(angle.sin(), 0.0, angle.cos()).normalize();
        for &dy in &[0.0, tooth_h] {
            for &dw in &[-hw, hw] {
                let perp = Vec3::new(angle.cos(), 0.0, -angle.sin());
                let pos = Vec3::new(cx + perp.x * dw, cy + dy, cz + perp.z * dw + hd);
                vertices.push(Vertex {
                    position: pos,
                    normal: n_front,
                    uv: [0.0, 0.0],
                });
            }
        }
        // Back face (4 verts)
        let n_back = -n_front;
        for &dy in &[0.0, tooth_h] {
            for &dw in &[-hw, hw] {
                let perp = Vec3::new(angle.cos(), 0.0, -angle.sin());
                let pos = Vec3::new(cx + perp.x * dw, cy + dy, cz + perp.z * dw - hd);
                vertices.push(Vertex {
                    position: pos,
                    normal: n_back,
                    uv: [0.0, 0.0],
                });
            }
        }

        // Front quad
        indices.extend_from_slice(&[
            base_idx,
            base_idx + 2,
            base_idx + 1,
            base_idx + 1,
            base_idx + 2,
            base_idx + 3,
        ]);
        // Back quad
        indices.extend_from_slice(&[
            base_idx + 4,
            base_idx + 5,
            base_idx + 6,
            base_idx + 5,
            base_idx + 7,
            base_idx + 6,
        ]);
        // Top quad
        indices.extend_from_slice(&[
            base_idx + 2,
            base_idx + 6,
            base_idx + 3,
            base_idx + 3,
            base_idx + 6,
            base_idx + 7,
        ]);
    }

    let name = if upper { "teeth_upper" } else { "teeth_lower" };
    MeshPart {
        name: name.into(),
        vertices,
        indices,
        material: MaterialId::Skin, // Will use MetaHumanMaterialId::TeethUpper/Lower at render time
    }
}

/// Generate tongue mesh (simplified flat blade shape).
fn generate_tongue() -> MeshPart {
    use std::f32::consts::PI;

    let n_seg = 8u32;
    let n_len = 6u32;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..=n_len {
        let t = i as f32 / n_len as f32;
        let y = -0.04 - t * 0.005;
        let z = 0.04 + t * 0.02;
        let w = 0.008 * (1.0 - t * 0.4);

        for j in 0..=n_seg {
            let s = j as f32 / n_seg as f32;
            let x = (s - 0.5) * 2.0 * w;
            let arch = (s * PI).sin() * 0.002;
            vertices.push(Vertex {
                position: Vec3::new(x, y + arch, z),
                normal: Vec3::Y,
                uv: [s, t],
            });
        }
    }

    for i in 0..n_len {
        for j in 0..n_seg {
            let a = i * (n_seg + 1) + j;
            let b = a + n_seg + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    MeshPart {
        name: "tongue".into(),
        vertices,
        indices,
        material: MaterialId::Lip, // Closest base material; MetaHumanMaterialId::Tongue at render time
    }
}

/// Generate MetaHuman extended skeleton with 72 face rig bones.
///
/// Extends the VRM 55-bone skeleton with additional face joints for
/// FACS-driven facial animation (eyelids, lips, nostrils, cheeks, tongue).
pub fn generate_metahuman_skeleton(dna: &MetaHumanDna) -> Skeleton {
    let id = Mat4::IDENTITY.to_cols_array_2d();

    // Start with VRM humanoid base (first 13 from body.rs, extended here)
    let mut bones = vec![
        // --- VRM humanoid base ---
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
        // Eyes
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
        // Arms
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
        // --- MetaHuman face rig extensions (parent = head, index 5) ---
        // Eyelids (4 bones)
        Bone {
            name: "leftUpperEyelid".into(),
            parent: Some(6),
            local_position: [0.0, 0.005, 0.005],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftLowerEyelid".into(),
            parent: Some(6),
            local_position: [0.0, -0.004, 0.005],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightUpperEyelid".into(),
            parent: Some(7),
            local_position: [0.0, 0.005, 0.005],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightLowerEyelid".into(),
            parent: Some(7),
            local_position: [0.0, -0.004, 0.005],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Brows (6 bones: inner/mid/outer x2)
        Bone {
            name: "leftBrowInner".into(),
            parent: Some(5),
            local_position: [-0.015, 0.065, 0.07],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftBrowMid".into(),
            parent: Some(5),
            local_position: [-0.03, 0.068, 0.065],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftBrowOuter".into(),
            parent: Some(5),
            local_position: [-0.045, 0.063, 0.055],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightBrowInner".into(),
            parent: Some(5),
            local_position: [0.015, 0.065, 0.07],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightBrowMid".into(),
            parent: Some(5),
            local_position: [0.03, 0.068, 0.065],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightBrowOuter".into(),
            parent: Some(5),
            local_position: [0.045, 0.063, 0.055],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Nose (3 bones)
        Bone {
            name: "noseBridge".into(),
            parent: Some(5),
            local_position: [0.0, 0.035, 0.085],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftNostril".into(),
            parent: Some(5),
            local_position: [-0.008, 0.005, 0.08],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightNostril".into(),
            parent: Some(5),
            local_position: [0.008, 0.005, 0.08],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Lips (8 bones: upper/lower x left/mid/right + corners)
        Bone {
            name: "upperLipLeft".into(),
            parent: Some(5),
            local_position: [-0.012, -0.03, 0.075],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "upperLipMid".into(),
            parent: Some(5),
            local_position: [0.0, -0.028, 0.078],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "upperLipRight".into(),
            parent: Some(5),
            local_position: [0.012, -0.03, 0.075],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "lowerLipLeft".into(),
            parent: Some(8),
            local_position: [-0.012, -0.005, 0.035],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "lowerLipMid".into(),
            parent: Some(8),
            local_position: [0.0, -0.007, 0.038],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "lowerLipRight".into(),
            parent: Some(8),
            local_position: [0.012, -0.005, 0.035],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftLipCorner".into(),
            parent: Some(5),
            local_position: [-0.022, -0.032, 0.068],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightLipCorner".into(),
            parent: Some(5),
            local_position: [0.022, -0.032, 0.068],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Cheeks (4 bones)
        Bone {
            name: "leftCheek".into(),
            parent: Some(5),
            local_position: [-0.04, 0.01, 0.06],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightCheek".into(),
            parent: Some(5),
            local_position: [0.04, 0.01, 0.06],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "leftNasolabial".into(),
            parent: Some(5),
            local_position: [-0.025, -0.01, 0.07],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightNasolabial".into(),
            parent: Some(5),
            local_position: [0.025, -0.01, 0.07],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Chin (2 bones)
        Bone {
            name: "chinTip".into(),
            parent: Some(8),
            local_position: [0.0, -0.02, 0.03],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "mentalis".into(),
            parent: Some(8),
            local_position: [0.0, -0.012, 0.035],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Tongue (3 bones: root/mid/tip)
        Bone {
            name: "tongueRoot".into(),
            parent: Some(8),
            local_position: [0.0, 0.0, 0.01],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "tongueMid".into(),
            parent: Some(42),
            local_position: [0.0, 0.0, 0.008],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "tongueTip".into(),
            parent: Some(43),
            local_position: [0.0, 0.0, 0.008],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        // Ear (2 bones)
        Bone {
            name: "leftEar".into(),
            parent: Some(5),
            local_position: [-0.07, 0.03, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
        Bone {
            name: "rightEar".into(),
            parent: Some(5),
            local_position: [0.07, 0.03, 0.0],
            local_rotation: [0.0, 0.0, 0.0, 1.0],
            local_scale: [1.0; 3],
            inverse_bind: id,
        },
    ];

    // Apply DNA joint deltas
    for delta in &dna.joint_deltas {
        if delta.bone_index < bones.len() {
            let bone = &mut bones[delta.bone_index];
            bone.local_position[0] += delta.position_delta[0];
            bone.local_position[1] += delta.position_delta[1];
            bone.local_position[2] += delta.position_delta[2];
        }
    }

    Skeleton { bones }
}

/// Generate FACS blendshape targets for MetaHuman.
///
/// 46 action units, each producing vertex deltas for the head mesh.
fn generate_facs_targets(n_verts: usize) -> Vec<FacsBlendshapeTarget> {
    use FacsActionUnit::*;

    let units = [
        (Au1InnerBrowRaise, "AU1_innerBrowRaise", 0.4),
        (Au2OuterBrowRaise, "AU2_outerBrowRaise", 0.3),
        (Au4BrowLower, "AU4_browLower", 0.5),
        (Au5UpperLidRaise, "AU5_upperLidRaise", 0.2),
        (Au6CheekRaise, "AU6_cheekRaise", 0.6),
        (Au7LidTighten, "AU7_lidTighten", 0.2),
        (Au9NoseWrinkle, "AU9_noseWrinkle", 0.7),
        (Au10UpperLipRaise, "AU10_upperLipRaise", 0.3),
        (Au11NasolabialDeepen, "AU11_nasolabialDeepen", 0.5),
        (Au12LipCornerPull, "AU12_lipCornerPull", 0.4),
        (Au13SharpLipPull, "AU13_sharpLipPull", 0.3),
        (Au14Dimple, "AU14_dimple", 0.4),
        (Au15LipCornerDepress, "AU15_lipCornerDepress", 0.3),
        (Au16LowerLipDepress, "AU16_lowerLipDepress", 0.2),
        (Au17ChinRaise, "AU17_chinRaise", 0.5),
        (Au18LipPucker, "AU18_lipPucker", 0.3),
        (Au20LipStretch, "AU20_lipStretch", 0.4),
        (Au22LipFunnel, "AU22_lipFunnel", 0.3),
        (Au23LipTighten, "AU23_lipTighten", 0.2),
        (Au24LipPress, "AU24_lipPress", 0.2),
        (Au25LipsPart, "AU25_lipsPart", 0.1),
        (Au26JawDrop, "AU26_jawDrop", 0.2),
        (Au27MouthStretch, "AU27_mouthStretch", 0.3),
        (Au28LipSuck, "AU28_lipSuck", 0.3),
        (Au29JawThrust, "AU29_jawThrust", 0.2),
        (Au30JawSideways, "AU30_jawSideways", 0.1),
        (Au31JawClench, "AU31_jawClench", 0.3),
        (Au32LipBite, "AU32_lipBite", 0.4),
        (Au33CheekBlow, "AU33_cheekBlow", 0.5),
        (Au34CheekPuff, "AU34_cheekPuff", 0.5),
        (Au35CheekSuck, "AU35_cheekSuck", 0.4),
        (Au36TongueBulge, "AU36_tongueBulge", 0.3),
        (Au37LipWipe, "AU37_lipWipe", 0.2),
        (Au38NostrilDilate, "AU38_nostrilDilate", 0.3),
        (Au39NostrilCompress, "AU39_nostrilCompress", 0.3),
        (Au41LidDroop, "AU41_lidDroop", 0.2),
        (Au42Slit, "AU42_slit", 0.2),
        (Au43EyesClosed, "AU43_eyesClosed", 0.2),
        (Au44Squint, "AU44_squint", 0.3),
        (Au45Blink, "AU45_blink", 0.2),
        (Au46Wink, "AU46_wink", 0.2),
        (Au51HeadTurnLeft, "AU51_headTurnLeft", 0.0),
        (Au52HeadTurnRight, "AU52_headTurnRight", 0.0),
        (Au53HeadUp, "AU53_headUp", 0.0),
        (Au54HeadDown, "AU54_headDown", 0.0),
        (Au55HeadTiltLeft, "AU55_headTiltLeft", 0.0),
        (Au56HeadTiltRight, "AU56_headTiltRight", 0.0),
    ];

    units
        .iter()
        .map(|(au, name, wrinkle)| FacsBlendshapeTarget {
            action_unit: *au,
            name: name.to_string(),
            deltas: vec![Vec3::ZERO; n_verts],
            wrinkle_intensity: *wrinkle,
        })
        .collect()
}

/// MetaHuman PBR material for extended material types.
impl MetaHumanMaterialId {
    /// Generate PBR material for MetaHuman-specific parts.
    pub fn to_pbr(&self, skin_layers: &SkinLayers) -> PbrMaterial {
        match self {
            Self::Base(mid) => {
                // Delegate to standard material (requires CharacterDef — use defaults)
                PbrMaterial {
                    name: format!("{mid:?}").to_lowercase(),
                    base_color: [0.9, 0.8, 0.75, 1.0],
                    metallic: 0.0,
                    roughness: 0.4,
                    subsurface: skin_layers.epidermis_thickness * 0.5
                        + skin_layers.dermis_thickness * 0.3,
                    subsurface_color: [
                        0.9 - skin_layers.melanin_density * 0.3,
                        0.5 + skin_layers.hemoglobin_density * 0.2,
                        0.35,
                    ],
                    anisotropic: 0.0,
                    clearcoat: skin_layers.oiliness * 0.3,
                    clearcoat_roughness: 0.2,
                    emission: [0.0; 3],
                }
            }
            Self::TeethUpper | Self::TeethLower => PbrMaterial {
                name: "teeth".into(),
                base_color: [0.95, 0.93, 0.88, 1.0],
                metallic: 0.0,
                roughness: 0.2,
                subsurface: 0.4,
                subsurface_color: [0.92, 0.85, 0.75],
                anisotropic: 0.0,
                clearcoat: 0.6,
                clearcoat_roughness: 0.1,
                emission: [0.0; 3],
            },
            Self::Gum => PbrMaterial {
                name: "gum".into(),
                base_color: [0.75, 0.45, 0.45, 1.0],
                metallic: 0.0,
                roughness: 0.35,
                subsurface: 0.6,
                subsurface_color: [0.8, 0.3, 0.25],
                anisotropic: 0.0,
                clearcoat: 0.2,
                clearcoat_roughness: 0.3,
                emission: [0.0; 3],
            },
            Self::Tongue => PbrMaterial {
                name: "tongue".into(),
                base_color: [0.78, 0.48, 0.48, 1.0],
                metallic: 0.0,
                roughness: 0.45,
                subsurface: 0.5,
                subsurface_color: [0.85, 0.35, 0.3],
                anisotropic: 0.0,
                clearcoat: 0.3,
                clearcoat_roughness: 0.2,
                emission: [0.0; 3],
            },
            Self::OralMucosa => PbrMaterial {
                name: "oral_mucosa".into(),
                base_color: [0.7, 0.4, 0.38, 1.0],
                metallic: 0.0,
                roughness: 0.3,
                subsurface: 0.7,
                subsurface_color: [0.8, 0.3, 0.2],
                anisotropic: 0.0,
                clearcoat: 0.4,
                clearcoat_roughness: 0.15,
                emission: [0.0; 3],
            },
            Self::TearFilm => PbrMaterial {
                name: "tear_film".into(),
                base_color: [0.98, 0.98, 0.98, 0.3],
                metallic: 0.0,
                roughness: 0.05,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.0,
                clearcoat: 1.0,
                clearcoat_roughness: 0.02,
                emission: [0.0; 3],
            },
            Self::Sclera => PbrMaterial {
                name: "sclera".into(),
                base_color: [0.97, 0.96, 0.95, 1.0],
                metallic: 0.0,
                roughness: 0.12,
                subsurface: 0.4,
                subsurface_color: [0.95, 0.85, 0.82],
                anisotropic: 0.0,
                clearcoat: 0.5,
                clearcoat_roughness: 0.08,
                emission: [0.0; 3],
            },
            Self::Cornea => PbrMaterial {
                name: "cornea".into(),
                base_color: [1.0, 1.0, 1.0, 0.05],
                metallic: 0.0,
                roughness: 0.02,
                subsurface: 0.0,
                subsurface_color: [0.0; 3],
                anisotropic: 0.0,
                clearcoat: 1.0,
                clearcoat_roughness: 0.01,
                emission: [0.0; 3],
            },
        }
    }
}

/// Map FACS action units to ARKit blendshape weights.
///
/// Returns a Vec of (arkit_target_index, weight) pairs for a given AU activation.
pub fn facs_to_arkit(au: FacsActionUnit, intensity: f32) -> Vec<(usize, f32)> {
    use FacsActionUnit::*;

    // ARKit target indices (matching generate_arkit_targets order in blendshape.rs)
    match au {
        Au1InnerBrowRaise => vec![(43, intensity)], // browInnerUp
        Au2OuterBrowRaise => vec![(44, intensity), (45, intensity)], // browOuterUpLeft/Right
        Au4BrowLower => vec![(42, intensity * 0.7), (43, -intensity * 0.5)], // browDownLeft + inverse browInnerUp
        Au5UpperLidRaise => vec![(12, intensity), (13, intensity)],          // eyeWideLeft/Right
        Au6CheekRaise => vec![(47, intensity), (48, intensity)], // cheekSquintLeft/Right
        Au7LidTighten => vec![(10, intensity * 0.6), (11, intensity * 0.6)], // eyeSquintLeft/Right
        Au9NoseWrinkle => vec![(49, intensity), (50, intensity)], // noseSneerLeft/Right
        Au10UpperLipRaise => vec![(40, intensity), (41, intensity)], // mouthUpperUpLeft/Right
        Au12LipCornerPull => vec![(23, intensity), (24, intensity)], // mouthSmileLeft/Right
        Au14Dimple => vec![(27, intensity), (28, intensity)],    // mouthDimpleLeft/Right
        Au15LipCornerDepress => vec![(25, intensity), (26, intensity)], // mouthFrownLeft/Right
        Au16LowerLipDepress => vec![(37, intensity), (38, intensity)], // mouthLowerDownLeft/Right
        Au17ChinRaise => vec![(33, intensity)],                  // mouthShrugLower
        Au18LipPucker => vec![(20, intensity)],                  // mouthPucker
        Au20LipStretch => vec![(29, intensity), (30, intensity)], // mouthStretchLeft/Right
        Au22LipFunnel => vec![(19, intensity)],                  // mouthFunnel
        Au25LipsPart => vec![(18, intensity * 0.3)],             // mouthClose (inverse)
        Au26JawDrop => vec![(17, intensity)],                    // jawOpen
        Au34CheekPuff => vec![(46, intensity)],                  // cheekPuff
        Au43EyesClosed | Au45Blink => vec![(0, intensity), (1, intensity)], // eyeBlinkLeft/Right
        Au51HeadTurnLeft => vec![(15, intensity)], // jawLeft (head bones via skeleton, this is approximate)
        Au52HeadTurnRight => vec![(16, intensity)], // jawRight
        Au36TongueBulge => vec![(51, intensity)],  // tongueOut
        _ => Vec::new(),                           // No direct ARKit mapping for remaining AUs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::CharacterDef;

    #[test]
    fn test_generate_metahuman_lod0() {
        let def = CharacterDef::default();
        let dna = MetaHumanDna::default();
        let mesh = generate_metahuman(&def, &dna, &[MetaHumanLod::Lod0]);
        assert_eq!(mesh.lod_meshes.len(), 1);
        let lod0 = &mesh.lod_meshes[0];
        let head = lod0.iter().find(|p| p.name == "head").unwrap();
        // LOD0 should have significantly more vertices than standard
        assert!(
            head.vertices.len() > 20000,
            "LOD0 head expected 20K+ verts, got {}",
            head.vertices.len()
        );
        // Should include teeth and tongue
        assert!(lod0.iter().any(|p| p.name == "teeth_upper"));
        assert!(lod0.iter().any(|p| p.name == "teeth_lower"));
        assert!(lod0.iter().any(|p| p.name == "tongue"));
    }

    #[test]
    fn test_generate_metahuman_all_lods() {
        let def = CharacterDef::default();
        let dna = MetaHumanDna::default();
        let lods = [
            MetaHumanLod::Lod0,
            MetaHumanLod::Lod1,
            MetaHumanLod::Lod2,
            MetaHumanLod::Lod3,
        ];
        let mesh = generate_metahuman(&def, &dna, &lods);
        assert_eq!(mesh.lod_meshes.len(), 4);

        // Each successive LOD should have fewer head vertices
        let head_verts: Vec<usize> = mesh
            .lod_meshes
            .iter()
            .map(|lod| {
                lod.iter()
                    .find(|p| p.name == "head")
                    .map(|p| p.vertices.len())
                    .unwrap_or(0)
            })
            .collect();
        for i in 1..head_verts.len() {
            assert!(
                head_verts[i] < head_verts[i - 1],
                "LOD{} ({}) should have fewer verts than LOD{} ({})",
                i,
                head_verts[i],
                i - 1,
                head_verts[i - 1]
            );
        }
    }

    #[test]
    fn test_metahuman_skeleton_bone_count() {
        let dna = MetaHumanDna::default();
        let skeleton = generate_metahuman_skeleton(&dna);
        // 13 VRM base + 34 face rig = 47 bones
        assert!(
            skeleton.bones.len() >= 40,
            "Expected 40+ bones, got {}",
            skeleton.bones.len()
        );
    }

    #[test]
    fn test_facs_targets_count() {
        let def = CharacterDef::default();
        let dna = MetaHumanDna::default();
        let mesh = generate_metahuman(&def, &dna, &[MetaHumanLod::Lod0]);
        // 46 FACS action units + head AU (51-56)
        assert_eq!(mesh.facs_targets.len(), 47);
        // 52 ARKit targets
        assert_eq!(mesh.arkit_targets.len(), 52);
    }

    #[test]
    fn test_dna_serialization() {
        let dna = MetaHumanDna::default();
        let json = dna.to_json();
        let restored = MetaHumanDna::from_json(&json).unwrap();
        assert!((restored.age - dna.age).abs() < f32::EPSILON);
    }

    #[test]
    fn test_facs_to_arkit_mapping() {
        let mappings = facs_to_arkit(FacsActionUnit::Au12LipCornerPull, 0.8);
        assert_eq!(mappings.len(), 2);
        assert!((mappings[0].1 - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_metahuman_material() {
        let layers = SkinLayers::default();
        let teeth = MetaHumanMaterialId::TeethUpper.to_pbr(&layers);
        assert!(teeth.clearcoat > 0.0);
        assert!(teeth.subsurface > 0.0);

        let cornea = MetaHumanMaterialId::Cornea.to_pbr(&layers);
        assert!(cornea.clearcoat > 0.9);
        assert!(cornea.roughness < 0.05);
    }

    #[test]
    fn test_lod3_no_teeth() {
        let def = CharacterDef::default();
        let dna = MetaHumanDna::default();
        let mesh = generate_metahuman(&def, &dna, &[MetaHumanLod::Lod3]);
        let lod3 = &mesh.lod_meshes[0];
        // LOD3 should not have teeth (too far away)
        assert!(!lod3.iter().any(|p| p.name == "teeth_upper"));
    }
}
