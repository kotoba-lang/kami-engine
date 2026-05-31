//! weld_field — moving heat-source weld-pass model for robotic steel fastening.
//!
//! Application-layer thermal stand-in (same honest posture as `deposit_field` /
//! kabitori `MoldField`): a 1-D seam of temperature nodes with a travelling heat
//! source (a reduced Rosenthal-style moving point source) + conduction + cooling.
//! A node is "fused" once its peak temperature exceeds the fusion threshold. It
//! captures *fusion progress along the seam + a bounded thermal field*, NOT a
//! real weld-pool / HAZ / metallurgy model — that needs a transient thermal FEM.
//!
//! Used by `robot:bolter` steps (steel column erection, roof trusses).

pub struct WeldField {
    pub n: usize,
    ambient: f32,
    fusion_t: f32,
    /// current temperature per seam node (°C).
    pub temp: Vec<f32>,
    /// peak temperature ever reached per node (°C).
    pub peak: Vec<f32>,
}

impl WeldField {
    pub fn new(n: usize, ambient: f32, fusion_t: f32) -> Self {
        let n = n.max(2);
        Self {
            n,
            ambient,
            fusion_t,
            temp: vec![ambient; n],
            peak: vec![ambient; n],
        }
    }

    /// Advance one step: a heat source at normalized seam position `head`
    /// (0..1) deposits `power` (°C/s equivalent), then conduction + Newton
    /// cooling over `dt` seconds.
    pub fn pass(&mut self, head: f32, power: f32, dt: f32) {
        let hp = head.clamp(0.0, 1.0) * (self.n - 1) as f32;
        // deposit a gaussian heat blob around the head node
        let sigma = 1.2_f32;
        for i in 0..self.n {
            let d = i as f32 - hp;
            let q = power * (-(d * d) / (2.0 * sigma * sigma)).exp();
            self.temp[i] += q * dt;
        }
        // conduction (explicit diffusion) + cooling toward ambient
        let alpha = 0.18_f32;
        let cool = 0.9_f32;
        let prev = self.temp.clone();
        for i in 0..self.n {
            let l = if i > 0 { prev[i - 1] } else { prev[i] };
            let r = if i + 1 < self.n { prev[i + 1] } else { prev[i] };
            let lap = l - 2.0 * prev[i] + r;
            self.temp[i] = prev[i] + alpha * lap - cool * (prev[i] - self.ambient) * dt;
            if self.temp[i] < self.ambient {
                self.temp[i] = self.ambient;
            }
            if self.temp[i] > self.peak[i] {
                self.peak[i] = self.temp[i];
            }
        }
    }

    /// Fraction of the seam whose peak temperature reached fusion (0..1).
    pub fn fused_fraction(&self) -> f32 {
        let f = self.peak.iter().filter(|&&p| p >= self.fusion_t).count();
        f as f32 / self.n as f32
    }

    /// Hottest current node temperature (°C) — for the glow indicator.
    pub fn max_temp(&self) -> f32 {
        self.temp.iter().cloned().fold(self.ambient, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeping_pass_fuses_the_seam() {
        let mut w = WeldField::new(40, 20.0, 1450.0);
        assert_eq!(w.fused_fraction(), 0.0);
        // travel the torch from one end to the other
        for k in 0..400 {
            let head = k as f32 / 399.0;
            w.pass(head, 9000.0, 1.0 / 60.0);
        }
        assert!(w.fused_fraction() > 0.8, "fused={}", w.fused_fraction());
        // field stays bounded (no runaway)
        assert!(w.max_temp() < 6000.0, "max_temp={}", w.max_temp());
    }

    #[test]
    fn field_cools_back_to_ambient() {
        let mut w = WeldField::new(20, 20.0, 1450.0);
        for _ in 0..50 {
            w.pass(0.5, 9000.0, 1.0 / 60.0);
        }
        // stop welding, let it cool
        for _ in 0..4000 {
            w.pass(0.5, 0.0, 1.0 / 60.0);
        }
        assert!(w.max_temp() < 60.0, "did not cool: {}", w.max_temp());
        // but the peak record (fusion evidence) persists
        assert!(w.peak.iter().cloned().fold(0.0, f32::max) > 1450.0);
    }
}
