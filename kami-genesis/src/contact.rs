//! contact — rigid contact / collision solver coupled to the 3-D
//! reduced-coordinate articulation (`articulation3d`).
//!
//! Same approach class as PhysX's TGS contact solver (clean-room, no NVIDIA
//! code): velocity-level **sequential impulses / projected Gauss-Seidel** with a
//! Coulomb friction cone, run in the articulation's *joint space*. The
//! contact-space effective inverse mass is the Delassus operator
//! `A = Jₖ M⁻¹ Jₖᵀ`, where `M` is the CRBA joint-space inertia already computed
//! by the dynamics core and `Jₖ` is the contact-point linear Jacobian
//! (`Articulation3dConfig::point_jacobian`). Penetration is corrected with
//! Baumgarte stabilization; restitution is supported (default 0 = inelastic).
//!
//! Collision shapes are spheres / capsules attached to links, resolved against
//! a static ground plane (z = `ground_z`, normal +z). This is the
//! contact-against-environment case; broadphase is trivial all-pairs (link
//! counts are small). Self-collision broad/narrow phase is a documented
//! follow-up.

use crate::articulation3d::{solve_ldlt, Articulation3dConfig, Articulation3dState};
use glam::Vec3;

#[derive(Clone, Debug)]
pub enum Collider {
    /// Sphere centred at `center` (body frame).
    Sphere { center: Vec3, radius: f32 },
    /// Capsule between `a` and `b` (body frame), swept radius `radius`.
    Capsule { a: Vec3, b: Vec3, radius: f32 },
}

#[derive(Clone, Debug)]
pub struct ContactParams {
    pub ground_z: f32,
    pub restitution: f32, // 0 = inelastic
    pub friction: f32,    // Coulomb μ
    pub baumgarte: f32,   // position-error feedback gain (0.1–0.2)
    pub slop: f32,        // penetration allowance before push-out
    pub iters: usize,     // PGS sweeps
}

impl Default for ContactParams {
    fn default() -> Self {
        Self {
            ground_z: 0.0,
            restitution: 0.0,
            friction: 0.8,
            baumgarte: 0.15,
            slop: 1.0e-3,
            iters: 12,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContactWorld {
    /// `(body index, collider in that body's frame)`.
    pub colliders: Vec<(usize, Collider)>,
    pub params: ContactParams,
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    link: usize,
    p: Vec3,    // world contact point
    n: Vec3,    // world normal (ground → body), unit
    depth: f32, // penetration (> 0 if overlapping)
}

impl ContactWorld {
    pub fn new(colliders: Vec<(usize, Collider)>, params: ContactParams) -> Self {
        Self { colliders, params }
    }

    /// Step the articulation one `dt` with contact resolution:
    /// predict free velocity → generate contacts → PGS velocity solve →
    /// integrate positions.
    pub fn step(
        &self,
        cfg: &Articulation3dConfig,
        st: &mut Articulation3dState,
        tau_applied: &[f32],
    ) {
        let (qddot, m) = cfg.forward_dynamics(st, tau_applied);
        // Semi-implicit velocity prediction.
        let dt = cfg.dt;
        for d in 0..cfg.ndof {
            st.qdot[d] += dt * qddot[d];
        }
        let contacts = self.generate(cfg, &st.q);
        if !contacts.is_empty() {
            self.solve_velocity(cfg, &m, st, &contacts);
        }
        cfg.integrate_positions(st);
    }

    /// Number of live ground contacts at the current pose (test/inspection).
    pub fn contact_count(&self, cfg: &Articulation3dConfig, q: &[f32]) -> usize {
        self.generate(cfg, q).len()
    }

    fn generate(&self, cfg: &Articulation3dConfig, q: &[f32]) -> Vec<Contact> {
        let lw = cfg.link_world(q);
        let gz = self.params.ground_z;
        let mut out = Vec::new();
        for (body, col) in &self.colliders {
            let (r, p0) = lw[*body];
            let mut probe = |center_body: Vec3, radius: f32| {
                let c = p0 + r * center_body;
                let depth = (gz + radius) - c.z;
                if depth > -self.params.slop {
                    out.push(Contact {
                        link: *body,
                        p: Vec3::new(c.x, c.y, gz), // contact on the plane
                        n: Vec3::Z,
                        depth,
                    });
                }
            };
            match col {
                Collider::Sphere { center, radius } => probe(*center, *radius),
                Collider::Capsule { a, b, radius } => {
                    probe(*a, *radius);
                    probe(*b, *radius);
                }
            }
        }
        out
    }

    fn solve_velocity(
        &self,
        cfg: &Articulation3dConfig,
        m: &[Vec<f32>],
        st: &mut Articulation3dState,
        contacts: &[Contact],
    ) {
        let n = cfg.ndof;
        let dt = cfg.dt;
        // Per-contact rows + Delassus diagonals (precomputed once).
        struct Row {
            jn: Vec<f32>,
            jt1: Vec<f32>,
            jt2: Vec<f32>,
            minv_jn: Vec<f32>,
            minv_jt1: Vec<f32>,
            minv_jt2: Vec<f32>,
            inv_mn: f32,
            inv_mt1: f32,
            inv_mt2: f32,
            bias_n: f32,
        }
        let mut rows = Vec::with_capacity(contacts.len());
        for c in contacts {
            let (t1, t2) = tangents(c.n);
            let pj = cfg.point_jacobian(c.link, c.p, &st.q);
            let jn = project(&pj, c.n, n);
            let jt1 = project(&pj, t1, n);
            let jt2 = project(&pj, t2, n);
            let minv_jn = m_inv_mul(m, &jn);
            let minv_jt1 = m_inv_mul(m, &jt1);
            let minv_jt2 = m_inv_mul(m, &jt2);
            let mn = dotv(&jn, &minv_jn).max(1e-9);
            let mt1 = dotv(&jt1, &minv_jt1).max(1e-9);
            let mt2 = dotv(&jt2, &minv_jt2).max(1e-9);
            // Penetration push-out (Baumgarte), capped at the slop band.
            let pen = (c.depth - self.params.slop).max(0.0);
            let vn_pre = dotv(&jn, &st.qdot);
            let restitution = if vn_pre < 0.0 { -self.params.restitution * vn_pre } else { 0.0 };
            let bias_n = (self.params.baumgarte / dt) * pen + restitution;
            rows.push(Row {
                jn,
                jt1,
                jt2,
                minv_jn,
                minv_jt1,
                minv_jt2,
                inv_mn: 1.0 / mn,
                inv_mt1: 1.0 / mt1,
                inv_mt2: 1.0 / mt2,
                bias_n,
            });
        }

        let mu = self.params.friction;
        let mut lam_n = vec![0.0_f32; contacts.len()];
        let mut lam_t1 = vec![0.0_f32; contacts.len()];
        let mut lam_t2 = vec![0.0_f32; contacts.len()];

        for _ in 0..self.params.iters {
            for k in 0..rows.len() {
                let row = &rows[k];
                // Normal: drive vn toward the separating target (bias_n ≥ 0).
                let vn = dotv(&row.jn, &st.qdot);
                let mut dln = (row.bias_n - vn) * row.inv_mn;
                let new_n = (lam_n[k] + dln).max(0.0);
                dln = new_n - lam_n[k];
                lam_n[k] = new_n;
                axpy_inplace(&mut st.qdot, dln, &row.minv_jn);

                // Friction: clamp tangential impulse to the cone |λt| ≤ μ λn.
                let lim = mu * lam_n[k];
                let vt1 = dotv(&row.jt1, &st.qdot);
                let mut dlt1 = (-vt1) * row.inv_mt1;
                let new_t1 = (lam_t1[k] + dlt1).clamp(-lim, lim);
                dlt1 = new_t1 - lam_t1[k];
                lam_t1[k] = new_t1;
                axpy_inplace(&mut st.qdot, dlt1, &row.minv_jt1);

                let vt2 = dotv(&row.jt2, &st.qdot);
                let mut dlt2 = (-vt2) * row.inv_mt2;
                let new_t2 = (lam_t2[k] + dlt2).clamp(-lim, lim);
                dlt2 = new_t2 - lam_t2[k];
                lam_t2[k] = new_t2;
                axpy_inplace(&mut st.qdot, dlt2, &row.minv_jt2);
            }
        }
    }
}

/// Project a per-DOF linear Jacobian (`pj[d] = ∂vₚ/∂q̇_d`) onto direction `dir`.
fn project(pj: &[[f32; 3]], dir: Vec3, ndof: usize) -> Vec<f32> {
    let mut row = vec![0.0_f32; ndof];
    for d in 0..ndof {
        row[d] = dir.x * pj[d][0] + dir.y * pj[d][1] + dir.z * pj[d][2];
    }
    row
}

fn m_inv_mul(m: &[Vec<f32>], j: &[f32]) -> Vec<f32> {
    let mut b = j.to_vec();
    solve_ldlt(m, &mut b).unwrap_or_else(|| vec![0.0; j.len()])
}

fn dotv(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn axpy_inplace(y: &mut [f32], s: f32, x: &[f32]) {
    for d in 0..y.len() {
        y[d] += s * x[d];
    }
}

/// Two orthonormal tangents spanning the plane ⟂ to unit `n`.
fn tangents(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let t1 = (a - n * a.dot(n)).normalize();
    let t2 = n.cross(t1);
    (t1, t2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::articulation3d::{Articulation3dConfig, Articulation3dState, Body3d, JointType3d};
    use crate::spatial::spatial_inertia;
    use glam::Mat3;

    /// A single revolute link (about +y) hanging under gravity, with a sphere
    /// collider at its tip and a ground plane partway down. The tip must come
    /// to rest **on** the plane: no run-away penetration, no residual motion.
    fn one_link_with_ground(ground_z: f32) -> (Articulation3dConfig, ContactWorld) {
        let m = 1.0;
        let l = 1.0;
        let i_perp = m * l * l / 12.0;
        let i_com = Mat3::from_diagonal(Vec3::new(i_perp, i_perp, 0.0));
        let com = Vec3::new(0.0, 0.0, -l / 2.0);
        let body = Body3d {
            name: "link".into(),
            parent: -1,
            joint_type: JointType3d::Revolute,
            axis: Vec3::new(0.0, -1.0, 0.0),
            e_tree: Mat3::IDENTITY,
            r_tree: Vec3::ZERO,
            inertia: spatial_inertia(m, com, i_com),
            mass: m,
            com,
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof: 0,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![body],
            gravity: Vec3::new(0.0, 0.0, -9.81),
            dt: 1.0 / 240.0,
            ndof: 1,
        };
        let cw = ContactWorld::new(
            vec![(0, Collider::Sphere { center: Vec3::new(0.0, 0.0, -l), radius: 0.05 })],
            ContactParams { ground_z, ..Default::default() },
        );
        (cfg, cw)
    }

    fn tip_z(cfg: &Articulation3dConfig, st: &Articulation3dState) -> f32 {
        let (r, p0) = cfg.link_world(&st.q)[0];
        (p0 + r * Vec3::new(0.0, 0.0, -1.0)).z
    }

    #[test]
    fn link_settles_on_ground_without_penetrating() {
        // Start horizontal (q=π/2 about −y swings tip toward −x/down). Ground
        // at z=−0.6 catches the tip (radius 0.05 → rest near −0.55).
        let (cfg, cw) = one_link_with_ground(-0.6);
        let mut st = Articulation3dState { q: vec![std::f32::consts::FRAC_PI_2], qdot: vec![0.0] };
        for _ in 0..2000 {
            cw.step(&cfg, &mut st, &[0.0]);
        }
        let z = tip_z(&cfg, &st);
        // Tip rests at/above ground minus slop; sphere radius keeps center above.
        assert!(z >= -0.62, "tip penetrated: z={z}");
        assert!(z <= -0.45, "tip should have fallen to the ground, z={z}");
        // At rest: joint velocity ≈ 0.
        assert!(st.qdot[0].abs() < 0.05, "should be at rest, qdot={}", st.qdot[0]);
    }

    #[test]
    fn no_contact_when_ground_is_far_below() {
        let (cfg, cw) = one_link_with_ground(-5.0);
        let st = Articulation3dState { q: vec![std::f32::consts::FRAC_PI_2], qdot: vec![0.0] };
        assert_eq!(cw.contact_count(&cfg, &st.q), 0);
    }

    #[test]
    fn contact_does_not_inject_energy() {
        // With restitution 0, total energy must not rise over the contact phase
        // (Baumgarte can add a little; bound generously but finite).
        let (cfg, cw) = one_link_with_ground(-0.6);
        let mut st = Articulation3dState { q: vec![std::f32::consts::FRAC_PI_2], qdot: vec![0.0] };
        let e0 = cfg.energy(&st);
        let mut emax = e0;
        for _ in 0..2000 {
            cw.step(&cfg, &mut st, &[0.0]);
            emax = emax.max(cfg.energy(&st));
        }
        assert!(emax <= e0 + 0.05 * e0.abs().max(1.0), "energy grew: e0={e0} emax={emax}");
    }
}
