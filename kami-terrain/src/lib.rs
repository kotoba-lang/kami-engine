//! kami-terrain: Decima-style heightmap terrain engine.
//!
//! Procedural generation (value noise + FBM), clipmap LOD, splatmap material
//! blending, and chunk-based mesh generation for open-world rendering.
//!
//! Design reference: Guerrilla Games Decima Engine (Horizon Zero Dawn).

pub mod biome;
pub mod chunk;
pub mod heightmap;
pub mod noise;
pub mod splatmap;
pub mod water;

pub use biome::{BiomePreset, MaterialPalette, SplatThresholds};
pub use chunk::{TerrainChunk, TerrainVertex, generate_chunk_mesh};
pub use heightmap::{Heightmap, HeightmapConfig};
pub use noise::fbm_noise;
pub use splatmap::Splatmap;
pub use water::{
    GerstnerWave, WaterConfig, WaterVertex, default_waves, generate_water_mesh, waves_from_wind,
};
