//! Field-value → atlas-icon mapping (N5).
//!
//! Scenes that render phenomena driven by scalar fields (heat /
//! moisture) used to have ad-hoc threshold ladders sprinkled inline:
//!
//!     if h > 50.0 { FLAME_LARGE } else if h > 25.0 { FLAME_MEDIUM } ...
//!
//! This crate centralises the mapping in a single ordered rule list
//! so every scene shares the same visual language (same heat threshold
//! → same flame size → same colour → same spring animation settings).
//!
//! Tweaking Nintendo-style feel = edit one preset, not N scenes.

use crate::atlas_slot;

/// One resolved icon recipe. Scene emitters read fields off this and
/// feed them straight into `AtlasVisAdapter::emit_bobbing` / `emit`.
#[derive(Debug, Clone, Copy)]
pub struct FieldIcon {
    pub slot: u32,
    pub tint: [f32; 3],
    pub size: f32,
    /// Whether the sprite should bob / pulse / wiggle (Nintendo feel).
    pub bobbing: bool,
    /// Life in seconds.
    pub life: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldIconRule {
    pub min_heat: f32,
    pub max_heat: f32,
    pub min_moist: f32,
    pub max_moist: f32,
    pub icon: FieldIcon,
}

pub struct FieldIconMap {
    pub rules: Vec<FieldIconRule>,
}

impl FieldIconMap {
    /// Most-specific-first rule table. The first rule whose heat /
    /// moist windows enclose the sample wins.
    ///
    /// Slots used:
    /// - `FLAME_SMALL/MEDIUM/LARGE` for dry-hot cells
    /// - `STEAM_PUFF` for hot ∧ wet (evaporating interface)
    /// - `BUBBLE` for warm-damp (suppressed paper)
    /// - `WATER_DROP` for cold-wet (gravity wins)
    /// - `EMBER` for hot remnant after water extinguish
    pub fn nintendo_default() -> Self {
        let dry = 0.12;  // "effectively dry" moisture threshold
        let wet = 0.10;  // "effectively wet" threshold
        let flame_tint = [1.0, 0.5, 0.1];
        Self {
            rules: vec![
                FieldIconRule {
                    min_heat: 50.0, max_heat: f32::INFINITY,
                    min_moist: 0.0, max_moist: dry,
                    icon: FieldIcon {
                        slot: atlas_slot::FLAME_LARGE, tint: flame_tint,
                        size: 1.8, bobbing: true, life: 0.35,
                    },
                },
                FieldIconRule {
                    min_heat: 22.0, max_heat: f32::INFINITY,
                    min_moist: 0.0, max_moist: dry,
                    icon: FieldIcon {
                        slot: atlas_slot::FLAME_MEDIUM, tint: flame_tint,
                        size: 1.3, bobbing: true, life: 0.32,
                    },
                },
                FieldIconRule {
                    min_heat: 8.0, max_heat: f32::INFINITY,
                    min_moist: 0.0, max_moist: dry,
                    icon: FieldIcon {
                        slot: atlas_slot::FLAME_SMALL, tint: flame_tint,
                        size: 0.95, bobbing: true, life: 0.30,
                    },
                },
                // Hot + wet → steam. Light blue-white drift.
                FieldIconRule {
                    min_heat: 5.0, max_heat: f32::INFINITY,
                    min_moist: wet, max_moist: f32::INFINITY,
                    icon: FieldIcon {
                        slot: atlas_slot::STEAM_PUFF, tint: [0.92, 0.95, 1.0],
                        size: 1.1, bobbing: false, life: 1.4,
                    },
                },
                // Cold + wet → gravity-loaded droplet.
                FieldIconRule {
                    min_heat: 0.0, max_heat: 4.0,
                    min_moist: 0.25, max_moist: f32::INFINITY,
                    icon: FieldIcon {
                        slot: atlas_slot::WATER_DROP, tint: [0.45, 0.7, 1.0],
                        size: 0.7, bobbing: false, life: 1.0,
                    },
                },
                // Lukewarm + damp → bubble (paper-soaked).
                FieldIconRule {
                    min_heat: 0.0, max_heat: 8.0,
                    min_moist: 0.12, max_moist: f32::INFINITY,
                    icon: FieldIcon {
                        slot: atlas_slot::BUBBLE, tint: [0.55, 0.8, 1.0],
                        size: 0.8, bobbing: true, life: 0.6,
                    },
                },
                // Dying heat, dry → ember.
                FieldIconRule {
                    min_heat: 2.0, max_heat: 8.0,
                    min_moist: 0.0, max_moist: dry,
                    icon: FieldIcon {
                        slot: atlas_slot::EMBER, tint: [1.0, 0.6, 0.3],
                        size: 0.5, bobbing: false, life: 0.6,
                    },
                },
            ],
        }
    }

    pub fn pick(&self, heat: f32, moist: f32) -> Option<FieldIcon> {
        self.rules.iter()
            .find(|r| heat >= r.min_heat && heat <= r.max_heat
                   && moist >= r.min_moist && moist <= r.max_moist)
            .map(|r| r.icon)
    }
}
