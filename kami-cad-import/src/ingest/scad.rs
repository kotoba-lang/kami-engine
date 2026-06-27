//! OpenSCAD-style parametric ingest.
//!
//! Mirrors the minimal `ScadEntity` shape from `kami-scad` (Sphere /
//! Cube / Cylinder + position / rotation / scale) without taking a
//! direct dependency — `kami-scad` pulls `kami-render` + voxel + SDF +
//! mesher + gltf, which would explode our build graph for what is
//! essentially three primitive shapes and an AABB calculation.
//!
//! When ingestion from real `.scad` files is needed, the caller runs
//! `kami_scad::evaluate(source)` and maps each `ScadEntity` to
//! `Annotated<ScadPrim>` — the field layout is identical.
//!
//! Pipeline:
//!
//! ```text
//! .scad source ─► kami_scad::evaluate ─► Vec<ScadEntity>
//!                                          │
//!                                          ▼
//!                          map by index/id to ScadAnnotation
//!                                          │
//!                                          ▼
//!                  ingest::scad::from_annotated(...)
//!                                          │
//!                                          ▼
//!                                  VehicleAssembly
//! ```

use glam::{Mat4, Quat, Vec3};

use crate::part::{
    AssemblyError, Material, PartKind, ProvenanceSource, Supplier, VehicleAssembly, VehiclePart,
};

/// Local primitive — bit-for-bit compatible with `kami_scad::ScadPrimitive`.
#[derive(Debug, Clone, Copy)]
pub enum ScadPrim {
    Sphere {
        radius: f32,
    },
    /// Axis-aligned box centred at the origin in primitive-local space.
    Cube {
        size: [f32; 3],
    },
    /// Cylinder along the +Y axis, centred at the origin.
    Cylinder {
        h: f32,
        r1: f32,
        r2: f32,
    },
}

impl ScadPrim {
    /// 8 corners of the primitive's tight local-space AABB.
    fn local_aabb_corners(&self) -> [Vec3; 8] {
        let (lo, hi) = self.local_aabb();
        [
            Vec3::new(lo.x, lo.y, lo.z),
            Vec3::new(hi.x, lo.y, lo.z),
            Vec3::new(hi.x, hi.y, lo.z),
            Vec3::new(lo.x, hi.y, lo.z),
            Vec3::new(lo.x, lo.y, hi.z),
            Vec3::new(hi.x, lo.y, hi.z),
            Vec3::new(hi.x, hi.y, hi.z),
            Vec3::new(lo.x, hi.y, hi.z),
        ]
    }

    fn local_aabb(&self) -> (Vec3, Vec3) {
        match *self {
            ScadPrim::Sphere { radius } => (Vec3::splat(-radius), Vec3::splat(radius)),
            ScadPrim::Cube { size } => {
                let h = Vec3::new(size[0] * 0.5, size[1] * 0.5, size[2] * 0.5);
                (-h, h)
            }
            ScadPrim::Cylinder { h, r1, r2 } => {
                let r = r1.max(r2);
                (Vec3::new(-r, -h * 0.5, -r), Vec3::new(r, h * 0.5, r))
            }
        }
    }
}

/// 3D affine transform — same semantics as `kami_scad::ScadEntity`.
#[derive(Debug, Clone, Copy)]
pub struct ScadTransform {
    pub position: [f32; 3],
    /// xyzw quaternion.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for ScadTransform {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl ScadTransform {
    pub fn translate(mut self, x: f32, y: f32, z: f32) -> Self {
        self.position = [x, y, z];
        self
    }
    pub fn scale(mut self, sx: f32, sy: f32, sz: f32) -> Self {
        self.scale = [sx, sy, sz];
        self
    }
    pub fn rotate_xyzw(mut self, x: f32, y: f32, z: f32, w: f32) -> Self {
        self.rotation = [x, y, z, w];
        self
    }
    fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::from(self.scale),
            Quat::from_xyzw(
                self.rotation[0],
                self.rotation[1],
                self.rotation[2],
                self.rotation[3],
            ),
            Vec3::from(self.position),
        )
    }
}

/// Per-entity vehicle metadata supplied by the caller. The annotation
/// answers the questions the SCAD source can't: "what *role* does this
/// shape play in the vehicle, and where did it come from?"
#[derive(Debug, Clone)]
pub struct ScadAnnotation {
    pub part_id: String,
    pub display_name: Option<String>,
    pub kind: PartKind,
    pub material: Material,
    /// Override the AABB-derived mass, if known.
    pub mass_kg: Option<f32>,
    pub parent: Option<String>,
    pub break_group: Option<u32>,
    pub supplier: Supplier,
    pub revision: Option<String>,
    /// Per-part provenance — falls back to the assembly-level
    /// `ProvenanceSource` if `None`. Use this to declare per-part SCAD
    /// modules (`scad://miata-na/chassis.scad`) when the SCAD program
    /// is split across files.
    pub source: Option<ProvenanceSource>,
}

/// One renderable primitive + its annotation. Construct from a SCAD
/// AST or programmatically.
#[derive(Debug, Clone)]
pub struct AnnotatedEntity {
    pub primitive: ScadPrim,
    pub transform: ScadTransform,
    pub annotation: ScadAnnotation,
}

/// World-space AABB after applying `transform` to `prim`'s local AABB.
fn world_aabb(prim: &ScadPrim, t: &ScadTransform) -> ([f32; 3], [f32; 3]) {
    let m = t.matrix();
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for c in prim.local_aabb_corners() {
        let w = m.transform_point3(c);
        lo = lo.min(w);
        hi = hi.max(w);
    }
    (lo.into(), hi.into())
}

/// Convert an annotated SCAD entity stream into a validated
/// `VehicleAssembly`. The assembly-level `source` is used for any part
/// whose `ScadAnnotation::source` is `None`.
pub fn from_annotated(
    vehicle_id: impl Into<String>,
    display_name: impl Into<String>,
    revision: impl Into<String>,
    assembly_source: ProvenanceSource,
    entities: &[AnnotatedEntity],
    hardpoints: Vec<crate::part::Hardpoint>,
) -> Result<VehicleAssembly, AssemblyError> {
    let vid = vehicle_id.into();
    let mut asm = VehicleAssembly::new(vid.clone(), assembly_source.clone());
    asm.display_name = display_name.into();
    asm.revision = revision.into();

    for e in entities {
        let (aabb_min, aabb_max) = world_aabb(&e.primitive, &e.transform);
        let part = VehiclePart {
            id: e.annotation.part_id.clone(),
            display_name: e
                .annotation
                .display_name
                .clone()
                .unwrap_or_else(|| e.annotation.part_id.clone()),
            kind: e.annotation.kind,
            material: e.annotation.material,
            aabb_min,
            aabb_max,
            mass_kg: e.annotation.mass_kg,
            parent: e.annotation.parent.clone(),
            break_group: e.annotation.break_group,
            source: e
                .annotation
                .source
                .clone()
                .unwrap_or_else(|| assembly_source.clone()),
            supplier: e.annotation.supplier.clone(),
            revision: e
                .annotation
                .revision
                .clone()
                .unwrap_or_else(|| asm.revision.clone()),
        };
        asm.add_part(part);
    }
    for hp in hardpoints {
        asm.add_hardpoint(hp);
    }
    asm.validate()?;
    Ok(asm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::{Hardpoint, HardpointKind};

    fn prov() -> ProvenanceSource {
        ProvenanceSource {
            uri: "scad://t".into(),
            sha256: "a".repeat(64),
            license: "MIT".into(),
        }
    }

    #[test]
    fn cube_aabb_matches_size() {
        let p = ScadPrim::Cube {
            size: [2.0, 1.0, 0.5],
        };
        let t = ScadTransform::default();
        let (lo, hi) = world_aabb(&p, &t);
        assert_eq!(lo, [-1.0, -0.5, -0.25]);
        assert_eq!(hi, [1.0, 0.5, 0.25]);
    }

    #[test]
    fn translate_moves_aabb() {
        let p = ScadPrim::Cube {
            size: [2.0, 2.0, 2.0],
        };
        let t = ScadTransform::default().translate(10.0, 0.0, 0.0);
        let (lo, hi) = world_aabb(&p, &t);
        assert_eq!(lo, [9.0, -1.0, -1.0]);
        assert_eq!(hi, [11.0, 1.0, 1.0]);
    }

    #[test]
    fn cylinder_y_axis_aabb() {
        let p = ScadPrim::Cylinder {
            h: 1.0,
            r1: 0.3,
            r2: 0.3,
        };
        let t = ScadTransform::default();
        let (lo, hi) = world_aabb(&p, &t);
        assert_eq!(lo, [-0.3, -0.5, -0.3]);
        assert_eq!(hi, [0.3, 0.5, 0.3]);
    }

    #[test]
    fn from_annotated_round_trip() {
        let entities = vec![
            AnnotatedEntity {
                primitive: ScadPrim::Cube {
                    size: [1.7, 0.35, 4.0],
                },
                transform: ScadTransform::default().translate(0.0, 0.4, 0.0),
                annotation: ScadAnnotation {
                    part_id: "chassis".into(),
                    display_name: Some("chassis main rail".into()),
                    kind: PartKind::Chassis,
                    material: Material::SteelHss,
                    mass_kg: Some(220.0),
                    parent: None,
                    break_group: None,
                    supplier: Supplier::default(),
                    revision: None,
                    source: None,
                },
            },
            AnnotatedEntity {
                primitive: ScadPrim::Cube {
                    size: [1.6, 0.08, 0.9],
                },
                transform: ScadTransform::default().translate(0.0, 0.74, 1.65),
                annotation: ScadAnnotation {
                    part_id: "hood".into(),
                    display_name: None,
                    kind: PartKind::Body,
                    material: Material::AluminiumSheet,
                    mass_kg: Some(11.0),
                    parent: Some("chassis".into()),
                    break_group: None,
                    supplier: Supplier::default(),
                    revision: None,
                    source: None,
                },
            },
        ];
        let hps = vec![Hardpoint {
            id: "hp_hood".into(),
            from_part: "chassis".into(),
            to_part: "hood".into(),
            position: [0.0, 0.7, 1.4],
            kind: HardpointKind::Hinge,
        }];
        let asm =
            from_annotated("test-v1", "Test Vehicle", "0.1.0", prov(), &entities, hps).unwrap();
        assert_eq!(asm.parts.len(), 2);
        assert_eq!(asm.hardpoints.len(), 1);
        assert_eq!(asm.parts[0].id, "chassis");
        // hood AABB should reflect its translation.
        let hood = asm.part("hood").unwrap();
        assert!((hood.aabb_min[1] - (0.74 - 0.04)).abs() < 1e-4);
        assert!((hood.aabb_max[2] - (1.65 + 0.45)).abs() < 1e-4);
    }

    #[test]
    fn rejects_unknown_parent() {
        let entities = vec![AnnotatedEntity {
            primitive: ScadPrim::Cube {
                size: [1.0, 1.0, 1.0],
            },
            transform: ScadTransform::default(),
            annotation: ScadAnnotation {
                part_id: "p1".into(),
                display_name: None,
                kind: PartKind::Body,
                material: Material::SteelMild,
                mass_kg: None,
                parent: Some("ghost".into()),
                break_group: None,
                supplier: Supplier::default(),
                revision: None,
                source: None,
            },
        }];
        let err = from_annotated("v", "v", "1.0", prov(), &entities, vec![]).unwrap_err();
        match err {
            AssemblyError::UnknownParent { parent, .. } => assert_eq!(parent, "ghost"),
            other => panic!("unexpected: {:?}", other),
        }
    }
}
