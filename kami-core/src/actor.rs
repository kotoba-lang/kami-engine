//! Actor model: network-authoritative entity state.
//!
//! Each Actor owns a hecs::World partition. Authority determines who is canonical.
//! Actors communicate via Mailbox (KNP transport-agnostic).

use crate::{EntityId, IslandId};

/// Who owns the canonical state for this actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Server is canonical (economy, HP, inventory).
    Server,
    /// Client is canonical (camera, input).
    Client,
    /// Client predicts, server reconciles (position, animation).
    Predicted,
}

/// Actor identity on the network.
#[derive(Debug, Clone)]
pub struct ActorId {
    pub entity_id: EntityId,
    pub island_id: IslandId,
    pub actor_type: ActorType,
    pub authority: Authority,
}

/// Actor type determines which components are attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ActorType {
    Player = 0,
    Npc = 1,
    Item = 2,
    Projectile = 3,
    Vehicle = 4,
    Trigger = 5,
    World = 6,
}

/// Standard ECS components for game actors.
pub mod components {
    use bytemuck::{Pod, Zeroable};

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct Position(pub [f32; 3]);

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct Rotation(pub [f32; 4]); // quaternion xyzw

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct Velocity(pub [f32; 3]);

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct Scale(pub [f32; 3]);

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct AnimationState {
        pub clip_id: u16,
        pub frame: u16,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct Health {
        pub current: u16,
        pub max: u16,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct MeshId(pub u32);

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Pod, Zeroable)]
    pub struct MaterialId(pub u32);
}
