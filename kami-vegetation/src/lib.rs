//! kami-vegetation: Decima-style procedural vegetation.
//!
//! Poisson-disk placement + biome rules (height + slope + splatmap) + GPU
//! instancing + wind-driven vertex animation.

pub mod cull;
pub mod instance;
pub mod lod;
pub mod mesh;
pub mod placement;
pub mod species;
pub mod taxonomy;

pub use cull::{cull_by_distance, cull_to_buffer};
pub use instance::InstanceData;
pub use lod::{LodTier, classify_lod};
pub use placement::{PlacementConfig, place_instances};
pub use species::{Species, SpeciesId, species_table};
