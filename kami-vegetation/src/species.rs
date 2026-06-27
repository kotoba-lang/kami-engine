//! Species definitions: grass, fern, palm, tree with placement rules.

use serde::{Deserialize, Serialize};

/// Species identifier (maps to GPU atlas index).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpeciesId {
    Grass = 0,
    Fern = 1,
    PalmTree = 2,
    Conifer = 3,
    Bush = 4,
}

impl SpeciesId {
    pub fn all() -> &'static [SpeciesId] {
        &[
            SpeciesId::Grass,
            SpeciesId::Fern,
            SpeciesId::PalmTree,
            SpeciesId::Conifer,
            SpeciesId::Bush,
        ]
    }
}

/// Species placement rules + render params.
#[derive(Debug, Clone)]
pub struct Species {
    pub id: SpeciesId,
    pub name: &'static str,

    // Placement constraints
    /// Min height above sea level.
    pub min_height: f32,
    /// Max height above sea level.
    pub max_height: f32,
    /// Max slope (0 = flat only, 1 = any).
    pub max_slope: f32,
    /// Splatmap weight preference (which material this grows on).
    /// (grass_weight, rock_weight, sand_weight, snow_weight) — multiplied with splatmap
    pub material_affinity: [f32; 4],
    /// Density: instances per 100 square units.
    pub density: f32,
    /// Minimum separation radius (Poisson disk).
    pub min_distance: f32,

    // Render params
    /// Base height in world units.
    pub height: f32,
    /// Height randomization [min, max] multiplier.
    pub scale_range: [f32; 2],
    /// Wind sway amplitude (how much the top bends).
    pub wind_sway: f32,
    /// Base color (RGB, 0-1).
    pub color: [f32; 3],
    /// Secondary tip color (for gradient).
    pub tip_color: [f32; 3],
}

/// Full species table — the set of things that can grow in the world.
pub fn species_table() -> Vec<Species> {
    vec![
        Species {
            id: SpeciesId::Grass,
            name: "grass",
            min_height: 16.0, // above water
            max_height: 95.0, // wider range
            max_slope: 0.55,
            // Grass tolerates rocky/sandy substrates (dry tussock grass in nature)
            material_affinity: [1.0, 0.5, 0.6, 0.0],
            density: 500.0, // dense
            min_distance: 0.5,
            height: 0.8,
            scale_range: [0.7, 1.4],
            wind_sway: 0.35,
            color: [0.18, 0.42, 0.08],
            tip_color: [0.42, 0.68, 0.15],
        },
        Species {
            id: SpeciesId::Fern,
            name: "fern",
            min_height: 18.0,
            max_height: 80.0,
            max_slope: 0.5,
            material_affinity: [0.8, 0.4, 0.2, 0.0],
            density: 60.0,
            min_distance: 2.0,
            height: 1.4,
            scale_range: [0.8, 1.5],
            wind_sway: 0.25,
            color: [0.12, 0.28, 0.04],
            tip_color: [0.3, 0.55, 0.12],
        },
        Species {
            id: SpeciesId::PalmTree,
            name: "palm",
            min_height: 16.0,
            max_height: 30.0, // tropical coast
            max_slope: 0.3,
            material_affinity: [0.5, 0.0, 0.6, 0.0], // sand + grass
            density: 2.0,
            min_distance: 8.0,
            height: 8.5,
            scale_range: [0.85, 1.25],
            wind_sway: 0.6,
            color: [0.35, 0.22, 0.08],
            tip_color: [0.18, 0.45, 0.1],
        },
        Species {
            id: SpeciesId::Conifer,
            name: "conifer",
            min_height: 30.0,
            max_height: 85.0,
            max_slope: 0.55,
            material_affinity: [0.9, 0.3, 0.0, 0.0],
            density: 5.0,
            min_distance: 5.5,
            height: 10.0,
            scale_range: [0.7, 1.3],
            wind_sway: 0.2,
            color: [0.25, 0.18, 0.08],
            tip_color: [0.12, 0.3, 0.08],
        },
        Species {
            id: SpeciesId::Bush,
            name: "bush",
            min_height: 17.0,
            max_height: 70.0,
            max_slope: 0.5,
            material_affinity: [0.7, 0.2, 0.2, 0.0],
            density: 15.0,
            min_distance: 3.0,
            height: 1.8,
            scale_range: [0.8, 1.4],
            wind_sway: 0.3,
            color: [0.15, 0.28, 0.06],
            tip_color: [0.28, 0.48, 0.1],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_table_populated() {
        let table = species_table();
        assert_eq!(table.len(), 5);
        for s in &table {
            assert!(s.density > 0.0);
            assert!(s.min_distance > 0.0);
            assert!(s.height > 0.0);
        }
    }
}
