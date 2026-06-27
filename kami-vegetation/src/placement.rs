//! Placement: scatter instances over terrain using Poisson-disk + biome filter.

use crate::instance::InstanceData;
use crate::species::{Species, SpeciesId, species_table};
use kami_terrain::{Heightmap, Splatmap};

/// Placement configuration.
pub struct PlacementConfig {
    /// Random seed.
    pub seed: u32,
    /// World extent covered (centered at terrain origin).
    pub extent: f32,
    /// Global density multiplier (scales all species densities).
    pub density_scale: f32,
    /// Species subset to place (empty = all).
    pub species_filter: Vec<SpeciesId>,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            extent: 256.0,
            density_scale: 1.0,
            species_filter: Vec::new(),
        }
    }
}

/// Simple xorshift32 RNG (deterministic, no std dep).
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() & 0x7fffffff) as f32 / 0x7fffffff as f32
    }
    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
}

/// Place instances for all configured species.
/// Returns flat instance buffer grouped by species (sort-by-species for batched draws).
pub fn place_instances(
    hm: &Heightmap,
    splat: &Splatmap,
    origin_x: f32,
    origin_z: f32,
    config: &PlacementConfig,
) -> Vec<InstanceData> {
    let mut instances = Vec::new();
    let table = species_table();

    let active_species: Vec<&Species> = if config.species_filter.is_empty() {
        table.iter().collect()
    } else {
        table
            .iter()
            .filter(|s| config.species_filter.contains(&s.id))
            .collect()
    };

    for (idx, species) in active_species.iter().enumerate() {
        let mut rng = Rng::new(config.seed.wrapping_add(idx as u32 * 7919));
        let count =
            (species.density * config.density_scale * (config.extent / 100.0).powi(2)) as usize;

        let mut placed: Vec<(f32, f32)> = Vec::with_capacity(count);
        let min_dist_sq = species.min_distance * species.min_distance;

        // Dart-throwing Poisson-disk (simplified, ~30 attempts per desired point)
        let max_attempts = count * 6;
        let mut attempts = 0;

        while placed.len() < count && attempts < max_attempts {
            attempts += 1;
            let x = rng.range(-config.extent * 0.5, config.extent * 0.5);
            let z = rng.range(-config.extent * 0.5, config.extent * 0.5);

            // Separation check (only vs same species — cheap brute force for small counts)
            let too_close = placed.iter().any(|&(px, pz)| {
                let dx = x - px;
                let dz = z - pz;
                dx * dx + dz * dz < min_dist_sq
            });
            if too_close {
                continue;
            }

            // Sample terrain: world(x,z) → grid(gx,gz) where (origin_x, origin_z) = hm[0,0]
            let hx = (x - origin_x).clamp(0.0, (hm.width - 1) as f32);
            let hz = (z - origin_z).clamp(0.0, (hm.depth - 1) as f32);
            let height = hm.sample(hx, hz);

            // Biome filter: height range
            if height < species.min_height || height > species.max_height {
                continue;
            }

            // Slope filter
            let ni = (hz as u32).min(hm.depth - 1);
            let nj = (hx as u32).min(hm.width - 1);
            let normal = hm.normal(nj, ni);
            let slope = 1.0 - normal[1];
            if slope > species.max_slope {
                continue;
            }

            // Splatmap affinity
            let splat_idx = (ni * splat.width + nj) as usize;
            let sw = splat.data[splat_idx].weights;
            let affinity = sw[0] * species.material_affinity[0]
                + sw[1] * species.material_affinity[1]
                + sw[2] * species.material_affinity[2]
                + sw[3] * species.material_affinity[3];
            if affinity < 0.2 {
                continue;
            }

            // Accept
            placed.push((x, z));
            let scale = rng.range(species.scale_range[0], species.scale_range[1]);
            let rotation = rng.range(0.0, std::f32::consts::TAU);
            let wind_phase = rng.range(0.0, std::f32::consts::TAU);
            let color_tint = rng.range(-0.15, 0.15);

            instances.push(InstanceData {
                position: [x, height, z],
                scale,
                rotation,
                species: species.id as u32 as f32,
                wind_phase,
                color_tint,
            });
        }
    }

    instances
}

#[cfg(test)]
mod tests {
    use super::*;
    use kami_terrain::{Heightmap, HeightmapConfig, Splatmap};

    #[test]
    fn place_grass_on_plains() {
        // Use 257 × 257 terrain with extent=200 to guarantee some grass-valid
        // patches (height 16-75m on grass splatmap) regardless of FBM seed.
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(257, 257, 0.0, 0.0, &cfg);
        let splat = Splatmap::from_heightmap(&hm, 15.0, 90.0, 0.45);

        let pc = PlacementConfig {
            seed: 42,
            extent: 200.0,
            density_scale: 0.3,
            species_filter: vec![SpeciesId::Grass],
        };
        let instances = place_instances(&hm, &splat, 0.0, 0.0, &pc);
        assert!(!instances.is_empty(), "should place at least some grass");
        for i in &instances {
            assert!(i.position[1] >= 16.0 && i.position[1] <= 95.0);
            assert!((i.species - 0.0).abs() < 0.01);
        }
    }

    #[test]
    fn deterministic_seed() {
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(65, 65, 0.0, 0.0, &cfg);
        let splat = Splatmap::from_heightmap(&hm, 15.0, 90.0, 0.45);
        let pc = PlacementConfig {
            seed: 7,
            extent: 32.0,
            density_scale: 0.3,
            species_filter: vec![],
        };
        let a = place_instances(&hm, &splat, 0.0, 0.0, &pc);
        let b = place_instances(&hm, &splat, 0.0, 0.0, &pc);
        assert_eq!(a.len(), b.len());
        if !a.is_empty() {
            assert_eq!(a[0].position, b[0].position);
        }
    }
}
