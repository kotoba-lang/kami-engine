//! KAMI Addons: game platform services refactored from Godot C5-C15.
//!
//! All addons communicate via KNP Channel 1 (reliable ordered).
//! Server-authoritative — client sends commands, server validates + broadcasts state.

use serde::{Deserialize, Serialize};

// ── C5: Social ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceStatus {
    pub user_id: String,
    pub status: PresenceState,
    pub island_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceState {
    Online,
    Away,
    InGame,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender_id: String,
    pub channel: String,
    pub content: String,
    pub tick: u32,
}

// ── C6: Leaderboard ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub user_id: String,
    pub display_name: String,
    pub score: i64,
    pub rank: u32,
}

pub struct Leaderboard {
    pub entries: Vec<LeaderboardEntry>,
}

impl Leaderboard {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn submit(&mut self, user_id: &str, display_name: &str, score: i64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.user_id == user_id) {
            if score > e.score {
                e.score = score;
            }
        } else {
            self.entries.push(LeaderboardEntry {
                user_id: user_id.into(),
                display_name: display_name.into(),
                score,
                rank: 0,
            });
        }
        self.entries.sort_by(|a, b| b.score.cmp(&a.score));
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.rank = i as u32 + 1;
        }
    }

    pub fn top(&self, n: usize) -> &[LeaderboardEntry] {
        &self.entries[..n.min(self.entries.len())]
    }
}

// ── C7: Economy (→ economy.rs に委譲) ──
// See economy.rs for Wallet

// ── C8: Engagement ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBonus {
    pub day_streak: u32,
    pub last_claim_date: String,
    pub gems_reward: i64,
}

impl DailyBonus {
    pub fn new() -> Self {
        Self {
            day_streak: 0,
            last_claim_date: String::new(),
            gems_reward: 10,
        }
    }

    pub fn claim(&mut self, today: &str) -> i64 {
        if self.last_claim_date == today {
            return 0;
        }
        self.day_streak += 1;
        self.last_claim_date = today.to_string();
        self.gems_reward = 10 + (self.day_streak as i64 - 1) * 5; // 10, 15, 20, 25, ...
        self.gems_reward
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: String,
    pub title: String,
    pub progress: u32,
    pub target: u32,
    pub reward_gems: i64,
    pub completed: bool,
}

impl Mission {
    pub fn advance(&mut self, amount: u32) -> bool {
        if self.completed {
            return false;
        }
        self.progress = (self.progress + amount).min(self.target);
        if self.progress >= self.target {
            self.completed = true;
        }
        self.completed
    }
}

// ── C9: Inventory (→ inventory.rs) ──
// ── C10: Gacha ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GachaResult {
    pub item_id: String,
    pub item_name: String,
    pub rarity: String,
}

pub fn gacha_draw(banner: &str, count: u32) -> Vec<GachaResult> {
    // Deterministic pseudo-random based on banner name hash
    let seed = banner
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    (0..count)
        .map(|i| {
            let roll = (seed.wrapping_mul(i as u64 + 1)) % 100;
            let (rarity, item_id, item_name) = if roll < 3 {
                ("legendary", "sword-legendary", "Legendary Blade")
            } else if roll < 15 {
                ("epic", "armor-epic", "Epic Armor")
            } else if roll < 35 {
                ("rare", "gem-blue", "Blue Gem")
            } else {
                ("common", "potion-hp", "Health Potion")
            };
            GachaResult {
                item_id: item_id.into(),
                item_name: item_name.into(),
                rarity: rarity.into(),
            }
        })
        .collect()
}

// ── C11: Energy ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergySystem {
    pub current: u32,
    pub maximum: u32,
    pub recovery_rate: u32, // per minute
}

impl EnergySystem {
    pub fn new(max: u32) -> Self {
        Self {
            current: max,
            maximum: max,
            recovery_rate: 1,
        }
    }

    pub fn spend(&mut self, amount: u32) -> bool {
        if self.current < amount {
            return false;
        }
        self.current -= amount;
        true
    }

    pub fn recover(&mut self, minutes_elapsed: u32) {
        self.current = (self.current + minutes_elapsed * self.recovery_rate).min(self.maximum);
    }
}

// ── C12: Telemetry ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEvent {
    pub event_type: String,
    pub entity_id: String,
    pub payload: String,
    pub tick: u32,
}

pub struct TelemetryBuffer {
    events: Vec<GameEvent>,
    max_buffer: usize,
}

impl TelemetryBuffer {
    pub fn new(max_buffer: usize) -> Self {
        Self {
            events: Vec::new(),
            max_buffer,
        }
    }

    pub fn track(&mut self, event: GameEvent) {
        if self.events.len() >= self.max_buffer {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    pub fn drain(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

// ── C13: World (KAMI-specific) ──
// Portal traversal + Island connection — handled by kami-knp session + trigger.rs

// ── C14: Actor Binding (KAMI-specific) ──
// Server-authoritative entity state — handled by kami-core actor.rs + physics.rs

// ── C15: AssetHub Runtime Loader (KAMI-specific) ──
// CDN fetch + cache — handled by kami-render mesh.rs + MeshRef::Asset

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaderboard_ranking() {
        let mut lb = Leaderboard::new();
        lb.submit("a", "Alice", 100);
        lb.submit("b", "Bob", 200);
        lb.submit("c", "Carol", 150);
        let top = lb.top(3);
        assert_eq!(top[0].user_id, "b");
        assert_eq!(top[0].rank, 1);
        assert_eq!(top[1].user_id, "c");
        assert_eq!(top[2].user_id, "a");
    }

    #[test]
    fn daily_bonus_streak() {
        let mut bonus = DailyBonus::new();
        assert_eq!(bonus.claim("2026-03-21"), 10);
        assert_eq!(bonus.claim("2026-03-21"), 0); // no double claim
        assert_eq!(bonus.claim("2026-03-22"), 15);
        assert_eq!(bonus.day_streak, 2);
    }

    #[test]
    fn mission_progress() {
        let mut m = Mission {
            id: "kill-10".into(),
            title: "Kill 10".into(),
            progress: 0,
            target: 10,
            reward_gems: 50,
            completed: false,
        };
        assert!(!m.advance(5));
        assert!(m.advance(5));
        assert!(!m.advance(1)); // already completed
    }

    #[test]
    fn energy_system() {
        let mut e = EnergySystem::new(100);
        assert!(e.spend(30));
        assert_eq!(e.current, 70);
        assert!(!e.spend(80)); // not enough
        e.recover(10);
        assert_eq!(e.current, 80);
    }

    #[test]
    fn gacha_draw_rates() {
        let results = gacha_draw("test-banner", 100);
        assert_eq!(results.len(), 100);
        let legendary_count = results.iter().filter(|r| r.rarity == "legendary").count();
        assert!(legendary_count < 10); // ~3%
    }
}
