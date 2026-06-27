//! Splatmap: per-vertex material weights for terrain texture blending.
//!
//! Decima uses 4-8 material layers blended by height, slope, and noise.
//! Each vertex stores 4 weights (grass, rock, sand, snow) summing to 1.0.

use crate::heightmap::Heightmap;

/// 4-channel material weights per vertex.
#[derive(Debug, Clone, Copy)]
pub struct SplatWeights {
    /// [grass, rock, sand, snow]
    pub weights: [f32; 4],
}

impl SplatWeights {
    pub fn normalize(&mut self) {
        let sum: f32 = self.weights.iter().sum();
        if sum > 0.0 {
            for w in &mut self.weights {
                *w /= sum;
            }
        }
    }
}

/// Material layer indices.
pub const MAT_GRASS: usize = 0;
pub const MAT_ROCK: usize = 1;
pub const MAT_SAND: usize = 2;
pub const MAT_SNOW: usize = 3;

/// Splatmap: parallel array to heightmap, one SplatWeights per vertex.
pub struct Splatmap {
    pub width: u32,
    pub depth: u32,
    pub data: Vec<SplatWeights>,
}

impl Splatmap {
    /// Generate splatmap from heightmap using height + slope rules.
    ///
    /// Rules (Decima-inspired):
    /// - Below `sand_line`: sand
    /// - Above `snow_line`: snow
    /// - Slope > `rock_threshold`: rock
    /// - Otherwise: grass
    pub fn from_heightmap(
        hm: &Heightmap,
        sand_line: f32,
        snow_line: f32,
        rock_threshold: f32,
    ) -> Self {
        let mut data = Vec::with_capacity((hm.width * hm.depth) as usize);

        for z in 0..hm.depth {
            for x in 0..hm.width {
                let h = hm.data[(z * hm.width + x) as usize];
                let n = hm.normal(x, z);
                let slope = 1.0 - n[1]; // 0 = flat, 1 = vertical

                let mut w = SplatWeights { weights: [0.0; 4] };

                // Height-based base material
                if h < sand_line {
                    w.weights[MAT_SAND] = 1.0;
                } else if h > snow_line {
                    w.weights[MAT_SNOW] = 1.0;
                } else {
                    // Blend grass ↔ snow by height
                    let t = ((h - sand_line) / (snow_line - sand_line)).clamp(0.0, 1.0);
                    w.weights[MAT_GRASS] = 1.0 - t * 0.3;
                    w.weights[MAT_SNOW] = t * 0.3;
                }

                // Slope override: steep = rock
                if slope > rock_threshold {
                    let rock_blend =
                        ((slope - rock_threshold) / (1.0 - rock_threshold)).clamp(0.0, 1.0);
                    for i in 0..4 {
                        w.weights[i] *= 1.0 - rock_blend;
                    }
                    w.weights[MAT_ROCK] += rock_blend;
                }

                w.normalize();
                data.push(w);
            }
        }

        Self {
            width: hm.width,
            depth: hm.depth,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightmap::{Heightmap, HeightmapConfig};

    #[test]
    fn weights_sum_to_one() {
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(32, 32, 0.0, 0.0, &cfg);
        let splat = Splatmap::from_heightmap(&hm, 10.0, 90.0, 0.4);
        for w in &splat.data {
            let sum: f32 = w.weights.iter().sum();
            assert!((sum - 1.0).abs() < 0.01, "weights don't sum to 1: {sum}");
        }
    }
}
