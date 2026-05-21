//! Arena: physics-based action game prototype.
//!
//! Floor + walls + ramps + dynamic cubes + player WASD + projectiles + NPC enemies + score.

use crate::physics::PhysicsWorld;
use crate::scene::{ComponentDef, EntityDef, IslandScene, MeshRef};
use glam::Vec3;

/// Create the Arena Island scene definition.
pub fn arena_island() -> IslandScene {
    IslandScene {
        context: None,
        ld_type: None,
        ld_id: None,
        name: "Battle Arena".into(),
        genre: None,
        description: None,
        max_players: None,
        characters: vec![],
        ambient_color: [0.02, 0.02, 0.04],
        sun_direction: [-0.5, -3.0, -1.0],
        sun_intensity: 4.0,
        camera_mode: None,
        layers: vec![],
        viewport: None,
        sun_color: None,
        point_lights: vec![],
        atmosphere: None,
        postfx_preset: None,
        ibl_env_map: None,
        shadow: None,
        entities: vec![
            // Floor
            entity(
                "floor",
                [0.0, -0.5, 0.0],
                [60.0, 1.0, 60.0],
                [0.3, 0.3, 0.35, 1.0],
                vec![],
            ),
            // Walls (4 sides)
            entity(
                "wall-n",
                [0.0, 2.0, -30.0],
                [60.0, 4.0, 1.0],
                [0.4, 0.4, 0.45, 1.0],
                vec![],
            ),
            entity(
                "wall-s",
                [0.0, 2.0, 30.0],
                [60.0, 4.0, 1.0],
                [0.4, 0.4, 0.45, 1.0],
                vec![],
            ),
            entity(
                "wall-e",
                [30.0, 2.0, 0.0],
                [1.0, 4.0, 60.0],
                [0.4, 0.4, 0.45, 1.0],
                vec![],
            ),
            entity(
                "wall-w",
                [-30.0, 2.0, 0.0],
                [1.0, 4.0, 60.0],
                [0.4, 0.4, 0.45, 1.0],
                vec![],
            ),
            // Ramps
            entity(
                "ramp-1",
                [10.0, 1.0, 10.0],
                [6.0, 0.3, 3.0],
                [0.5, 0.4, 0.3, 1.0],
                vec![ComponentDef::Physics { dynamic: false }],
            ),
            entity(
                "ramp-2",
                [-10.0, 1.5, -8.0],
                [4.0, 0.3, 6.0],
                [0.5, 0.4, 0.3, 1.0],
                vec![ComponentDef::Physics { dynamic: false }],
            ),
            // Cover blocks (dynamic — can be pushed)
            entity(
                "cover-1",
                [5.0, 0.5, -5.0],
                [2.0, 1.0, 2.0],
                [0.6, 0.5, 0.4, 1.0],
                vec![ComponentDef::Physics { dynamic: true }],
            ),
            entity(
                "cover-2",
                [-8.0, 0.5, 3.0],
                [1.5, 1.5, 1.5],
                [0.6, 0.5, 0.4, 1.0],
                vec![ComponentDef::Physics { dynamic: true }],
            ),
            entity(
                "cover-3",
                [0.0, 0.5, 12.0],
                [3.0, 0.8, 1.0],
                [0.6, 0.5, 0.4, 1.0],
                vec![ComponentDef::Physics { dynamic: true }],
            ),
            // Player spawns
            entity(
                "spawn-0",
                [-5.0, 1.0, -5.0],
                [0.8, 1.6, 0.8],
                [0.2, 0.7, 1.0, 1.0],
                vec![
                    ComponentDef::PlayerSpawn,
                    ComponentDef::Physics { dynamic: true },
                ],
            ),
            entity(
                "spawn-1",
                [5.0, 1.0, 5.0],
                [0.8, 1.6, 0.8],
                [1.0, 0.4, 0.2, 1.0],
                vec![
                    ComponentDef::PlayerSpawn,
                    ComponentDef::Physics { dynamic: true },
                ],
            ),
            // NPC enemies
            entity(
                "enemy-1",
                [15.0, 0.8, 0.0],
                [1.0, 1.6, 1.0],
                [0.9, 0.1, 0.1, 1.0],
                vec![ComponentDef::Npc {
                    name: "Sentinel".into(),
                    waypoints: vec![[15.0, 0.8, -10.0], [15.0, 0.8, 10.0]],
                }],
            ),
            entity(
                "enemy-2",
                [-15.0, 0.8, 5.0],
                [1.0, 1.6, 1.0],
                [0.9, 0.1, 0.1, 1.0],
                vec![ComponentDef::Npc {
                    name: "Hunter".into(),
                    waypoints: vec![[-15.0, 0.8, 5.0], [-5.0, 0.8, -10.0], [10.0, 0.8, 8.0]],
                }],
            ),
            entity(
                "enemy-3",
                [0.0, 0.8, -20.0],
                [1.2, 2.0, 1.2],
                [1.0, 0.0, 0.0, 1.0],
                vec![ComponentDef::Npc {
                    name: "Boss".into(),
                    waypoints: vec![[0.0, 0.8, -20.0], [0.0, 0.8, -10.0]],
                }],
            ),
            // Items: health + ammo + gems
            entity(
                "hp-1",
                [8.0, 0.3, -12.0],
                [0.4, 0.6, 0.4],
                [1.0, 0.2, 0.3, 1.0],
                vec![ComponentDef::Item {
                    item_id: "potion-hp".into(),
                    item_name: "Health Potion".into(),
                }],
            ),
            entity(
                "hp-2",
                [-12.0, 0.3, 8.0],
                [0.4, 0.6, 0.4],
                [1.0, 0.2, 0.3, 1.0],
                vec![ComponentDef::Item {
                    item_id: "potion-hp".into(),
                    item_name: "Health Potion".into(),
                }],
            ),
            entity(
                "ammo-1",
                [0.0, 0.3, 0.0],
                [0.3, 0.3, 0.6],
                [0.8, 0.8, 0.2, 1.0],
                vec![ComponentDef::Item {
                    item_id: "ammo-box".into(),
                    item_name: "Ammo Box".into(),
                }],
            ),
            entity(
                "gem-1",
                [20.0, 0.3, 20.0],
                [0.5, 0.5, 0.5],
                [0.0, 0.7, 1.0, 1.0],
                vec![ComponentDef::Item {
                    item_id: "gem-blue".into(),
                    item_name: "Blue Gem".into(),
                }],
            ),
            entity(
                "gem-2",
                [-20.0, 0.3, -20.0],
                [0.5, 0.5, 0.5],
                [1.0, 0.5, 0.0, 1.0],
                vec![ComponentDef::Item {
                    item_id: "gem-gold".into(),
                    item_name: "Gold Gem".into(),
                }],
            ),
            // Portal back to hub
            entity(
                "portal-hub",
                [0.0, 1.5, -28.0],
                [3.0, 3.0, 0.5],
                [0.5, 0.0, 1.0, 0.8],
                vec![ComponentDef::Portal {
                    target_island: "hub".into(),
                }],
            ),
            // Pillars (decoration + cover)
            entity(
                "pillar-1",
                [12.0, 1.5, -12.0],
                [1.0, 3.0, 1.0],
                [0.5, 0.5, 0.55, 1.0],
                vec![],
            ),
            entity(
                "pillar-2",
                [-12.0, 1.5, 12.0],
                [1.0, 3.0, 1.0],
                [0.5, 0.5, 0.55, 1.0],
                vec![],
            ),
            entity(
                "pillar-3",
                [20.0, 1.5, 0.0],
                [1.0, 3.0, 1.0],
                [0.5, 0.5, 0.55, 1.0],
                vec![],
            ),
            entity(
                "pillar-4",
                [-20.0, 1.5, 0.0],
                [1.0, 3.0, 1.0],
                [0.5, 0.5, 0.55, 1.0],
                vec![],
            ),
        ],
    }
}

fn entity(
    id: &str,
    pos: [f32; 3],
    scale: [f32; 3],
    color: [f32; 4],
    components: Vec<ComponentDef>,
) -> EntityDef {
    EntityDef {
        id: id.into(),
        position: pos,
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale,
        mesh: MeshRef::Cube { color },
        components,
        layer: None,
    }
}

/// Projectile definition for shooting mechanic.
#[derive(Debug, Clone)]
pub struct Projectile {
    pub position: Vec3,
    pub velocity: Vec3,
    pub damage: u16,
    pub lifetime: f32,
    pub owner_id: u32,
}

impl Projectile {
    pub fn new(origin: Vec3, direction: Vec3, speed: f32, damage: u16, owner_id: u32) -> Self {
        Self {
            position: origin,
            velocity: direction.normalize_or_zero() * speed,
            damage,
            lifetime: 3.0,
            owner_id,
        }
    }

    /// Advance projectile. Returns false if expired.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.position += self.velocity * dt;
        self.lifetime -= dt;
        self.lifetime > 0.0
    }
}

/// Score tracking per player.
#[derive(Debug, Clone, Default)]
pub struct ScoreBoard {
    pub scores: Vec<(u32, PlayerScore)>, // (client_id, score)
}

#[derive(Debug, Clone, Default)]
pub struct PlayerScore {
    pub kills: u32,
    pub deaths: u32,
    pub gems: u32,
    pub damage_dealt: u32,
}

impl ScoreBoard {
    pub fn add_kill(&mut self, client_id: u32) {
        self.ensure(client_id).kills += 1;
    }
    pub fn add_death(&mut self, client_id: u32) {
        self.ensure(client_id).deaths += 1;
    }
    pub fn add_gem(&mut self, client_id: u32) {
        self.ensure(client_id).gems += 1;
    }
    pub fn add_damage(&mut self, client_id: u32, amount: u32) {
        self.ensure(client_id).damage_dealt += amount;
    }

    fn ensure(&mut self, client_id: u32) -> &mut PlayerScore {
        if let Some(pos) = self.scores.iter().position(|(id, _)| *id == client_id) {
            &mut self.scores[pos].1
        } else {
            self.scores.push((client_id, PlayerScore::default()));
            &mut self.scores.last_mut().unwrap().1
        }
    }

    /// Get sorted leaderboard (by kills desc).
    pub fn leaderboard(&self) -> Vec<(u32, &PlayerScore)> {
        let mut sorted: Vec<_> = self.scores.iter().map(|(id, s)| (*id, s)).collect();
        sorted.sort_by(|a, b| b.1.kills.cmp(&a.1.kills));
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_scene_valid() {
        let scene = arena_island();
        assert_eq!(scene.name, "Battle Arena");
        assert!(scene.entities.len() >= 20);
        // Has spawns, NPCs, items, portal
        assert!(scene.entities.iter().any(|e| {
            e.components
                .iter()
                .any(|c| matches!(c, ComponentDef::PlayerSpawn))
        }));
        assert!(scene.entities.iter().any(|e| {
            e.components
                .iter()
                .any(|c| matches!(c, ComponentDef::Npc { .. }))
        }));
        assert!(scene.entities.iter().any(|e| {
            e.components
                .iter()
                .any(|c| matches!(c, ComponentDef::Portal { .. }))
        }));
    }

    #[test]
    fn projectile_lifetime() {
        let mut p = Projectile::new(Vec3::ZERO, Vec3::X, 20.0, 10, 1);
        assert!(p.tick(1.0 / 60.0)); // alive
        assert!(!p.tick(10.0)); // expired
    }

    #[test]
    fn scoreboard() {
        let mut sb = ScoreBoard::default();
        sb.add_kill(1);
        sb.add_kill(1);
        sb.add_kill(2);
        sb.add_gem(2);
        let lb = sb.leaderboard();
        assert_eq!(lb[0].0, 1); // player 1 has 2 kills
        assert_eq!(lb[0].1.kills, 2);
    }
}
