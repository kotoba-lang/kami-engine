//! kami-engine: top-level integration.
//!
//! Ties together:
//!   kami-core  — Actor + hecs ECS + KAMI Interface (columnar IPC)
//!   kami-knp   — KNP network protocol (custom UDP, all platforms)
//!   kami-render — wgpu renderer (single backend, all platforms)
//!
//! Game loop: fixed timestep simulation + interpolated rendering.
//! Network: KamiDelta (columnar diff) over KNP channels.

pub use kami_core as core;
pub use kami_knp as knp;
pub use kami_render as render;
pub use kami_rtc as rtc;
pub use kami_mine_ai as mine_ai;
pub use kami_mine_pds as mine_pds;

pub use kami_core::actor::components::*;
pub use kami_core::actor::{ActorId, ActorType, Authority};
pub use kami_core::ipc::{Column, Delta, Dtype, Frame, compute_delta};
pub use kami_core::time::GameClock;
pub use kami_core::{EntityId, IslandId, Tick};

use hecs::World;
use kami_knp::channel::ChannelManager;
use kami_knp::packet::Channel;

/// KAMI Engine instance. One per client or server process.
pub struct Engine {
    pub world: World,
    pub clock: GameClock,
    pub net: ChannelManager,
    prev_frame: Option<Frame>,
}

impl Engine {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            world: World::new(),
            clock: GameClock::new(tick_rate),
            net: ChannelManager::new(),
            prev_frame: None,
        }
    }

    /// Spawn an entity with standard components.
    pub fn spawn(&mut self, pos: [f32; 3], rot: [f32; 4], actor_type: ActorType) -> hecs::Entity {
        self.world.spawn((Position(pos), Rotation(rot), actor_type))
    }

    /// Extract current positions as a KAMI Frame (zero-copy column view).
    pub fn snapshot_positions(&self) -> Frame {
        let mut positions: Vec<f32> = Vec::new();
        for (_, pos) in self.world.query::<&Position>().iter() {
            positions.extend_from_slice(&pos.0);
        }
        let n_entities = positions.len() / 3;
        let mut frame = Frame::new(self.clock.tick(), n_entities as u32);
        let data = bytemuck::cast_slice::<f32, u8>(&positions).to_vec();
        frame.push_column_owned(data, Dtype::F32, 3);
        frame
    }

    /// Compute network delta from previous snapshot. Returns KNP wire bytes.
    pub fn compute_and_send_delta(&mut self) -> Option<Vec<u8>> {
        let current = self.snapshot_positions();

        let result = if let Some(ref prev) = self.prev_frame {
            if prev.n_entities == current.n_entities
                && current.n_columns() > 0
                && prev.n_columns() > 0
            {
                let delta = compute_delta(prev, &current);
                if !delta.changed_indices.is_empty() {
                    let delta_bytes = delta.to_bytes();
                    let wire = self.net.send(Channel::Unreliable, delta_bytes);
                    Some(wire)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        self.prev_frame = Some(current);
        result
    }

    /// Apply received delta to local world.
    pub fn apply_received_delta(&mut self, knp_bytes: &[u8]) {
        if let Some((channel, payload)) = self.net.receive(knp_bytes) {
            if channel == Channel::Unreliable {
                if let Some(delta) = Delta::from_bytes(&payload) {
                    // Apply delta positions to ECS entities
                    // (In full implementation: map changed_indices to hecs entities)
                    let _ = delta;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_spawn_and_snapshot() {
        let mut engine = Engine::new(60);
        engine.spawn([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], ActorType::Player);
        engine.spawn([4.0, 5.0, 6.0], [0.0, 0.0, 0.0, 1.0], ActorType::Npc);

        let frame = engine.snapshot_positions();
        assert_eq!(frame.n_entities, 2);
        assert_eq!(frame.n_columns(), 1);
        // 2 entities: 24B data, 28B metadata → η = 46%. η > 95% requires ≥100 entities.
        assert!(frame.efficiency() > 0.4);
    }

    #[test]
    fn engine_delta_cycle() {
        let mut engine = Engine::new(60);
        engine.spawn([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], ActorType::Player);
        engine.spawn([1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0], ActorType::Npc);

        // First snapshot (no delta — no previous frame)
        assert!(engine.compute_and_send_delta().is_none());

        // Move entity 0
        for (_, pos) in engine.world.query_mut::<&mut Position>() {
            pos.0[0] += 0.1;
            break; // only first entity
        }

        // Second snapshot should produce delta
        let wire = engine.compute_and_send_delta();
        assert!(wire.is_some());
    }
}
