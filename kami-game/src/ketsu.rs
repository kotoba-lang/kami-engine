//! Goriketsu Dash!! — Chase game on KAMI Engine (Rust WASM + JSON-LD).
//!
//! Slap the gorilla's butt → bananas fall from trees → collect them while fleeing.
//! 3 waves of escalating difficulty. Near-miss bonus, combo system, sprint stamina.
//! Peaceful path: wait 30s without slapping → true ending.

use crate::input::InputState;
use glam::Vec3;
use serde::Serialize;

// ── Constants ──

const ARENA: f32 = 30.0;
const PLAYER_SPEED: f32 = 5.0;
const SPRINT_SPEED: f32 = 8.5;
const STAMINA_MAX: f32 = 100.0;
const STAMINA_DRAIN: f32 = 1.2;
const STAMINA_REGEN: f32 = 0.4;
const STAMINA_RATE_BASELINE: f32 = 60.0;
const SLAP_RANGE: f32 = 4.5;
const CATCH_RANGE: f32 = 2.5;
const NEAR_MISS_RANGE: f32 = 4.0;
const TREE_COUNT: usize = 12;
const TREE_SAFE_RADIUS: f32 = 3.0;
const BANANA_PICKUP_RANGE: f32 = 2.75;
const HIDE_DURATION: f32 = 3.0;
const PEACE_WAIT: f32 = 30.0;
const MAX_CATCHES_PER_WAVE: u32 = 5;

/// Wave configuration: banana count, golden count, gorilla speed range, ground pound.
struct WaveConfig {
    bananas: usize,
    golden: usize,
    speed_min: f32,
    speed_max: f32,
    ground_pound: bool,
}

const WAVES: [WaveConfig; 3] = [
    WaveConfig {
        bananas: 4,
        golden: 0,
        speed_min: 4.0,
        speed_max: 5.2,
        ground_pound: false,
    },
    WaveConfig {
        bananas: 6,
        golden: 1,
        speed_min: 4.6,
        speed_max: 6.5,
        ground_pound: true,
    },
    WaveConfig {
        bananas: 5,
        golden: 1,
        speed_min: 5.0,
        speed_max: 7.1,
        ground_pound: true,
    },
];

// ── Types ──

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Title,
    Sneak,
    Alert,
    Chase,
    Rest,
    Victory,
    PeaceVictory,
    GameOver,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Sneak => "Sneak",
            Self::Alert => "Alert",
            Self::Chase => "Chase",
            Self::Rest => "Rest",
            Self::Victory => "Victory",
            Self::PeaceVictory => "PeaceVictory",
            Self::GameOver => "GameOver",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GorillaState {
    Sleeping,
    Waking,
    WakingPeace,
    Chasing,
    Friendly,
    Resting,
    Caught,
}

#[derive(Debug, Clone)]
pub struct Tree {
    pub pos: Vec3,
    pub height: f32,
    pub shake_timer: f32,
}

#[derive(Debug, Clone)]
pub struct Banana {
    pub pos: Vec3,
    pub golden: bool,
    pub collected: bool,
    pub fall_height: f32,
    pub grounded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KetsuPoint {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct KetsuBananaSnapshot {
    pub x: f32,
    pub z: f32,
    pub grounded: bool,
    pub collected: bool,
    pub golden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoriketsuSnapshot {
    pub phase: String,
    pub tick: u64,
    pub score: i32,
    pub hi_score: i32,
    pub wave: usize,
    pub bananas_needed: usize,
    pub bananas_collected: usize,
    pub stamina: f32,
    pub hiding: bool,
    pub peace_path: bool,
    pub catches_taken: u32,
    pub catches_remaining: u32,
    pub combo: u32,
    pub player: KetsuPoint,
    pub gorilla: KetsuPoint,
    pub message: Option<String>,
    pub bananas: Vec<KetsuBananaSnapshot>,
    pub trees: Vec<KetsuPoint>,
}

// ── Game State ──

#[derive(Debug, Clone)]
pub struct GoriketsuGame {
    pub phase: Phase,
    pub tick: u64,
    pub score: i32,
    pub hi_score: i32,
    pub wave: usize,
    pub wave_rest_timer: f32,

    // Player
    pub player_pos: Vec3,
    pub player_vel: Vec3,
    pub stamina: f32,
    pub sprinting: bool,
    pub hiding: bool,
    pub hide_timer: f32,
    pub catches_taken: u32,

    // Gorilla
    pub gorilla_pos: Vec3,
    pub gorilla_vel: Vec3,
    pub gorilla_state: GorillaState,
    pub anger: f32,
    pub wake_timer: f32,

    // World
    pub trees: Vec<Tree>,
    pub bananas: Vec<Banana>,
    pub bananas_needed: usize,
    pub bananas_collected: usize,
    pub combo: u32,
    pub combo_timer: f32,
    pub near_miss_count: u32,
    pub near_miss_cooldown: f32,
    pub peace_timer: f32,
    pub peace_path: bool,

    // Feedback (consumed by renderer each frame)
    pub screen_shake: f32,
    pub hit_stop_frames: u32,
    pub flash_color: Option<[f32; 4]>,
    pub message: Option<(String, f32)>,
}

impl GoriketsuGame {
    pub fn new() -> Self {
        let mut trees = Vec::with_capacity(TREE_COUNT);
        for i in 0..TREE_COUNT {
            let angle = (i as f32 / TREE_COUNT as f32) * std::f32::consts::TAU;
            let r = 8.0 + (i as f32 * 1.7) % 14.0;
            trees.push(Tree {
                pos: Vec3::new(angle.cos() * r, 0.0, angle.sin() * r),
                height: 4.0 + (i as f32 * 0.7) % 3.0,
                shake_timer: 0.0,
            });
        }

        Self {
            phase: Phase::Sneak,
            tick: 0,
            score: 0,
            hi_score: 0,
            wave: 0,
            wave_rest_timer: 0.0,
            player_pos: Vec3::new(0.0, 0.0, -18.0),
            player_vel: Vec3::ZERO,
            stamina: STAMINA_MAX,
            sprinting: false,
            hiding: false,
            hide_timer: 0.0,
            catches_taken: 0,
            gorilla_pos: Vec3::ZERO,
            gorilla_vel: Vec3::ZERO,
            gorilla_state: GorillaState::Sleeping,
            anger: 0.0,
            wake_timer: 0.0,
            trees,
            bananas: Vec::new(),
            bananas_needed: 0,
            bananas_collected: 0,
            combo: 0,
            combo_timer: 0.0,
            near_miss_count: 0,
            near_miss_cooldown: 0.0,
            peace_timer: 0.0,
            peace_path: false,
            screen_shake: 0.0,
            hit_stop_frames: 0,
            flash_color: None,
            message: None,
        }
    }

    /// Main game tick. dt in seconds (target 1/60).
    pub fn update(&mut self, input: &InputState, dt: f32) {
        // Hit stop: freeze gameplay but keep rendering
        if self.hit_stop_frames > 0 {
            self.hit_stop_frames -= 1;
            return;
        }

        self.screen_shake = (self.screen_shake - dt * 10.0).max(0.0);
        self.flash_color = None;

        match self.phase {
            Phase::Title | Phase::Victory | Phase::PeaceVictory | Phase::GameOver => return,
            Phase::Rest => {
                self.wave_rest_timer -= dt;
                if self.wave_rest_timer <= 0.0 {
                    self.wave += 1;
                    if self.wave >= WAVES.len() {
                        self.phase = Phase::Victory;
                        self.score += 500 * self.wave as i32;
                        self.screen_shake = 1.0;
                        self.message = Some(("MORI WA MODOTTA!".into(), 5.0));
                        self.update_hi_score();
                    } else {
                        self.gorilla_state = GorillaState::Sleeping;
                        self.anger = self.wave as f32 * 15.0;
                        self.gorilla_pos = Vec3::ZERO;
                        self.gorilla_vel = Vec3::ZERO;
                        self.phase = Phase::Sneak;
                        self.message = Some((format!("Wave {}", self.wave + 1), 1.5));
                    }
                }
                return;
            }
            _ => {}
        }

        self.tick += 1;
        if let Some((_, ref mut dur)) = self.message {
            *dur -= dt;
            if *dur <= 0.0 {
                self.message = None;
            }
        }
        if self.combo_timer > 0.0 {
            self.combo_timer -= dt;
            if self.combo_timer <= 0.0 {
                self.combo = 0;
            }
        }
        if self.near_miss_cooldown > 0.0 {
            self.near_miss_cooldown -= dt;
        }

        // ── Player movement ──
        let mut dir = Vec3::ZERO;
        if input.left {
            dir.x -= 1.0;
        }
        if input.right {
            dir.x += 1.0;
        }
        if input.forward {
            dir.z -= 1.0;
        }
        if input.backward {
            dir.z += 1.0;
        }
        if dir.length_squared() > 0.0 {
            dir = dir.normalize();
        }

        self.sprinting = input.jump; // Space = sprint in this game
        let stamina_step = dt * STAMINA_RATE_BASELINE;
        let speed = if self.sprinting && self.stamina > 0.0 {
            self.stamina = (self.stamina - STAMINA_DRAIN * stamina_step).max(0.0);
            SPRINT_SPEED
        } else {
            self.stamina = (self.stamina + STAMINA_REGEN * stamina_step).min(STAMINA_MAX);
            PLAYER_SPEED
        };

        self.player_vel = dir * speed;
        self.player_pos += self.player_vel * dt;
        self.player_pos = self
            .player_pos
            .clamp(Vec3::new(-ARENA, 0.0, -ARENA), Vec3::new(ARENA, 0.0, ARENA));

        // ── Slap (interact key = E) ──
        if input.interact && self.phase == Phase::Sneak {
            let d = self.player_pos.distance(self.gorilla_pos);
            if d < SLAP_RANGE {
                self.phase = Phase::Alert;
                self.gorilla_state = GorillaState::Waking;
                self.wake_timer = 1.0;
                self.anger = 30.0;
                self.score += 100;
                self.screen_shake = 1.2;
                self.hit_stop_frames = 6;
                self.message = Some(("BACHI!!".into(), 1.5));
                self.spawn_bananas_from_trees();
                for t in &mut self.trees {
                    if t.pos.distance(self.player_pos) < 15.0 {
                        t.shake_timer = 0.3;
                    }
                }
            } else {
                self.message = Some(("Too far! Get closer!".into(), 0.8));
            }
        }

        // ── Peaceful path ──
        if self.phase == Phase::Sneak && !self.peace_path {
            self.peace_timer += dt;
            if self.peace_timer >= PEACE_WAIT {
                self.peace_path = true;
                self.gorilla_state = GorillaState::WakingPeace;
                self.wake_timer = 1.5;
                self.message = Some(("gorira ga sotto me wo aketa...".into(), 2.0));
            }
        }

        // ── Tree collision / hiding ──
        let in_tree_cover = self.phase == Phase::Chase
            && self
                .trees
                .iter()
                .any(|t| self.player_pos.distance(t.pos) < TREE_SAFE_RADIUS);
        if in_tree_cover {
            if !self.hiding {
                self.hide_timer = HIDE_DURATION;
            }
            self.hiding = true;
        } else if self.hiding {
            self.hide_timer -= dt;
            if self.hide_timer <= 0.0 {
                self.hiding = false;
            }
        }

        // ── Tree shake decay ──
        for t in &mut self.trees {
            t.shake_timer = (t.shake_timer - dt).max(0.0);
        }

        // ── Banana fall physics ──
        for b in &mut self.bananas {
            if !b.grounded {
                b.fall_height -= 9.81 * dt;
                if b.fall_height <= 0.0 {
                    b.fall_height = 0.0;
                    b.grounded = true;
                }
            }
        }

        // ── Banana collection ──
        for b in &mut self.bananas {
            if b.collected || !b.grounded {
                continue;
            }
            if self.player_pos.distance(b.pos) < BANANA_PICKUP_RANGE {
                b.collected = true;
                self.bananas_collected += 1;
                self.combo += 1;
                self.combo_timer = 1.5;
                let mult = if b.golden { 3 } else { 1 };
                let bonus = 10 * self.combo as i32 * mult;
                self.score += bonus;
            }
        }

        // ── Gorilla AI ──
        match self.gorilla_state {
            GorillaState::Sleeping => {}
            GorillaState::Waking => {
                self.wake_timer -= dt;
                if self.wake_timer <= 0.0 {
                    self.gorilla_state = GorillaState::Chasing;
                    self.phase = Phase::Chase;
                    self.message = Some(("NIGEROOOO!!".into(), 1.0));
                }
            }
            GorillaState::WakingPeace => {
                self.wake_timer -= dt;
                if self.wake_timer <= 0.0 {
                    self.gorilla_state = GorillaState::Friendly;
                    self.phase = Phase::PeaceVictory;
                    self.score += 2000;
                    self.message = Some(("tataku yori wakariaeta!".into(), 5.0));
                    self.update_hi_score();
                }
            }
            GorillaState::Chasing => {
                let wc = &WAVES[self.wave.min(WAVES.len() - 1)];
                self.anger = (self.anger + 0.08 * dt * 60.0).min(100.0);
                let chase_speed =
                    wc.speed_min + (self.anger / 100.0) * (wc.speed_max - wc.speed_min);

                if self.hiding {
                    // Wander
                    let t = self.tick as f32 * 0.02;
                    self.gorilla_vel = Vec3::new(t.cos(), 0.0, t.sin()) * 2.0;
                } else {
                    let to_player = self.player_pos - self.gorilla_pos;
                    let d = to_player.length();
                    if d > 0.1 {
                        self.gorilla_vel = (to_player / d) * chase_speed;
                    }
                }
                self.gorilla_pos += self.gorilla_vel * dt;
                self.gorilla_pos = self
                    .gorilla_pos
                    .clamp(Vec3::new(-ARENA, 0.0, -ARENA), Vec3::new(ARENA, 0.0, ARENA));

                // ── Near-miss ──
                let pd = self.player_pos.distance(self.gorilla_pos);
                if pd < NEAR_MISS_RANGE
                    && pd >= CATCH_RANGE
                    && !self.hiding
                    && self.near_miss_cooldown <= 0.0
                {
                    self.near_miss_count += 1;
                    self.near_miss_cooldown = 0.5;
                    self.score += 25 * self.near_miss_count as i32;
                    self.flash_color = Some([1.0, 0.0, 0.0, 0.15]);
                }

                // ── Catch / knockback ──
                if !self.hiding && pd < CATCH_RANGE {
                    let knockback = (self.player_pos - self.gorilla_pos).normalize_or_zero() * 20.0;
                    self.player_vel = knockback;
                    self.player_pos += knockback * dt * 3.0;
                    self.screen_shake = 1.8;
                    self.hit_stop_frames = 8;
                    self.score = (self.score - 50).max(0);
                    self.catches_taken += 1;
                    self.message = Some(("FUKITOBASHI!!".into(), 0.5));

                    if self.catches_taken >= MAX_CATCHES_PER_WAVE {
                        self.phase = Phase::GameOver;
                        self.gorilla_state = GorillaState::Caught;
                        self.screen_shake = 2.0;
                        self.message = Some(("TSUKAMATTA...".into(), 5.0));
                        self.update_hi_score();
                    }
                }

                // ── Tree collision (gorilla) ──
                for t in &mut self.trees {
                    if self.gorilla_pos.distance(t.pos) < 3.0 {
                        t.shake_timer = 0.3;
                    }
                }
            }
            _ => {}
        }

        // ── Wave clear check ──
        if self.phase == Phase::Chase && self.bananas_collected >= self.bananas_needed {
            if self.wave >= WAVES.len() - 1 {
                self.phase = Phase::Victory;
                self.score += 500 * (self.wave as i32 + 1);
                self.screen_shake = 1.0;
                self.message = Some(("MORI WA MODOTTA!".into(), 5.0));
                self.update_hi_score();
            } else {
                self.phase = Phase::Rest;
                self.wave_rest_timer = 2.0;
                self.gorilla_state = GorillaState::Resting;
                self.gorilla_vel = Vec3::ZERO;
                self.catches_taken = 0;
                self.screen_shake = 0.6;
                self.message = Some((format!("Wave {} CLEAR!", self.wave + 1), 1.5));
            }
        }
    }

    fn spawn_bananas_from_trees(&mut self) {
        let wc = &WAVES[self.wave.min(WAVES.len() - 1)];
        self.bananas.clear();
        self.bananas_collected = 0;
        self.bananas_needed = wc.bananas + wc.golden;

        // Normal bananas near random trees
        for i in 0..wc.bananas {
            let t = &self.trees[i % self.trees.len()];
            let offset = Vec3::new(
                ((i as f32 * 7.3).sin()) * 3.0,
                0.0,
                ((i as f32 * 4.1).cos()) * 3.0,
            );
            self.bananas.push(Banana {
                pos: t.pos + offset,
                golden: false,
                collected: false,
                fall_height: 3.0 + (i as f32 * 0.5) % 2.0,
                grounded: false,
            });
        }

        // Golden bananas near gorilla (risk-reward)
        for i in 0..wc.golden {
            let offset = Vec3::new(
                ((i as f32 * 3.7).sin()) * 5.0,
                0.0,
                ((i as f32 * 2.3).cos()) * 5.0,
            );
            self.bananas.push(Banana {
                pos: self.gorilla_pos + offset,
                golden: true,
                collected: false,
                fall_height: 4.0,
                grounded: false,
            });
        }
    }

    fn update_hi_score(&mut self) {
        if self.score > self.hi_score {
            self.hi_score = self.score;
        }
    }

    pub fn snapshot(&self) -> GoriketsuSnapshot {
        GoriketsuSnapshot {
            phase: self.phase.as_str().to_string(),
            tick: self.tick,
            score: self.score,
            hi_score: self.hi_score,
            wave: self.wave,
            bananas_needed: self.bananas_needed,
            bananas_collected: self.bananas_collected,
            stamina: self.stamina,
            hiding: self.hiding,
            peace_path: self.peace_path,
            catches_taken: self.catches_taken,
            catches_remaining: MAX_CATCHES_PER_WAVE.saturating_sub(self.catches_taken),
            combo: self.combo,
            player: KetsuPoint {
                x: self.player_pos.x,
                z: self.player_pos.z,
            },
            gorilla: KetsuPoint {
                x: self.gorilla_pos.x,
                z: self.gorilla_pos.z,
            },
            message: self.message.as_ref().map(|(msg, _)| msg.clone()),
            bananas: self
                .bananas
                .iter()
                .map(|b| KetsuBananaSnapshot {
                    x: b.pos.x,
                    z: b.pos.z,
                    grounded: b.grounded,
                    collected: b.collected,
                    golden: b.golden,
                })
                .collect(),
            trees: self
                .trees
                .iter()
                .map(|t| KetsuPoint {
                    x: t.pos.x,
                    z: t.pos.z,
                })
                .collect(),
        }
    }

    /// Generate entity updates for the renderer. Called each frame after update().
    pub fn entity_positions(&self) -> Vec<EntityUpdate> {
        let mut out = Vec::with_capacity(2 + self.trees.len() + self.bananas.len());

        // Player
        out.push(EntityUpdate {
            id: "player".into(),
            position: self.player_pos + Vec3::Y * 0.8,
            scale: Vec3::splat(1.0),
            visible: !self.hiding || (self.tick % 10 < 7),
        });

        // Gorilla
        let gs = if self.gorilla_state == GorillaState::Waking {
            1.08
        } else {
            1.0
        };
        out.push(EntityUpdate {
            id: "gorilla".into(),
            position: self.gorilla_pos + Vec3::Y * 1.4,
            scale: Vec3::splat(gs),
            visible: true,
        });

        // Gorilla butt (red sphere, follows gorilla)
        let butt_pulse = 0.3 + ((self.tick as f32 * 0.08).sin().abs()) * 0.7;
        out.push(EntityUpdate {
            id: "gorilla-butt".into(),
            position: self.gorilla_pos + Vec3::new(0.0, 1.0, -1.0),
            scale: Vec3::splat(1.0 + butt_pulse * 0.3),
            visible: true,
        });

        // Trees
        for (i, t) in self.trees.iter().enumerate() {
            let shake_offset = if t.shake_timer > 0.0 {
                (self.tick as f32 * 20.0).sin() * t.shake_timer * 0.5
            } else {
                0.0
            };
            out.push(EntityUpdate {
                id: format!("tree-{i}"),
                position: t.pos + Vec3::new(shake_offset, t.height * 0.5, 0.0),
                scale: Vec3::new(1.0, 1.0, 1.0),
                visible: true,
            });
        }

        // Bananas
        for (i, b) in self.bananas.iter().enumerate() {
            if !b.collected {
                let bob = if b.grounded {
                    (self.tick as f32 * 0.08 + i as f32).sin() * 0.15
                } else {
                    0.0
                };
                out.push(EntityUpdate {
                    id: format!("banana-{i}"),
                    position: b.pos + Vec3::Y * (b.fall_height + 0.3 + bob),
                    scale: if b.golden {
                        Vec3::splat(1.3)
                    } else {
                        Vec3::splat(1.0)
                    },
                    visible: true,
                });
            }
        }

        out
    }
}

/// Per-frame position update for the renderer to apply to scene entities.
#[derive(Debug, Clone)]
pub struct EntityUpdate {
    pub id: String,
    pub position: Vec3,
    pub scale: Vec3,
    pub visible: bool,
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn dummy_input() -> InputState {
        InputState {
            forward: false,
            backward: false,
            left: false,
            right: false,
            jump: false,
            interact: false,
            chat: false,
        }
    }

    #[test]
    fn new_game_initializes() {
        let g = GoriketsuGame::new();
        assert_eq!(g.phase, Phase::Sneak);
        assert_eq!(g.wave, 0);
        assert_eq!(g.trees.len(), TREE_COUNT);
        assert_eq!(g.gorilla_state, GorillaState::Sleeping);
        assert!((g.stamina - STAMINA_MAX).abs() < 0.01);
    }

    #[test]
    fn slap_in_range_triggers_alert() {
        let mut g = GoriketsuGame::new();
        g.player_pos = Vec3::new(0.0, 0.0, -3.0); // within SLAP_RANGE of origin
        let mut input = dummy_input();
        input.interact = true;
        g.update(&input, 1.0 / 60.0);
        assert_eq!(g.phase, Phase::Alert);
        assert_eq!(g.gorilla_state, GorillaState::Waking);
        assert!(g.score >= 100);
        assert!(!g.bananas.is_empty());
    }

    #[test]
    fn slap_out_of_range_stays_sneak() {
        let mut g = GoriketsuGame::new();
        g.player_pos = Vec3::new(0.0, 0.0, -20.0); // far away
        let mut input = dummy_input();
        input.interact = true;
        g.update(&input, 1.0 / 60.0);
        assert_eq!(g.phase, Phase::Sneak);
    }

    #[test]
    fn peace_path_triggers_after_wait() {
        let mut g = GoriketsuGame::new();
        let input = dummy_input();
        // Simulate 31s of waiting (1860 frames at 60fps, extra for float accumulation)
        for _ in 0..1860 {
            g.update(&input, 1.0 / 60.0);
        }
        assert!(g.peace_path);
        assert!(
            g.gorilla_state == GorillaState::WakingPeace
                || g.gorilla_state == GorillaState::Friendly
        );
    }

    #[test]
    fn sprint_drains_stamina() {
        let mut g = GoriketsuGame::new();
        let mut input = dummy_input();
        input.forward = true;
        input.jump = true; // sprint
        for _ in 0..60 {
            g.update(&input, 1.0 / 60.0);
        }
        assert!(g.stamina < STAMINA_MAX);
    }

    #[test]
    fn banana_collection_increases_score() {
        let mut g = GoriketsuGame::new();
        g.phase = Phase::Chase;
        g.gorilla_state = GorillaState::Chasing;
        g.bananas.push(Banana {
            pos: g.player_pos + Vec3::Z * 1.0,
            golden: false,
            collected: false,
            fall_height: 0.0,
            grounded: true,
        });
        g.bananas_needed = 1;
        let input = dummy_input();
        let score_before = g.score;
        // Move player close to banana
        g.player_pos = g.bananas[0].pos;
        g.update(&input, 1.0 / 60.0);
        assert!(g.bananas[0].collected);
        assert!(g.score > score_before);
    }

    #[test]
    fn entity_positions_count() {
        let g = GoriketsuGame::new();
        let ents = g.entity_positions();
        // player + gorilla + gorilla-butt + 12 trees = 15 (no bananas yet)
        assert_eq!(ents.len(), 3 + TREE_COUNT);
    }

    #[test]
    fn wave_clear_advances() {
        let mut g = GoriketsuGame::new();
        g.phase = Phase::Chase;
        g.gorilla_state = GorillaState::Chasing;
        g.bananas_needed = 1;
        g.bananas_collected = 1;
        g.gorilla_pos = Vec3::new(20.0, 0.0, 20.0); // far from player
        let input = dummy_input();
        g.update(&input, 1.0 / 60.0);
        assert!(g.phase == Phase::Rest || g.phase == Phase::Victory);
    }

    #[test]
    fn catches_are_not_immediate_game_over() {
        let mut g = GoriketsuGame::new();
        g.phase = Phase::Chase;
        g.gorilla_state = GorillaState::Chasing;
        g.bananas_needed = 99;
        g.player_pos = Vec3::ZERO;
        g.gorilla_pos = Vec3::new(0.0, 0.0, 1.0);
        let input = dummy_input();
        g.update(&input, 1.0 / 60.0);
        assert_eq!(g.phase, Phase::Chase);
        assert_eq!(g.catches_taken, 1);
    }

    fn move_towards(
        game: &GoriketsuGame,
        target: Vec3,
        sprint: bool,
        interact: bool,
    ) -> InputState {
        let mut input = dummy_input();
        let delta = target - game.player_pos;
        if delta.x < -0.4 {
            input.left = true;
        } else if delta.x > 0.4 {
            input.right = true;
        }
        if delta.z < -0.4 {
            input.forward = true;
        } else if delta.z > 0.4 {
            input.backward = true;
        }
        input.jump = sprint;
        input.interact = interact;
        input
    }

    fn nearest_tree(game: &GoriketsuGame) -> Vec3 {
        game.trees
            .iter()
            .min_by(|a, b| {
                game.player_pos
                    .distance(a.pos)
                    .partial_cmp(&game.player_pos.distance(b.pos))
                    .unwrap_or(Ordering::Equal)
            })
            .map(|t| t.pos)
            .unwrap_or(Vec3::ZERO)
    }

    fn nearest_tree_to_point(game: &GoriketsuGame, point: Vec3) -> Vec3 {
        game.trees
            .iter()
            .min_by(|a, b| {
                point
                    .distance(a.pos)
                    .partial_cmp(&point.distance(b.pos))
                    .unwrap_or(Ordering::Equal)
            })
            .map(|t| t.pos)
            .unwrap_or(Vec3::ZERO)
    }

    fn best_banana_target(game: &GoriketsuGame, include_airborne: bool) -> Option<Vec3> {
        game.bananas
            .iter()
            .filter(|b| !b.collected && (include_airborne || b.grounded))
            .min_by(|a, b| {
                let a_cover = nearest_tree_to_point(game, a.pos).distance(a.pos);
                let b_cover = nearest_tree_to_point(game, b.pos).distance(b.pos);
                let a_cost = game.player_pos.distance(a.pos) + a_cover * 0.9
                    - game.gorilla_pos.distance(a.pos) * 0.2
                    - if a.golden { 0.8 } else { 0.0 };
                let b_cost = game.player_pos.distance(b.pos) + b_cover * 0.9
                    - game.gorilla_pos.distance(b.pos) * 0.2
                    - if b.golden { 0.8 } else { 0.0 };
                a_cost.partial_cmp(&b_cost).unwrap_or(Ordering::Equal)
            })
            .map(|b| b.pos)
    }

    fn best_banana_cluster(game: &GoriketsuGame, include_airborne: bool) -> Option<(Vec3, Vec3)> {
        game.bananas
            .iter()
            .filter(|b| !b.collected && (include_airborne || b.grounded))
            .min_by(|a, b| {
                let a_cover = nearest_tree_to_point(game, a.pos);
                let b_cover = nearest_tree_to_point(game, b.pos);
                let a_cost = game.player_pos.distance(a_cover) + a_cover.distance(a.pos) * 0.7
                    - game.gorilla_pos.distance(a_cover) * 0.15;
                let b_cost = game.player_pos.distance(b_cover) + b_cover.distance(b.pos) * 0.7
                    - game.gorilla_pos.distance(b_cover) * 0.15;
                a_cost.partial_cmp(&b_cost).unwrap_or(Ordering::Equal)
            })
            .map(|b| {
                let cover = nearest_tree_to_point(game, b.pos);
                (b.pos, cover)
            })
    }

    fn autoplay_input(game: &GoriketsuGame) -> InputState {
        match game.phase {
            Phase::Sneak => {
                let target = Vec3::new(0.0, 0.0, -3.2);
                let interact = game.player_pos.distance(game.gorilla_pos) < SLAP_RANGE - 0.3;
                move_towards(game, target, false, interact)
            }
            Phase::Alert => {
                let target = best_banana_target(game, true).unwrap_or_else(|| nearest_tree(game));
                move_towards(game, target, false, false)
            }
            Phase::Chase => {
                let gorilla_dist = game.player_pos.distance(game.gorilla_pos);
                let tree = nearest_tree(game);
                let tree_dist = game.player_pos.distance(tree);
                let (target, cover_tree) = best_banana_cluster(game, false)
                    .or_else(|| best_banana_cluster(game, true))
                    .unwrap_or((tree, tree));
                let cover_dist = game.player_pos.distance(cover_tree);
                if gorilla_dist < 7.5 && tree_dist > 1.25 && cover_dist > 1.25 {
                    return move_towards(game, tree, game.stamina > 10.0, false);
                }
                if !game.hiding && cover_dist > 1.25 {
                    let sprint = gorilla_dist < 9.5 || cover_dist > 10.0;
                    return move_towards(game, cover_tree, sprint && game.stamina > 12.0, false);
                }
                if game.hiding && gorilla_dist < 7.0 && cover_dist > 0.8 {
                    return move_towards(game, cover_tree, false, false);
                }
                let sprint = gorilla_dist < 9.0 || game.player_pos.distance(target) > 10.0;
                move_towards(game, target, sprint && game.stamina > 12.0, false)
            }
            _ => dummy_input(),
        }
    }

    #[test]
    fn autoplay_can_reach_terminal_clear_state() {
        let mut g = GoriketsuGame::new();
        for _ in 0..(60 * 180) {
            let input = autoplay_input(&g);
            g.update(&input, 1.0 / 60.0);
            if matches!(
                g.phase,
                Phase::Victory | Phase::PeaceVictory | Phase::GameOver
            ) {
                break;
            }
        }
        assert!(
            matches!(g.phase, Phase::Victory | Phase::PeaceVictory),
            "autoplay ended in phase {:?} score={} wave={} bananas={}/{}",
            g.phase,
            g.score,
            g.wave,
            g.bananas_collected,
            g.bananas_needed
        );
    }
}
