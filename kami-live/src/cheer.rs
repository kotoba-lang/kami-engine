//! Cheer aggregation.
//!
//! Each viewer can emit a cheer via XRPC `ai.gftd.apps.live.sendCheer`.
//! Server-side aggregator collapses thousands of incoming events into a
//! cheap rolling histogram per [`CheerKind`] that the renderer queries
//! per-frame to drive crowd reactions and PostFX intensity.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheerKind {
    Clap,
    Yell,
    LightStick,
    Jump,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CheerSample {
    /// Show-time when the cheer arrived (seconds).
    pub at: f32,
    pub kind: CheerKind,
    /// Optional weight (e.g. for paid/VIP boost). Default 1.0.
    pub weight: f32,
}

/// Sliding-window aggregate.
#[derive(Debug, Clone)]
pub struct CheerAggregate {
    window: f32,
    samples: VecDeque<CheerSample>,
}

impl CheerAggregate {
    pub fn new(window_seconds: f32) -> Self {
        Self {
            window: window_seconds.max(0.05),
            samples: VecDeque::new(),
        }
    }

    pub fn push(&mut self, s: CheerSample) {
        self.samples.push_back(s);
    }

    /// Drop samples older than `now - window`. Call once per tick.
    pub fn evict(&mut self, now: f32) {
        let cutoff = now - self.window;
        while let Some(s) = self.samples.front() {
            if s.at < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Sum of weights for a given kind in the current window.
    pub fn weight_of(&self, kind: CheerKind) -> f32 {
        self.samples
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| s.weight)
            .sum()
    }

    /// Total cheer "loudness" — sum across all kinds. Useful as an FX gain.
    pub fn loudness(&self) -> f32 {
        self.samples.iter().map(|s| s.weight).sum()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evict_drops_old_samples() {
        let mut a = CheerAggregate::new(1.0);
        a.push(CheerSample {
            at: 0.0,
            kind: CheerKind::Clap,
            weight: 1.0,
        });
        a.push(CheerSample {
            at: 0.5,
            kind: CheerKind::Yell,
            weight: 1.0,
        });
        a.push(CheerSample {
            at: 1.5,
            kind: CheerKind::Yell,
            weight: 1.0,
        });
        a.evict(2.0); // window = 1, cutoff = 1.0
        assert_eq!(a.len(), 1);
        assert!((a.weight_of(CheerKind::Yell) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn loudness_sums_weights() {
        let mut a = CheerAggregate::new(5.0);
        for w in [0.5, 1.0, 2.0] {
            a.push(CheerSample {
                at: 0.0,
                kind: CheerKind::Clap,
                weight: w,
            });
        }
        assert!((a.loudness() - 3.5).abs() < 1e-6);
    }
}
