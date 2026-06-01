//! mpm — Material Point Method continuum solver (2-D MLS-MPM).
//!
//! A real continuum solver (the family Isaac/PhysX use for deformables/granular)
//! to replace the `kami-app-*` field stand-ins (`DepositField`, `MoldField`) with
//! actual elasto-plastic / granular / fluid material: concrete pour + soil/砂 =
//! granular plasticity, fresh slurry = weakly-compressible fluid, sealant = soft
//! elastic. Moving Least Squares MPM (Hu et al. 2018), APIC transfer, quadratic
//! B-spline grid, fixed-corotated elasticity + return-mapping plasticity.
//!
//! Honest scope: 2-D, explicit, single-grid, CPU/WASM, f32 — an order of
//! magnitude simpler than PhysX GPU FEM/MPM, but a *genuine* continuum solver
//! (mass conserved, momentum from a real constitutive model), not a stand-in.

use glam::{Mat2, Vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum MpmMaterial {
    /// Soft elastic (sealant / jelly) — no plasticity.
    Elastic,
    /// Granular plasticity (concrete / soil / 砂) — settles into a pile.
    Granular,
    /// Weakly-compressible fluid (slurry) — flows and spreads.
    Fluid,
}

/// A static rigid obstacle the continuum collides with (one-way coupling): grid
/// velocities inside it lose their inward normal component, so material flows
/// *around* it (a column on a formwork core, slurry past a pile cap).
#[derive(Clone, Copy)]
pub enum MpmObstacle {
    Sphere { center: Vec2, radius: f32 },
    Box { min: Vec2, max: Vec2 },
}

impl MpmObstacle {
    /// Move the obstacle (kinematic / swept obstacles advance each step).
    fn translate(&mut self, d: Vec2) {
        match self {
            MpmObstacle::Sphere { center, .. } => *center += d,
            MpmObstacle::Box { min, max } => {
                *min += d;
                *max += d;
            }
        }
    }

    /// Boundary condition: if `pos` is inside the obstacle and the grid velocity
    /// moves *into* it relative to the obstacle's own velocity `v_ob`, cancel the
    /// inward relative normal component — so a moving obstacle (screed / piston /
    /// vibrator) drags the material along instead of letting it penetrate.
    fn project(&self, pos: Vec2, v: Vec2, v_ob: Vec2) -> Vec2 {
        let vrel = v - v_ob;
        match self {
            MpmObstacle::Sphere { center, radius } => {
                let d = pos - *center;
                let dist = d.length();
                if dist < *radius {
                    let n = if dist > 1e-6 { d / dist } else { Vec2::Y };
                    let vn = vrel.dot(n);
                    if vn < 0.0 {
                        return v_ob + (vrel - n * vn);
                    }
                }
                v
            }
            MpmObstacle::Box { min, max } => {
                if pos.x > min.x && pos.x < max.x && pos.y > min.y && pos.y < max.y {
                    let dl = pos - *min;
                    let dh = *max - pos;
                    let mut best = f32::INFINITY;
                    let mut n = Vec2::Y;
                    if dl.x < best {
                        best = dl.x;
                        n = Vec2::new(-1.0, 0.0);
                    }
                    if dh.x < best {
                        best = dh.x;
                        n = Vec2::new(1.0, 0.0);
                    }
                    if dl.y < best {
                        best = dl.y;
                        n = Vec2::new(0.0, -1.0);
                    }
                    if dh.y < best {
                        n = Vec2::new(0.0, 1.0);
                    }
                    let vn = vrel.dot(n);
                    if vn < 0.0 {
                        return v_ob + (vrel - n * vn);
                    }
                }
                v
            }
        }
    }
}

#[derive(Clone)]
struct Particle {
    x: Vec2,
    v: Vec2,
    c: Mat2, // APIC affine velocity
    f: Mat2, // deformation gradient
    jp: f32, // plastic volume ratio
    mat: MpmMaterial,
}

pub struct MpmSolver {
    pub n: usize, // grid nodes per axis (domain = unit square)
    dx: f32,
    inv_dx: f32,
    pub dt: f32,
    p_mass: f32,
    p_vol: f32,
    e: f32,  // Young's modulus
    nu: f32, // Poisson ratio
    gravity: f32,
    particles: Vec<Particle>,
    grid_v: Vec<Vec2>,
    grid_m: Vec<f32>,
    obstacles: Vec<(MpmObstacle, Vec2)>, // (shape, velocity)
}

impl MpmSolver {
    pub fn new(n: usize) -> Self {
        let n = n.max(16);
        Self {
            n,
            dx: 1.0 / n as f32,
            inv_dx: n as f32,
            dt: 1e-4,
            p_mass: 1.0,
            p_vol: 1.0,
            e: 1.0e4,
            nu: 0.2,
            gravity: -200.0,
            particles: Vec::new(),
            grid_v: vec![Vec2::ZERO; (n + 1) * (n + 1)],
            grid_m: vec![0.0; (n + 1) * (n + 1)],
            obstacles: Vec::new(),
        }
    }

    pub fn with_youngs(mut self, e: f32) -> Self {
        self.e = e;
        self
    }

    pub fn with_gravity(mut self, g: f32) -> Self {
        self.gravity = g;
        self
    }

    /// Add a uniform velocity to every particle (an impulse / initial kick).
    pub fn kick(&mut self, v: Vec2) {
        for p in &mut self.particles {
            p.v += v;
        }
    }

    /// Total linear momentum Σ mᵢ·vᵢ (conserved by the MLS-MPM transfer when no
    /// external force or boundary acts).
    pub fn linear_momentum(&self) -> Vec2 {
        self.particles.iter().map(|p| p.v).sum::<Vec2>() * self.p_mass
    }

    /// Initialise every particle to a rigid rotation about `center` at rate
    /// `omega` (sets both the velocity and the APIC affine matrix C).
    pub fn swirl(&mut self, center: Vec2, omega: f32) {
        for p in &mut self.particles {
            let r = p.x - center;
            p.v = Vec2::new(-omega * r.y, omega * r.x);
            // C = ∂v/∂x = omega·[[0,-1],[1,0]] (column-major).
            p.c = Mat2::from_cols(Vec2::new(0.0, omega), Vec2::new(-omega, 0.0));
        }
    }

    /// Total APIC angular momentum about `about` = Σ mᵢ[ rᵢ×vᵢ + ¼dx²(C_yx−C_xy) ].
    /// The affine term is what makes APIC conserve angular momentum (unlike PIC).
    pub fn angular_momentum(&self, about: Vec2) -> f32 {
        let dp = 0.25 * self.dx * self.dx; // quadratic B-spline inertia
        self.particles
            .iter()
            .map(|p| {
                let r = p.x - about;
                let lin = r.x * p.v.y - r.y * p.v.x;
                let aff = dp * (p.c.col(0).y - p.c.col(1).x);
                self.p_mass * (lin + aff)
            })
            .sum()
    }

    /// Add a static rigid obstacle the continuum flows around (one-way coupling).
    pub fn add_obstacle(&mut self, ob: MpmObstacle) {
        self.obstacles.push((ob, Vec2::ZERO));
    }

    /// Add a kinematic (swept) obstacle moving at `vel` — a screed / piston /
    /// vibrator that drags the continuum along as it advances.
    pub fn add_moving_obstacle(&mut self, ob: MpmObstacle, vel: Vec2) {
        self.obstacles.push((ob, vel));
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Seed a filled rectangular block of particles (domain coords in 0..1).
    pub fn add_block(&mut self, min: Vec2, max: Vec2, per_axis: usize, mat: MpmMaterial) {
        for i in 0..per_axis {
            for j in 0..per_axis {
                let fx = (i as f32 + 0.5) / per_axis as f32;
                let fy = (j as f32 + 0.5) / per_axis as f32;
                self.particles.push(Particle {
                    x: Vec2::new(min.x + fx * (max.x - min.x), min.y + fy * (max.y - min.y)),
                    v: Vec2::ZERO,
                    c: Mat2::ZERO,
                    f: Mat2::IDENTITY,
                    jp: 1.0,
                    mat,
                });
            }
        }
    }

    pub fn total_mass(&self) -> f32 {
        self.particles.len() as f32 * self.p_mass
    }

    pub fn bounds(&self) -> (Vec2, Vec2) {
        let mut mn = Vec2::splat(f32::INFINITY);
        let mut mx = Vec2::splat(f32::NEG_INFINITY);
        for p in &self.particles {
            mn = mn.min(p.x);
            mx = mx.max(p.x);
        }
        (mn, mx)
    }

    pub fn mean_height(&self) -> f32 {
        if self.particles.is_empty() {
            return 0.0;
        }
        self.particles.iter().map(|p| p.x.y).sum::<f32>() / self.particles.len() as f32
    }

    pub fn all_finite(&self) -> bool {
        self.particles
            .iter()
            .all(|p| p.x.is_finite() && p.v.is_finite())
    }

    /// Particle positions (for inspection / rendering).
    pub fn positions(&self) -> Vec<Vec2> {
        self.particles.iter().map(|p| p.x).collect()
    }

    /// Top surface height of the deposited material per x-bin over the unit
    /// x-domain (`0.0` where no particle reaches that column). This is the
    /// fill-level / slump readout a vertical-slice pour controller targets —
    /// e.g. "has the slab reached its finish thickness across the footprint?".
    pub fn surface_profile(&self, bins: usize) -> Vec<f32> {
        let bins = bins.max(1);
        let mut prof = vec![0.0_f32; bins];
        for p in &self.particles {
            let b = ((p.x.x.clamp(0.0, 1.0) * bins as f32) as usize).min(bins - 1);
            if p.x.y > prof[b] {
                prof[b] = p.x.y;
            }
        }
        prof
    }

    /// Overall peak fill height (max particle y); `0.0` if empty.
    pub fn fill_height(&self) -> f32 {
        self.particles.iter().map(|p| p.x.y).fold(0.0, f32::max)
    }

    /// One MLS-MPM step (P2G → grid update → G2P + constitutive update).
    pub fn step(&mut self) {
        // advance kinematic obstacles by their velocity.
        let dt_ob = self.dt;
        for (ob, vel) in self.obstacles.iter_mut() {
            if *vel != Vec2::ZERO {
                ob.translate(*vel * dt_ob);
            }
        }
        for m in self.grid_m.iter_mut() {
            *m = 0.0;
        }
        for v in self.grid_v.iter_mut() {
            *v = Vec2::ZERO;
        }
        let mu0 = self.e / (2.0 * (1.0 + self.nu));
        let la0 = self.e * self.nu / ((1.0 + self.nu) * (1.0 - 2.0 * self.nu));
        let dt = self.dt;

        // ── P2G ──
        for p in &self.particles {
            let base = (p.x * self.inv_dx - Vec2::splat(0.5)).floor();
            let fx = p.x * self.inv_dx - base;
            let w = quad_weights(fx);

            // constitutive model → first Piola term PF = P·Fᵀ
            let (svd_u, sig, svd_v) = svd2(p.f);
            let mut mu = mu0;
            let mut la = la0;
            let j = sig.x * sig.y;
            let pf = match p.mat {
                MpmMaterial::Fluid => {
                    // volumetric only (mu = 0): PF = λ·J·(J−1)·I
                    Mat2::IDENTITY * (la * j * (j - 1.0))
                }
                MpmMaterial::Elastic => {
                    mu *= 0.3;
                    la *= 0.3;
                    let r = svd_u * svd_v.transpose();
                    (p.f - r) * p.f.transpose() * (2.0 * mu) + Mat2::IDENTITY * (la * j * (j - 1.0))
                }
                MpmMaterial::Granular => {
                    let h = (10.0 * (1.0 - p.jp)).exp().clamp(0.05, 8.0);
                    mu *= h;
                    la *= h;
                    let r = svd_u * svd_v.transpose();
                    (p.f - r) * p.f.transpose() * (2.0 * mu) + Mat2::IDENTITY * (la * j * (j - 1.0))
                }
            };
            let stress = pf * (-dt * self.p_vol * 4.0 * self.inv_dx * self.inv_dx);
            let affine = stress + p.c * self.p_mass;

            for gi in 0..3 {
                for gj in 0..3 {
                    let dpos = (Vec2::new(gi as f32, gj as f32) - fx) * self.dx;
                    let weight = w[gi].x * w[gj].y;
                    let ni = (base.x as i32 + gi as i32).clamp(0, self.n as i32) as usize;
                    let nj = (base.y as i32 + gj as i32).clamp(0, self.n as i32) as usize;
                    let g = ni * (self.n + 1) + nj;
                    self.grid_v[g] += (p.v * self.p_mass + affine * dpos) * weight;
                    self.grid_m[g] += weight * self.p_mass;
                }
            }
        }

        // ── grid update ──
        let bound = 3;
        for i in 0..=self.n {
            for j in 0..=self.n {
                let g = i * (self.n + 1) + j;
                if self.grid_m[g] > 0.0 {
                    self.grid_v[g] /= self.grid_m[g];
                    self.grid_v[g].y += dt * self.gravity;
                    // sticky/slip walls
                    if i < bound && self.grid_v[g].x < 0.0 {
                        self.grid_v[g].x = 0.0;
                    }
                    if i > self.n - bound && self.grid_v[g].x > 0.0 {
                        self.grid_v[g].x = 0.0;
                    }
                    if j < bound && self.grid_v[g].y < 0.0 {
                        self.grid_v[g].y = 0.0;
                    }
                    if j > self.n - bound && self.grid_v[g].y > 0.0 {
                        self.grid_v[g].y = 0.0;
                    }
                    // rigid obstacles (one-way coupling; moving obstacles drag)
                    if !self.obstacles.is_empty() {
                        let pos = Vec2::new(i as f32, j as f32) * self.dx;
                        for (ob, vel) in &self.obstacles {
                            self.grid_v[g] = ob.project(pos, self.grid_v[g], *vel);
                        }
                    }
                }
            }
        }

        // ── G2P + advect + plasticity ──
        for p in &mut self.particles {
            let base = (p.x * self.inv_dx - Vec2::splat(0.5)).floor();
            let fx = p.x * self.inv_dx - base;
            let w = quad_weights(fx);
            let mut new_v = Vec2::ZERO;
            let mut new_c = Mat2::ZERO;
            for gi in 0..3 {
                for gj in 0..3 {
                    let dpos = Vec2::new(gi as f32, gj as f32) - fx;
                    let ni = (base.x as i32 + gi as i32).clamp(0, self.n as i32) as usize;
                    let nj = (base.y as i32 + gj as i32).clamp(0, self.n as i32) as usize;
                    let gv = self.grid_v[ni * (self.n + 1) + nj];
                    let weight = w[gi].x * w[gj].y;
                    new_v += gv * weight;
                    // APIC: C += 4·inv_dx·weight·(gv ⊗ dpos)
                    new_c += outer(gv * (weight * 4.0 * self.inv_dx), dpos);
                }
            }
            p.v = new_v;
            p.c = new_c;
            p.x += new_v * dt;
            p.x = p.x.clamp(Vec2::splat(0.0), Vec2::splat(1.0));

            // update deformation gradient
            let mut f = (Mat2::IDENTITY + new_c * dt) * p.f;
            // plasticity / volume tracking
            match p.mat {
                MpmMaterial::Fluid => {
                    // reset shear; keep volume J so it stays fluid
                    let jnew = (p.jp * (1.0 + dt * trace(new_c))).clamp(0.4, 1.6);
                    p.jp = jnew;
                    f = Mat2::IDENTITY * jnew.sqrt();
                }
                MpmMaterial::Granular => {
                    // Drucker–Prager sand plasticity (Klár et al. 2016): return-map
                    // the Hencky strain to the cohesionless yield cone, so the
                    // material shears like granular soil and settles at its angle
                    // of repose (a real pile, not a fluid puddle).
                    let (u, sig, v) = svd2(f);
                    let (sig_new, dq) = drucker_prager(sig, mu0, la0);
                    p.jp = (p.jp + dq).clamp(0.0, 40.0);
                    f = u
                        * Mat2::from_cols(Vec2::new(sig_new.x, 0.0), Vec2::new(0.0, sig_new.y))
                        * v.transpose();
                }
                MpmMaterial::Elastic => {}
            }
            p.f = f;
        }
    }
}

#[inline]
fn quad_weights(fx: Vec2) -> [Vec2; 3] {
    // quadratic B-spline weights per axis for the 3-cell stencil.
    let a = Vec2::splat(1.5) - fx;
    let b = fx - Vec2::splat(1.0);
    let c = fx - Vec2::splat(0.5);
    [a * a * 0.5, Vec2::splat(0.75) - b * b, c * c * 0.5]
}

#[inline]
fn outer(a: Vec2, b: Vec2) -> Mat2 {
    Mat2::from_cols(a * b.x, a * b.y)
}

#[inline]
fn trace(m: Mat2) -> f32 {
    m.col(0).x + m.col(1).y
}

/// 2×2 SVD via the symmetric eigendecomposition of MᵀM: returns (U, singular
/// values σ₁≥σ₂≥0, V) with M = U·diag(σ)·Vᵀ.
fn svd2(m: Mat2) -> (Mat2, Vec2, Mat2) {
    // S = MᵀM (symmetric): [[s11, s12],[s12, s22]]
    let c0 = m.col(0);
    let c1 = m.col(1);
    let s11 = c0.dot(c0);
    let s12 = c0.dot(c1);
    let s22 = c1.dot(c1);
    let tr = s11 + s22;
    let disc = ((s11 - s22) * (s11 - s22) + 4.0 * s12 * s12)
        .max(0.0)
        .sqrt();
    let l1 = 0.5 * (tr + disc); // larger eigenvalue
    let l2 = (0.5 * (tr - disc)).max(0.0);
    let s1 = l1.sqrt();
    let s2 = l2.sqrt();
    // eigenvector of S for l1 → first column of V
    let v1 = if s12.abs() > 1e-9 {
        Vec2::new(s12, l1 - s11).normalize_or(Vec2::X)
    } else if s11 >= s22 {
        Vec2::X
    } else {
        Vec2::Y
    };
    let v2 = Vec2::new(-v1.y, v1.x); // orthonormal
    let v = Mat2::from_cols(v1, v2);
    // U columns = M·v_i / σ_i
    let u1 = if s1 > 1e-9 { (m * v1) / s1 } else { Vec2::X };
    let u2 = if s2 > 1e-9 {
        (m * v2) / s2
    } else {
        Vec2::new(-u1.y, u1.x)
    };
    let u = Mat2::from_cols(u1, u2);
    (u, Vec2::new(s1, s2), v)
}

/// Friction-angle coefficient of the Drucker–Prager cone (≈ 35° sand).
const DP_ALPHA: f32 = 0.386;

/// Drucker–Prager return mapping (Klár et al. 2016) on the 2-D principal
/// stretches `sig`, with shear modulus `mu` and Lamé `la`. Returns the projected
/// stretches and the plastic-flow magnitude (for hardening). This is what makes
/// the granular material settle at an angle of repose rather than flowing flat.
fn drucker_prager(sig: Vec2, mu: f32, la: f32) -> (Vec2, f32) {
    let eps = Vec2::new(sig.x.max(1e-4).ln(), sig.y.max(1e-4).ln());
    let tr = eps.x + eps.y;
    let eps_hat = eps - Vec2::splat(tr * 0.5);
    let eps_hat_norm = eps_hat.length();
    if tr > 0.0 {
        // pure expansion: no tensile strength → collapse to the cone tip.
        return (Vec2::ONE, eps.length());
    }
    if eps_hat_norm < 1e-9 {
        return (sig, 0.0); // hydrostatic compression → elastic
    }
    // δγ = ‖ε̂‖ + α·(d·la + 2μ)/(2μ)·tr(ε), d = 2
    let dg = eps_hat_norm + DP_ALPHA * (2.0 * la + 2.0 * mu) / (2.0 * mu) * tr;
    if dg <= 0.0 {
        (sig, 0.0) // inside the yield cone → elastic
    } else {
        let eps_new = eps - eps_hat * (dg / eps_hat_norm);
        (Vec2::new(eps_new.x.exp(), eps_new.y.exp()), dg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svd_reconstructs() {
        let m = Mat2::from_cols(Vec2::new(1.2, 0.3), Vec2::new(-0.4, 0.9));
        let (u, sig, v) = svd2(m);
        let recon =
            u * Mat2::from_cols(Vec2::new(sig.x, 0.0), Vec2::new(0.0, sig.y)) * v.transpose();
        for k in 0..2 {
            assert!(
                (recon.col(k) - m.col(k)).length() < 1e-4,
                "col {k} mismatch"
            );
        }
        assert!(sig.x >= sig.y && sig.y >= 0.0);
    }

    #[test]
    fn linear_momentum_is_conserved_by_the_transfer() {
        // a free blob (no gravity, no boundary contact) given an initial kick
        // must keep its linear momentum — the core MLS-MPM transfer guarantee.
        let mut s = MpmSolver::new(64).with_gravity(0.0);
        s.add_block(
            Vec2::new(0.4, 0.4),
            Vec2::new(0.6, 0.6),
            16,
            MpmMaterial::Fluid,
        );
        s.kick(Vec2::new(2.0, 0.5));
        let p0 = s.linear_momentum();
        assert!(p0.length() > 0.0);
        for _ in 0..60 {
            s.step();
        }
        let p1 = s.linear_momentum();
        assert!(
            (p1 - p0).length() < 0.02 * p0.length(),
            "momentum not conserved: {p0:?} → {p1:?}"
        );
        assert!(s.all_finite());
    }

    #[test]
    fn apic_conserves_angular_momentum() {
        // a freely spinning blob (no gravity / boundary) keeps its angular
        // momentum — APIC's signature property (PIC would dissipate it).
        let mut s = MpmSolver::new(64).with_gravity(0.0);
        let c = Vec2::new(0.5, 0.5);
        s.add_block(
            Vec2::new(0.4, 0.4),
            Vec2::new(0.6, 0.6),
            16,
            MpmMaterial::Fluid,
        );
        s.swirl(c, 3.0);
        let l0 = s.angular_momentum(c);
        assert!(l0.abs() > 1e-6, "no initial spin");
        for _ in 0..80 {
            s.step();
        }
        let l1 = s.angular_momentum(c);
        assert!(
            (l1 - l0).abs() < 0.05 * l0.abs(),
            "angular momentum not conserved: {l0} → {l1}"
        );
        assert!(s.all_finite());
    }

    #[test]
    fn surface_profile_reports_the_block_top_then_tracks_the_pour() {
        let bins = 20;
        let occupied = |prof: &[f32]| prof.iter().filter(|&&h| h > 1e-4).count();

        // Deterministic semantics: a freshly-seeded block (no physics yet) must
        // report its top surface only in the columns it actually spans, at the
        // block's top y. Block x∈[0.40,0.60], top row ≈ 0.713 → bins 8..=11.
        let mut s = MpmSolver::new(64);
        s.add_block(
            Vec2::new(0.40, 0.50),
            Vec2::new(0.60, 0.72),
            16,
            MpmMaterial::Fluid,
        );
        let prof0 = s.surface_profile(bins);
        for (b, &h) in prof0.iter().enumerate() {
            if (8..=11).contains(&b) {
                assert!((h - 0.713).abs() < 0.02, "bin {b} top = {h}");
            } else {
                assert_eq!(h, 0.0, "bin {b} should be empty");
            }
        }
        let occ0 = occupied(&prof0);
        let h0 = s.fill_height();

        // Integration: the pour drops onto the floor, spreads across more columns
        // and its peak surface settles lower — the fill-level readout a screed /
        // print-head controller watches to know the slab thickness.
        for _ in 0..4000 {
            s.step();
        }
        assert!(s.all_finite());
        assert!(
            occupied(&s.surface_profile(bins)) > occ0,
            "pour did not spread laterally"
        );
        let hf = s.fill_height();
        assert!(
            hf < h0 && hf > 0.0,
            "pour did not settle to a finite lower surface"
        );
    }

    #[test]
    fn mass_is_conserved_and_finite() {
        let mut s = MpmSolver::new(48);
        s.add_block(
            Vec2::new(0.35, 0.55),
            Vec2::new(0.65, 0.85),
            24,
            MpmMaterial::Granular,
        );
        let m0 = s.total_mass();
        let n0 = s.particle_count();
        for _ in 0..600 {
            s.step();
        }
        assert_eq!(s.particle_count(), n0, "particles lost");
        assert!((s.total_mass() - m0).abs() < 1e-3);
        assert!(s.all_finite());
        // all particles remain inside the domain
        let (mn, mx) = s.bounds();
        assert!(mn.x >= -1e-3 && mn.y >= -1e-3 && mx.x <= 1.001 && mx.y <= 1.001);
    }

    #[test]
    fn elastic_block_falls_under_gravity() {
        let mut s = MpmSolver::new(48).with_youngs(5.0e4);
        s.add_block(
            Vec2::new(0.4, 0.6),
            Vec2::new(0.6, 0.8),
            20,
            MpmMaterial::Elastic,
        );
        let h0 = s.mean_height();
        assert!(h0 > 0.6, "seeded height {h0}");
        for _ in 0..500 {
            s.step();
        }
        let h1 = s.mean_height();
        // fell under gravity, stayed finite + inside the domain (rests on the floor).
        assert!(h1 < h0, "did not fall: {h0} → {h1}");
        assert!(s.all_finite());
        let (mn, mx) = s.bounds();
        assert!(mn.y >= -1e-3 && mx.y <= 1.001 && mn.x >= -1e-3 && mx.x <= 1.001);
    }

    #[test]
    fn granular_settles_into_a_pile_not_a_puddle() {
        // a Drucker–Prager granular column holds a pile (angle of repose): it
        // spreads LESS and stays TALLER than a fluid column of the same shape.
        let cmin = Vec2::new(0.45, 0.2);
        let cmax = Vec2::new(0.55, 0.7);
        let run = |mat| {
            let mut s = MpmSolver::new(48);
            s.add_block(cmin, cmax, 18, mat);
            for _ in 0..800 {
                s.step();
            }
            let (mn, mx) = s.bounds();
            (mx.x - mn.x, mx.y - mn.y, s.all_finite())
        };
        let (gw, gh, gf) = run(MpmMaterial::Granular);
        let (fw, _fh, ff) = run(MpmMaterial::Fluid);
        assert!(gf && ff, "non-finite");
        assert!(gw < fw, "granular spread {gw} not < fluid spread {fw}");
        assert!(gh > 0.05, "granular pile collapsed flat: h={gh}");
    }

    #[test]
    fn moving_obstacle_drags_the_continuum() {
        // a box sweeping rightward through a granular slab (a screed) pushes the
        // material's mean-x further right than a static box at the same start.
        let run = |moving: bool| -> f32 {
            let mut s = MpmSolver::new(48);
            s.add_block(
                Vec2::new(0.3, 0.1),
                Vec2::new(0.65, 0.28),
                18,
                MpmMaterial::Granular,
            );
            let bmin = Vec2::new(0.22, 0.05);
            let bmax = Vec2::new(0.32, 0.32);
            if moving {
                s.add_moving_obstacle(
                    MpmObstacle::Box {
                        min: bmin,
                        max: bmax,
                    },
                    Vec2::new(0.6, 0.0),
                );
            } else {
                s.add_obstacle(MpmObstacle::Box {
                    min: bmin,
                    max: bmax,
                });
            }
            for _ in 0..2500 {
                s.step();
            }
            s.positions().iter().map(|p| p.x).sum::<f32>() / s.particle_count() as f32
        };
        let swept = run(true);
        let still = run(false);
        assert!(
            swept > still + 0.05,
            "sweep did not drag material: moving={swept} static={still}"
        );
    }

    #[test]
    fn obstacle_deflects_the_continuum() {
        // a box obstacle in the lower-middle; granular poured from above must
        // flow AROUND it — no particle ends up deep inside the obstacle.
        let mut s = MpmSolver::new(48);
        let ob_min = Vec2::new(0.4, 0.25);
        let ob_max = Vec2::new(0.6, 0.42);
        s.add_obstacle(MpmObstacle::Box {
            min: ob_min,
            max: ob_max,
        });
        s.add_block(
            Vec2::new(0.42, 0.6),
            Vec2::new(0.58, 0.85),
            16,
            MpmMaterial::Granular,
        );
        for _ in 0..1200 {
            s.step();
        }
        assert!(s.all_finite());
        // allow a one-cell boundary layer; the interior must stay empty.
        let m = 0.04;
        let inside = s
            .positions()
            .iter()
            .filter(|p| {
                p.x > ob_min.x + m && p.x < ob_max.x - m && p.y > ob_min.y + m && p.y < ob_max.y - m
            })
            .count();
        assert_eq!(inside, 0, "{inside} particles penetrated the obstacle");
    }

    #[test]
    fn fluid_column_spreads() {
        let mut s = MpmSolver::new(48);
        s.add_block(
            Vec2::new(0.45, 0.2),
            Vec2::new(0.55, 0.7),
            18,
            MpmMaterial::Fluid,
        );
        let (mn0, mx0) = s.bounds();
        let w0 = mx0.x - mn0.x;
        for _ in 0..800 {
            s.step();
        }
        let (mn1, mx1) = s.bounds();
        let w1 = mx1.x - mn1.x;
        assert!(w1 > w0 * 1.3, "fluid did not spread: {w0} → {w1}");
        assert!(s.all_finite());
    }
}
