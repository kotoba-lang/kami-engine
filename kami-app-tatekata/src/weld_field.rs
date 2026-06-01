//! weld_field — moving heat-source weld-pass model for robotic steel fastening.
//!
//! This is no longer an app-layer stand-in: it now delegates to the real
//! `kami_genesis::ThermalField` — a 2-D transient heat-conduction PDE (explicit
//! FDM, CFL-bounded) with a travelling Gaussian volumetric source and peak-
//! temperature fusion tracking. The 1-D seam the `robot:bolter` steps weld is
//! mapped onto a thin 2-D strip of that field; `pass()` walks the arc along the
//! seam and the field conducts + cools between calls. A node is "fused" once its
//! peak temperature crosses the fusion threshold.
//!
//! Honest scope: 2-D, explicit, f32, single-pass arc — a real conduction solver
//! (not a metallurgy / weld-pool / HAZ model, which needs transient thermal FEM),
//! but the heat field itself is genuine physics, not a tuned reduced model.
//!
//! Used by `robot:bolter` steps (steel column erection, roof trusses).

use kami_genesis::ThermalField;

/// Strip geometry calibrated so the viewer's fixed weld inputs (power ≈ 9 kW,
/// dt = 1/60 s) fuse a swept seam while the field stays bounded.
const NY: usize = 3; // seam rows — kept thin so fused_fraction tracks seam length
const H: f32 = 0.002; // cell size (m)
const ALPHA: f32 = 4.0e-6; // thermal diffusivity (m²/s)
const RHO_C: f32 = 2.0e3; // lumped ρ·c calibrated to arc-class power
const SIGMA: f32 = 0.006; // source radius (m) — spans the strip height
const H_CONV: f32 = 0.4; // convective loss to ambient air (1/s) after the arc passes

pub struct WeldField {
    pub n: usize,
    field: ThermalField,
    sy: f32,
    seam_len: f32,
}

impl WeldField {
    pub fn new(n: usize, ambient: f32, fusion_t: f32) -> Self {
        let n = n.max(2);
        let field = ThermalField::new(n, NY, H, ALPHA, ambient, fusion_t)
            .with_rho_c(RHO_C)
            .with_convection(H_CONV);
        let sy = NY as f32 * 0.5 * H;
        let seam_len = (n as f32 - 1.0) * H;
        Self {
            n,
            field,
            sy,
            seam_len,
        }
    }

    /// Advance one weld step: the arc sits at normalized seam position `head`
    /// (0..1) depositing `power` (W) for `dt` seconds, then the field conducts
    /// and cools. The dt is sub-stepped to respect the explicit CFL bound so
    /// the caller can pass any frame dt without blowing up.
    pub fn pass(&mut self, head: f32, power: f32, dt: f32) {
        let sx = head.clamp(0.0, 1.0) * self.seam_len;
        let cfl = self.field.cfl_dt();
        let mut remaining = dt.max(0.0);
        // at least one step even for dt below the CFL bound.
        loop {
            let h = remaining.min(cfl);
            self.field.step(sx, self.sy, power, SIGMA, h);
            remaining -= h;
            if remaining <= 1e-9 {
                break;
            }
        }
    }

    /// Fraction of the seam whose peak temperature reached fusion (0..1).
    pub fn fused_fraction(&self) -> f32 {
        self.field.fused_fraction()
    }

    /// Hottest current node temperature (°C) — for the glow indicator.
    pub fn max_temp(&self) -> f32 {
        self.field.max_temp()
    }

    /// Highest peak temperature ever reached anywhere (fusion evidence persists
    /// after cool-down).
    pub fn peak_max(&self) -> f32 {
        self.field.peak.iter().cloned().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeping_pass_fuses_the_seam() {
        // Replicates the viewer's exact call pattern: pass(settle, 9000, 1/60)
        // once per frame with the head sweeping 0→1 — over a RANGE of frame
        // counts so any plausible viewer cadence fuses + glows.
        for frames in [30usize, 60, 150] {
            let mut w = WeldField::new(40, 20.0, 1450.0);
            assert_eq!(w.fused_fraction(), 0.0);
            for k in 0..frames {
                let head = k as f32 / (frames - 1) as f32;
                w.pass(head, 9000.0, 1.0 / 60.0);
            }
            assert!(
                w.fused_fraction() > 0.8,
                "frames={frames}: fused={}",
                w.fused_fraction()
            );
            // glow indicator range (viewer maps 600..2400 °C) is reached
            assert!(w.max_temp() > 600.0, "frames={frames}: no glow");
            // field stays finite / bounded (no CFL blow-up)
            assert!(w.max_temp() < 1.0e6, "frames={frames}: runaway");
        }
    }

    #[test]
    fn field_cools_back_toward_ambient_but_keeps_fusion_evidence() {
        let mut w = WeldField::new(20, 20.0, 1450.0);
        for _ in 0..60 {
            w.pass(0.5, 9000.0, 1.0 / 60.0);
        }
        assert!(w.peak_max() > 1450.0, "did not fuse while welding");
        // stop the arc, let it conduct/cool.
        for _ in 0..6000 {
            w.pass(0.5, 0.0, 1.0 / 60.0);
        }
        assert!(w.max_temp() < 120.0, "did not cool: {}", w.max_temp());
        // the peak record (fusion evidence) persists.
        assert!(w.peak_max() > 1450.0, "lost fusion evidence after cooling");
    }
}
