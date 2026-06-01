//! thermal — transient heat-conduction PDE solver (2-D explicit FDM).
//!
//! Real continuum heat transfer `∂T/∂t = α ∇²T + Q/(ρc)` on a uniform grid, with
//! a travelling volumetric heat source (Gaussian / Goldak-style) for robotic arc
//! welding, and Dirichlet / Neumann boundary conditions. This **replaces** the
//! `kami-app-tatekata::WeldField` application-layer stand-in with an actual
//! discretized PDE: the fusion zone, heat-affected zone and cool-down are
//! emergent from conduction, not scripted.
//!
//! Honest scope: 2-D explicit (forward-Euler) FDM — first-order, CFL-bounded,
//! single material, no phase-change latent heat or thermo-mechanical coupling
//! (those need an implicit / FEM thermo-mechanical solver). It is, however, a
//! genuine PDE: verified against the 1-D steady-state analytic profile and an
//! energy-conservation invariant. Clean-room (no NVIDIA).

/// Boundary condition on one edge.
#[derive(Clone, Copy, PartialEq)]
pub enum Bc {
    /// Fixed temperature (°C).
    Dirichlet(f32),
    /// Insulated (zero normal gradient).
    Neumann,
}

pub struct ThermalField {
    pub nx: usize,
    pub ny: usize,
    pub h: f32,         // cell size (m)
    pub alpha: f32,     // thermal diffusivity (m²/s)
    pub ambient: f32,   // °C
    pub t_melt: f32,    // fusion threshold (°C)
    pub rho_c: f32,     // ρ·c volumetric heat capacity (J/m³K)
    pub h_conv: f32,    // Newton convection rate to ambient (1/s); 0 = insulated
    pub t: Vec<f32>,    // temperature
    pub peak: Vec<f32>, // peak temperature ever reached (fusion evidence)
    pub bc: [Bc; 4],    // [-x, +x, -y, +y]
}

impl ThermalField {
    pub fn new(nx: usize, ny: usize, h: f32, alpha: f32, ambient: f32, t_melt: f32) -> Self {
        let nx = nx.max(3);
        let ny = ny.max(3);
        Self {
            nx,
            ny,
            h,
            alpha,
            ambient,
            t_melt,
            rho_c: 4.0e6, // ~steel ≈ ρ7850·c500
            h_conv: 0.0,  // insulated by default (conduction only)
            t: vec![ambient; nx * ny],
            peak: vec![ambient; nx * ny],
            bc: [Bc::Neumann; 4],
        }
    }

    pub fn with_bc(mut self, bc: [Bc; 4]) -> Self {
        self.bc = bc;
        self
    }

    /// Override the lumped volumetric heat capacity ρ·c (J/m³K). Lower = a given
    /// power raises temperature faster (use to calibrate the weld source).
    pub fn with_rho_c(mut self, rho_c: f32) -> Self {
        self.rho_c = rho_c;
        self
    }

    /// Enable Newton convective heat loss to ambient at volumetric rate `k`
    /// (1/s): each cell sheds `k·(T − ambient)` per second, modelling a member
    /// cooling to surrounding air after the arc passes. Default 0 = insulated
    /// (pure conduction, heat conserved). Sub-step so `k·dt < 1` for stability.
    pub fn with_convection(mut self, k: f32) -> Self {
        self.h_conv = k.max(0.0);
        self
    }

    #[inline]
    fn idx(&self, i: usize, j: usize) -> usize {
        j * self.nx + i
    }

    pub fn temp(&self, i: usize, j: usize) -> f32 {
        self.t[self.idx(i, j)]
    }

    pub fn cell_center(&self, i: usize, j: usize) -> (f32, f32) {
        ((i as f32 + 0.5) * self.h, (j as f32 + 0.5) * self.h)
    }

    /// The largest stable explicit timestep (2-D CFL: αΔt/h² ≤ 1/4).
    pub fn cfl_dt(&self) -> f32 {
        0.2 * self.h * self.h / self.alpha.max(1e-9)
    }

    /// Advance by `dt` with a travelling volumetric heat source: total power
    /// `power` (W) deposited as a Gaussian of radius `sigma` (m) centred at the
    /// world position `(sx, sy)` (m).
    pub fn step(&mut self, sx: f32, sy: f32, power: f32, sigma: f32, dt: f32) {
        self.step_multi(&[(sx, sy, power, sigma)], dt);
    }

    /// Advance by `dt` with *several* simultaneous heat sources, each a tuple
    /// `(sx, sy, power, sigma)` whose Gaussian contributions superpose. Models
    /// multi-pass / multi-torch welding (and the bridging seam where two members
    /// are fastened from both ends at once). An empty slice = pure conduction.
    pub fn step_multi(&mut self, sources: &[(f32, f32, f32, f32)], dt: f32) {
        let inv_h2 = 1.0 / (self.h * self.h);
        let prev = self.t.clone();
        for j in 0..self.ny {
            for i in 0..self.nx {
                let c = prev[self.idx(i, j)];
                let l = if i > 0 {
                    prev[self.idx(i - 1, j)]
                } else {
                    bc_val(self.bc[0], c)
                };
                let r = if i + 1 < self.nx {
                    prev[self.idx(i + 1, j)]
                } else {
                    bc_val(self.bc[1], c)
                };
                let d = if j > 0 {
                    prev[self.idx(i, j - 1)]
                } else {
                    bc_val(self.bc[2], c)
                };
                let u = if j + 1 < self.ny {
                    prev[self.idx(i, j + 1)]
                } else {
                    bc_val(self.bc[3], c)
                };
                let lap = (l + r + d + u - 4.0 * c) * inv_h2;
                // superposed Gaussian source terms
                let (cx, cy) = self.cell_center(i, j);
                let mut q = 0.0;
                for &(sx, sy, power, sigma) in sources {
                    let two_sig2 = 2.0 * sigma * sigma;
                    // Gaussian normalisation so the integral ≈ power (per unit thickness).
                    let norm = power / (std::f32::consts::PI * two_sig2 * self.rho_c);
                    let dist2 = (cx - sx) * (cx - sx) + (cy - sy) * (cy - sy);
                    q += norm * (-dist2 / two_sig2).exp();
                }
                // conduction + source − Newton convection to ambient (explicit).
                let conv = self.h_conv * (c - self.ambient);
                let mut next = c + (self.alpha * lap + q - conv) * dt;
                if next < self.ambient {
                    next = self.ambient;
                }
                let kk = j * self.nx + i;
                self.t[kk] = next;
                if next > self.peak[kk] {
                    self.peak[kk] = next;
                }
            }
        }
        // re-apply Dirichlet edges exactly.
        self.apply_dirichlet();
    }

    fn apply_dirichlet(&mut self) {
        if let Bc::Dirichlet(v) = self.bc[0] {
            for j in 0..self.ny {
                let k = self.idx(0, j);
                self.t[k] = v;
            }
        }
        if let Bc::Dirichlet(v) = self.bc[1] {
            for j in 0..self.ny {
                let k = self.idx(self.nx - 1, j);
                self.t[k] = v;
            }
        }
        if let Bc::Dirichlet(v) = self.bc[2] {
            for i in 0..self.nx {
                let k = self.idx(i, 0);
                self.t[k] = v;
            }
        }
        if let Bc::Dirichlet(v) = self.bc[3] {
            for i in 0..self.nx {
                let k = self.idx(i, self.ny - 1);
                self.t[k] = v;
            }
        }
    }

    pub fn max_temp(&self) -> f32 {
        self.t.iter().cloned().fold(self.ambient, f32::max)
    }

    /// Fraction of cells whose peak temperature reached fusion.
    pub fn fused_fraction(&self) -> f32 {
        let n = self.peak.iter().filter(|&&p| p >= self.t_melt).count();
        n as f32 / self.peak.len() as f32
    }

    /// Total thermal energy above ambient (∝ Σ(T−ambient)), for conservation.
    pub fn total_heat(&self) -> f32 {
        self.t.iter().map(|&v| v - self.ambient).sum()
    }
}

#[inline]
fn bc_val(bc: Bc, interior: f32) -> f32 {
    match bc {
        Bc::Dirichlet(v) => v,
        Bc::Neumann => interior, // mirror = zero gradient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_state_matches_1d_analytic() {
        // hot left (Dirichlet 100), cold right (Dirichlet 0), insulated top/bottom
        // → steady solution is a linear profile T(x) = 100·(1 − x/L).
        let mut f = ThermalField::new(21, 5, 0.01, 1e-4, 0.0, 9999.0).with_bc([
            Bc::Dirichlet(100.0),
            Bc::Dirichlet(0.0),
            Bc::Neumann,
            Bc::Neumann,
        ]);
        // seed the Dirichlet edges
        f.apply_dirichlet();
        let dt = f.cfl_dt();
        for _ in 0..40000 {
            f.step(-100.0, -100.0, 0.0, 0.01, dt); // no source (far away)
        }
        let j = 2;
        for i in 0..f.nx {
            let expect = 100.0 * (1.0 - i as f32 / (f.nx - 1) as f32);
            assert!(
                (f.temp(i, j) - expect).abs() < 2.0,
                "i={i} T={} expect={expect}",
                f.temp(i, j)
            );
        }
    }

    #[test]
    fn insulated_no_source_conserves_then_relaxes() {
        // fully insulated, no source, a hot blob → total heat conserved, peak falls.
        let mut f = ThermalField::new(25, 25, 0.01, 1e-4, 20.0, 9999.0);
        for j in 10..15 {
            for i in 10..15 {
                let k = f.idx(i, j);
                f.t[k] = 520.0;
            }
        }
        let h0 = f.total_heat();
        let tmax0 = f.max_temp();
        let dt = f.cfl_dt();
        for _ in 0..5000 {
            f.step(-100.0, -100.0, 0.0, 0.01, dt);
        }
        let h1 = f.total_heat();
        assert!(
            (h1 - h0).abs() / h0 < 0.05,
            "heat not conserved: {h0} → {h1}"
        );
        assert!(f.max_temp() < tmax0 * 0.9, "blob did not diffuse");
        assert!(f.t.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn moving_weld_source_creates_tracking_fusion_zone() {
        // calibrated lumped capacity so an arc-class power reaches fusion.
        let mut f = ThermalField::new(60, 12, 0.002, 4e-6, 20.0, 1450.0).with_rho_c(2.0e3);
        let dt = f.cfl_dt();
        assert_eq!(f.fused_fraction(), 0.0);
        // travel the torch along the seam (y = mid) left → right.
        let yc = f.ny as f32 * 0.5 * f.h;
        let steps = 2500;
        for s in 0..steps {
            let sx = (s as f32 / steps as f32) * (f.nx as f32 * f.h);
            f.step(sx, yc, 150.0, 0.004, dt);
        }
        assert!(f.fused_fraction() > 0.1, "fused={}", f.fused_fraction());
        // bounded (no numerical blow-up under CFL)
        assert!(f.max_temp() < 50_000.0, "max_temp={}", f.max_temp());
        assert!(f.t.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn two_simultaneous_sources_each_fuse_their_own_zone() {
        // Two stationary torches well apart along the seam, fired at once via
        // step_multi. Both spots must reach fusion. The discriminator: with a
        // *single* source the second spot stays essentially ambient — so the
        // second fusion zone can only come from the superposed second source.
        let mk = || ThermalField::new(60, 12, 0.002, 4e-6, 20.0, 1450.0).with_rho_c(2.0e3);
        let dt = mk().cfl_dt();
        let f0 = mk();
        let y = f0.cell_center(0, 6).1;
        let sx1 = f0.cell_center(15, 6).0;
        let sx2 = f0.cell_center(45, 6).0;
        let k1 = f0.idx(15, 6);
        let k2 = f0.idx(45, 6);
        let steps = 150;

        let mut two = mk();
        for _ in 0..steps {
            two.step_multi(&[(sx1, y, 150.0, 0.004), (sx2, y, 150.0, 0.004)], dt);
        }
        assert!(two.peak[k1] >= 1450.0, "zone 1 not fused: {}", two.peak[k1]);
        assert!(two.peak[k2] >= 1450.0, "zone 2 not fused: {}", two.peak[k2]);

        let mut one = mk();
        for _ in 0..steps {
            one.step(sx1, y, 150.0, 0.004, dt);
        }
        assert!(one.peak[k1] >= 1450.0, "single-source zone 1 not fused");
        assert!(
            one.peak[k2] < 100.0,
            "zone 2 heated with no source there: {}",
            one.peak[k2]
        );
        assert!(two.t.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn cfl_dt_is_stable() {
        let mut f = ThermalField::new(30, 30, 0.01, 1e-4, 20.0, 9999.0);
        let dt = f.cfl_dt();
        for _ in 0..20000 {
            f.step(0.15, 0.15, 500.0, 0.02, dt);
        }
        assert!(f.t.iter().all(|v| v.is_finite()));
    }
}
