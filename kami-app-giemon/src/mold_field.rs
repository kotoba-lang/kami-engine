//! mold_field — a scrubbable mold-coverage field over the cleaning surface.
//!
//! The kami-genesis contact solver is rigid-body only (no FEM/MPM erodible
//! material in R1.1), so the mold itself is modelled here, at the giemon
//! application layer, as a 2-D coverage grid lying on the contact ground plane.
//! Each cell holds a coverage value in `[0, 1]` (1 = full mold, 0 = clean).
//! The brush removes coverage where it presses AND slides — i.e. proportional
//! to scrub work (a pressure proxy × tangential slip), localised to the brush
//! footprint. This consumes only the public articulation state, so it does not
//! couple into the solver.

use glam::Vec2;

/// 2-D mold-coverage field on the surface plane (world x–y, row-major).
#[derive(Clone, Debug)]
pub struct MoldField {
    /// World (x, y) of the cell-(0,0) corner.
    pub origin: Vec2,
    /// Square cell size (m).
    pub cell: f32,
    pub nx: usize,
    pub ny: usize,
    /// Coverage per cell, `[0, 1]`, index `iy * nx + ix`.
    pub coverage: Vec<f32>,
}

impl MoldField {
    /// A field of `nx × ny` cells, every cell initialised to `initial`.
    pub fn new(origin: Vec2, cell: f32, nx: usize, ny: usize, initial: f32) -> Self {
        Self { origin, cell, nx, ny, coverage: vec![initial.clamp(0.0, 1.0); nx * ny] }
    }

    /// Total remaining mold coverage (sum over cells).
    pub fn total_coverage(&self) -> f32 {
        self.coverage.iter().sum()
    }

    /// Cell index for a world (x, y), if inside the grid.
    pub fn cell_index(&self, x: f32, y: f32) -> Option<usize> {
        let ix = ((x - self.origin.x) / self.cell).floor();
        let iy = ((y - self.origin.y) / self.cell).floor();
        if ix < 0.0 || iy < 0.0 {
            return None;
        }
        let (ix, iy) = (ix as usize, iy as usize);
        (ix < self.nx && iy < self.ny).then_some(iy * self.nx + ix)
    }

    /// Coverage at a world (x, y), or `None` if outside the grid.
    pub fn coverage_at(&self, x: f32, y: f32) -> Option<f32> {
        self.cell_index(x, y).map(|i| self.coverage[i])
    }

    /// Remove mold under a circular brush footprint centred at world `(cx, cy)`
    /// with `radius`, at the given scrub `intensity` (removal at the centre per
    /// call; falls off linearly to the footprint edge). Returns the total
    /// coverage removed this call.
    pub fn scrub(&mut self, cx: f32, cy: f32, radius: f32, intensity: f32) -> f32 {
        if intensity <= 0.0 || radius <= 0.0 {
            return 0.0;
        }
        let r2 = radius * radius;
        let ix0 = (((cx - radius) - self.origin.x) / self.cell).floor().max(0.0) as usize;
        let iy0 = (((cy - radius) - self.origin.y) / self.cell).floor().max(0.0) as usize;
        let ix1 = (((cx + radius) - self.origin.x) / self.cell).ceil().max(0.0) as usize;
        let iy1 = (((cy + radius) - self.origin.y) / self.cell).ceil().max(0.0) as usize;
        let mut removed = 0.0;
        for iy in iy0..iy1.min(self.ny) {
            for ix in ix0..ix1.min(self.nx) {
                // cell centre in world
                let wx = self.origin.x + (ix as f32 + 0.5) * self.cell;
                let wy = self.origin.y + (iy as f32 + 0.5) * self.cell;
                let d2 = (wx - cx) * (wx - cx) + (wy - cy) * (wy - cy);
                if d2 > r2 {
                    continue;
                }
                let falloff = 1.0 - d2.sqrt() / radius; // 1 at centre → 0 at edge
                let idx = iy * self.nx + ix;
                let take = (intensity * falloff).min(self.coverage[idx]).max(0.0);
                self.coverage[idx] -= take;
                removed += take;
            }
        }
        removed
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    #[test]
    fn scrub_is_local_and_bounded() {
        let mut f = MoldField::new(Vec2::new(0.0, 0.0), 0.1, 10, 10, 1.0);
        let total0 = f.total_coverage();
        assert!((total0 - 100.0).abs() < 1e-3, "100 cells × 1.0");
        // Scrub hard at the centre of the grid.
        let removed = f.scrub(0.5, 0.5, 0.25, 1.0);
        assert!(removed > 0.0, "should remove some mold");
        assert!(f.total_coverage() < total0, "total coverage decreases");
        // A far corner cell is untouched.
        assert_eq!(f.coverage_at(0.05, 0.05), Some(1.0), "corner stays full");
        // Coverage never goes negative.
        assert!(f.coverage.iter().all(|&c| c >= 0.0), "no negative coverage");
        // Over-scrubbing saturates to clean, not below zero.
        for _ in 0..50 {
            f.scrub(0.5, 0.5, 0.25, 1.0);
        }
        assert!(f.coverage_at(0.5, 0.5).unwrap() <= 1e-6, "centre fully cleaned");
        assert!(f.coverage.iter().all(|&c| c >= 0.0));
    }

    #[test]
    fn cell_index_maps_world_to_grid() {
        let f = MoldField::new(Vec2::new(-0.2, -0.3), 0.05, 8, 6, 1.0);
        assert_eq!(f.cell_index(-0.2 + 0.01, -0.3 + 0.01), Some(0));
        assert!(f.cell_index(-0.5, 0.0).is_none(), "left of grid");
        assert!(f.cell_index(10.0, 10.0).is_none(), "right of grid");
    }
}
