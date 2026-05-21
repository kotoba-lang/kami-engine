//! kami-core: Actor + hecs ECS + KAMI Interface (columnar zero-copy)

pub mod actor;
pub mod ipc;
pub mod time;

pub use glam;
pub use hecs;

/// Entity ID (globally unique within an island)
pub type EntityId = u64;
/// Island ID
pub type IslandId = u64;
/// Network tick counter
pub type Tick = u32;
