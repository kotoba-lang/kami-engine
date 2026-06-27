//! kami-cad-import — bridge from CAD source to vehicle part graph,
//! JBeam topology, and CycloneDX SBOM.
//!
//! See ADR 2605051430.
//!
//! Pipeline:
//!
//! ```text
//! STEP / glTF / OpenSCAD source
//!   │
//!   ▼
//! kami_cad_import::part::VehicleAssembly
//!   ├─► jbeam_emit::emit  → kami-vehicle JBeam JSON
//!   └─► sbom::cyclonedx   → CycloneDX 1.5 JSON  →  sbom.etzhayyim.com
//! ```
//!
//! Phase 0/1 PoC: programmatic / OpenSCAD source. STEP and glTF
//! ingest land in Phase 1.1.

pub mod demos;
pub mod ingest;
pub mod jbeam_emit;
pub mod part;
pub mod register;
pub mod sbom;

pub use part::{
    Hardpoint, HardpointKind, Material, PartKind, ProvenanceSource, Supplier, VehicleAssembly,
    VehiclePart,
};
