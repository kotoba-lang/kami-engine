//! VJ deck — back-stage LED-wall visuals.
//!
//! Output is a small struct (palette + pattern + intensity) that the
//! renderer feeds into the LED-wall pipeline. Patterns advance per-bar
//! so the wall flips on the bar line; palette eases per-frame.

use serde::{Deserialize, Serialize};

use crate::beat::BeatPhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VJPattern {
    /// Solid wash (no motion). Used during callouts.
    Solid,
    /// Vertical stripes scrolling down at 1 stripe/beat.
    Stripes,
    /// Pulsing radial gradient from centre.
    Pulse,
    /// Concentric rings expanding on each beat.
    Rings,
    /// Sound-wave style scope.
    Scope,
    /// Random noise (TV static look) — for breakdown/glitch moments.
    Noise,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub primary: [f32; 3],
    pub secondary: [f32; 3],
    pub accent: [f32; 3],
}

impl Palette {
    pub const NEON_PINK: Palette = Palette {
        primary: [1.0, 0.2, 0.6],
        secondary: [0.2, 0.1, 0.4],
        accent: [1.0, 1.0, 1.0],
    };
    pub const COOL_WAVE: Palette = Palette {
        primary: [0.2, 0.6, 1.0],
        secondary: [0.05, 0.1, 0.3],
        accent: [0.7, 0.95, 1.0],
    };
    pub const SUNSET: Palette = Palette {
        primary: [1.0, 0.5, 0.2],
        secondary: [0.6, 0.1, 0.4],
        accent: [1.0, 0.85, 0.3],
    };
    pub const MONOCHROME: Palette = Palette {
        primary: [1.0, 1.0, 1.0],
        secondary: [0.05, 0.05, 0.05],
        accent: [0.5, 0.5, 0.5],
    };
}

#[derive(Debug, Clone, Copy)]
pub struct VJFrame {
    pub pattern: VJPattern,
    pub palette: Palette,
    /// 0..1 — modulates pattern brightness.
    pub intensity: f32,
    /// Phase 0..1 within current bar — pattern animator uses this.
    pub bar_phase: f32,
}

#[derive(Debug, Clone)]
pub struct VJDeck {
    /// Per-phrase loop of (pattern, palette).
    pub program: Vec<(VJPattern, Palette)>,
    /// Smoothed colour (eased toward target each tick).
    eased_intensity: f32,
}

impl VJDeck {
    pub fn new(program: Vec<(VJPattern, Palette)>) -> Self {
        Self {
            program: if program.is_empty() {
                vec![(VJPattern::Solid, Palette::COOL_WAVE)]
            } else {
                program
            },
            eased_intensity: 0.5,
        }
    }

    /// Default 4-step program suitable for a generic 4-track set.
    pub fn default_program() -> Self {
        VJDeck::new(vec![
            (VJPattern::Stripes, Palette::COOL_WAVE),
            (VJPattern::Pulse, Palette::NEON_PINK),
            (VJPattern::Rings, Palette::SUNSET),
            (VJPattern::Noise, Palette::MONOCHROME),
        ])
    }

    /// Resolve the current frame. Pattern picked by `phase.phrase % len`.
    /// Intensity eases toward `target` (0..1).
    pub fn frame(&mut self, phase: BeatPhase, target_intensity: f32) -> VJFrame {
        let idx = (phase.phrase as usize) % self.program.len();
        let (pattern, palette) = self.program[idx];
        // Exponential ease — same per-frame factor regardless of dt is
        // OK here because the deck just needs to feel smooth, not be
        // dt-correct. Simple and robust.
        let alpha = 0.15;
        self.eased_intensity = self.eased_intensity * (1.0 - alpha) + target_intensity * alpha;
        VJFrame {
            pattern,
            palette,
            intensity: self.eased_intensity.clamp(0.0, 1.0),
            bar_phase: phase.bar_frac,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_advances_with_phrase() {
        let mut d = VJDeck::default_program();
        let f0 = d.frame(p(0), 0.5);
        let f1 = d.frame(p(1), 0.5);
        let f2 = d.frame(p(2), 0.5);
        assert_ne!(f0.pattern, f1.pattern);
        assert_ne!(f1.pattern, f2.pattern);
    }

    #[test]
    fn intensity_eases_toward_target() {
        let mut d = VJDeck::default_program();
        let mut last = 0.5;
        for _ in 0..50 {
            let f = d.frame(p(0), 1.0);
            assert!(f.intensity >= last - 1e-3);
            last = f.intensity;
        }
        assert!(last > 0.95);
    }

    #[test]
    fn empty_program_falls_back_to_solid() {
        let mut d = VJDeck::new(vec![]);
        let f = d.frame(p(0), 0.5);
        assert_eq!(f.pattern, VJPattern::Solid);
    }

    fn p(phrase: u32) -> BeatPhase {
        BeatPhase {
            time: 0.0,
            beat: 0,
            bar: 0,
            phrase,
            beat_frac: 0.0,
            bar_frac: 0.0,
        }
    }
}
