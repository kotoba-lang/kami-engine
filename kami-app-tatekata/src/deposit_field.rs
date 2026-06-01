//! deposit_field — concrete deposition / screed-levelling height field.
//!
//! An application-layer material-process stand-in (the same honest pattern as
//! kabitori's `MoldField`): the kami-genesis rigid solver has no granular/MPM
//! material model, so robotic concrete 3-D printing / floor screeding is
//! approximated as a 2-D height grid that a tool head raises toward a target
//! level. It captures *coverage progress + tool path*, NOT real concrete
//! rheology (slump, segregation, cold-joint) — those need a granular/MPM solver.
//!
//! Used by `robot:printer` steps (foundation slab, epoxy floor, paving).

/// A rectangular height field over a footprint (sim x/y, metres).
pub struct DepositField {
    pub nx: usize,
    pub ny: usize,
    x0: f32,
    y0: f32,
    dx: f32,
    dy: f32,
    target: f32,
    /// current deposited height per cell.
    pub h: Vec<f32>,
}

impl DepositField {
    /// `rect` = `[min_x, min_y, max_x, max_y]`; `target` = finish level (m).
    pub fn new(rect: [f32; 4], nx: usize, ny: usize, target: f32) -> Self {
        let nx = nx.max(2);
        let ny = ny.max(2);
        Self {
            nx,
            ny,
            x0: rect[0],
            y0: rect[1],
            dx: (rect[2] - rect[0]) / nx as f32,
            dy: (rect[3] - rect[1]) / ny as f32,
            target,
            h: vec![0.0; nx * ny],
        }
    }

    #[inline]
    fn idx(&self, ix: usize, iy: usize) -> usize {
        iy * self.nx + ix
    }

    pub fn cell_center(&self, ix: usize, iy: usize) -> (f32, f32) {
        (
            self.x0 + (ix as f32 + 0.5) * self.dx,
            self.y0 + (iy as f32 + 0.5) * self.dy,
        )
    }

    pub fn height_at(&self, ix: usize, iy: usize) -> f32 {
        self.h[self.idx(ix, iy)]
    }

    /// Deposit/level material under a tool head at `(x, y)` of `radius`,
    /// raising covered cells toward `target` at `rate` (m/s) for `dt` seconds.
    pub fn deposit_at(&mut self, x: f32, y: f32, radius: f32, rate: f32, dt: f32) {
        let r2 = radius * radius;
        for iy in 0..self.ny {
            for ix in 0..self.nx {
                let (cx, cy) = self.cell_center(ix, iy);
                let d2 = (cx - x) * (cx - x) + (cy - y) * (cy - y);
                if d2 <= r2 {
                    let i = self.idx(ix, iy);
                    let next = (self.h[i] + rate * dt).min(self.target);
                    self.h[i] = next;
                }
            }
        }
    }

    /// Fraction of the footprint brought up to the target level (0..1).
    pub fn progress(&self) -> f32 {
        if self.target <= 0.0 {
            return 1.0;
        }
        let mean: f32 = self.h.iter().sum::<f32>() / self.h.len() as f32;
        (mean / self.target).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_deposit_fills_to_target() {
        let mut f = DepositField::new([0.0, 0.0, 8.0, 8.0], 16, 16, 0.2);
        assert!(f.progress() < 0.01);
        // sweep a print head over the whole field
        for _ in 0..40 {
            for iy in 0..f.ny {
                for ix in 0..f.nx {
                    let (x, y) = f.cell_center(ix, iy);
                    f.deposit_at(x, y, 0.6, 0.05, 1.0 / 30.0);
                }
            }
        }
        assert!(f.progress() > 0.95, "progress={}", f.progress());
        // never overshoots the finish level
        assert!(f.h.iter().all(|&h| h <= 0.2 + 1e-6));
    }

    #[test]
    fn localized_deposit_is_partial() {
        let mut f = DepositField::new([0.0, 0.0, 10.0, 10.0], 20, 20, 0.2);
        for _ in 0..100 {
            f.deposit_at(2.0, 2.0, 1.0, 0.05, 1.0 / 30.0);
        }
        let p = f.progress();
        assert!(p > 0.0 && p < 0.3, "localized progress={p}");
    }
}
