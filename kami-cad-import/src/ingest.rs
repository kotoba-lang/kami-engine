//! Source-format ingest adapters.
//!
//! Each adapter converts a stream of primitives + annotations into a
//! `VehicleAssembly`. Phase 1.1 ships `scad`; `step` and `gltf` follow.

pub mod gltf;
pub mod scad;
pub mod step;
