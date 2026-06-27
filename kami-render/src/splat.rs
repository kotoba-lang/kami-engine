//! 3D Gaussian Splatting data structures and GPU buffer management.
//!
//! Behind `gaussian-splat` feature. Experimental — for photorealistic backgrounds.

use bytemuck::{Pod, Zeroable};

/// Single Gaussian splat (56 bytes, GPU storage buffer element).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GaussianSplat {
    pub position: [f32; 3], // 12B — world position
    pub opacity: f32,       // 4B  — log-space, sigmoid at render
    pub scale: [f32; 3],    // 12B — log-space, exp at render
    pub _pad0: f32,         // 4B  — align
    pub rotation: [f32; 4], // 16B — quaternion (wxyz)
    pub sh_dc: [f32; 3],    // 12B — spherical harmonics DC band (color)
    pub _pad1: f32,         // 4B  — align to 64B
}

/// Collection of Gaussian splats.
///
/// `sh_degree` is the spherical-harmonics degree (0..3) the trainer
/// converged at. The DC band is always present in
/// `splats[i].sh_dc`; bands 1..sh_degree are stored separately in
/// `sh_rest`, **coefficient-major** (per-splat `(K-1)` RGB triples
/// for `K = (sh_degree+1)²`). Loaders re-arrange from the 3DGS PLY's
/// channel-major layout so the renderer's storage-buffer indexing
/// is straightforward.
///
/// `sh_rest.len() == 0` ⇔ `sh_degree == 0`. Renderers MUST treat the
/// DC-only path identically to a degree-0 cloud (no behavioral
/// change for existing assets).
pub struct SplatCloud {
    pub splats: Vec<GaussianSplat>,
    pub sh_degree: u8,
    pub sh_rest: Vec<[f32; 3]>,
}

impl SplatCloud {
    pub fn new() -> Self {
        Self {
            splats: Vec::new(),
            sh_degree: 0,
            sh_rest: Vec::new(),
        }
    }

    pub fn count(&self) -> u32 {
        self.splats.len() as u32
    }

    /// Number of SH coefficients per splat for `sh_degree`. K = (d+1)².
    pub fn sh_coef_count(&self) -> u32 {
        let d = self.sh_degree as u32 + 1;
        d * d
    }

    /// Cull splats beyond max_distance from camera. Returns indices of visible splats.
    pub fn cull_indices(&self, camera_pos: [f32; 3], max_distance: f32) -> Vec<u32> {
        let max_dist_sq = max_distance * max_distance;
        self.splats
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let dx = s.position[0] - camera_pos[0];
                let dy = s.position[1] - camera_pos[1];
                let dz = s.position[2] - camera_pos[2];
                if dx * dx + dy * dy + dz * dz <= max_dist_sq {
                    Some(i as u32)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get bounding box [min, max] of all splat positions.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        if self.splats.is_empty() {
            return ([0.0; 3], [0.0; 3]);
        }
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for s in &self.splats {
            for i in 0..3 {
                min[i] = min[i].min(s.position[i]);
                max[i] = max[i].max(s.position[i]);
            }
        }
        (min, max)
    }
}

impl Default for SplatCloud {
    fn default() -> Self {
        Self::new()
    }
}

/// GPU buffers for Gaussian splatting.
#[cfg(feature = "wgpu-backend")]
pub struct SplatGpuBuffers {
    pub splat_buffer: wgpu::Buffer, // storage, read — splat data
    pub sort_keys: wgpu::Buffer,    // storage, read-write — distance + index pairs
    pub count: u32,
}

#[cfg(feature = "wgpu-backend")]
impl SplatGpuBuffers {
    /// Upload splat cloud to GPU storage buffers.
    pub fn upload(device: &wgpu::Device, cloud: &SplatCloud) -> Self {
        use wgpu::util::DeviceExt;

        let splat_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("splat-data"),
            contents: bytemuck::cast_slice(&cloud.splats),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Sort keys: [distance: f32, index: u32] × count
        let sort_data = vec![0u8; cloud.count() as usize * 8];
        let sort_keys = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("splat-sort-keys"),
            contents: &sort_data,
            usage: wgpu::BufferUsages::STORAGE,
        });

        Self {
            splat_buffer,
            sort_keys,
            count: cloud.count(),
        }
    }
}

/// Sort entry for GPU radix/bitonic sort.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SortEntry {
    pub distance: f32,
    pub index: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splat_size() {
        assert_eq!(std::mem::size_of::<GaussianSplat>(), 64);
    }

    #[test]
    fn sort_entry_size() {
        assert_eq!(std::mem::size_of::<SortEntry>(), 8);
    }

    #[test]
    fn cloud_empty() {
        let cloud = SplatCloud::new();
        assert_eq!(cloud.count(), 0);
        let (min, max) = cloud.bounds();
        assert_eq!(min, [0.0; 3]);
        assert_eq!(max, [0.0; 3]);
    }

    #[test]
    fn cloud_cull() {
        let mut cloud = SplatCloud::new();
        cloud.splats.push(GaussianSplat {
            position: [0.0, 0.0, 0.0],
            opacity: 1.0,
            scale: [0.1, 0.1, 0.1],
            _pad0: 0.0,
            rotation: [1.0, 0.0, 0.0, 0.0],
            sh_dc: [0.5, 0.5, 0.5],
            _pad1: 0.0,
        });
        cloud.splats.push(GaussianSplat {
            position: [100.0, 0.0, 0.0],
            opacity: 1.0,
            scale: [0.1, 0.1, 0.1],
            _pad0: 0.0,
            rotation: [1.0, 0.0, 0.0, 0.0],
            sh_dc: [0.5, 0.5, 0.5],
            _pad1: 0.0,
        });
        let visible = cloud.cull_indices([0.0, 0.0, 0.0], 50.0);
        assert_eq!(visible, vec![0]); // only first splat within 50 units
    }

    #[test]
    fn cloud_bounds() {
        let mut cloud = SplatCloud::new();
        cloud.splats.push(GaussianSplat {
            position: [-1.0, 2.0, 3.0],
            opacity: 0.0,
            scale: [0.0; 3],
            _pad0: 0.0,
            rotation: [1.0, 0.0, 0.0, 0.0],
            sh_dc: [0.0; 3],
            _pad1: 0.0,
        });
        cloud.splats.push(GaussianSplat {
            position: [5.0, -1.0, 0.0],
            opacity: 0.0,
            scale: [0.0; 3],
            _pad0: 0.0,
            rotation: [1.0, 0.0, 0.0, 0.0],
            sh_dc: [0.0; 3],
            _pad1: 0.0,
        });
        let (min, max) = cloud.bounds();
        assert_eq!(min, [-1.0, -1.0, 0.0]);
        assert_eq!(max, [5.0, 2.0, 3.0]);
    }
}
