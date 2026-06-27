//! Distance-based culling for vegetation instances.
//!
//! Returns the `budget` closest instances to the camera (XZ plane).
//! LOD billboard ranges per species come from `lod.rs`.

use crate::instance::InstanceData;
use crate::lod::{LodTier, classify_lod};
use crate::species::SpeciesId;

/// Cull + sort by distance. Returns indices into the input slice, closest first.
/// Skips instances beyond their species' billboard range.
pub fn cull_by_distance(
    instances: &[InstanceData],
    cam_x: f32,
    cam_z: f32,
    budget: usize,
) -> Vec<u32> {
    // Precompute (distance², index) for visible instances only
    let mut visible: Vec<(f32, u32)> = Vec::with_capacity(instances.len() / 4);
    for (i, inst) in instances.iter().enumerate() {
        let dx = inst.position[0] - cam_x;
        let dz = inst.position[2] - cam_z;
        let d2 = dx * dx + dz * dz;
        // Species-based distance filter
        let species = match inst.species as u32 {
            0 => SpeciesId::Grass,
            1 => SpeciesId::Fern,
            2 => SpeciesId::PalmTree,
            3 => SpeciesId::Conifer,
            _ => SpeciesId::Bush,
        };
        // Use sqrt only if needed (compare d2 vs billboard² via classify_lod below)
        let tier = classify_lod(d2.sqrt(), species);
        if tier != LodTier::Culled {
            visible.push((d2, i as u32));
        }
    }
    // Partial sort: keep closest `budget` entries
    if visible.len() > budget {
        visible.select_nth_unstable_by(budget, |a, b| a.0.partial_cmp(&b.0).unwrap());
        visible.truncate(budget);
    }
    visible.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    visible.into_iter().map(|(_, i)| i).collect()
}

/// Convenience: returns the culled instances as a flat f32 buffer (8 floats per instance).
pub fn cull_to_buffer(
    instances: &[InstanceData],
    cam_x: f32,
    cam_z: f32,
    budget: usize,
) -> Vec<f32> {
    let idxs = cull_by_distance(instances, cam_x, cam_z, budget);
    let mut out = Vec::with_capacity(idxs.len() * 8);
    for i in idxs {
        let inst = &instances[i as usize];
        out.extend_from_slice(&[
            inst.position[0],
            inst.position[1],
            inst.position[2],
            inst.scale,
            inst.rotation,
            inst.species,
            inst.wind_phase,
            inst.color_tint,
        ]);
    }
    out
}

// ── Stage 5: Patch clustering (spatial cell grouping) ──

/// A spatial patch containing nearby instance indices (used for batch draw
/// and patch-LOD skipping when an entire cell is off-screen).
pub struct Patch {
    /// Grid cell coordinate.
    pub cell_x: i32,
    pub cell_z: i32,
    /// Center of the cell (world space XZ).
    pub center: [f32; 2],
    /// Instance indices inside this cell.
    pub instances: Vec<u32>,
}

/// Bin instances into spatial cells of `cell_size` × `cell_size` world units.
/// For N=10k instances with cell_size=32, produces ~O(N/cell_area) patches.
/// Patches enable whole-cell frustum cull before per-instance tests.
pub fn build_patches(instances: &[InstanceData], cell_size: f32) -> Vec<Patch> {
    use std::collections::HashMap;
    let inv = 1.0 / cell_size;
    let mut map: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, inst) in instances.iter().enumerate() {
        let cx = (inst.position[0] * inv).floor() as i32;
        let cz = (inst.position[2] * inv).floor() as i32;
        map.entry((cx, cz)).or_default().push(i as u32);
    }
    let half = cell_size * 0.5;
    map.into_iter()
        .map(|((cx, cz), ids)| Patch {
            cell_x: cx,
            cell_z: cz,
            center: [cx as f32 * cell_size + half, cz as f32 * cell_size + half],
            instances: ids,
        })
        .collect()
}

/// Fast patch-level cull: reject entire cells beyond `max_dist` from camera
/// (XZ plane). Cells with any overlap are kept, passed to per-instance cull.
pub fn patches_in_range(
    patches: &[Patch],
    cam_x: f32,
    cam_z: f32,
    max_dist: f32,
    cell_size: f32,
) -> Vec<usize> {
    let max_d2 = (max_dist + cell_size * 0.71).powi(2); // add half-diagonal
    patches
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let dx = p.center[0] - cam_x;
            let dz = p.center[1] - cam_z;
            if dx * dx + dz * dz < max_d2 {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

/// Patch-aware cull: use spatial cells to pre-reject far patches, then do
/// per-instance distance sort within remaining patches.
///
/// This is the N > ~10k scaling variant. For N < 5k, `cull_to_buffer`
/// (flat iteration) is faster due to cache locality. Heuristic: use patches
/// when instance count × cull overhead > patch-map build cost.
pub fn cull_with_patches(
    instances: &[InstanceData],
    patches: &[Patch],
    cam_x: f32,
    cam_z: f32,
    budget: usize,
    max_dist: f32,
    cell_size: f32,
) -> Vec<f32> {
    let nearby = patches_in_range(patches, cam_x, cam_z, max_dist, cell_size);
    // Collect candidate indices from nearby patches only
    let mut candidates: Vec<u32> = Vec::new();
    for pi in nearby {
        candidates.extend_from_slice(&patches[pi].instances);
    }
    // Per-instance distance sort within candidates
    let mut visible: Vec<(f32, u32)> = candidates
        .into_iter()
        .filter_map(|i| {
            let inst = &instances[i as usize];
            let dx = inst.position[0] - cam_x;
            let dz = inst.position[2] - cam_z;
            let d2 = dx * dx + dz * dz;
            // Species-based LOD
            let species = match inst.species as u32 {
                0 => crate::species::SpeciesId::Grass,
                1 => crate::species::SpeciesId::Fern,
                2 => crate::species::SpeciesId::PalmTree,
                3 => crate::species::SpeciesId::Conifer,
                _ => crate::species::SpeciesId::Bush,
            };
            if crate::lod::classify_lod(d2.sqrt(), species) == crate::lod::LodTier::Culled {
                None
            } else {
                Some((d2, i))
            }
        })
        .collect();
    if visible.len() > budget {
        visible.select_nth_unstable_by(budget, |a, b| a.0.partial_cmp(&b.0).unwrap());
        visible.truncate(budget);
    }
    visible.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut out = Vec::with_capacity(visible.len() * 8);
    for (_, i) in visible {
        let inst = &instances[i as usize];
        out.extend_from_slice(&[
            inst.position[0],
            inst.position[1],
            inst.position[2],
            inst.scale,
            inst.rotation,
            inst.species,
            inst.wind_phase,
            inst.color_tint,
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(x: f32, z: f32, species: f32) -> InstanceData {
        InstanceData {
            position: [x, 0.0, z],
            scale: 1.0,
            rotation: 0.0,
            species,
            wind_phase: 0.0,
            color_tint: 0.0,
        }
    }

    #[test]
    fn closest_first() {
        let inst = vec![
            mk(50.0, 0.0, 0.0), // far grass
            mk(5.0, 0.0, 0.0),  // near grass
            mk(20.0, 0.0, 0.0), // mid grass
        ];
        let idxs = cull_by_distance(&inst, 0.0, 0.0, 10);
        assert_eq!(idxs, vec![1, 2, 0]);
    }

    #[test]
    fn budget_caps_count() {
        let inst: Vec<_> = (0..100).map(|i| mk(i as f32, 0.0, 0.0)).collect();
        let idxs = cull_by_distance(&inst, 0.0, 0.0, 5);
        assert_eq!(idxs.len(), 5);
        // First 5 closest are indices 0-4
        assert_eq!(idxs, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn grass_beyond_billboard_culled() {
        let inst = vec![mk(500.0, 0.0, 0.0)]; // grass at 500m, billboard=60m
        let idxs = cull_by_distance(&inst, 0.0, 0.0, 10);
        assert!(idxs.is_empty());
    }

    #[test]
    fn tree_beyond_grass_range_kept() {
        let inst = vec![mk(400.0, 0.0, 2.0)]; // palm at 400m, billboard=600m
        let idxs = cull_by_distance(&inst, 0.0, 0.0, 10);
        assert_eq!(idxs.len(), 1);
    }

    #[test]
    fn patches_bin_instances() {
        let inst = vec![
            mk(5.0, 5.0, 0.0),   // cell (0, 0)
            mk(70.0, 5.0, 0.0),  // cell (2, 0) with cell_size=32
            mk(10.0, 10.0, 0.0), // cell (0, 0)
        ];
        let patches = build_patches(&inst, 32.0);
        assert_eq!(patches.len(), 2);
        let total: usize = patches.iter().map(|p| p.instances.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn patches_reject_far_cells() {
        let inst: Vec<_> = (0..20).map(|i| mk((i * 20) as f32, 0.0, 0.0)).collect();
        let patches = build_patches(&inst, 32.0);
        let nearby = patches_in_range(&patches, 0.0, 0.0, 40.0, 32.0);
        // Only patches within ~40m + half diagonal of origin should be kept
        assert!(nearby.len() < patches.len(), "should reject far cells");
        assert!(!nearby.is_empty(), "should keep near cells");
    }
}
