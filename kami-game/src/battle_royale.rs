//! Battle Royale: Fortnite-style 100-player last-one-standing with storm, loot, building.
//!
//! Server-authoritative match simulation. Storm circle shrinks over phases.
//! Players loot weapons/shields/materials, build structures, and fight to be #1.

use crate::inventory::{ItemDef, ItemType, Rarity};
use crate::scene::{ComponentDef, EntityDef, IslandScene, MeshRef};
use glam::Vec3;
use serde::{Deserialize, Serialize};

// ── Constants ──

pub const MAX_PLAYERS: u32 = 100;
pub const MATCH_TICK_RATE: u32 = 60;
pub const MAP_SIZE: f32 = 2000.0; // 2km × 2km
pub const BUS_ALTITUDE: f32 = 200.0;
pub const BUS_SPEED: f32 = 80.0;
pub const GLIDE_SPEED: f32 = 30.0;
pub const DEPLOY_ALTITUDE: f32 = 50.0;
pub const DBNO_DURATION: f32 = 30.0;
pub const DBNO_BLEED_DPS: f32 = 3.33;

// ── Match Phases ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchPhase {
    /// Pre-game lobby on spawn island
    Warmup,
    /// Battle Bus flying across map
    BattleBus,
    /// Players gliding/landing — storm not yet moving
    EarlyGame,
    /// Storm circles are shrinking
    MidGame,
    /// Final circles — fast shrink, high damage
    EndGame,
    /// Match over — Victory Royale
    Victory,
}

/// Storm circle configuration per phase.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StormPhaseConfig {
    pub phase_index: u8,
    pub wait_seconds: f32,
    pub shrink_seconds: f32,
    pub end_radius: f32,
    pub damage_per_second: f32,
}

pub fn default_storm_phases() -> Vec<StormPhaseConfig> {
    vec![
        StormPhaseConfig {
            phase_index: 0,
            wait_seconds: 120.0,
            shrink_seconds: 90.0,
            end_radius: 700.0,
            damage_per_second: 1.0,
        },
        StormPhaseConfig {
            phase_index: 1,
            wait_seconds: 90.0,
            shrink_seconds: 75.0,
            end_radius: 450.0,
            damage_per_second: 2.0,
        },
        StormPhaseConfig {
            phase_index: 2,
            wait_seconds: 75.0,
            shrink_seconds: 60.0,
            end_radius: 280.0,
            damage_per_second: 5.0,
        },
        StormPhaseConfig {
            phase_index: 3,
            wait_seconds: 60.0,
            shrink_seconds: 45.0,
            end_radius: 150.0,
            damage_per_second: 8.0,
        },
        StormPhaseConfig {
            phase_index: 4,
            wait_seconds: 45.0,
            shrink_seconds: 30.0,
            end_radius: 70.0,
            damage_per_second: 10.0,
        },
        StormPhaseConfig {
            phase_index: 5,
            wait_seconds: 30.0,
            shrink_seconds: 20.0,
            end_radius: 25.0,
            damage_per_second: 15.0,
        },
        StormPhaseConfig {
            phase_index: 6,
            wait_seconds: 20.0,
            shrink_seconds: 15.0,
            end_radius: 5.0,
            damage_per_second: 20.0,
        },
        StormPhaseConfig {
            phase_index: 7,
            wait_seconds: 15.0,
            shrink_seconds: 10.0,
            end_radius: 0.0,
            damage_per_second: 25.0,
        },
    ]
}

// ── Storm Circle ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StormCircle {
    pub current_center: Vec3,
    pub current_radius: f32,
    pub target_center: Vec3,
    pub target_radius: f32,
    pub phase_index: u8,
    pub phase_timer: f32,
    pub shrinking: bool,
    pub damage_per_second: f32,
    phases: Vec<StormPhaseConfig>,
}

impl StormCircle {
    pub fn new(map_center: Vec3) -> Self {
        let phases = default_storm_phases();
        Self {
            current_center: map_center,
            current_radius: MAP_SIZE * 0.5,
            target_center: map_center,
            target_radius: MAP_SIZE * 0.5,
            phase_index: 0,
            phase_timer: 0.0,
            shrinking: false,
            damage_per_second: 1.0,
            phases,
        }
    }

    /// Advance storm by dt seconds. Returns true if phase changed.
    pub fn tick(&mut self, dt: f32) -> bool {
        let pi = self.phase_index as usize;
        if pi >= self.phases.len() {
            return false;
        }
        let cfg = self.phases[pi];
        self.phase_timer += dt;

        if !self.shrinking {
            // Waiting phase
            if self.phase_timer >= cfg.wait_seconds {
                self.shrinking = true;
                self.phase_timer = 0.0;
                self.target_radius = cfg.end_radius;
                // Shift target center toward a random-ish offset (deterministic from phase)
                let offset_scale = cfg.end_radius * 0.3;
                let angle = (pi as f32) * 1.618 * std::f32::consts::TAU;
                self.target_center = Vec3::new(
                    self.current_center.x + offset_scale * angle.cos(),
                    0.0,
                    self.current_center.z + offset_scale * angle.sin(),
                );
                self.damage_per_second = cfg.damage_per_second;
            }
            false
        } else {
            // Shrinking phase
            let t = (self.phase_timer / cfg.shrink_seconds).min(1.0);
            let start_radius = if pi > 0 {
                self.phases[pi - 1].end_radius
            } else {
                MAP_SIZE * 0.5
            };
            self.current_radius = start_radius + (self.target_radius - start_radius) * t;
            self.current_center = self.current_center.lerp(self.target_center, t * dt * 0.1);

            if self.phase_timer >= cfg.shrink_seconds {
                self.current_radius = self.target_radius;
                self.current_center = self.target_center;
                self.phase_index += 1;
                self.phase_timer = 0.0;
                self.shrinking = false;
                if (self.phase_index as usize) < self.phases.len() {
                    self.damage_per_second =
                        self.phases[self.phase_index as usize].damage_per_second;
                }
                return true;
            }
            false
        }
    }

    /// Check if a position is inside the safe zone.
    pub fn is_inside(&self, pos: Vec3) -> bool {
        let dx = pos.x - self.current_center.x;
        let dz = pos.z - self.current_center.z;
        (dx * dx + dz * dz).sqrt() <= self.current_radius
    }

    /// Get storm damage at a position (0 if inside safe zone).
    pub fn damage_at(&self, pos: Vec3) -> f32 {
        if self.is_inside(pos) {
            0.0
        } else {
            self.damage_per_second
        }
    }

    /// Distance from position to safe zone edge (negative = inside).
    pub fn distance_to_edge(&self, pos: Vec3) -> f32 {
        let dx = pos.x - self.current_center.x;
        let dz = pos.z - self.current_center.z;
        (dx * dx + dz * dz).sqrt() - self.current_radius
    }
}

// ── Weapons ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeaponType {
    AssaultRifle,
    Shotgun,
    SMG,
    SniperRifle,
    Pistol,
    RocketLauncher,
    GrenadeLauncher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponDef {
    pub weapon_type: WeaponType,
    pub name: String,
    pub rarity: Rarity,
    pub damage: u16,
    pub headshot_multiplier: f32,
    pub fire_rate: f32, // rounds per second
    pub magazine_size: u16,
    pub reload_time: f32,    // seconds
    pub spread: f32,         // degrees
    pub damage_falloff: f32, // distance where damage halves
    pub range: f32,
    pub projectile_speed: f32,
}

pub fn weapon_pool() -> Vec<WeaponDef> {
    vec![
        // Assault Rifles
        WeaponDef {
            weapon_type: WeaponType::AssaultRifle,
            name: "Assault Rifle".into(),
            rarity: Rarity::Common,
            damage: 30,
            headshot_multiplier: 1.5,
            fire_rate: 5.5,
            magazine_size: 30,
            reload_time: 2.3,
            spread: 2.5,
            damage_falloff: 50.0,
            range: 200.0,
            projectile_speed: 500.0,
        },
        WeaponDef {
            weapon_type: WeaponType::AssaultRifle,
            name: "Assault Rifle".into(),
            rarity: Rarity::Uncommon,
            damage: 31,
            headshot_multiplier: 1.5,
            fire_rate: 5.5,
            magazine_size: 30,
            reload_time: 2.2,
            spread: 2.3,
            damage_falloff: 55.0,
            range: 200.0,
            projectile_speed: 500.0,
        },
        WeaponDef {
            weapon_type: WeaponType::AssaultRifle,
            name: "Assault Rifle".into(),
            rarity: Rarity::Rare,
            damage: 33,
            headshot_multiplier: 1.5,
            fire_rate: 5.5,
            magazine_size: 30,
            reload_time: 2.1,
            spread: 2.0,
            damage_falloff: 60.0,
            range: 200.0,
            projectile_speed: 500.0,
        },
        WeaponDef {
            weapon_type: WeaponType::AssaultRifle,
            name: "SCAR".into(),
            rarity: Rarity::Epic,
            damage: 35,
            headshot_multiplier: 1.5,
            fire_rate: 5.5,
            magazine_size: 30,
            reload_time: 2.0,
            spread: 1.5,
            damage_falloff: 65.0,
            range: 200.0,
            projectile_speed: 500.0,
        },
        WeaponDef {
            weapon_type: WeaponType::AssaultRifle,
            name: "SCAR".into(),
            rarity: Rarity::Legendary,
            damage: 36,
            headshot_multiplier: 1.5,
            fire_rate: 5.5,
            magazine_size: 30,
            reload_time: 2.0,
            spread: 1.2,
            damage_falloff: 70.0,
            range: 200.0,
            projectile_speed: 500.0,
        },
        // Shotguns
        WeaponDef {
            weapon_type: WeaponType::Shotgun,
            name: "Pump Shotgun".into(),
            rarity: Rarity::Common,
            damage: 80,
            headshot_multiplier: 2.0,
            fire_rate: 0.7,
            magazine_size: 5,
            reload_time: 4.5,
            spread: 6.0,
            damage_falloff: 10.0,
            range: 30.0,
            projectile_speed: 400.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Shotgun,
            name: "Pump Shotgun".into(),
            rarity: Rarity::Uncommon,
            damage: 85,
            headshot_multiplier: 2.0,
            fire_rate: 0.7,
            magazine_size: 5,
            reload_time: 4.3,
            spread: 5.5,
            damage_falloff: 12.0,
            range: 30.0,
            projectile_speed: 400.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Shotgun,
            name: "Pump Shotgun".into(),
            rarity: Rarity::Rare,
            damage: 90,
            headshot_multiplier: 2.0,
            fire_rate: 0.7,
            magazine_size: 5,
            reload_time: 4.0,
            spread: 5.0,
            damage_falloff: 14.0,
            range: 30.0,
            projectile_speed: 400.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Shotgun,
            name: "Spaz-12".into(),
            rarity: Rarity::Epic,
            damage: 100,
            headshot_multiplier: 2.0,
            fire_rate: 0.7,
            magazine_size: 5,
            reload_time: 3.8,
            spread: 4.5,
            damage_falloff: 15.0,
            range: 30.0,
            projectile_speed: 400.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Shotgun,
            name: "Spaz-12".into(),
            rarity: Rarity::Legendary,
            damage: 110,
            headshot_multiplier: 2.0,
            fire_rate: 0.7,
            magazine_size: 5,
            reload_time: 3.5,
            spread: 4.0,
            damage_falloff: 16.0,
            range: 30.0,
            projectile_speed: 400.0,
        },
        // SMGs
        WeaponDef {
            weapon_type: WeaponType::SMG,
            name: "SMG".into(),
            rarity: Rarity::Common,
            damage: 17,
            headshot_multiplier: 1.5,
            fire_rate: 12.0,
            magazine_size: 30,
            reload_time: 2.0,
            spread: 3.5,
            damage_falloff: 25.0,
            range: 100.0,
            projectile_speed: 450.0,
        },
        WeaponDef {
            weapon_type: WeaponType::SMG,
            name: "SMG".into(),
            rarity: Rarity::Uncommon,
            damage: 18,
            headshot_multiplier: 1.5,
            fire_rate: 12.0,
            magazine_size: 30,
            reload_time: 1.9,
            spread: 3.2,
            damage_falloff: 28.0,
            range: 100.0,
            projectile_speed: 450.0,
        },
        WeaponDef {
            weapon_type: WeaponType::SMG,
            name: "Rapid Fire SMG".into(),
            rarity: Rarity::Rare,
            damage: 15,
            headshot_multiplier: 1.5,
            fire_rate: 15.0,
            magazine_size: 26,
            reload_time: 1.7,
            spread: 4.0,
            damage_falloff: 22.0,
            range: 80.0,
            projectile_speed: 450.0,
        },
        // Snipers
        WeaponDef {
            weapon_type: WeaponType::SniperRifle,
            name: "Bolt-Action Sniper".into(),
            rarity: Rarity::Rare,
            damage: 105,
            headshot_multiplier: 2.5,
            fire_rate: 0.33,
            magazine_size: 1,
            reload_time: 3.0,
            spread: 0.0,
            damage_falloff: 200.0,
            range: 500.0,
            projectile_speed: 800.0,
        },
        WeaponDef {
            weapon_type: WeaponType::SniperRifle,
            name: "Heavy Sniper".into(),
            rarity: Rarity::Epic,
            damage: 132,
            headshot_multiplier: 2.5,
            fire_rate: 0.25,
            magazine_size: 1,
            reload_time: 4.0,
            spread: 0.0,
            damage_falloff: 250.0,
            range: 500.0,
            projectile_speed: 900.0,
        },
        WeaponDef {
            weapon_type: WeaponType::SniperRifle,
            name: "Heavy Sniper".into(),
            rarity: Rarity::Legendary,
            damage: 150,
            headshot_multiplier: 2.5,
            fire_rate: 0.25,
            magazine_size: 1,
            reload_time: 4.0,
            spread: 0.0,
            damage_falloff: 250.0,
            range: 500.0,
            projectile_speed: 900.0,
        },
        // Pistols
        WeaponDef {
            weapon_type: WeaponType::Pistol,
            name: "Pistol".into(),
            rarity: Rarity::Common,
            damage: 24,
            headshot_multiplier: 1.5,
            fire_rate: 6.75,
            magazine_size: 16,
            reload_time: 1.3,
            spread: 3.0,
            damage_falloff: 30.0,
            range: 100.0,
            projectile_speed: 400.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Pistol,
            name: "Pistol".into(),
            rarity: Rarity::Uncommon,
            damage: 25,
            headshot_multiplier: 1.5,
            fire_rate: 6.75,
            magazine_size: 16,
            reload_time: 1.2,
            spread: 2.8,
            damage_falloff: 35.0,
            range: 100.0,
            projectile_speed: 400.0,
        },
        // Explosives
        WeaponDef {
            weapon_type: WeaponType::RocketLauncher,
            name: "Rocket Launcher".into(),
            rarity: Rarity::Epic,
            damage: 110,
            headshot_multiplier: 1.0,
            fire_rate: 0.75,
            magazine_size: 1,
            reload_time: 3.0,
            spread: 0.0,
            damage_falloff: 0.0,
            range: 300.0,
            projectile_speed: 100.0,
        },
        WeaponDef {
            weapon_type: WeaponType::RocketLauncher,
            name: "Rocket Launcher".into(),
            rarity: Rarity::Legendary,
            damage: 121,
            headshot_multiplier: 1.0,
            fire_rate: 0.75,
            magazine_size: 1,
            reload_time: 2.8,
            spread: 0.0,
            damage_falloff: 0.0,
            range: 300.0,
            projectile_speed: 100.0,
        },
        // Brainrot weapons
        WeaponDef {
            weapon_type: WeaponType::RocketLauncher,
            name: "Skibidi Launcher".into(),
            rarity: Rarity::Epic,
            damage: 125,
            headshot_multiplier: 1.0,
            fire_rate: 0.6,
            magazine_size: 1,
            reload_time: 3.5,
            spread: 0.0,
            damage_falloff: 0.0,
            range: 250.0,
            projectile_speed: 90.0,
        },
        WeaponDef {
            weapon_type: WeaponType::AssaultRifle,
            name: "Ohio Anomaly Rifle".into(),
            rarity: Rarity::Legendary,
            damage: 38,
            headshot_multiplier: 2.0,
            fire_rate: 6.0,
            magazine_size: 25,
            reload_time: 1.8,
            spread: 1.0,
            damage_falloff: 80.0,
            range: 250.0,
            projectile_speed: 550.0,
        },
        WeaponDef {
            weapon_type: WeaponType::SniperRifle,
            name: "Sigma Sniper".into(),
            rarity: Rarity::Legendary,
            damage: 165,
            headshot_multiplier: 3.0,
            fire_rate: 0.2,
            magazine_size: 1,
            reload_time: 4.5,
            spread: 0.0,
            damage_falloff: 300.0,
            range: 600.0,
            projectile_speed: 1000.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Pistol,
            name: "Rizz Pistol".into(),
            rarity: Rarity::Epic,
            damage: 30,
            headshot_multiplier: 1.5,
            fire_rate: 8.0,
            magazine_size: 12,
            reload_time: 1.0,
            spread: 2.0,
            damage_falloff: 40.0,
            range: 120.0,
            projectile_speed: 450.0,
        },
        WeaponDef {
            weapon_type: WeaponType::Shotgun,
            name: "Fanum Shotgun".into(),
            rarity: Rarity::Legendary,
            damage: 120,
            headshot_multiplier: 2.0,
            fire_rate: 0.8,
            magazine_size: 6,
            reload_time: 3.0,
            spread: 3.5,
            damage_falloff: 18.0,
            range: 35.0,
            projectile_speed: 420.0,
        },
    ]
}

// ── Consumables ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsumableType {
    SmallShield,  // +25 shield, cap 50
    LargeShield,  // +50 shield, cap 100
    MiniHP,       // +15 HP
    Medkit,       // full HP (takes 10s)
    Chug,         // full HP + shield (takes 15s)
    SmallFry,     // +25 HP, cap 75
    Flopper,      // full HP
    ShieldFish,   // +50 shield
    GrimaceShake, // full HP + full shield (takes 8s)
    GyattEnergy,  // instant 75 shield
    OhioMilk,     // +50 HP
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableDef {
    pub consumable_type: ConsumableType,
    pub name: String,
    pub rarity: Rarity,
    pub use_time: f32,
    pub hp_restore: u16,
    pub shield_restore: u16,
    pub hp_cap: u16,
    pub shield_cap: u16,
    pub stack_size: u16,
}

pub fn consumable_pool() -> Vec<ConsumableDef> {
    vec![
        ConsumableDef {
            consumable_type: ConsumableType::SmallShield,
            name: "Mini Shield".into(),
            rarity: Rarity::Common,
            use_time: 2.0,
            hp_restore: 0,
            shield_restore: 25,
            hp_cap: 100,
            shield_cap: 50,
            stack_size: 6,
        },
        ConsumableDef {
            consumable_type: ConsumableType::LargeShield,
            name: "Shield Potion".into(),
            rarity: Rarity::Uncommon,
            use_time: 5.0,
            hp_restore: 0,
            shield_restore: 50,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 2,
        },
        ConsumableDef {
            consumable_type: ConsumableType::MiniHP,
            name: "Bandage".into(),
            rarity: Rarity::Common,
            use_time: 3.5,
            hp_restore: 15,
            shield_restore: 0,
            hp_cap: 75,
            shield_cap: 100,
            stack_size: 15,
        },
        ConsumableDef {
            consumable_type: ConsumableType::Medkit,
            name: "Med Kit".into(),
            rarity: Rarity::Uncommon,
            use_time: 10.0,
            hp_restore: 100,
            shield_restore: 0,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 3,
        },
        ConsumableDef {
            consumable_type: ConsumableType::Chug,
            name: "Chug Jug".into(),
            rarity: Rarity::Legendary,
            use_time: 15.0,
            hp_restore: 100,
            shield_restore: 100,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 1,
        },
        ConsumableDef {
            consumable_type: ConsumableType::SmallFry,
            name: "Small Fry".into(),
            rarity: Rarity::Common,
            use_time: 1.0,
            hp_restore: 25,
            shield_restore: 0,
            hp_cap: 75,
            shield_cap: 100,
            stack_size: 6,
        },
        ConsumableDef {
            consumable_type: ConsumableType::Flopper,
            name: "Flopper".into(),
            rarity: Rarity::Uncommon,
            use_time: 1.0,
            hp_restore: 100,
            shield_restore: 0,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 4,
        },
        ConsumableDef {
            consumable_type: ConsumableType::ShieldFish,
            name: "Shield Fish".into(),
            rarity: Rarity::Rare,
            use_time: 1.0,
            hp_restore: 0,
            shield_restore: 50,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 3,
        },
        // Brainrot consumables
        ConsumableDef {
            consumable_type: ConsumableType::GrimaceShake,
            name: "Grimace Shake".into(),
            rarity: Rarity::Legendary,
            use_time: 8.0,
            hp_restore: 100,
            shield_restore: 100,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 1,
        },
        ConsumableDef {
            consumable_type: ConsumableType::GyattEnergy,
            name: "Gyatt Energy".into(),
            rarity: Rarity::Rare,
            use_time: 0.5,
            hp_restore: 0,
            shield_restore: 75,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 3,
        },
        ConsumableDef {
            consumable_type: ConsumableType::OhioMilk,
            name: "Ohio Milk".into(),
            rarity: Rarity::Uncommon,
            use_time: 2.0,
            hp_restore: 50,
            shield_restore: 0,
            hp_cap: 100,
            shield_cap: 100,
            stack_size: 5,
        },
    ]
}

// ── Building ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildPiece {
    Wall,
    Floor,
    Ramp,
    Roof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialType {
    Wood,
    Brick,
    Metal,
}

impl MaterialType {
    pub fn initial_hp(self) -> u16 {
        match self {
            MaterialType::Wood => 90,
            MaterialType::Brick => 100,
            MaterialType::Metal => 110,
        }
    }

    pub fn max_hp(self) -> u16 {
        match self {
            MaterialType::Wood => 150,
            MaterialType::Brick => 300,
            MaterialType::Metal => 500,
        }
    }

    pub fn build_time(self) -> f32 {
        match self {
            MaterialType::Wood => 4.0,
            MaterialType::Brick => 12.0,
            MaterialType::Metal => 20.0,
        }
    }

    pub fn cost(self) -> u16 {
        10
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStructure {
    pub id: u32,
    pub piece: BuildPiece,
    pub material: MaterialType,
    pub position: Vec3,
    pub rotation_y: f32,
    pub current_hp: u16,
    pub max_hp: u16,
    pub building: bool,
    pub build_progress: f32,
    pub owner_id: u32,
}

impl BuildStructure {
    pub fn new(
        id: u32,
        piece: BuildPiece,
        material: MaterialType,
        position: Vec3,
        rotation_y: f32,
        owner_id: u32,
    ) -> Self {
        Self {
            id,
            piece,
            material,
            position,
            rotation_y,
            current_hp: material.initial_hp(),
            max_hp: material.max_hp(),
            building: true,
            build_progress: 0.0,
            owner_id,
        }
    }

    /// Advance build. Returns true when complete.
    pub fn tick_build(&mut self, dt: f32) -> bool {
        if !self.building {
            return false;
        }
        self.build_progress += dt / self.material.build_time();
        if self.build_progress >= 1.0 {
            self.building = false;
            self.build_progress = 1.0;
            self.current_hp = self.max_hp;
            return true;
        }
        // HP scales with build progress
        let target_hp = self.material.initial_hp() as f32
            + (self.max_hp as f32 - self.material.initial_hp() as f32) * self.build_progress;
        self.current_hp = target_hp as u16;
        false
    }

    /// Apply damage. Returns true if destroyed.
    pub fn take_damage(&mut self, damage: u16) -> bool {
        self.current_hp = self.current_hp.saturating_sub(damage);
        self.current_hp == 0
    }

    pub fn dimensions(&self) -> Vec3 {
        match self.piece {
            BuildPiece::Wall => Vec3::new(5.12, 2.8, 0.15),
            BuildPiece::Floor => Vec3::new(5.12, 0.15, 5.12),
            BuildPiece::Ramp => Vec3::new(5.12, 2.8, 5.12),
            BuildPiece::Roof => Vec3::new(5.12, 0.15, 5.12),
        }
    }
}

// ── Player State ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerStatus {
    InLobby,
    OnBus,
    Gliding,
    Alive,
    DBNO, // Down But Not Out (solo: skip → eliminated)
    Eliminated,
    Spectating,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BRPlayer {
    pub client_id: u32,
    pub did: String,
    pub display_name: String,
    pub status: PlayerStatus,
    pub position: Vec3,
    pub rotation_y: f32,
    pub hp: u16,
    pub shield: u16,
    pub max_hp: u16,
    pub max_shield: u16,
    pub wood: u16,
    pub brick: u16,
    pub metal: u16,
    pub weapon_slots: [Option<WeaponDef>; 5],
    pub active_slot: u8,
    pub ammo_light: u16,  // SMG, pistol
    pub ammo_medium: u16, // AR
    pub ammo_heavy: u16,  // sniper
    pub ammo_shells: u16, // shotgun
    pub ammo_rockets: u16,
    pub kills: u16,
    pub assists: u16,
    pub damage_dealt: u32,
    pub placement: u16,
    pub dbno_timer: f32,
    pub eliminated_by: Option<u32>,
    pub bus_jumped: bool,
    pub landed: bool,
}

impl BRPlayer {
    pub fn new(client_id: u32, did: &str, display_name: &str) -> Self {
        Self {
            client_id,
            did: did.to_string(),
            display_name: display_name.to_string(),
            status: PlayerStatus::InLobby,
            position: Vec3::ZERO,
            rotation_y: 0.0,
            hp: 100,
            shield: 0,
            max_hp: 100,
            max_shield: 100,
            wood: 0,
            brick: 0,
            metal: 0,
            weapon_slots: [None, None, None, None, None],
            active_slot: 0,
            ammo_light: 0,
            ammo_medium: 0,
            ammo_heavy: 0,
            ammo_shells: 0,
            ammo_rockets: 0,
            kills: 0,
            assists: 0,
            damage_dealt: 0,
            placement: 0,
            dbno_timer: 0.0,
            eliminated_by: None,
            bus_jumped: false,
            landed: false,
        }
    }

    pub fn effective_hp(&self) -> u16 {
        self.hp + self.shield
    }

    pub fn is_alive(&self) -> bool {
        matches!(
            self.status,
            PlayerStatus::Alive | PlayerStatus::Gliding | PlayerStatus::OnBus
        )
    }

    /// Apply damage (shield first). Returns actual damage dealt.
    pub fn take_damage(&mut self, damage: u16, attacker_id: u32) -> u16 {
        if !matches!(self.status, PlayerStatus::Alive | PlayerStatus::DBNO) {
            return 0;
        }
        let mut remaining = damage;
        // Shield absorbs first
        let shield_absorbed = remaining.min(self.shield);
        self.shield -= shield_absorbed;
        remaining -= shield_absorbed;
        // Then HP
        let hp_absorbed = remaining.min(self.hp);
        self.hp -= hp_absorbed;
        let total = shield_absorbed + hp_absorbed;

        if self.hp == 0 {
            match self.status {
                PlayerStatus::Alive => {
                    // In solos, go directly to eliminated
                    self.status = PlayerStatus::Eliminated;
                    self.eliminated_by = Some(attacker_id);
                }
                PlayerStatus::DBNO => {
                    self.status = PlayerStatus::Eliminated;
                    self.eliminated_by = Some(attacker_id);
                }
                _ => {}
            }
        }
        total
    }

    /// Heal HP (capped).
    pub fn heal(&mut self, amount: u16, cap: u16) {
        self.hp = (self.hp + amount).min(cap).min(self.max_hp);
    }

    /// Add shield (capped).
    pub fn add_shield(&mut self, amount: u16, cap: u16) {
        self.shield = (self.shield + amount).min(cap).min(self.max_shield);
    }

    /// Harvest materials from objects.
    pub fn harvest(&mut self, material: MaterialType, amount: u16) {
        let cap = 999u16;
        match material {
            MaterialType::Wood => self.wood = (self.wood + amount).min(cap),
            MaterialType::Brick => self.brick = (self.brick + amount).min(cap),
            MaterialType::Metal => self.metal = (self.metal + amount).min(cap),
        }
    }

    pub fn can_build(&self, material: MaterialType) -> bool {
        let cost = material.cost();
        match material {
            MaterialType::Wood => self.wood >= cost,
            MaterialType::Brick => self.brick >= cost,
            MaterialType::Metal => self.metal >= cost,
        }
    }

    pub fn spend_material(&mut self, material: MaterialType) -> bool {
        let cost = material.cost();
        match material {
            MaterialType::Wood => {
                if self.wood < cost {
                    return false;
                }
                self.wood -= cost;
                true
            }
            MaterialType::Brick => {
                if self.brick < cost {
                    return false;
                }
                self.brick -= cost;
                true
            }
            MaterialType::Metal => {
                if self.metal < cost {
                    return false;
                }
                self.metal -= cost;
                true
            }
        }
    }

    pub fn pick_up_weapon(&mut self, weapon: WeaponDef) -> Option<WeaponDef> {
        // Find empty slot
        for slot in self.weapon_slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(weapon);
                return None;
            }
        }
        // Swap with active slot
        let idx = self.active_slot as usize;
        let dropped = self.weapon_slots[idx].take();
        self.weapon_slots[idx] = Some(weapon);
        dropped
    }
}

// ── Kill Feed ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillFeedEntry {
    pub tick: u32,
    pub eliminator_name: String,
    pub eliminator_id: u32,
    pub victim_name: String,
    pub victim_id: u32,
    pub weapon_name: String,
    pub headshot: bool,
    pub distance: f32,
}

// ── Loot Spawn ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LootType {
    Weapon,
    Consumable,
    Ammo,
    Material,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootSpawn {
    pub id: u32,
    pub position: Vec3,
    pub loot_type: LootType,
    pub item_data: String, // JSON weapon/consumable def
    pub rarity: Rarity,
    pub picked_up: bool,
}

// ── Battle Bus ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleBus {
    pub start: Vec3,
    pub end: Vec3,
    pub speed: f32,
    pub progress: f32,
    pub active: bool,
}

impl BattleBus {
    pub fn new(map_seed: u64) -> Self {
        let angle = (map_seed % 360) as f32 * std::f32::consts::PI / 180.0;
        let half_map = MAP_SIZE * 0.5;
        let start = Vec3::new(
            -half_map * angle.cos(),
            BUS_ALTITUDE,
            -half_map * angle.sin(),
        );
        let end = Vec3::new(half_map * angle.cos(), BUS_ALTITUDE, half_map * angle.sin());
        Self {
            start,
            end,
            speed: BUS_SPEED,
            progress: 0.0,
            active: true,
        }
    }

    pub fn current_position(&self) -> Vec3 {
        self.start.lerp(self.end, self.progress)
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.active {
            return false;
        }
        let total_dist = self.start.distance(self.end);
        self.progress += (self.speed * dt) / total_dist;
        if self.progress >= 1.0 {
            self.progress = 1.0;
            self.active = false;
            return true; // bus finished
        }
        false
    }
}

// ── Match State ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BRMatchState {
    pub match_id: String,
    pub phase: MatchPhase,
    pub tick: u32,
    pub elapsed_seconds: f32,
    pub storm: StormCircle,
    pub bus: BattleBus,
    pub players: Vec<BRPlayer>,
    pub structures: Vec<BuildStructure>,
    pub loot_spawns: Vec<LootSpawn>,
    pub kill_feed: Vec<KillFeedEntry>,
    pub alive_count: u32,
    pub next_structure_id: u32,
    pub next_loot_id: u32,
    pub winner_id: Option<u32>,
    pub map_seed: u64,
}

impl BRMatchState {
    pub fn new(match_id: &str, map_seed: u64) -> Self {
        Self {
            match_id: match_id.to_string(),
            phase: MatchPhase::Warmup,
            tick: 0,
            elapsed_seconds: 0.0,
            storm: StormCircle::new(Vec3::ZERO),
            bus: BattleBus::new(map_seed),
            players: Vec::new(),
            structures: Vec::new(),
            loot_spawns: Vec::new(),
            kill_feed: Vec::new(),
            alive_count: 0,
            next_structure_id: 1,
            next_loot_id: 1,
            winner_id: None,
            map_seed,
        }
    }

    pub fn add_player(&mut self, client_id: u32, did: &str, name: &str) -> bool {
        if self.players.len() >= MAX_PLAYERS as usize {
            return false;
        }
        if self.phase != MatchPhase::Warmup {
            return false;
        }
        self.players.push(BRPlayer::new(client_id, did, name));
        true
    }

    /// Start the match: transition from Warmup to BattleBus.
    pub fn start_match(&mut self) {
        self.phase = MatchPhase::BattleBus;
        self.alive_count = self.players.len() as u32;
        for p in &mut self.players {
            p.status = PlayerStatus::OnBus;
            p.position = self.bus.current_position();
        }
    }

    /// Main server tick. dt = 1/60.
    pub fn tick(&mut self, dt: f32) {
        self.tick += 1;
        self.elapsed_seconds += dt;

        match self.phase {
            MatchPhase::Warmup => {}
            MatchPhase::BattleBus => {
                self.tick_bus(dt);
            }
            MatchPhase::EarlyGame | MatchPhase::MidGame | MatchPhase::EndGame => {
                self.tick_storm(dt);
                self.tick_storm_damage(dt);
                self.tick_dbno(dt);
                self.tick_structures(dt);
                self.check_victory();
            }
            MatchPhase::Victory => {}
        }
    }

    fn tick_bus(&mut self, dt: f32) {
        let bus_finished = self.bus.tick(dt);
        // Update bus riders
        let bus_pos = self.bus.current_position();
        for p in &mut self.players {
            if p.status == PlayerStatus::OnBus && !p.bus_jumped {
                p.position = bus_pos;
            }
        }
        // Force-drop remaining riders when bus finishes
        if bus_finished {
            for p in &mut self.players {
                if p.status == PlayerStatus::OnBus {
                    p.status = PlayerStatus::Gliding;
                    p.bus_jumped = true;
                }
            }
            self.phase = MatchPhase::EarlyGame;
        }
    }

    fn tick_storm(&mut self, dt: f32) {
        let phase_changed = self.storm.tick(dt);
        if phase_changed {
            match self.storm.phase_index {
                0..=2 => self.phase = MatchPhase::EarlyGame,
                3..=5 => self.phase = MatchPhase::MidGame,
                _ => self.phase = MatchPhase::EndGame,
            }
        }
    }

    fn tick_storm_damage(&mut self, dt: f32) {
        for p in &mut self.players {
            if !matches!(p.status, PlayerStatus::Alive) {
                continue;
            }
            let dmg_per_sec = self.storm.damage_at(p.position);
            if dmg_per_sec > 0.0 {
                let dmg = (dmg_per_sec * dt).ceil() as u16;
                p.take_damage(dmg, 0); // 0 = storm kill
            }
        }
        self.update_alive_count();
    }

    fn tick_dbno(&mut self, dt: f32) {
        for p in &mut self.players {
            if p.status == PlayerStatus::DBNO {
                p.dbno_timer += dt;
                let bleed = (DBNO_BLEED_DPS * dt).ceil() as u16;
                p.hp = p.hp.saturating_sub(bleed);
                if p.hp == 0 || p.dbno_timer >= DBNO_DURATION {
                    p.status = PlayerStatus::Eliminated;
                }
            }
        }
        self.update_alive_count();
    }

    fn tick_structures(&mut self, dt: f32) {
        for s in &mut self.structures {
            s.tick_build(dt);
        }
        self.structures.retain(|s| s.current_hp > 0);
    }

    fn update_alive_count(&mut self) {
        self.alive_count = self
            .players
            .iter()
            .filter(|p| {
                matches!(
                    p.status,
                    PlayerStatus::Alive | PlayerStatus::Gliding | PlayerStatus::DBNO
                )
            })
            .count() as u32;
    }

    fn check_victory(&mut self) {
        if self.alive_count <= 1 && self.phase != MatchPhase::Victory {
            self.phase = MatchPhase::Victory;
            // Assign placements
            let mut alive_players: Vec<_> = self
                .players
                .iter()
                .filter(|p| p.is_alive() || p.status == PlayerStatus::DBNO)
                .map(|p| p.client_id)
                .collect();
            if alive_players.len() == 1 {
                self.winner_id = Some(alive_players[0]);
            }
            // Assign placements to eliminated players (reverse order of elimination)
            let total = self.players.len() as u16;
            let mut placement = total;
            for p in &mut self.players {
                if p.status == PlayerStatus::Eliminated && p.placement == 0 {
                    p.placement = placement;
                    placement -= 1;
                }
            }
            // Winner gets #1
            if let Some(winner) = self.winner_id {
                if let Some(p) = self.players.iter_mut().find(|p| p.client_id == winner) {
                    p.placement = 1;
                }
            }
        }
    }

    /// Player jumps from bus.
    pub fn player_jump(&mut self, client_id: u32) {
        if let Some(p) = self.players.iter_mut().find(|p| p.client_id == client_id) {
            if p.status == PlayerStatus::OnBus {
                p.status = PlayerStatus::Gliding;
                p.bus_jumped = true;
                p.position = self.bus.current_position();
            }
        }
    }

    /// Player lands (transition from gliding to alive).
    pub fn player_land(&mut self, client_id: u32, position: Vec3) {
        if let Some(p) = self.players.iter_mut().find(|p| p.client_id == client_id) {
            if p.status == PlayerStatus::Gliding {
                p.status = PlayerStatus::Alive;
                p.position = position;
                p.landed = true;
            }
        }
    }

    /// Player builds a structure.
    pub fn player_build(
        &mut self,
        client_id: u32,
        piece: BuildPiece,
        material: MaterialType,
        position: Vec3,
        rotation_y: f32,
    ) -> Option<u32> {
        let player = self.players.iter_mut().find(|p| p.client_id == client_id)?;
        if player.status != PlayerStatus::Alive {
            return None;
        }
        if !player.spend_material(material) {
            return None;
        }

        let id = self.next_structure_id;
        self.next_structure_id += 1;
        self.structures.push(BuildStructure::new(
            id, piece, material, position, rotation_y, client_id,
        ));
        Some(id)
    }

    /// Process hit from attacker to victim.
    pub fn process_hit(
        &mut self,
        attacker_id: u32,
        victim_id: u32,
        damage: u16,
        weapon_name: &str,
        headshot: bool,
        distance: f32,
    ) {
        // Get victim and apply damage
        let actual_dmg;
        let eliminated;
        {
            let victim = match self.players.iter_mut().find(|p| p.client_id == victim_id) {
                Some(p) => p,
                None => return,
            };
            actual_dmg = victim.take_damage(damage, attacker_id);
            eliminated = victim.status == PlayerStatus::Eliminated;
        }

        // Credit attacker
        if let Some(attacker) = self.players.iter_mut().find(|p| p.client_id == attacker_id) {
            attacker.damage_dealt += actual_dmg as u32;
            if eliminated {
                attacker.kills += 1;
            }
        }

        if eliminated {
            let attacker_name = self
                .players
                .iter()
                .find(|p| p.client_id == attacker_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_default();
            let victim_name = self
                .players
                .iter()
                .find(|p| p.client_id == victim_id)
                .map(|p| p.display_name.clone())
                .unwrap_or_default();

            // Assign placement
            let alive = self
                .players
                .iter()
                .filter(|p| p.is_alive() || p.status == PlayerStatus::DBNO)
                .count() as u16;
            if let Some(victim) = self.players.iter_mut().find(|p| p.client_id == victim_id) {
                victim.placement = alive + 1;
            }

            self.kill_feed.push(KillFeedEntry {
                tick: self.tick,
                eliminator_name: attacker_name,
                eliminator_id: attacker_id,
                victim_name,
                victim_id,
                weapon_name: weapon_name.to_string(),
                headshot,
                distance,
            });
            if self.kill_feed.len() > 50 {
                self.kill_feed.remove(0);
            }
        }
        self.update_alive_count();
    }

    /// Damage a structure. Returns true if destroyed.
    pub fn damage_structure(&mut self, structure_id: u32, damage: u16) -> bool {
        if let Some(s) = self.structures.iter_mut().find(|s| s.id == structure_id) {
            s.take_damage(damage)
        } else {
            false
        }
    }
}

// ── Map Generation: POIs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct POI {
    pub name: String,
    pub center: Vec3,
    pub radius: f32,
    pub poi_type: POIType,
    pub loot_density: f32,
    pub building_count: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum POIType {
    City,
    Town,
    Landmark,
    Industrial,
    Military,
}

pub fn generate_br_pois(seed: u64) -> Vec<POI> {
    vec![
        POI {
            name: "Tilted Towers".into(),
            center: Vec3::new(-200.0, 0.0, 100.0),
            radius: 120.0,
            poi_type: POIType::City,
            loot_density: 1.0,
            building_count: 24,
        },
        POI {
            name: "Pleasant Park".into(),
            center: Vec3::new(300.0, 0.0, 400.0),
            radius: 100.0,
            poi_type: POIType::Town,
            loot_density: 0.7,
            building_count: 14,
        },
        POI {
            name: "Retail Row".into(),
            center: Vec3::new(500.0, 0.0, -200.0),
            radius: 90.0,
            poi_type: POIType::Town,
            loot_density: 0.8,
            building_count: 16,
        },
        POI {
            name: "Salty Springs".into(),
            center: Vec3::new(100.0, 0.0, -100.0),
            radius: 70.0,
            poi_type: POIType::Town,
            loot_density: 0.6,
            building_count: 8,
        },
        POI {
            name: "Dusty Depot".into(),
            center: Vec3::new(0.0, 0.0, 0.0),
            radius: 80.0,
            poi_type: POIType::Industrial,
            loot_density: 0.5,
            building_count: 6,
        },
        POI {
            name: "Lonely Lodge".into(),
            center: Vec3::new(-600.0, 0.0, -500.0),
            radius: 70.0,
            poi_type: POIType::Landmark,
            loot_density: 0.4,
            building_count: 6,
        },
        POI {
            name: "Junk Junction".into(),
            center: Vec3::new(-700.0, 0.0, 600.0),
            radius: 60.0,
            poi_type: POIType::Industrial,
            loot_density: 0.5,
            building_count: 4,
        },
        POI {
            name: "Haunted Hills".into(),
            center: Vec3::new(-800.0, 0.0, 300.0),
            radius: 65.0,
            poi_type: POIType::Landmark,
            loot_density: 0.4,
            building_count: 8,
        },
        POI {
            name: "Fatal Fields".into(),
            center: Vec3::new(-100.0, 0.0, -500.0),
            radius: 90.0,
            poi_type: POIType::Town,
            loot_density: 0.6,
            building_count: 10,
        },
        POI {
            name: "Moisty Mire".into(),
            center: Vec3::new(600.0, 0.0, -600.0),
            radius: 100.0,
            poi_type: POIType::Landmark,
            loot_density: 0.3,
            building_count: 4,
        },
        POI {
            name: "Snobby Shores".into(),
            center: Vec3::new(-800.0, 0.0, -100.0),
            radius: 80.0,
            poi_type: POIType::Town,
            loot_density: 0.7,
            building_count: 10,
        },
        POI {
            name: "Greasy Grove".into(),
            center: Vec3::new(-400.0, 0.0, -400.0),
            radius: 75.0,
            poi_type: POIType::Town,
            loot_density: 0.7,
            building_count: 10,
        },
        POI {
            name: "Flush Factory".into(),
            center: Vec3::new(-500.0, 0.0, -700.0),
            radius: 65.0,
            poi_type: POIType::Industrial,
            loot_density: 0.5,
            building_count: 4,
        },
        POI {
            name: "Tomato Town".into(),
            center: Vec3::new(200.0, 0.0, 500.0),
            radius: 50.0,
            poi_type: POIType::Landmark,
            loot_density: 0.4,
            building_count: 4,
        },
        POI {
            name: "Wailing Woods".into(),
            center: Vec3::new(700.0, 0.0, 400.0),
            radius: 100.0,
            poi_type: POIType::Landmark,
            loot_density: 0.3,
            building_count: 2,
        },
        POI {
            name: "Risky Reels".into(),
            center: Vec3::new(400.0, 0.0, 600.0),
            radius: 60.0,
            poi_type: POIType::Landmark,
            loot_density: 0.5,
            building_count: 4,
        },
        POI {
            name: "Loot Lake".into(),
            center: Vec3::new(0.0, 0.0, 300.0),
            radius: 90.0,
            poi_type: POIType::Landmark,
            loot_density: 0.6,
            building_count: 6,
        },
        POI {
            name: "Shifty Shafts".into(),
            center: Vec3::new(-300.0, 0.0, -200.0),
            radius: 55.0,
            poi_type: POIType::Industrial,
            loot_density: 0.6,
            building_count: 4,
        },
        POI {
            name: "Anarchy Acres".into(),
            center: Vec3::new(200.0, 0.0, 700.0),
            radius: 85.0,
            poi_type: POIType::Town,
            loot_density: 0.5,
            building_count: 8,
        },
        POI {
            name: "KAMI Citadel".into(),
            center: Vec3::new(0.0, 20.0, 0.0),
            radius: 50.0,
            poi_type: POIType::Military,
            loot_density: 1.0,
            building_count: 4,
        },
        // Brainrot POIs
        POI {
            name: "Skibidi Sewers".into(),
            center: Vec3::new(350.0, 0.0, 350.0),
            radius: 85.0,
            poi_type: POIType::Landmark,
            loot_density: 0.8,
            building_count: 8,
        },
        POI {
            name: "Sigma Summit".into(),
            center: Vec3::new(-450.0, 30.0, 500.0),
            radius: 70.0,
            poi_type: POIType::Landmark,
            loot_density: 0.6,
            building_count: 6,
        },
        POI {
            name: "Ohio Outpost".into(),
            center: Vec3::new(700.0, 0.0, -400.0),
            radius: 75.0,
            poi_type: POIType::Military,
            loot_density: 0.9,
            building_count: 10,
        },
        POI {
            name: "Grimace Grotto".into(),
            center: Vec3::new(-300.0, -5.0, 600.0),
            radius: 80.0,
            poi_type: POIType::Landmark,
            loot_density: 0.5,
            building_count: 4,
        },
        POI {
            name: "Rizz Resort".into(),
            center: Vec3::new(500.0, 0.0, 500.0),
            radius: 90.0,
            poi_type: POIType::City,
            loot_density: 0.9,
            building_count: 16,
        },
        POI {
            name: "Fanum Food Court".into(),
            center: Vec3::new(-600.0, 0.0, -300.0),
            radius: 65.0,
            poi_type: POIType::Town,
            loot_density: 0.7,
            building_count: 8,
        },
    ]
}

/// Generate the BR island scene with POI buildings and loot spawns.
pub fn generate_br_map(seed: u64) -> IslandScene {
    let pois = generate_br_pois(seed);
    let mut entities = Vec::new();
    let half = MAP_SIZE * 0.5;

    // Terrain: ground plane
    entities.push(EntityDef {
        id: "terrain".into(),
        position: [0.0, -0.5, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [MAP_SIZE, 1.0, MAP_SIZE],
        mesh: MeshRef::Cube {
            color: [0.25, 0.45, 0.2, 1.0],
        },
        components: vec![],
        layer: None,
    });

    // Map boundary walls (invisible kill walls)
    for (id, pos, scale) in [
        ("wall-n", [0.0, 50.0, -half], [MAP_SIZE, 100.0, 2.0]),
        ("wall-s", [0.0, 50.0, half], [MAP_SIZE, 100.0, 2.0]),
        ("wall-e", [half, 50.0, 0.0], [2.0, 100.0, MAP_SIZE]),
        ("wall-w", [-half, 50.0, 0.0], [2.0, 100.0, MAP_SIZE]),
    ] {
        entities.push(EntityDef {
            id: id.into(),
            position: pos,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale,
            mesh: MeshRef::Cube {
                color: [0.0, 0.0, 0.0, 0.0],
            },
            components: vec![ComponentDef::Trigger {
                kind: "kill-wall".into(),
                data: "{}".into(),
            }],
            layer: None,
        });
    }

    // Generate buildings per POI
    let mut building_idx = 0u32;
    for poi in &pois {
        let bc = poi.building_count as u32;
        for i in 0..bc {
            let angle = (i as f32 / bc as f32) * std::f32::consts::TAU
                + (seed.wrapping_mul(poi.name.len() as u64) % 360) as f32 * 0.01;
            let r = poi.radius * 0.3 + (poi.radius * 0.6) * ((i as f32 * 1.618) % 1.0);
            let bx = poi.center.x + r * angle.cos();
            let bz = poi.center.z + r * angle.sin();
            let floors = match poi.poi_type {
                POIType::City => 2 + (building_idx % 5) as u16,
                POIType::Town => 1 + (building_idx % 3) as u16,
                POIType::Military => 1 + (building_idx % 2) as u16,
                _ => 1 + (building_idx % 2) as u16,
            };
            let h = floors as f32 * 3.0;
            let w = 6.0 + (building_idx % 4) as f32 * 2.0;

            entities.push(EntityDef {
                id: format!("bldg-{}", building_idx),
                position: [bx, h * 0.5, bz],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [w, h, w],
                mesh: MeshRef::Cube {
                    color: poi_color(poi.poi_type),
                },
                components: vec![
                    ComponentDef::Physics { dynamic: false },
                    ComponentDef::Trigger {
                        kind: "harvestable".into(),
                        data: format!(r#"{{"material":"brick","amount":30}}"#),
                    },
                ],
                layer: None,
            });

            // Chest inside building
            if building_idx % 2 == 0 {
                entities.push(EntityDef {
                    id: format!("chest-{}", building_idx),
                    position: [bx, 0.5, bz],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.8, 0.6, 0.6],
                    mesh: MeshRef::Cube {
                        color: [0.85, 0.75, 0.1, 1.0],
                    },
                    components: vec![ComponentDef::Item {
                        item_id: "chest".into(),
                        item_name: "Chest".into(),
                    }],
                    layer: None,
                });
            }

            building_idx += 1;
        }
    }

    // Trees (scattered across map for wood harvesting)
    let tree_count = 500u32;
    for i in 0..tree_count {
        let s = (i as u64).wrapping_mul(2654435761).wrapping_add(seed);
        let x = (((s >> 0) & 0xFFFF) as f32 / 65535.0 - 0.5) * MAP_SIZE * 0.9;
        let z = (((s >> 16) & 0xFFFF) as f32 / 65535.0 - 0.5) * MAP_SIZE * 0.9;
        // Skip if too close to a POI center
        let near_poi = pois.iter().any(|p| {
            let dx = p.center.x - x;
            let dz = p.center.z - z;
            (dx * dx + dz * dz).sqrt() < p.radius * 0.5
        });
        if near_poi {
            continue;
        }

        entities.push(EntityDef {
            id: format!("tree-{}", i),
            position: [x, 3.0, z],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.5, 6.0, 1.5],
            mesh: MeshRef::Cube {
                color: [0.2, 0.5, 0.15, 1.0],
            },
            components: vec![
                ComponentDef::Physics { dynamic: false },
                ComponentDef::Trigger {
                    kind: "harvestable".into(),
                    data: r#"{"material":"wood","amount":50}"#.into(),
                },
            ],
            layer: None,
        });
    }

    // Rocks (scattered for brick)
    let rock_count = 200u32;
    for i in 0..rock_count {
        let s = (i as u64).wrapping_mul(1103515245).wrapping_add(seed);
        let x = (((s >> 0) & 0xFFFF) as f32 / 65535.0 - 0.5) * MAP_SIZE * 0.9;
        let z = (((s >> 16) & 0xFFFF) as f32 / 65535.0 - 0.5) * MAP_SIZE * 0.9;
        entities.push(EntityDef {
            id: format!("rock-{}", i),
            position: [x, 1.0, z],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [2.5, 2.0, 2.5],
            mesh: MeshRef::Cube {
                color: [0.5, 0.5, 0.5, 1.0],
            },
            components: vec![
                ComponentDef::Physics { dynamic: false },
                ComponentDef::Trigger {
                    kind: "harvestable".into(),
                    data: r#"{"material":"brick","amount":40}"#.into(),
                },
            ],
            layer: None,
        });
    }

    // Cars (scattered for metal + cover)
    let car_count = 100u32;
    for i in 0..car_count {
        let s = (i as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(seed);
        let x = (((s >> 0) & 0xFFFF) as f32 / 65535.0 - 0.5) * MAP_SIZE * 0.85;
        let z = (((s >> 16) & 0xFFFF) as f32 / 65535.0 - 0.5) * MAP_SIZE * 0.85;
        entities.push(EntityDef {
            id: format!("car-{}", i),
            position: [x, 0.7, z],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [3.5, 1.4, 1.8],
            mesh: MeshRef::Cube {
                color: [0.6, 0.25, 0.25, 1.0],
            },
            components: vec![
                ComponentDef::Physics { dynamic: false },
                ComponentDef::Trigger {
                    kind: "harvestable".into(),
                    data: r#"{"material":"metal","amount":60}"#.into(),
                },
            ],
            layer: None,
        });
    }

    // Spawn island (off-map for warmup)
    entities.push(EntityDef {
        id: "spawn-island".into(),
        position: [-MAP_SIZE, -0.5, -MAP_SIZE],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [50.0, 1.0, 50.0],
        mesh: MeshRef::Cube {
            color: [0.4, 0.4, 0.6, 1.0],
        },
        components: vec![ComponentDef::PlayerSpawn],
        layer: None,
    });

    IslandScene {
        context: Some("https://gftd.co.jp/ns/kami/scene".into()),
        ld_type: Some("BattleRoyaleScene".into()),
        ld_id: Some(format!("urn:kami:br:map:{seed}")),
        name: "KAMI Battle Royale — Brainrot Edition".into(),
        genre: Some("battle-royale".into()),
        description: Some("100-player battle royale with Brainrot POIs — Skibidi Sewers, Sigma Summit, Ohio Outpost".into()),
        max_players: Some(100),
        characters: crate::island_gen::brainrot_characters(),
        ambient_color: [0.02, 0.025, 0.04],
        sun_direction: [-0.5, -2.0, -1.0],
        sun_intensity: 4.5,
        entities,
        camera_mode: None,
        layers: vec![],
        viewport: None,
        sun_color: None,
        point_lights: vec![],
        atmosphere: None,
        postfx_preset: None,
        ibl_env_map: None,
        shadow: None,
    }
}

fn poi_color(poi_type: POIType) -> [f32; 4] {
    match poi_type {
        POIType::City => [0.55, 0.55, 0.6, 1.0],
        POIType::Town => [0.6, 0.5, 0.4, 1.0],
        POIType::Landmark => [0.5, 0.55, 0.5, 1.0],
        POIType::Industrial => [0.45, 0.45, 0.5, 1.0],
        POIType::Military => [0.35, 0.4, 0.35, 1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storm_circle_phases() {
        let mut storm = StormCircle::new(Vec3::ZERO);
        assert!(storm.is_inside(Vec3::new(100.0, 0.0, 0.0)));
        assert!(!storm.is_inside(Vec3::new(MAP_SIZE, 0.0, 0.0)));

        // Advance through wait phase
        for _ in 0..7200 {
            storm.tick(1.0 / 60.0);
        }
        assert!(storm.shrinking);

        // Advance through shrink
        for _ in 0..5400 {
            storm.tick(1.0 / 60.0);
        }
        assert!(storm.phase_index >= 1);
        assert!(storm.current_radius < MAP_SIZE * 0.5);
    }

    #[test]
    fn storm_damage_outside() {
        let storm = StormCircle::new(Vec3::ZERO);
        assert_eq!(storm.damage_at(Vec3::new(0.0, 0.0, 0.0)), 0.0);
        assert!(storm.damage_at(Vec3::new(MAP_SIZE, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn battle_bus_traversal() {
        let mut bus = BattleBus::new(42);
        assert!(bus.active);
        let start_pos = bus.current_position();

        // Fly for some time
        for _ in 0..600 {
            bus.tick(1.0 / 60.0);
        }
        let mid_pos = bus.current_position();
        assert!(mid_pos.distance(start_pos) > 10.0);

        // Fly until done
        for _ in 0..6000 {
            bus.tick(1.0 / 60.0);
        }
        assert!(!bus.active);
    }

    #[test]
    fn player_damage_shield_first() {
        let mut player = BRPlayer::new(1, "did:test:1", "TestPlayer");
        player.status = PlayerStatus::Alive;
        player.shield = 50;

        let dealt = player.take_damage(75, 2);
        assert_eq!(dealt, 75);
        assert_eq!(player.shield, 0);
        assert_eq!(player.hp, 75); // 100 - (75-50)
    }

    #[test]
    fn player_elimination() {
        let mut player = BRPlayer::new(1, "did:test:1", "TestPlayer");
        player.status = PlayerStatus::Alive;
        player.hp = 30;
        player.shield = 0;

        player.take_damage(30, 2);
        assert_eq!(player.status, PlayerStatus::Eliminated);
        assert_eq!(player.eliminated_by, Some(2));
    }

    #[test]
    fn building_system() {
        let mut s =
            BuildStructure::new(1, BuildPiece::Wall, MaterialType::Wood, Vec3::ZERO, 0.0, 1);
        assert!(s.building);
        assert_eq!(s.current_hp, 90); // wood initial

        // Build over time
        for _ in 0..240 {
            s.tick_build(1.0 / 60.0);
        }
        assert!(s.current_hp > 90);

        // Complete build
        for _ in 0..600 {
            s.tick_build(1.0 / 60.0);
        }
        assert!(!s.building);
        assert_eq!(s.current_hp, 150); // wood max

        // Damage
        assert!(!s.take_damage(100));
        assert_eq!(s.current_hp, 50);
        assert!(s.take_damage(50));
        assert_eq!(s.current_hp, 0);
    }

    #[test]
    fn material_harvesting() {
        let mut player = BRPlayer::new(1, "did:test:1", "TestPlayer");
        player.harvest(MaterialType::Wood, 100);
        player.harvest(MaterialType::Brick, 50);
        player.harvest(MaterialType::Metal, 30);
        assert_eq!(player.wood, 100);
        assert_eq!(player.brick, 50);
        assert_eq!(player.metal, 30);

        assert!(player.can_build(MaterialType::Wood));
        assert!(player.spend_material(MaterialType::Wood));
        assert_eq!(player.wood, 90);
    }

    #[test]
    fn weapon_pickup_swap() {
        let mut player = BRPlayer::new(1, "did:test:1", "TestPlayer");
        let weapons = weapon_pool();

        // Fill all 5 slots
        for w in weapons.iter().take(5) {
            assert!(player.pick_up_weapon(w.clone()).is_none());
        }

        // 6th weapon swaps active slot
        let dropped = player.pick_up_weapon(weapons[5].clone());
        assert!(dropped.is_some());
    }

    #[test]
    fn match_lifecycle() {
        let mut m = BRMatchState::new("match-001", 42);
        for i in 0..10 {
            m.add_player(i, &format!("did:test:{}", i), &format!("Player{}", i));
        }
        assert_eq!(m.players.len(), 10);
        assert_eq!(m.phase, MatchPhase::Warmup);

        // Start match
        m.start_match();
        assert_eq!(m.phase, MatchPhase::BattleBus);
        assert_eq!(m.alive_count, 10);

        // Players jump
        for i in 0..5 {
            m.player_jump(i);
        }
        assert!(m.players[0].status == PlayerStatus::Gliding);

        // Simulate bus finishing
        for _ in 0..6000 {
            m.tick(1.0 / 60.0);
        }
        assert!(m.phase != MatchPhase::BattleBus);

        // All gliding players land
        for p in &m.players {
            assert!(!matches!(p.status, PlayerStatus::OnBus));
        }
    }

    #[test]
    fn match_combat_and_victory() {
        let mut m = BRMatchState::new("match-002", 99);
        m.add_player(1, "did:test:1", "Alice");
        m.add_player(2, "did:test:2", "Bob");
        m.start_match();

        // Force land both
        for i in 1..=2 {
            m.player_jump(i);
            m.player_land(i, Vec3::new(i as f32 * 10.0, 0.0, 0.0));
        }
        m.phase = MatchPhase::EarlyGame;

        // Alice kills Bob
        m.process_hit(1, 2, 200, "SCAR", false, 30.0);
        assert_eq!(m.players[1].status, PlayerStatus::Eliminated);

        m.tick(1.0 / 60.0);
        assert_eq!(m.phase, MatchPhase::Victory);
        assert_eq!(m.winner_id, Some(1));
        assert_eq!(m.kill_feed.len(), 1);
        assert!(m.kill_feed[0].eliminator_name == "Alice");
    }

    #[test]
    fn br_map_generation() {
        let scene = generate_br_map(42);
        assert_eq!(scene.name, "KAMI Battle Royale — Brainrot Edition");
        assert!(scene.entities.len() > 100);
        assert!(scene.context.is_some());
        assert_eq!(scene.ld_type.as_deref(), Some("BattleRoyaleScene"));
        assert!(!scene.characters.is_empty()); // brainrot characters included
        let pois = generate_br_pois(42);
        assert_eq!(pois.len(), 26); // 20 original + 6 brainrot
        // Verify brainrot POIs exist
        assert!(pois.iter().any(|p| p.name == "Skibidi Sewers"));
        assert!(pois.iter().any(|p| p.name == "Sigma Summit"));
        assert!(pois.iter().any(|p| p.name == "Ohio Outpost"));
    }

    #[test]
    fn consumable_pool_valid() {
        let consumables = consumable_pool();
        assert!(consumables.len() >= 11);
        for c in &consumables {
            assert!(c.use_time > 0.0);
            assert!(c.stack_size > 0);
        }
    }

    #[test]
    fn weapon_pool_valid() {
        let weapons = weapon_pool();
        assert!(weapons.len() >= 25);
        for w in &weapons {
            assert!(w.damage > 0);
            assert!(w.fire_rate > 0.0);
            assert!(w.magazine_size > 0);
        }
    }
}
