//! kami-bim: BIM (Building Information Modeling) kernel.
//!
//! Provides an IFC-like model for building authoring:
//! - Spatial hierarchy: Project → Site → Building → Storey → Space
//! - Element taxonomy: Wall / Slab / Column / Beam / Door / Window / Roof / Stair
//!   / Railing / Furniture / MepSegment (HVAC/piping) / Opening
//! - PropertySet (Pset_*) and Qto_* quantities (IFC convention)
//! - Material + Layer + Classification
//! - Link to `kami-cad::brep` for element geometry (BREP body or Axis curve)
//! - Scene projection for `kami-pipelines::BimSceneAdapter` (storey LOD)
//!
//! f64 precision (DVec3/DAffine3) matches kami-cad. IFC semantics are
//! preserved — a subset can be serialised to / imported from IFC STEP
//! (`.ifc`) by a companion importer (out of scope here; this crate
//! defines the in-memory model only).

use glam::{DAffine3, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use kami_cad::brep::BrepSolid;

/// Stable identifier for any BIM entity (project-local, GUID or u64).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BimId {
    /// 22-char IFC GlobalId (base64 / IfcGUID).
    Guid(String),
    /// Local numeric id (import-time or fresh-created).
    Local(u64),
}

impl BimId {
    pub fn local(v: u64) -> Self {
        BimId::Local(v)
    }
    pub fn guid(s: impl Into<String>) -> Self {
        BimId::Guid(s.into())
    }
}

// ── Spatial Hierarchy ──

/// Top-level container (1 per BIM model). Mirrors `IfcProject`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: BimId,
    pub name: String,
    pub description: String,
    /// Length / angle / time units (IfcUnitAssignment).
    pub units: UnitSystem,
    /// Model world coordinate system origin (IfcGeometricRepresentationContext).
    pub world_origin: DVec3,
    /// True north azimuth (radians from +Y).
    pub true_north_rad: f64,
    pub sites: Vec<Site>,
    /// Global property sets (project-wide constants).
    pub psets: BTreeMap<String, PropertySet>,
}

/// Geographic site (mirrors `IfcSite`). One project may contain multiple.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: BimId,
    pub name: String,
    /// WGS-84 lat/lon/elevation. Optional (pure-local models omit).
    pub geo: Option<GeoRef>,
    pub placement: DAffine3,
    pub buildings: Vec<Building>,
}

/// WGS-84 reference for the site origin.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoRef {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub elevation_m: f64,
}

/// Building (mirrors `IfcBuilding`). Contains storeys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub id: BimId,
    pub name: String,
    pub placement: DAffine3,
    /// Elevation of reference datum (m above site).
    pub reference_elevation: f64,
    pub storeys: Vec<Storey>,
}

/// Storey / level (mirrors `IfcBuildingStorey`). Primary LOD unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storey {
    pub id: BimId,
    pub name: String,
    /// Signed elevation (m, positive up) from building datum.
    pub elevation: f64,
    /// Floor-to-floor height (m).
    pub height: f64,
    pub placement: DAffine3,
    pub spaces: Vec<Space>,
    /// Elements directly contained in this storey (walls, slabs, columns ...).
    pub elements: Vec<Element>,
}

/// Space / room (mirrors `IfcSpace`). Bounded by walls / slabs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: BimId,
    pub name: String,
    /// Long name ("Executive meeting room 2F-A").
    pub long_name: String,
    /// Room number / label.
    pub label: String,
    pub category: SpaceCategory,
    /// 2D boundary as polygon in storey-local XY (m).
    pub boundary: Vec<glam::DVec2>,
    /// Ceiling height relative to storey floor (m).
    pub height: f64,
    pub quantities: Quantities,
    pub psets: BTreeMap<String, PropertySet>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpaceCategory {
    Office,
    Residential,
    Circulation,
    Service,
    MechanicalRoom,
    OutdoorCovered,
    External,
    Other,
}

// ── Elements ──

/// Physical building component. Geometry is either a kami-cad BREP
/// body (authored / imported precise solid) or an implicit
/// Axis+Profile sweep (wall/beam/column/mepSegment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub id: BimId,
    pub kind: ElementKind,
    pub name: String,
    /// GlobalId string for IFC export (22-char base64). Optional.
    pub global_id: Option<String>,
    pub placement: DAffine3,
    pub geometry: ElementGeometry,
    pub material_layers: Vec<MaterialLayer>,
    /// IFC classification (Uniclass / Omniclass / MasterFormat / ...).
    pub classification: Option<ClassificationRef>,
    pub quantities: Quantities,
    pub psets: BTreeMap<String, PropertySet>,
    /// Openings host-voids contained in this element (door / window cuts).
    pub openings: Vec<Opening>,
    /// Soft links to adjacent elements by id (wall → slab, column → beam).
    pub connected_to: Vec<BimId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ElementKind {
    Wall,
    Slab,
    Roof,
    Column,
    Beam,
    Door,
    Window,
    Stair,
    Railing,
    Curtain,
    Furniture,
    /// HVAC / piping / electrical segment.
    MepSegment,
    Opening,
    Other,
}

/// Geometry representation. Precise (BREP) for solids / landings,
/// Axis+Profile for extruded linear elements (walls/beams/columns/MEP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElementGeometry {
    /// Boundary-represented solid from kami-cad.
    Brep(BrepSolid),
    /// Swept profile along a 3D axis polyline.
    AxisSweep { axis: Vec<DVec3>, profile: Profile },
    /// Pure tessellation reference (triangle mesh id in blob store).
    /// Used for imported IFC geometry that is already meshed.
    MeshRef {
        blob_key: String,
        triangle_count: u32,
    },
    /// Placeholder for elements without authored geometry (spec-only).
    None,
}

/// 2D cross-section profile for AxisSweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Profile {
    /// Axis-aligned rectangle (thickness × height).
    Rectangle { thickness: f64, height: f64 },
    /// Circle (pipe, column).
    Circle { diameter: f64 },
    /// I-section (beam) with flange + web.
    IShape {
        height: f64,
        flange_width: f64,
        flange_thickness: f64,
        web_thickness: f64,
    },
    /// Free polygon in local XY (CCW).
    Polygon(Vec<glam::DVec2>),
}

/// Void cut in a host element (door / window hole).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opening {
    pub id: BimId,
    pub placement: DAffine3,
    pub profile: Profile,
    pub depth: f64,
    /// Filled by this element id (typically a Door or Window).
    pub filled_by: Option<BimId>,
}

// ── Material ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialLayer {
    pub material: String,
    pub thickness: f64,
    pub is_ventilated: bool,
    pub category: MaterialCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialCategory {
    Concrete,
    Steel,
    Timber,
    Masonry,
    Gypsum,
    Insulation,
    Glass,
    Finish,
    Other,
}

// ── Classification / Properties / Quantities ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRef {
    /// Source (`Uniclass2015`, `OmniClass`, `MasterFormat`, ...).
    pub source: String,
    /// Code (`Ss_25_13_45`).
    pub code: String,
    pub description: String,
}

/// IFC-style Pset_* / Qto_* container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PropertySet {
    pub name: String,
    pub props: BTreeMap<String, PropertyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PropertyValue {
    Bool(bool),
    Int(i64),
    Real(f64),
    Text(String),
    /// Real value with IFC unit string (`"kg"`, `"W/m2K"`, ...).
    Measured {
        value: f64,
        unit: String,
    },
    /// Enumerated choice with allowed-values list.
    Enum {
        value: String,
        allowed: Vec<String>,
    },
}

/// IFC Qto_* element quantities (gross/net surface/volume/weight).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Quantities {
    pub gross_area_m2: Option<f64>,
    pub net_area_m2: Option<f64>,
    pub gross_volume_m3: Option<f64>,
    pub net_volume_m3: Option<f64>,
    pub weight_kg: Option<f64>,
    pub length_m: Option<f64>,
}

// ── Units ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSystem {
    pub length: LengthUnit,
    pub angle: AngleUnit,
    pub time: TimeUnit,
}

impl Default for UnitSystem {
    fn default() -> Self {
        UnitSystem {
            length: LengthUnit::Metre,
            angle: AngleUnit::Radian,
            time: TimeUnit::Second,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    Metre,
    Millimetre,
    Centimetre,
    Foot,
    Inch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AngleUnit {
    Radian,
    Degree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeUnit {
    Second,
    Minute,
    Hour,
}

// ── Scene projection (consumed by kami-pipelines::BimSceneAdapter) ──

/// Projection of a storey to a camera-ready scene graph. Each entry is
/// one visible element with pre-computed world transform + mesh / axis
/// geometry + colour / highlight hint. Heavier than authoring model,
/// but purely derived — safe to regenerate on any model edit.
///
/// Wire format: `#[serde(rename_all = "camelCase")]` so Rust ↔ XRPC
/// `app.etzhayyim.apps.bim.getStoreyScene` JSON round-trips without a
/// separate transport type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreyScene {
    pub storey_id: BimId,
    pub storey_name: String,
    pub elevation: f64,
    pub items: Vec<SceneItem>,
    /// Axis-aligned bounding box (world, m).
    pub bounds_min: DVec3,
    pub bounds_max: DVec3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneItem {
    pub element_id: BimId,
    pub kind: ElementKind,
    pub world_transform: DAffine3,
    /// One of: explicit triangle list, axis polyline, or mesh blob ref.
    pub geom: SceneGeom,
    /// sRGB 0..1.
    pub base_color: [f32; 3],
    pub highlight: Highlight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SceneGeom {
    #[serde(rename_all = "camelCase")]
    Triangles {
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        normals: Vec<[f32; 3]>,
    },
    Axis(Vec<[f32; 3]>),
    #[serde(rename_all = "camelCase")]
    MeshRef {
        blob_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Highlight {
    None,
    Selected,
    Reviewed,
    /// Has one or more open comments anchored on this element.
    HasIssue,
}

// ── Model helpers ──

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Project {
            id: BimId::Local(0),
            name: name.into(),
            description: String::new(),
            units: UnitSystem::default(),
            world_origin: DVec3::ZERO,
            true_north_rad: 0.0,
            sites: Vec::new(),
            psets: BTreeMap::new(),
        }
    }

    /// Walk every element in the project. Useful for bulk operations.
    pub fn for_each_element<F: FnMut(&Element)>(&self, mut f: F) {
        for s in &self.sites {
            for b in &s.buildings {
                for st in &b.storeys {
                    for e in &st.elements {
                        f(e);
                    }
                }
            }
        }
    }

    /// Find a storey by id (linear scan; fine for authoring-scale models).
    pub fn find_storey(&self, id: &BimId) -> Option<&Storey> {
        self.sites
            .iter()
            .flat_map(|s| s.buildings.iter())
            .flat_map(|b| b.storeys.iter())
            .find(|s| &s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_roundtrip() {
        let mut p = Project::new("Test");
        p.sites.push(Site {
            id: BimId::local(1),
            name: "Site 1".into(),
            geo: None,
            placement: DAffine3::IDENTITY,
            buildings: vec![Building {
                id: BimId::local(2),
                name: "Building A".into(),
                placement: DAffine3::IDENTITY,
                reference_elevation: 0.0,
                storeys: vec![Storey {
                    id: BimId::local(3),
                    name: "L1".into(),
                    elevation: 0.0,
                    height: 3.5,
                    placement: DAffine3::IDENTITY,
                    spaces: vec![],
                    elements: vec![],
                }],
            }],
        });
        let json = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Test");
        assert_eq!(back.sites.len(), 1);
        assert_eq!(back.sites[0].buildings[0].storeys[0].height, 3.5);
    }

    #[test]
    fn find_storey_by_id() {
        let mut p = Project::new("Test");
        let storey_id = BimId::local(42);
        p.sites.push(Site {
            id: BimId::local(1),
            name: "Site".into(),
            geo: None,
            placement: DAffine3::IDENTITY,
            buildings: vec![Building {
                id: BimId::local(2),
                name: "B".into(),
                placement: DAffine3::IDENTITY,
                reference_elevation: 0.0,
                storeys: vec![Storey {
                    id: storey_id.clone(),
                    name: "GF".into(),
                    elevation: 0.0,
                    height: 3.0,
                    placement: DAffine3::IDENTITY,
                    spaces: vec![],
                    elements: vec![],
                }],
            }],
        });
        assert!(p.find_storey(&storey_id).is_some());
        assert!(p.find_storey(&BimId::local(999)).is_none());
    }
}
