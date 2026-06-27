//! Vehicle part graph data model.
//!
//! `VehicleAssembly` is the single source of truth shared between the
//! soft-body simulator (`kami-vehicle`) and the SBOM emitter
//! (`sbom.etzhayyim.com`). Every part carries provenance — without it the
//! emitters refuse to produce output (ADR 2605051430).

use glam::Vec3;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssemblyError {
    #[error("part `{0}` missing provenance source — license clearance required")]
    MissingProvenance(String),
    #[error("hardpoint references unknown part `{0}`")]
    UnknownHardpointPart(String),
    #[error("duplicate part id `{0}`")]
    DuplicatePart(String),
    #[error("parent `{parent}` referenced by `{child}` does not exist")]
    UnknownParent { child: String, parent: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartKind {
    /// Load-bearing structural frame (chassis rails, subframe, A/B/C pillars).
    Chassis,
    /// Body panel (door, hood, fender, roof skin).
    Body,
    /// Glazing (windscreen, side windows, rear window).
    Window,
    /// Engine block, transmission case, drivetrain housing.
    Powertrain,
    /// Suspension (strut, control arm, anti-roll bar).
    Suspension,
    /// Wheel (hub + rim + tire).
    Wheel,
    /// Brake (caliper, disc, master cylinder).
    Brake,
    /// Interior (seat, dashboard, trim).
    Interior,
    /// Electrical / electronic component (battery, ECU, harness).
    Electrical,
    /// Fluid container (tank, radiator, reservoir).
    Fluid,
    /// Trim / aerodynamic non-structural.
    Trim,
}

impl PartKind {
    /// Default break group used by the BeamNG-style detach API.
    /// Mirrors `kami-vehicle::models::sedan` group conventions.
    pub fn default_break_group(&self) -> u32 {
        match self {
            PartKind::Chassis => 1,
            PartKind::Body => 2,
            PartKind::Window => 2,
            PartKind::Powertrain => 4,
            PartKind::Suspension => 5,
            PartKind::Wheel => 5,
            PartKind::Brake => 5,
            PartKind::Interior => 3,
            PartKind::Electrical => 4,
            PartKind::Fluid => 4,
            PartKind::Trim => 3,
        }
    }
}

/// Bulk material — used to derive mass from CAD volume and to pick a
/// reasonable beam stiffness / break threshold. Densities are kg/m^3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Material {
    /// High-strength steel (chassis rails, B-pillar).
    SteelHss,
    /// Mild steel (floor pan, generic body).
    SteelMild,
    /// Cast aluminium (engine block, suspension knuckle).
    AluminiumCast,
    /// Aluminium sheet (hood, modern body panels).
    AluminiumSheet,
    /// Tempered automotive glass.
    Glass,
    /// Rubber (tire, bushing).
    Rubber,
    /// Engineering plastic (bumper cover, dash).
    Plastic,
    /// Lithium-ion cell stack.
    LiIon,
    /// Composite (CFRP / GFRP).
    Composite,
    /// Generic — caller specifies `density_kg_m3` directly via override.
    Other,
}

impl Material {
    pub fn density_kg_m3(&self) -> f32 {
        match self {
            Material::SteelHss => 7850.0,
            Material::SteelMild => 7850.0,
            Material::AluminiumCast => 2700.0,
            Material::AluminiumSheet => 2700.0,
            Material::Glass => 2500.0,
            Material::Rubber => 1100.0,
            Material::Plastic => 1100.0,
            Material::LiIon => 2500.0,
            Material::Composite => 1600.0,
            Material::Other => 1000.0,
        }
    }

    /// Beam axial stiffness (N/m) used as the JBeam emitter default.
    /// Tuned to the same order of magnitude as the existing
    /// `kami-vehicle::models::sedan` hand-written values.
    pub fn beam_spring_n_m(&self) -> f32 {
        match self {
            Material::SteelHss => 800_000.0,
            Material::SteelMild => 500_000.0,
            Material::AluminiumCast => 350_000.0,
            Material::AluminiumSheet => 200_000.0,
            Material::Glass => 100_000.0,
            Material::Rubber => 80_000.0,
            Material::Plastic => 80_000.0,
            Material::LiIon => 250_000.0,
            Material::Composite => 600_000.0,
            Material::Other => 200_000.0,
        }
    }

    /// Plastic-strain break threshold (dimensionless, fraction of rest length).
    pub fn break_strain(&self) -> f32 {
        match self {
            Material::SteelHss => 0.20,
            Material::SteelMild => 0.18,
            Material::AluminiumCast => 0.10,
            Material::AluminiumSheet => 0.14,
            Material::Glass => 0.02,
            Material::Rubber => 0.50,
            Material::Plastic => 0.08,
            Material::LiIon => 0.06,
            Material::Composite => 0.06,
            Material::Other => 0.10,
        }
    }
}

/// Joint kind between two parts. Drives JBeam beam type + break behaviour
/// and is also surfaced in the SBOM as a logical relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardpointKind {
    /// Threaded fastener — bounded beam, breaks under shock.
    Bolt,
    /// Welded joint — normal beam, high break threshold.
    Weld,
    /// Hinge — bounded beam, allows rotation about one axis (we emit
    /// it as a soft beam pair; full revolute joint comes in Phase 2).
    Hinge,
    /// Latch — bounded beam, low break threshold (door pops on impact).
    Latch,
    /// Press / interference fit (bearing race, bushing).
    Press,
    /// Adhesive bond (windshield, glass).
    Adhesive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hardpoint {
    /// Stable id within the assembly.
    pub id: String,
    /// Source part id.
    pub from_part: String,
    /// Target part id.
    pub to_part: String,
    /// World-space attach point in metres (single-point Phase 1; pair-of-points
    /// in Phase 2 for revolute hinge axes).
    pub position: [f32; 3],
    pub kind: HardpointKind,
}

/// Provenance — required, non-empty, by construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceSource {
    /// `file://`, `b2://drive/cad/...`, `https://`, or `scad://` for
    /// `kami-scad`-generated parts.
    pub uri: String,
    /// SHA-256 hex of the source artifact (CAD file or SCAD program).
    /// Empty string is rejected at validation time.
    pub sha256: String,
    /// SPDX expression — `MIT`, `CC-BY-4.0`, `proprietary`, etc.
    pub license: String,
}

/// Supplier — populated when the part comes from an OEM / Tier-1 catalog.
/// For `kami-scad` self-authored parts this is `Supplier { name: "gftd",
/// .. }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Supplier {
    pub name: String,
    /// Common Platform Enumeration when published by the supplier.
    /// Empty when none — we fall back to the synthesized purl.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cpe: String,
    /// Manufacturer part number.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mpn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehiclePart {
    /// Stable identifier — used as JBeam node-id prefix and CycloneDX `bom-ref`.
    pub id: String,
    pub display_name: String,
    pub kind: PartKind,
    pub material: Material,
    /// AABB min corner (metres, vehicle frame).
    pub aabb_min: [f32; 3],
    /// AABB max corner (metres, vehicle frame).
    pub aabb_max: [f32; 3],
    /// Mass override in kg. When `None` the emitter computes it from
    /// AABB volume × material density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg: Option<f32>,
    /// Parent part id (assembly tree). `None` for the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Detach group; `None` falls back to `kind.default_break_group()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_group: Option<u32>,
    pub source: ProvenanceSource,
    #[serde(default)]
    pub supplier: Supplier,
    /// Free-form revision tag — appears in the synthesized purl.
    #[serde(default = "default_revision")]
    pub revision: String,
}

fn default_revision() -> String {
    "0.1.0".to_string()
}

impl VehiclePart {
    pub fn aabb_min_v(&self) -> Vec3 {
        Vec3::from(self.aabb_min)
    }
    pub fn aabb_max_v(&self) -> Vec3 {
        Vec3::from(self.aabb_max)
    }
    pub fn aabb_centre(&self) -> Vec3 {
        0.5 * (self.aabb_min_v() + self.aabb_max_v())
    }
    pub fn aabb_volume_m3(&self) -> f32 {
        let s = self.aabb_max_v() - self.aabb_min_v();
        s.x.max(0.0) * s.y.max(0.0) * s.z.max(0.0)
    }
    /// Effective mass — explicit override if set, else volume × density.
    pub fn effective_mass_kg(&self) -> f32 {
        self.mass_kg
            .unwrap_or_else(|| self.aabb_volume_m3() * self.material.density_kg_m3())
    }
    pub fn effective_break_group(&self) -> u32 {
        self.break_group
            .unwrap_or_else(|| self.kind.default_break_group())
    }
    /// 8 AABB corners, used by the JBeam emitter as the part's mass-node
    /// scaffolding.
    pub fn aabb_corners(&self) -> [Vec3; 8] {
        let lo = self.aabb_min_v();
        let hi = self.aabb_max_v();
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
}

/// A complete vehicle part graph — fed by ingest adapters, consumed by
/// `jbeam_emit` and `sbom`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleAssembly {
    /// Stable vehicle id, e.g. `mx5-na-1989`. Appears in synthesized purls.
    pub vehicle_id: String,
    pub display_name: String,
    /// Free-form vehicle revision (model year + build).
    #[serde(default = "default_revision")]
    pub revision: String,
    /// Top-level provenance (e.g. JBeam author, design house).
    pub source: ProvenanceSource,
    pub parts: Vec<VehiclePart>,
    #[serde(default)]
    pub hardpoints: Vec<Hardpoint>,
}

impl VehicleAssembly {
    pub fn new(vehicle_id: impl Into<String>, source: ProvenanceSource) -> Self {
        let vid = vehicle_id.into();
        Self {
            display_name: vid.clone(),
            vehicle_id: vid,
            revision: default_revision(),
            source,
            parts: Vec::new(),
            hardpoints: Vec::new(),
        }
    }

    pub fn add_part(&mut self, part: VehiclePart) -> &mut Self {
        self.parts.push(part);
        self
    }

    pub fn add_hardpoint(&mut self, hp: Hardpoint) -> &mut Self {
        self.hardpoints.push(hp);
        self
    }

    pub fn part(&self, id: &str) -> Option<&VehiclePart> {
        self.parts.iter().find(|p| p.id == id)
    }

    /// Aggregate mass — sum of `effective_mass_kg` over every part.
    pub fn total_mass_kg(&self) -> f32 {
        self.parts.iter().map(|p| p.effective_mass_kg()).sum()
    }

    /// BeamNG-style "break group → parts" rollup, same shape as
    /// `kami-vehicle::models::sedan` break_group conventions.
    pub fn parts_by_break_group(&self) -> Vec<(u32, Vec<&VehiclePart>)> {
        let mut groups: std::collections::BTreeMap<u32, Vec<&VehiclePart>> =
            std::collections::BTreeMap::new();
        for p in &self.parts {
            groups.entry(p.effective_break_group()).or_default().push(p);
        }
        groups.into_iter().collect()
    }

    /// Validate the assembly. Refuses to emit JBeam / SBOM if this fails.
    pub fn validate(&self) -> Result<(), AssemblyError> {
        // duplicate ids
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for p in &self.parts {
            if !seen.insert(p.id.as_str()) {
                return Err(AssemblyError::DuplicatePart(p.id.clone()));
            }
        }
        // provenance — every part must have a non-empty source.sha256
        for p in &self.parts {
            if p.source.sha256.is_empty() || p.source.uri.is_empty() {
                return Err(AssemblyError::MissingProvenance(p.id.clone()));
            }
        }
        // parent fk
        for p in &self.parts {
            if let Some(parent) = &p.parent {
                if !self.parts.iter().any(|q| &q.id == parent) {
                    return Err(AssemblyError::UnknownParent {
                        child: p.id.clone(),
                        parent: parent.clone(),
                    });
                }
            }
        }
        // hardpoint fk
        for hp in &self.hardpoints {
            if !self.parts.iter().any(|p| p.id == hp.from_part) {
                return Err(AssemblyError::UnknownHardpointPart(hp.from_part.clone()));
            }
            if !self.parts.iter().any(|p| p.id == hp.to_part) {
                return Err(AssemblyError::UnknownHardpointPart(hp.to_part.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance(sha: &str) -> ProvenanceSource {
        ProvenanceSource {
            uri: "scad://test".into(),
            sha256: sha.into(),
            license: "MIT".into(),
        }
    }

    fn synth_part(id: &str, kind: PartKind) -> VehiclePart {
        VehiclePart {
            id: id.into(),
            display_name: id.into(),
            kind,
            material: Material::SteelMild,
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 0.5, 0.3],
            mass_kg: None,
            parent: None,
            break_group: None,
            source: provenance("a".repeat(64).as_str()),
            supplier: Supplier::default(),
            revision: "1.0.0".into(),
        }
    }

    #[test]
    fn density_volume_to_mass() {
        let p = synth_part("rail", PartKind::Chassis);
        // 1.0 × 0.5 × 0.3 m³ × 7850 kg/m³ = 1177.5 kg
        let m = p.effective_mass_kg();
        assert!((m - 1177.5).abs() < 1e-2, "got {m}");
    }

    #[test]
    fn break_group_inherits_kind() {
        let p = synth_part("hood", PartKind::Body);
        assert_eq!(
            p.effective_break_group(),
            PartKind::Body.default_break_group()
        );
        let mut p2 = synth_part("strut", PartKind::Suspension);
        p2.break_group = Some(99);
        assert_eq!(p2.effective_break_group(), 99);
    }

    #[test]
    fn validate_rejects_missing_provenance() {
        let mut a = VehicleAssembly::new(
            "v1",
            ProvenanceSource {
                uri: "x".into(),
                sha256: "z".into(),
                license: "MIT".into(),
            },
        );
        let mut bad = synth_part("rail", PartKind::Chassis);
        bad.source.sha256 = String::new();
        a.add_part(bad);
        let err = a.validate().unwrap_err();
        match err {
            AssemblyError::MissingProvenance(id) => assert_eq!(id, "rail"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn validate_rejects_unknown_parent() {
        let mut a = VehicleAssembly::new("v1", provenance("a".repeat(64).as_str()));
        let mut child = synth_part("door", PartKind::Body);
        child.parent = Some("nope".into());
        a.add_part(child);
        match a.validate().unwrap_err() {
            AssemblyError::UnknownParent { child, parent } => {
                assert_eq!(child, "door");
                assert_eq!(parent, "nope");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn validate_rejects_dangling_hardpoint() {
        let mut a = VehicleAssembly::new("v1", provenance("a".repeat(64).as_str()));
        a.add_part(synth_part("rail", PartKind::Chassis));
        a.add_hardpoint(Hardpoint {
            id: "hp1".into(),
            from_part: "rail".into(),
            to_part: "ghost".into(),
            position: [0.0, 0.0, 0.0],
            kind: HardpointKind::Bolt,
        });
        match a.validate().unwrap_err() {
            AssemblyError::UnknownHardpointPart(p) => assert_eq!(p, "ghost"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parts_by_break_group_aggregates() {
        let mut a = VehicleAssembly::new("v1", provenance("a".repeat(64).as_str()));
        a.add_part(synth_part("rail", PartKind::Chassis)); // group 1
        a.add_part(synth_part("door", PartKind::Body)); // group 2
        a.add_part(synth_part("hood", PartKind::Body)); // group 2
        let groups = a.parts_by_break_group();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, 1);
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[1].0, 2);
        assert_eq!(groups[1].1.len(), 2);
    }
}
