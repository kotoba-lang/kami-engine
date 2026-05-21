//! kami-nerf: NeRF density field → VoxelVolume.
//! Loads pre-trained NeRF density grids and samples into voxel volumes.

use glam::Vec3;
use kami_voxel::{Voxel, VoxelVolume};

/// 3D density grid from a pre-trained NeRF model.
pub struct DensityGrid {
    pub data: Vec<f32>,
    pub color_data: Vec<[f32; 3]>,
    pub dims: [u32; 3],
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl DensityGrid {
    pub fn new(data: Vec<f32>, dims: [u32; 3], bounds_min: Vec3, bounds_max: Vec3) -> Self {
        Self {
            data,
            color_data: Vec::new(),
            dims,
            bounds_min,
            bounds_max,
        }
    }

    pub fn with_colors(mut self, colors: Vec<[f32; 3]>) -> Self {
        self.color_data = colors;
        self
    }

    pub fn sample(&self, p: Vec3) -> f32 {
        let [dx, dy, dz] = self.dims;
        let range = self.bounds_max - self.bounds_min;
        let norm = (p - self.bounds_min) / range;
        let gx = (norm.x * (dx - 1) as f32).clamp(0.0, (dx - 1) as f32);
        let gy = (norm.y * (dy - 1) as f32).clamp(0.0, (dy - 1) as f32);
        let gz = (norm.z * (dz - 1) as f32).clamp(0.0, (dz - 1) as f32);
        let ix = gx as u32;
        let iy = gy as u32;
        let iz = gz as u32;
        let fx = gx.fract();
        let fy = gy.fract();
        let fz = gz.fract();
        let idx = |x: u32, y: u32, z: u32| -> f32 {
            self.data[(z.min(dz - 1) * dy * dx + y.min(dy - 1) * dx + x.min(dx - 1)) as usize]
        };
        idx(ix, iy, iz) * (1.0 - fx) * (1.0 - fy) * (1.0 - fz)
            + idx(ix + 1, iy, iz) * fx * (1.0 - fy) * (1.0 - fz)
            + idx(ix, iy + 1, iz) * (1.0 - fx) * fy * (1.0 - fz)
            + idx(ix + 1, iy + 1, iz) * fx * fy * (1.0 - fz)
            + idx(ix, iy, iz + 1) * (1.0 - fx) * (1.0 - fy) * fz
            + idx(ix + 1, iy, iz + 1) * fx * (1.0 - fy) * fz
            + idx(ix, iy + 1, iz + 1) * (1.0 - fx) * fy * fz
            + idx(ix + 1, iy + 1, iz + 1) * fx * fy * fz
    }

    pub fn sample_color(&self, p: Vec3) -> [f32; 3] {
        if self.color_data.is_empty() {
            return [0.5; 3];
        }
        let [dx, dy, dz] = self.dims;
        let range = self.bounds_max - self.bounds_min;
        let norm = (p - self.bounds_min) / range;
        let ix = (norm.x * (dx - 1) as f32)
            .round()
            .clamp(0.0, (dx - 1) as f32) as u32;
        let iy = (norm.y * (dy - 1) as f32)
            .round()
            .clamp(0.0, (dy - 1) as f32) as u32;
        let iz = (norm.z * (dz - 1) as f32)
            .round()
            .clamp(0.0, (dz - 1) as f32) as u32;
        let idx = (iz * dy * dx + iy * dx + ix) as usize;
        if idx < self.color_data.len() {
            self.color_data[idx]
        } else {
            [0.5; 3]
        }
    }

    pub fn to_volume(&self, resolution: u32, threshold: f32) -> VoxelVolume {
        let mut volume = VoxelVolume::new_dense(resolution, resolution, resolution);
        let range = self.bounds_max - self.bounds_min;
        let step = range / resolution as f32;
        for z in 0..resolution {
            for y in 0..resolution {
                for x in 0..resolution {
                    let p = self.bounds_min
                        + Vec3::new(
                            (x as f32 + 0.5) * step.x,
                            (y as f32 + 0.5) * step.y,
                            (z as f32 + 0.5) * step.z,
                        );
                    if self.sample(p) >= threshold {
                        let [r, g, b] = self.sample_color(p);
                        volume.set(
                            x,
                            y,
                            z,
                            Voxel {
                                material: 1,
                                color: [r, g, b, 1.0],
                            },
                        );
                    }
                }
            }
        }
        volume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_density() {
        let mut data = vec![0.0f32; 512];
        let center = Vec3::splat(3.5);
        for z in 0..8u32 {
            for y in 0..8u32 {
                for x in 0..8u32 {
                    let d = (Vec3::new(x as f32, y as f32, z as f32) - center).length();
                    data[(z * 64 + y * 8 + x) as usize] = if d < 3.0 { 1.0 } else { 0.0 };
                }
            }
        }
        let grid = DensityGrid::new(data, [8, 8, 8], Vec3::ZERO, Vec3::splat(8.0));
        let vol = grid.to_volume(8, 0.5);
        assert!(vol.count_filled() > 0 && vol.count_filled() < 512);
    }

    #[test]
    fn with_colors() {
        let data = vec![1.0f32; 64];
        let colors: Vec<[f32; 3]> = (0..64).map(|i| [i as f32 / 64.0, 0.0, 0.0]).collect();
        let grid =
            DensityGrid::new(data, [4, 4, 4], Vec3::ZERO, Vec3::splat(4.0)).with_colors(colors);
        let vol = grid.to_volume(4, 0.5);
        assert_eq!(vol.count_filled(), 64);
    }
}
