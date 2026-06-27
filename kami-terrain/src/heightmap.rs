//! Heightmap: 2D grid of elevation values generated via FBM noise.
//!
//! Decima-style clipmap design: each LOD level is a fixed-size grid centered
//! on the camera, with coarser resolution at greater distances.

use crate::noise::fbm_noise;
use serde::{Deserialize, Serialize};

/// Configuration for procedural heightmap generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeightmapConfig {
    /// Seed offset for deterministic generation.
    pub seed: f32,
    /// Maximum terrain height in world units.
    pub max_height: f32,
    /// Noise frequency (higher = more detail, smaller features).
    pub frequency: f32,
    /// FBM octaves (4-8 typical).
    pub octaves: u32,
    /// FBM lacunarity (frequency multiplier, typically 2.0).
    pub lacunarity: f32,
    /// FBM persistence (amplitude falloff, typically 0.5).
    pub persistence: f32,
}

impl Default for HeightmapConfig {
    fn default() -> Self {
        Self {
            seed: 0.0,
            max_height: 120.0,
            frequency: 0.005,
            octaves: 6,
            lacunarity: 2.0,
            persistence: 0.5,
        }
    }
}

/// A 2D grid of height values.
pub struct Heightmap {
    pub width: u32,
    pub depth: u32,
    pub data: Vec<f32>,
    pub config: HeightmapConfig,
}

impl Heightmap {
    /// Generate a heightmap for a terrain patch at world origin (wx, wz).
    pub fn generate(width: u32, depth: u32, wx: f32, wz: f32, config: &HeightmapConfig) -> Self {
        let mut data = Vec::with_capacity((width * depth) as usize);
        for z in 0..depth {
            for x in 0..width {
                let world_x = wx + x as f32;
                let world_z = wz + z as f32;
                let nx = world_x * config.frequency + config.seed;
                let nz = world_z * config.frequency + config.seed * 0.7;
                let h = fbm_noise(
                    nx,
                    nz,
                    config.octaves,
                    config.lacunarity,
                    config.persistence,
                );
                // Apply curve: flatten valleys, sharpen peaks (Decima-style)
                let curved = h * h * (3.0 - 2.0 * h); // smoothstep
                data.push(curved * config.max_height);
            }
        }
        Self {
            width,
            depth,
            data,
            config: config.clone(),
        }
    }

    /// Sample height at fractional position with bilinear interpolation.
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        let x = x.clamp(0.0, (self.width - 1) as f32);
        let z = z.clamp(0.0, (self.depth - 1) as f32);
        let xi = x.floor() as u32;
        let zi = z.floor() as u32;
        let xf = x - x.floor();
        let zf = z - z.floor();

        let x0 = xi.min(self.width - 1);
        let x1 = (xi + 1).min(self.width - 1);
        let z0 = zi.min(self.depth - 1);
        let z1 = (zi + 1).min(self.depth - 1);

        let h00 = self.data[(z0 * self.width + x0) as usize];
        let h10 = self.data[(z0 * self.width + x1) as usize];
        let h01 = self.data[(z1 * self.width + x0) as usize];
        let h11 = self.data[(z1 * self.width + x1) as usize];

        let ix0 = h00 * (1.0 - xf) + h10 * xf;
        let ix1 = h01 * (1.0 - xf) + h11 * xf;
        ix0 * (1.0 - zf) + ix1 * zf
    }

    /// Compute normal at a grid point via central differences.
    pub fn normal(&self, x: u32, z: u32) -> [f32; 3] {
        let idx = |cx: u32, cz: u32| self.data[(cz * self.width + cx) as usize];
        let x0 = if x > 0 { x - 1 } else { 0 };
        let x1 = (x + 1).min(self.width - 1);
        let z0 = if z > 0 { z - 1 } else { 0 };
        let z1 = (z + 1).min(self.depth - 1);

        let dx = idx(x1, z) - idx(x0, z);
        let dz = idx(x, z1) - idx(x, z0);

        let nx = -dx;
        let ny = 2.0;
        let nz = -dz;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        [nx / len, ny / len, nz / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sample() {
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(64, 64, 0.0, 0.0, &cfg);
        assert_eq!(hm.data.len(), 64 * 64);
        let h = hm.sample(32.0, 32.0);
        assert!(h >= 0.0 && h <= cfg.max_height);
    }

    #[test]
    fn normal_unit_length() {
        let cfg = HeightmapConfig::default();
        let hm = Heightmap::generate(16, 16, 0.0, 0.0, &cfg);
        let n = hm.normal(8, 8);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 0.001);
    }
}
