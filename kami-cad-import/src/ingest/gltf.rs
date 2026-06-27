//! glTF 2.0 → VehicleAssembly ingest.
//!
//! Pure-Rust subset reader. We only consume what the part graph needs:
//!
//! 1. Scene → root nodes
//! 2. Node hierarchy + per-node TRS (translation / rotation / scale or
//!    matrix) — accumulated to a world transform per node
//! 3. Mesh primitive POSITION accessor `min` / `max` (glTF spec mandates
//!    these on the POSITION accessor) — used to derive the per-part AABB
//!    without ever loading vertex data
//! 4. `extras` block on each node + on the asset — picks up the etzhayyim
//!    annotation that says "this node *is* a VehiclePart, and here is
//!    its kind / material / supplier / source"
//!
//! Annotation contract (glTF `extras`):
//!
//! ```jsonc
//! // Node-level — each annotated node becomes a VehiclePart
//! "extras": {
//!   "gftd_part": {
//!     "id": "chassis",
//!     "display_name": "Chassis main rail",
//!     "kind": "chassis",          // see kind_from_str below
//!     "material": "steel-hss",    // see material_from_str
//!     "mass_kg": 220.0,           // optional override
//!     "parent": "...",            // optional — defaults to glTF parent
//!     "break_group": 1,           // optional
//!     "supplier": { "name": "...", "cpe": "...", "mpn": "..." },
//!     "source": { "uri": "...", "sha256": "...", "license": "MIT" },
//!     "revision": "1.0.0"
//!   }
//! }
//!
//! // Scene-level — list of inter-part hardpoints
//! "scenes[0].extras.gftd_hardpoints": [
//!   { "id": "hp_hood", "from": "chassis", "to": "hood",
//!     "position": [0,0.7,1.4], "kind": "hinge" }
//! ]
//!
//! // Asset-level — vehicle-wide source + license + revision
//! "asset.extras.gftd_vehicle": {
//!   "id": "miata-na-1989",
//!   "display_name": "...",
//!   "revision": "1.0.0",
//!   "source": { "uri": "...", "sha256": "...", "license": "MIT" }
//! }
//! ```
//!
//! Nodes without `gftd_part` are skipped — they're typically render-only
//! decoration. The caller can opt every mesh node in by passing
//! `IngestOptions::auto_part_kind` to fall back to a default `PartKind`
//! and material when annotations are missing.

use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;
use thiserror::Error;

use crate::part::{
    AssemblyError, Hardpoint, HardpointKind, Material, PartKind, ProvenanceSource, Supplier,
    VehicleAssembly, VehiclePart,
};

#[derive(Debug, Error)]
pub enum GltfError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("assembly: {0}")]
    Assembly(#[from] AssemblyError),
    #[error("scene index {0} out of range")]
    BadScene(usize),
    #[error("node index {0} out of range")]
    BadNodeIndex(usize),
    #[error("mesh index {0} out of range")]
    BadMeshIndex(usize),
    #[error("accessor index {0} out of range")]
    BadAccessor(usize),
    #[error("POSITION accessor on mesh `{mesh}` is missing min/max — re-export with bounds")]
    MissingAccessorBounds { mesh: String },
    #[error("vehicle asset.extras.gftd_vehicle missing — required for provenance")]
    MissingVehicleAnnotation,
    #[error("unknown PartKind `{0}`")]
    UnknownKind(String),
    #[error("unknown Material `{0}`")]
    UnknownMaterial(String),
    #[error("unknown HardpointKind `{0}`")]
    UnknownHardpointKind(String),
    #[error("node `{node_idx}` (`{name}`) carries gftd_part but has no mesh")]
    PartWithoutMesh { node_idx: usize, name: String },
}

#[derive(Debug, Clone)]
pub struct IngestOptions {
    /// When `Some`, glTF nodes that own a mesh but lack a `gftd_part`
    /// extra get a default annotation built from `(kind, material)` and
    /// the glTF node `name`. When `None` (default), unannotated nodes
    /// are skipped — strict mode.
    pub auto_part_kind: Option<(PartKind, Material)>,
    /// Override the gltf root scene if the document has multiple. None →
    /// uses `gltf.scene` field.
    pub scene_index: Option<usize>,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            auto_part_kind: None,
            scene_index: None,
        }
    }
}

// ── glTF 2.0 minimal schema ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GltfDoc {
    asset: GltfAsset,
    #[serde(default)]
    scene: Option<usize>,
    #[serde(default)]
    scenes: Vec<GltfScene>,
    #[serde(default)]
    nodes: Vec<GltfNode>,
    #[serde(default)]
    meshes: Vec<GltfMesh>,
    #[serde(default)]
    accessors: Vec<GltfAccessor>,
}

#[derive(Debug, Deserialize)]
struct GltfAsset {
    #[serde(default)]
    extras: Option<GltfAssetExtras>,
}

#[derive(Debug, Deserialize)]
struct GltfAssetExtras {
    #[serde(default)]
    gftd_vehicle: Option<etzhayyimVehicle>,
}

#[derive(Debug, Deserialize)]
struct etzhayyimVehicle {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    revision: Option<String>,
    source: ProvenanceSource,
}

#[derive(Debug, Deserialize)]
struct GltfScene {
    #[serde(default)]
    nodes: Vec<usize>,
    #[serde(default)]
    extras: Option<GltfSceneExtras>,
}

#[derive(Debug, Deserialize)]
struct GltfSceneExtras {
    #[serde(default)]
    gftd_hardpoints: Vec<etzhayyimHardpoint>,
}

#[derive(Debug, Deserialize)]
struct etzhayyimHardpoint {
    id: String,
    from: String,
    to: String,
    position: [f32; 3],
    kind: String,
}

#[derive(Debug, Deserialize)]
struct GltfNode {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    mesh: Option<usize>,
    #[serde(default)]
    children: Vec<usize>,
    #[serde(default)]
    matrix: Option<[f32; 16]>,
    #[serde(default)]
    translation: Option<[f32; 3]>,
    #[serde(default)]
    rotation: Option<[f32; 4]>,
    #[serde(default)]
    scale: Option<[f32; 3]>,
    #[serde(default)]
    extras: Option<GltfNodeExtras>,
}

#[derive(Debug, Deserialize)]
struct GltfNodeExtras {
    #[serde(default)]
    gftd_part: Option<etzhayyimPart>,
}

#[derive(Debug, Deserialize)]
struct etzhayyimPart {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    kind: String,
    material: String,
    #[serde(default)]
    mass_kg: Option<f32>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    break_group: Option<u32>,
    #[serde(default)]
    supplier: Option<Supplier>,
    #[serde(default)]
    source: Option<ProvenanceSource>,
    #[serde(default)]
    revision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GltfMesh {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    primitives: Vec<GltfPrimitive>,
}

#[derive(Debug, Deserialize)]
struct GltfPrimitive {
    #[serde(default)]
    attributes: GltfAttributes,
}

#[derive(Debug, Deserialize, Default)]
struct GltfAttributes {
    #[serde(rename = "POSITION", default)]
    position: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GltfAccessor {
    #[serde(default)]
    min: Option<Vec<f32>>,
    #[serde(default)]
    max: Option<Vec<f32>>,
}

// ── parse helpers ───────────────────────────────────────────────────────

fn kind_from_str(s: &str) -> Result<PartKind, GltfError> {
    Ok(match s {
        "chassis" => PartKind::Chassis,
        "body" => PartKind::Body,
        "window" => PartKind::Window,
        "powertrain" => PartKind::Powertrain,
        "suspension" => PartKind::Suspension,
        "wheel" => PartKind::Wheel,
        "brake" => PartKind::Brake,
        "interior" => PartKind::Interior,
        "electrical" => PartKind::Electrical,
        "fluid" => PartKind::Fluid,
        "trim" => PartKind::Trim,
        other => return Err(GltfError::UnknownKind(other.into())),
    })
}

fn material_from_str(s: &str) -> Result<Material, GltfError> {
    Ok(match s {
        "steel-hss" => Material::SteelHss,
        "steel-mild" => Material::SteelMild,
        "aluminium-cast" => Material::AluminiumCast,
        "aluminium-sheet" => Material::AluminiumSheet,
        "glass" => Material::Glass,
        "rubber" => Material::Rubber,
        "plastic" => Material::Plastic,
        "lithium-ion" => Material::LiIon,
        "composite" => Material::Composite,
        "other" => Material::Other,
        other => return Err(GltfError::UnknownMaterial(other.into())),
    })
}

fn hardpoint_kind_from_str(s: &str) -> Result<HardpointKind, GltfError> {
    Ok(match s {
        "bolt" => HardpointKind::Bolt,
        "weld" => HardpointKind::Weld,
        "hinge" => HardpointKind::Hinge,
        "latch" => HardpointKind::Latch,
        "press" => HardpointKind::Press,
        "adhesive" => HardpointKind::Adhesive,
        other => return Err(GltfError::UnknownHardpointKind(other.into())),
    })
}

fn node_local_transform(n: &GltfNode) -> Mat4 {
    if let Some(m) = n.matrix {
        return Mat4::from_cols_array(&m);
    }
    let t = n.translation.unwrap_or([0.0; 3]);
    let r = n.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let s = n.scale.unwrap_or([1.0; 3]);
    Mat4::from_scale_rotation_translation(
        Vec3::from(s),
        Quat::from_xyzw(r[0], r[1], r[2], r[3]),
        Vec3::from(t),
    )
}

fn mesh_local_aabb(doc: &GltfDoc, mesh_idx: usize) -> Result<(Vec3, Vec3), GltfError> {
    let mesh = doc
        .meshes
        .get(mesh_idx)
        .ok_or(GltfError::BadMeshIndex(mesh_idx))?;
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for prim in &mesh.primitives {
        let acc_idx = match prim.attributes.position {
            Some(i) => i,
            None => continue,
        };
        let acc = doc
            .accessors
            .get(acc_idx)
            .ok_or(GltfError::BadAccessor(acc_idx))?;
        let mn = acc
            .min
            .as_ref()
            .ok_or_else(|| GltfError::MissingAccessorBounds {
                mesh: mesh.name.clone().unwrap_or_default(),
            })?;
        let mx = acc
            .max
            .as_ref()
            .ok_or_else(|| GltfError::MissingAccessorBounds {
                mesh: mesh.name.clone().unwrap_or_default(),
            })?;
        if mn.len() < 3 || mx.len() < 3 {
            return Err(GltfError::MissingAccessorBounds {
                mesh: mesh.name.clone().unwrap_or_default(),
            });
        }
        lo = lo.min(Vec3::new(mn[0], mn[1], mn[2]));
        hi = hi.max(Vec3::new(mx[0], mx[1], mx[2]));
    }
    if !lo.x.is_finite() {
        return Err(GltfError::MissingAccessorBounds {
            mesh: mesh.name.clone().unwrap_or_default(),
        });
    }
    Ok((lo, hi))
}

fn aabb_world_corners(local_lo: Vec3, local_hi: Vec3, world: &Mat4) -> (Vec3, Vec3) {
    let corners = [
        Vec3::new(local_lo.x, local_lo.y, local_lo.z),
        Vec3::new(local_hi.x, local_lo.y, local_lo.z),
        Vec3::new(local_hi.x, local_hi.y, local_lo.z),
        Vec3::new(local_lo.x, local_hi.y, local_lo.z),
        Vec3::new(local_lo.x, local_lo.y, local_hi.z),
        Vec3::new(local_hi.x, local_lo.y, local_hi.z),
        Vec3::new(local_hi.x, local_hi.y, local_hi.z),
        Vec3::new(local_lo.x, local_hi.y, local_hi.z),
    ];
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for c in corners {
        let w = world.transform_point3(c);
        lo = lo.min(w);
        hi = hi.max(w);
    }
    (lo, hi)
}

// ── walk ────────────────────────────────────────────────────────────────

struct WalkCtx<'a> {
    doc: &'a GltfDoc,
    parts: Vec<VehiclePart>,
    parent_part: Vec<Option<String>>,
    options: &'a IngestOptions,
    asm_source: ProvenanceSource,
    asm_revision: String,
}

fn walk_node(
    ctx: &mut WalkCtx,
    node_idx: usize,
    parent_world: Mat4,
    parent_part_id: Option<String>,
) -> Result<(), GltfError> {
    let node = ctx
        .doc
        .nodes
        .get(node_idx)
        .ok_or(GltfError::BadNodeIndex(node_idx))?;
    let world = parent_world * node_local_transform(node);

    // Decide whether this node becomes a VehiclePart.
    let part_for_this_node: Option<(etzhayyimPart, usize)> =
        if let Some(part) = node.extras.as_ref().and_then(|e| e.gftd_part.as_ref()) {
            let mesh_idx = node.mesh.ok_or_else(|| GltfError::PartWithoutMesh {
                node_idx,
                name: node.name.clone().unwrap_or_default(),
            })?;
            Some((part.clone_into_owned(), mesh_idx))
        } else if let (Some((auto_kind, auto_mat)), Some(mesh_idx)) =
            (ctx.options.auto_part_kind, node.mesh)
        {
            let auto_id = node
                .name
                .clone()
                .unwrap_or_else(|| format!("node_{node_idx}"));
            Some((
                etzhayyimPart {
                    id: auto_id.clone(),
                    display_name: node.name.clone(),
                    kind: kind_label(auto_kind).to_string(),
                    material: material_label(auto_mat).to_string(),
                    mass_kg: None,
                    parent: None,
                    break_group: None,
                    supplier: None,
                    source: None,
                    revision: None,
                },
                mesh_idx,
            ))
        } else {
            None
        };

    let this_part_id = if let Some((part, mesh_idx)) = part_for_this_node {
        let (lo, hi) = mesh_local_aabb(ctx.doc, mesh_idx)?;
        let (wlo, whi) = aabb_world_corners(lo, hi, &world);
        let kind = kind_from_str(&part.kind)?;
        let material = material_from_str(&part.material)?;
        let id = part.id.clone();
        ctx.parts.push(VehiclePart {
            id: id.clone(),
            display_name: part.display_name.unwrap_or_else(|| id.clone()),
            kind,
            material,
            aabb_min: [wlo.x, wlo.y, wlo.z],
            aabb_max: [whi.x, whi.y, whi.z],
            mass_kg: part.mass_kg,
            parent: part.parent.or_else(|| parent_part_id.clone()),
            break_group: part.break_group,
            source: part.source.unwrap_or_else(|| ctx.asm_source.clone()),
            supplier: part.supplier.unwrap_or_default(),
            revision: part.revision.unwrap_or_else(|| ctx.asm_revision.clone()),
        });
        Some(id)
    } else {
        parent_part_id.clone()
    };

    for &child in &node.children {
        walk_node(ctx, child, world, this_part_id.clone())?;
    }
    let _ = ctx.parent_part;
    Ok(())
}

// `clone_into_owned` mirrors a `.clone()` but explicit so the macro
// parser doesn't trip over the shared by-ref reference.
impl etzhayyimPart {
    fn clone_into_owned(&self) -> Self {
        Self {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            kind: self.kind.clone(),
            material: self.material.clone(),
            mass_kg: self.mass_kg,
            parent: self.parent.clone(),
            break_group: self.break_group,
            supplier: self.supplier.clone(),
            source: self.source.clone(),
            revision: self.revision.clone(),
        }
    }
}

fn kind_label(k: PartKind) -> &'static str {
    match k {
        PartKind::Chassis => "chassis",
        PartKind::Body => "body",
        PartKind::Window => "window",
        PartKind::Powertrain => "powertrain",
        PartKind::Suspension => "suspension",
        PartKind::Wheel => "wheel",
        PartKind::Brake => "brake",
        PartKind::Interior => "interior",
        PartKind::Electrical => "electrical",
        PartKind::Fluid => "fluid",
        PartKind::Trim => "trim",
    }
}
fn material_label(m: Material) -> &'static str {
    match m {
        Material::SteelHss => "steel-hss",
        Material::SteelMild => "steel-mild",
        Material::AluminiumCast => "aluminium-cast",
        Material::AluminiumSheet => "aluminium-sheet",
        Material::Glass => "glass",
        Material::Rubber => "rubber",
        Material::Plastic => "plastic",
        Material::LiIon => "lithium-ion",
        Material::Composite => "composite",
        Material::Other => "other",
    }
}

// ── public API ──────────────────────────────────────────────────────────

/// Parse a glTF 2.0 JSON document and produce a validated VehicleAssembly.
pub fn from_gltf_json(json: &str, opts: &IngestOptions) -> Result<VehicleAssembly, GltfError> {
    let doc: GltfDoc = serde_json::from_str(json)?;
    let vehicle = doc
        .asset
        .extras
        .as_ref()
        .and_then(|e| e.gftd_vehicle.as_ref().map(|v| v.clone_into_owned()))
        .ok_or(GltfError::MissingVehicleAnnotation)?;

    let scene_idx = opts.scene_index.or(doc.scene).unwrap_or(0);
    let scene = doc
        .scenes
        .get(scene_idx)
        .ok_or(GltfError::BadScene(scene_idx))?;

    let mut ctx = WalkCtx {
        doc: &doc,
        parts: Vec::new(),
        parent_part: Vec::new(),
        options: opts,
        asm_source: vehicle.source.clone(),
        asm_revision: vehicle.revision.clone().unwrap_or_else(|| "0.1.0".into()),
    };
    for &root in &scene.nodes {
        walk_node(&mut ctx, root, Mat4::IDENTITY, None)?;
    }

    let mut asm = VehicleAssembly::new(vehicle.id.clone(), vehicle.source.clone());
    asm.display_name = vehicle.display_name.unwrap_or(vehicle.id.clone());
    asm.revision = ctx.asm_revision.clone();
    for p in ctx.parts {
        asm.add_part(p);
    }

    if let Some(extras) = scene.extras.as_ref() {
        for hp in &extras.gftd_hardpoints {
            asm.add_hardpoint(Hardpoint {
                id: hp.id.clone(),
                from_part: hp.from.clone(),
                to_part: hp.to.clone(),
                position: hp.position,
                kind: hardpoint_kind_from_str(&hp.kind)?,
            });
        }
    }

    asm.validate()?;
    Ok(asm)
}

impl etzhayyimVehicle {
    fn clone_into_owned(&self) -> Self {
        Self {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            revision: self.revision.clone(),
            source: self.source.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle_extras() -> &'static str {
        r#"{
          "id": "test-v1",
          "display_name": "Test V1",
          "revision": "1.0.0",
          "source": {
            "uri": "gltf://test/v1.glb",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "license": "MIT"
          }
        }"#
    }

    fn minimal_doc() -> String {
        format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0] }}],
              "nodes": [
                {{
                  "name": "chassis",
                  "mesh": 0,
                  "translation": [0, 0.3, 0],
                  "extras": {{
                    "gftd_part": {{
                      "id": "chassis",
                      "kind": "chassis",
                      "material": "steel-hss",
                      "mass_kg": 220
                    }}
                  }}
                }}
              ],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }}],
              "accessors": [{{ "min": [-0.85, 0, -2.0], "max": [0.85, 0.5, 2.0] }}]
            }}"#,
            vehicle_extras()
        )
    }

    #[test]
    fn ingest_minimal_single_part() {
        let asm = from_gltf_json(&minimal_doc(), &IngestOptions::default()).unwrap();
        assert_eq!(asm.vehicle_id, "test-v1");
        assert_eq!(asm.parts.len(), 1);
        let p = &asm.parts[0];
        assert_eq!(p.id, "chassis");
        // AABB = mesh AABB translated by node translation [0, 0.3, 0]
        assert!((p.aabb_min[1] - 0.3).abs() < 1e-4);
        assert!((p.aabb_max[1] - 0.8).abs() < 1e-4);
        assert!(p.mass_kg == Some(220.0));
    }

    #[test]
    fn ingest_inherits_provenance_when_part_missing_source() {
        let asm = from_gltf_json(&minimal_doc(), &IngestOptions::default()).unwrap();
        let p = &asm.parts[0];
        assert!(p.source.uri.starts_with("gltf://"));
        assert_eq!(p.source.license, "MIT");
    }

    #[test]
    fn ingest_rejects_missing_vehicle_annotation() {
        // Same as minimal but without asset.extras.gftd_vehicle.
        let bad = r#"{
          "asset": { "version": "2.0" },
          "scenes": [{ "nodes": [] }],
          "nodes": []
        }"#;
        let err = from_gltf_json(bad, &IngestOptions::default()).unwrap_err();
        match err {
            GltfError::MissingVehicleAnnotation => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn ingest_rejects_missing_accessor_bounds() {
        let bad = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0] }}],
              "nodes": [{{
                "name": "x", "mesh": 0,
                "extras": {{ "gftd_part": {{ "id": "x", "kind": "body", "material": "steel-mild" }} }}
              }}],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }}],
              "accessors": [{{}}]
            }}"#,
            vehicle_extras()
        );
        let err = from_gltf_json(&bad, &IngestOptions::default()).unwrap_err();
        match err {
            GltfError::MissingAccessorBounds { .. } => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn ingest_walks_child_nodes_and_inherits_parent() {
        let doc = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0] }}],
              "nodes": [
                {{
                  "name": "chassis", "mesh": 0,
                  "children": [1],
                  "extras": {{ "gftd_part": {{ "id": "chassis", "kind": "chassis", "material": "steel-hss" }} }}
                }},
                {{
                  "name": "hood", "mesh": 1,
                  "translation": [0, 0.4, 1.5],
                  "extras": {{ "gftd_part": {{ "id": "hood", "kind": "body", "material": "aluminium-sheet" }} }}
                }}
              ],
              "meshes": [
                {{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }},
                {{ "primitives": [{{ "attributes": {{ "POSITION": 1 }} }}] }}
              ],
              "accessors": [
                {{ "min": [-0.85, 0, -2], "max": [0.85, 0.5, 2] }},
                {{ "min": [-0.5, 0, -0.4], "max": [0.5, 0.05, 0.4] }}
              ]
            }}"#,
            vehicle_extras()
        );
        let asm = from_gltf_json(&doc, &IngestOptions::default()).unwrap();
        assert_eq!(asm.parts.len(), 2);
        let hood = asm.part("hood").unwrap();
        // Hood should auto-inherit chassis as parent because the glTF
        // hierarchy says so.
        assert_eq!(hood.parent.as_deref(), Some("chassis"));
    }

    #[test]
    fn ingest_walks_hardpoints_from_scene_extras() {
        let doc = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{
                "nodes": [0, 1],
                "extras": {{
                  "gftd_hardpoints": [
                    {{ "id": "hp1", "from": "chassis", "to": "hood", "position": [0,0.4,1.5], "kind": "hinge" }}
                  ]
                }}
              }}],
              "nodes": [
                {{
                  "name": "chassis", "mesh": 0,
                  "extras": {{ "gftd_part": {{ "id": "chassis", "kind": "chassis", "material": "steel-hss" }} }}
                }},
                {{
                  "name": "hood", "mesh": 1,
                  "extras": {{ "gftd_part": {{ "id": "hood", "kind": "body", "material": "aluminium-sheet" }} }}
                }}
              ],
              "meshes": [
                {{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }},
                {{ "primitives": [{{ "attributes": {{ "POSITION": 1 }} }}] }}
              ],
              "accessors": [
                {{ "min": [-1,0,-2], "max": [1,0.5,2] }},
                {{ "min": [-0.5,0,-0.4], "max": [0.5,0.05,0.4] }}
              ]
            }}"#,
            vehicle_extras()
        );
        let asm = from_gltf_json(&doc, &IngestOptions::default()).unwrap();
        assert_eq!(asm.hardpoints.len(), 1);
        let hp = &asm.hardpoints[0];
        assert_eq!(hp.id, "hp1");
        assert!(matches!(hp.kind, HardpointKind::Hinge));
    }

    #[test]
    fn ingest_auto_part_kind_picks_up_unannotated_meshes() {
        let doc = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0] }}],
              "nodes": [
                {{ "name": "untagged_panel", "mesh": 0 }}
              ],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }}],
              "accessors": [{{ "min": [-1,0,-1], "max": [1,0.05,1] }}]
            }}"#,
            vehicle_extras()
        );
        let asm = from_gltf_json(
            &doc,
            &IngestOptions {
                auto_part_kind: Some((PartKind::Body, Material::SteelMild)),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(asm.parts.len(), 1);
        let p = &asm.parts[0];
        assert_eq!(p.id, "untagged_panel");
        assert!(matches!(p.kind, PartKind::Body));
    }

    #[test]
    fn ingest_strict_mode_skips_unannotated_meshes() {
        let doc = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0, 1] }}],
              "nodes": [
                {{ "name": "decoration", "mesh": 0 }},
                {{
                  "name": "chassis", "mesh": 1,
                  "extras": {{ "gftd_part": {{ "id": "chassis", "kind": "chassis", "material": "steel-hss" }} }}
                }}
              ],
              "meshes": [
                {{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }},
                {{ "primitives": [{{ "attributes": {{ "POSITION": 1 }} }}] }}
              ],
              "accessors": [
                {{ "min": [-0.1,0,-0.1], "max": [0.1,0.1,0.1] }},
                {{ "min": [-1,0,-2], "max": [1,0.5,2] }}
              ]
            }}"#,
            vehicle_extras()
        );
        let asm = from_gltf_json(&doc, &IngestOptions::default()).unwrap();
        // Only the annotated chassis ends up in the assembly.
        assert_eq!(asm.parts.len(), 1);
        assert_eq!(asm.parts[0].id, "chassis");
    }

    #[test]
    fn ingest_unknown_kind_errors() {
        let doc = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0] }}],
              "nodes": [
                {{
                  "name": "x", "mesh": 0,
                  "extras": {{ "gftd_part": {{ "id": "x", "kind": "fairy_dust", "material": "steel-hss" }} }}
                }}
              ],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }}],
              "accessors": [{{ "min": [0,0,0], "max": [1,1,1] }}]
            }}"#,
            vehicle_extras()
        );
        let err = from_gltf_json(&doc, &IngestOptions::default()).unwrap_err();
        assert!(matches!(err, GltfError::UnknownKind(_)));
    }

    #[test]
    fn ingest_rotated_node_aabb_grows() {
        // A unit cube rotated 45° about Y → quaternion [0, sin(π/8), 0,
        // cos(π/8)] = [0, ~0.3827, ~0.9239]. World-space AABB in X-Z
        // expands from ±0.5 to ±0.5·√2 ≈ ±0.7071.
        let half_angle = std::f32::consts::FRAC_PI_8;
        let qy = half_angle.sin();
        let qw = half_angle.cos();
        let doc = format!(
            r#"{{
              "asset": {{ "version": "2.0", "extras": {{ "gftd_vehicle": {} }} }},
              "scene": 0,
              "scenes": [{{ "nodes": [0] }}],
              "nodes": [
                {{
                  "name": "cube", "mesh": 0,
                  "rotation": [0.0, {qy}, 0.0, {qw}],
                  "extras": {{ "gftd_part": {{ "id": "cube", "kind": "chassis", "material": "steel-hss" }} }}
                }}
              ],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }}],
              "accessors": [{{ "min": [-0.5, -0.5, -0.5], "max": [0.5, 0.5, 0.5] }}]
            }}"#,
            vehicle_extras(),
            qy = qy,
            qw = qw
        );
        let asm = from_gltf_json(&doc, &IngestOptions::default()).unwrap();
        let p = &asm.parts[0];
        let expected = 0.5_f32 * 2.0_f32.sqrt();
        assert!(
            (p.aabb_max[0] - expected).abs() < 1e-3,
            "x: {}",
            p.aabb_max[0]
        );
        assert!(
            (p.aabb_max[2] - expected).abs() < 1e-3,
            "z: {}",
            p.aabb_max[2]
        );
        // Y axis is unchanged by a Y-rotation.
        assert!((p.aabb_max[1] - 0.5).abs() < 1e-4, "y: {}", p.aabb_max[1]);
    }
}
