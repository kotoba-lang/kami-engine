// Simulation kinds (spring bone, cloth, particles, DEC fields).
// P0 skeleton: enum only. Wiring lands in P1–P2.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FxKind {
    Dust,
    HitSpark,
    Splash,
    Sparkle,
    Smoke,
    SpeedLines3d,
}
