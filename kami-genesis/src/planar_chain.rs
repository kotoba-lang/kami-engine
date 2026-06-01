//! Planar n-link revolute serial chain — generalizes DoublePendulum to N joints.
//!
//! All revolute joints rotate about world +y; chain lies in the xz-plane;
//! gravity points along -z. Uniform-rod assumption: COM at half length,
//! inertia about COM = m·l²/12.
//!
//! Forward dynamics by `M(q)·qddot = τ − h(q, qdot)` where:
//!   - `h(q, qdot)` is the bias force from RNEA with qddot = 0
//!   - `M(q)` is the joint-space inertia matrix from CRBA (recursive composite
//!     rigid body algorithm)
//!
//! Solves the resulting N×N symmetric positive-definite linear system via
//! in-place Cholesky `LDL^T`. Designed for N up to ~10 (enough for Franka-7DoF
//! while still trivial to compute on a watch).
//!
//! At N=2 the dynamics reduce to the double pendulum implemented in
//! `double_pendulum.rs`; that is verified by an integration test (regression).
//!
//! References:
//!   - Featherstone, "Rigid Body Dynamics Algorithms", Ch. 5 (RNEA) and Ch. 6
//!     (CRBA), specialized here for a single-axis planar chain.
//!   - Spong et al., "Robot Modeling and Control" (planar manipulator).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanarChainConfig {
    pub n: u32,
    pub masses: Vec<f32>,
    pub lengths: Vec<f32>,
    pub gravity: f32,
    pub effort_limit: f32,
    pub dt: f32,
}

impl PlanarChainConfig {
    /// A uniform N-link chain with each link mass=1.0 kg, length=1.0 m.
    pub fn uniform(n: u32) -> Self {
        PlanarChainConfig {
            n,
            masses: vec![1.0; n as usize],
            lengths: vec![1.0; n as usize],
            gravity: 9.81,
            effort_limit: 50.0,
            dt: 1.0 / 240.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanarChainState {
    pub q: Vec<f32>,
    pub qdot: Vec<f32>,
}

impl PlanarChainState {
    pub fn zeros(n: u32) -> Self {
        PlanarChainState {
            q: vec![0.0; n as usize],
            qdot: vec![0.0; n as usize],
        }
    }

    /// Semi-implicit Euler step under joint torques `tau[i]` (clamped to
    /// ±cfg.effort_limit).
    pub fn step(&mut self, tau: &[f32], cfg: &PlanarChainConfig) {
        let n = cfg.n as usize;
        assert_eq!(self.q.len(), n);
        assert_eq!(self.qdot.len(), n);
        assert_eq!(tau.len(), n);

        // Clamp torques.
        let tau_clamped: Vec<f32> = tau
            .iter()
            .map(|t| t.clamp(-cfg.effort_limit, cfg.effort_limit))
            .collect();

        // h(q, qdot) = bias from RNEA with qddot = 0.
        let h = rnea_planar(&self.q, &self.qdot, &vec![0.0; n], cfg, true);
        // M(q) via CRBA (column by column: M[:,j] = rnea(0, 0, e_j, no_gravity)).
        let m = mass_matrix_crba(&self.q, cfg);
        // Solve M·qddot = τ − h.
        let mut rhs: Vec<f32> = tau_clamped
            .iter()
            .zip(h.iter())
            .map(|(t, hi)| t - hi)
            .collect();
        let qddot = solve_ldlt(&m, &mut rhs).unwrap_or_else(|| vec![0.0; n]);

        // Semi-implicit Euler.
        for i in 0..n {
            self.qdot[i] += cfg.dt * qddot[i];
            self.q[i] += cfg.dt * self.qdot[i];
        }
    }

    /// Total mechanical energy (KE + PE), useful for symplectic-stability test.
    pub fn energy(&self, cfg: &PlanarChainConfig) -> f32 {
        let n = cfg.n as usize;
        // Use the kinematic recursion to compute v_com and z_com for each link.
        let mut theta_cum = 0.0_f32;
        let mut omega_cum = 0.0_f32;
        let mut p_joint_x = 0.0_f32;
        let mut p_joint_z = 0.0_f32;
        let mut v_joint_x = 0.0_f32;
        let mut v_joint_z = 0.0_f32;
        let mut ke = 0.0_f32;
        let mut pe = 0.0_f32;
        for i in 0..n {
            theta_cum += self.q[i];
            omega_cum += self.qdot[i];
            let l = cfg.lengths[i];
            let lc = l * 0.5;
            let m = cfg.masses[i];
            let s = theta_cum.sin();
            let c = theta_cum.cos();
            // COM kinematics.
            let p_com_x = p_joint_x + lc * s;
            let p_com_z = p_joint_z - lc * c;
            let v_com_x = v_joint_x + lc * omega_cum * c;
            let v_com_z = v_joint_z + lc * omega_cum * s;
            let i_com = m * l * l / 12.0;
            ke += 0.5 * m * (v_com_x * v_com_x + v_com_z * v_com_z)
                + 0.5 * i_com * omega_cum * omega_cum;
            pe += m * cfg.gravity * p_com_z;
            // Advance to next joint.
            p_joint_x += l * s;
            p_joint_z -= l * c;
            v_joint_x += l * omega_cum * c;
            v_joint_z += l * omega_cum * s;
        }
        ke + pe
    }
}

/// Recursive Newton-Euler inverse dynamics for the planar chain.
/// Returns tau[i] for i = 0..n. If `with_gravity == false`, gravity is dropped
/// (useful inside CRBA where the gravity bias is not part of M).
fn rnea_planar(
    q: &[f32],
    qdot: &[f32],
    qddot: &[f32],
    cfg: &PlanarChainConfig,
    with_gravity: bool,
) -> Vec<f32> {
    let n = cfg.n as usize;
    let g = if with_gravity { cfg.gravity } else { 0.0 };

    // Forward kinematics + per-link COM accelerations.
    // theta_i = cumulative absolute angle, omega_i = derivative, alpha_i = qddot cumulative.
    // a_com_i = a_joint_i + lc_i * (alpha_cum*cos θ - omega_cum^2 * sin θ,
    //                                0,
    //                                alpha_cum*sin θ + omega_cum^2 * cos θ)
    // a_joint_{i+1} = a_joint_i + l_i * (alpha_cum*cos θ - omega_cum^2 * sin θ,
    //                                    0,
    //                                    alpha_cum*sin θ + omega_cum^2 * cos θ)
    let mut theta = vec![0.0_f32; n];
    let mut omega = vec![0.0_f32; n];
    let mut alpha = vec![0.0_f32; n];
    let mut a_com_x = vec![0.0_f32; n];
    let mut a_com_z = vec![0.0_f32; n];
    let mut a_joint_x = vec![0.0_f32; n + 1]; // joint 0..n; joint 0 = world origin
    let mut a_joint_z = vec![0.0_f32; n + 1];
    let mut p_com_x = vec![0.0_f32; n];
    let mut p_com_z = vec![0.0_f32; n];
    let mut p_joint_x = vec![0.0_f32; n + 1];
    let mut p_joint_z = vec![0.0_f32; n + 1];

    let mut cum_theta = 0.0_f32;
    let mut cum_omega = 0.0_f32;
    let mut cum_alpha = 0.0_f32;
    let mut prev_pjx = 0.0_f32;
    let mut prev_pjz = 0.0_f32;
    let mut prev_ajx = 0.0_f32;
    let mut prev_ajz = 0.0_f32;
    for i in 0..n {
        cum_theta += q[i];
        cum_omega += qdot[i];
        cum_alpha += qddot[i];
        theta[i] = cum_theta;
        omega[i] = cum_omega;
        alpha[i] = cum_alpha;
        let s = cum_theta.sin();
        let c = cum_theta.cos();
        let lc = cfg.lengths[i] * 0.5;
        let l = cfg.lengths[i];

        // COM com pos + accel from joint i to com.
        p_com_x[i] = prev_pjx + lc * s;
        p_com_z[i] = prev_pjz - lc * c;
        a_com_x[i] = prev_ajx + lc * (cum_alpha * c - cum_omega * cum_omega * s);
        a_com_z[i] = prev_ajz + lc * (cum_alpha * s + cum_omega * cum_omega * c);

        // Next joint at link tip.
        let pj_next_x = prev_pjx + l * s;
        let pj_next_z = prev_pjz - l * c;
        let aj_next_x = prev_ajx + l * (cum_alpha * c - cum_omega * cum_omega * s);
        let aj_next_z = prev_ajz + l * (cum_alpha * s + cum_omega * cum_omega * c);

        p_joint_x[i + 1] = pj_next_x;
        p_joint_z[i + 1] = pj_next_z;
        a_joint_x[i + 1] = aj_next_x;
        a_joint_z[i + 1] = aj_next_z;
        prev_pjx = pj_next_x;
        prev_pjz = pj_next_z;
        prev_ajx = aj_next_x;
        prev_ajz = aj_next_z;
    }

    // Backward pass: per-link force and torque about joint i.
    // F_i = sum_{k=i..n-1} m_k * (a_com_k + g_world)  (g_world = (0, 0, -g))
    // tau_i = sum_{k=i..n-1} [ (r_joint_i_to_com_k × m_k*(a_com_k + g_world))_y
    //                          + I_k * alpha_k ]
    // where alpha_k is cumulative angular acceleration of link k.
    let mut tau_out = vec![0.0_f32; n];
    for i in 0..n {
        let mut tau_i = 0.0_f32;
        for k in i..n {
            let m = cfg.masses[k];
            let lk = cfg.lengths[k];
            let i_com = m * lk * lk / 12.0;
            // Force on link k from inertia + gravity: f = m*a - m*g_world.
            // In our convention world gravity is along -z, so g_world.z = -g.
            // The reaction force the link exerts on the joint =
            // m * (a_com - g_world).
            let f_x = m * a_com_x[k];
            // a_com_z minus (-g) = a_com_z + g
            let f_z = m * (a_com_z[k] + g);
            // r from joint i to com of link k.
            let r_x = p_com_x[k] - p_joint_x[i];
            let r_z = p_com_z[k] - p_joint_z[i];
            // Joint torque projected onto the q-axis. Our q convention is
            // "q=0 means link points along world -z, +q rotates toward +x",
            // which corresponds to rotation about world -y. The torque about
            // -y axis is -(r × f)_y = r_x * f_z - r_z * f_x. (Standard
            // right-handed (r × f)_y = r_z*f_x − r_x*f_z; project onto -y.)
            let torque_q = r_x * f_z - r_z * f_x;
            tau_i += torque_q + i_com * alpha[k];
        }
        tau_out[i] = tau_i;
    }
    tau_out
}

/// Compose the joint-space mass matrix `M(q)` via the standard CRBA recipe
/// that delegates to RNEA: column `j` of `M` = RNEA(q, qdot=0, qddot=e_j,
/// gravity=0).
fn mass_matrix_crba(q: &[f32], cfg: &PlanarChainConfig) -> Vec<Vec<f32>> {
    let n = cfg.n as usize;
    let zero = vec![0.0_f32; n];
    let mut m = vec![vec![0.0_f32; n]; n];
    for j in 0..n {
        let mut qddot = vec![0.0_f32; n];
        qddot[j] = 1.0;
        let col = rnea_planar(q, &zero, &qddot, cfg, false);
        for i in 0..n {
            m[i][j] = col[i];
        }
    }
    // Symmetrize numerically (cancel small asymmetric noise).
    for i in 0..n {
        for j in (i + 1)..n {
            let mean = 0.5 * (m[i][j] + m[j][i]);
            m[i][j] = mean;
            m[j][i] = mean;
        }
    }
    m
}

/// Solve M·x = b for symmetric positive-definite M using LDL^T factorisation,
/// in-place on b. Returns Some(x) if the matrix is SPD, None otherwise.
fn solve_ldlt(mat: &[Vec<f32>], b: &mut [f32]) -> Option<Vec<f32>> {
    let n = b.len();
    if mat.len() != n || mat.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut a = mat.iter().map(|r| r.clone()).collect::<Vec<_>>();
    // LDL^T: A[i][i] = D[i]; A[i][j] (i > j) = L[i][j] (lower triangular factor)
    for j in 0..n {
        let mut sum = a[j][j];
        for k in 0..j {
            sum -= a[j][k] * a[j][k] * a[k][k];
        }
        if sum.abs() < 1e-12 {
            return None;
        }
        a[j][j] = sum;
        for i in (j + 1)..n {
            let mut s = a[i][j];
            for k in 0..j {
                s -= a[i][k] * a[j][k] * a[k][k];
            }
            a[i][j] = s / a[j][j];
        }
    }
    // Solve L·y = b
    let mut y = vec![0.0_f32; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= a[i][k] * y[k];
        }
        y[i] = s;
    }
    // D·z = y
    let mut z = y;
    for i in 0..n {
        z[i] /= a[i][i];
    }
    // L^T·x = z
    let mut x = vec![0.0_f32; n];
    for i in (0..n).rev() {
        let mut s = z[i];
        for k in (i + 1)..n {
            s -= a[k][i] * x[k];
        }
        x[i] = s;
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n1_reduces_to_single_pendulum_period() {
        // N=1: single pendulum about y. Small-amplitude period is
        // T = 2π√(I_pivot / (m·g·lc)) where I_pivot = I_com + m·lc² = m·l²/3.
        // For l=1, m=1, g=9.81, lc=0.5: I_pivot = 1/3, m·g·lc = 4.905
        // → T = 2π√(1/3 / 4.905) = 2π · 0.2606 ≈ 1.637 s.
        let cfg = PlanarChainConfig::uniform(1);
        let mut s = PlanarChainState::zeros(1);
        s.q[0] = 0.05;
        // Run 1 period worth of steps.
        let steps = (1.637 / cfg.dt) as usize;
        let e0 = s.energy(&cfg);
        for _ in 0..steps {
            s.step(&[0.0], &cfg);
        }
        let e1 = s.energy(&cfg);
        // Energy drift < 5% under semi-implicit Euler at 240 Hz over 1 period.
        assert!(
            (e1 - e0).abs() / e0.abs().max(1.0) < 0.05,
            "energy drift: e0={e0:.4}, e1={e1:.4}"
        );
        // Pendulum returns close to start: |q[0] - 0.05| < 0.02 rad.
        assert!((s.q[0] - 0.05).abs() < 0.02 || (s.q[0] + 0.05).abs() < 0.02);
    }

    #[test]
    fn n2_matches_double_pendulum_dynamics() {
        // Compare against the explicit DoublePendulum implementation for a
        // short rollout. Use the same uniform-rod parameters.
        use crate::double_pendulum::{DoublePendulumConfig, DoublePendulumState};
        let dp_cfg = DoublePendulumConfig::default();
        let chain_cfg = PlanarChainConfig {
            n: 2,
            masses: vec![dp_cfg.m1, dp_cfg.m2],
            lengths: vec![dp_cfg.l1, dp_cfg.l2],
            gravity: dp_cfg.gravity,
            effort_limit: dp_cfg.effort_limit,
            dt: dp_cfg.dt,
        };
        let mut dp = DoublePendulumState {
            q1: 0.4,
            q2: 0.2,
            ..Default::default()
        };
        let mut chain = PlanarChainState {
            q: vec![0.4, 0.2],
            qdot: vec![0.0, 0.0],
        };
        // 0.5 s rollout under no torque.
        for _ in 0..(0.5 / dp_cfg.dt) as usize {
            dp.step([0.0, 0.0], &dp_cfg);
            chain.step(&[0.0, 0.0], &chain_cfg);
        }
        // Both should agree on q1, q2 within numerical tolerance after 0.5 s.
        assert!(
            (chain.q[0] - dp.q1).abs() < 5e-3,
            "q1: chain={}, dp={}",
            chain.q[0],
            dp.q1
        );
        assert!(
            (chain.q[1] - dp.q2).abs() < 5e-3,
            "q2: chain={}, dp={}",
            chain.q[1],
            dp.q2
        );
    }

    #[test]
    fn n3_triple_pendulum_falls_from_horizontal() {
        // Triple pendulum released from q=(π/2, 0, 0); should swing down.
        let cfg = PlanarChainConfig::uniform(3);
        let mut s = PlanarChainState::zeros(3);
        s.q[0] = std::f32::consts::FRAC_PI_2;
        let _e0 = s.energy(&cfg);
        for _ in 0..(0.3 / cfg.dt) as usize {
            s.step(&[0.0, 0.0, 0.0], &cfg);
        }
        assert!(
            s.q[0] < std::f32::consts::FRAC_PI_2,
            "released triple pendulum should swing toward 0, got q1={}",
            s.q[0]
        );
    }

    #[test]
    fn balanced_at_zero_stays_balanced_for_any_n() {
        // Initial q = 0 is stable equilibrium (all links hanging down).
        // No torque, small dt → should stay there.
        for n in 1..=4 {
            let cfg = PlanarChainConfig::uniform(n);
            let mut s = PlanarChainState::zeros(n);
            for _ in 0..400 {
                s.step(&vec![0.0; n as usize], &cfg);
            }
            for &qi in &s.q {
                assert!(qi.abs() < 1e-3, "N={n}: q={:?}", s.q);
            }
        }
    }

    #[test]
    fn effort_clamped_to_limit() {
        // With effort_limit = 50 N·m clamping a wildly large input, the
        // post-clamp torque is bounded; therefore the angular acceleration is
        // bounded by 50 / lambda_min(M), where lambda_min(M) for a uniform
        // 3-link chain at q=0 is ~0.05 (tip-most link is most accelerable).
        // qdot after a single dt=1/240 step is bounded by ~ 50 / 0.05 / 240 ≈ 4.2.
        // Allow some headroom (50× margin); the point is "finite and bounded",
        // not the exact magnitude.
        let cfg = PlanarChainConfig::uniform(3);
        let mut s = PlanarChainState::zeros(3);
        s.step(&[10_000.0, -10_000.0, 10_000.0], &cfg);
        for &qd in &s.qdot {
            assert!(qd.is_finite(), "qdot was NaN/Inf: {qd}");
            assert!(qd.abs() < 50.0, "qdot too large after one step: {qd}");
        }
    }

    // ── Analytical-solution validation ─────────────────────────────────────
    //
    // These are textbook closed-form results that ANY correct rigid-body
    // engine — PhysX / Isaac Sim included — must reproduce. Matching them is
    // the clean-room evidence that kami-genesis agrees with NVIDIA on the same
    // physics, without running NVIDIA. (ADR-2605261800 §D10.1; G5 numeric
    // cross-check against an Isaac reference CSV is a separate offline path.)

    #[test]
    fn uniform_rod_horizontal_initial_angular_accel_is_3g_over_2l() {
        // A uniform rod pivoted at one end, released from horizontal, has
        // initial angular acceleration α = 3g / (2L) (Goldstein, rigid-body
        // rotation about a fixed axis: τ = m·g·(L/2), I_end = m·L²/3 ⇒
        // α = τ/I = 3g/2L). Independent of mass.
        let cfg = PlanarChainConfig::uniform(1); // L=1, m=1, g=9.81
        let mut s = PlanarChainState::zeros(1);
        s.q[0] = std::f32::consts::FRAC_PI_2; // horizontal (+x)
        // One step from rest: qdot ≈ α·dt; recover α = qdot/dt.
        s.step(&[0.0], &cfg);
        let alpha_measured = s.qdot[0] / cfg.dt;
        let alpha_expected = -3.0 * cfg.gravity / (2.0 * cfg.lengths[0]); // toward q=0
        let rel = (alpha_measured - alpha_expected).abs() / alpha_expected.abs();
        assert!(
            rel < 0.02,
            "horizontal-rod α: measured={alpha_measured:.4}, expected={alpha_expected:.4} (rel {rel:.4})"
        );
    }

    #[test]
    fn uniform_rod_bottom_speed_matches_energy_conservation() {
        // Released from horizontal (q=π/2), the rod's COM falls by lc = L/2.
        // Energy conservation: ½·I_end·ω² = m·g·(L/2) ⇒ ω_bottom = √(3g/L)
        // when it passes through the downward vertical (q=0).
        // For L=1, g=9.81: ω = √29.43 ≈ 5.4249 rad/s.
        let cfg = PlanarChainConfig::uniform(1);
        let mut s = PlanarChainState::zeros(1);
        s.q[0] = std::f32::consts::FRAC_PI_2;
        // Integrate until the rod first crosses the bottom (q goes ≤ 0).
        let mut prev_q = s.q[0];
        let mut omega_at_bottom = 0.0;
        for _ in 0..2000 {
            s.step(&[0.0], &cfg);
            if prev_q > 0.0 && s.q[0] <= 0.0 {
                omega_at_bottom = s.qdot[0].abs();
                break;
            }
            prev_q = s.q[0];
        }
        let omega_expected = (3.0 * cfg.gravity / cfg.lengths[0]).sqrt();
        let rel = (omega_at_bottom - omega_expected).abs() / omega_expected;
        assert!(
            rel < 0.02,
            "bottom speed ω: measured={omega_at_bottom:.4}, expected={omega_expected:.4} (rel {rel:.4})"
        );
    }

    #[test]
    fn large_swing_energy_drift_bounded() {
        // A frictionless pendulum released from horizontal must conserve total
        // mechanical energy. Semi-implicit Euler (symplectic) keeps the drift
        // bounded (no secular growth) over a multi-second rollout.
        let cfg = PlanarChainConfig::uniform(2);
        let mut s = PlanarChainState::zeros(2);
        s.q[0] = std::f32::consts::FRAC_PI_2;
        let e0 = s.energy(&cfg);
        let mut e_min = e0;
        let mut e_max = e0;
        for _ in 0..(3.0 / cfg.dt) as usize {
            s.step(&[0.0, 0.0], &cfg);
            let e = s.energy(&cfg);
            e_min = e_min.min(e);
            e_max = e_max.max(e);
            assert!(e.is_finite(), "energy non-finite");
        }
        // Peak-to-peak energy oscillation stays small relative to the kinetic
        // scale (|PE| swing ~ m·g·L). Symplectic ⇒ bounded, not growing.
        let scale =
            (cfg.masses.iter().sum::<f32>() * cfg.gravity * cfg.lengths.iter().sum::<f32>())
                .max(1.0);
        let drift = (e_max - e_min) / scale;
        assert!(
            drift < 0.05,
            "energy drift too large: {drift:.4} (e0={e0:.4})"
        );
    }

    #[test]
    fn mass_matrix_is_symmetric_pd() {
        // For random-but-bounded q, M should be symmetric and have positive
        // diagonal. CRBA + LDL^T relies on that.
        let cfg = PlanarChainConfig::uniform(3);
        let q = vec![0.3_f32, -0.2, 0.1];
        let m = mass_matrix_crba(&q, &cfg);
        // Diagonal positive.
        for i in 0..3 {
            assert!(m[i][i] > 0.0, "M[{i}][{i}] = {}", m[i][i]);
        }
        // Symmetric.
        for i in 0..3 {
            for j in (i + 1)..3 {
                assert!((m[i][j] - m[j][i]).abs() < 1e-5);
            }
        }
        // SPD (Cholesky succeeds).
        let mut b = vec![1.0, 2.0, 3.0];
        let x = solve_ldlt(&m, &mut b);
        assert!(x.is_some());
    }

    #[test]
    fn crba_mass_matrix_matches_kinetic_energy() {
        // Two independent derivations must agree: the CRBA mass matrix M(q)
        // and the kinematic energy recursion. With gravity off, energy() is
        // pure kinetic energy, which by definition equals ½·q̇ᵀ M(q) q̇.
        // Non-uniform link masses/lengths make this a real cross-check.
        let mut cfg = PlanarChainConfig::uniform(4);
        cfg.gravity = 0.0;
        cfg.masses = vec![1.3, 0.7, 1.1, 0.9];
        cfg.lengths = vec![0.8, 1.2, 0.6, 1.0];
        let q = vec![0.4_f32, -0.6, 0.25, -0.15];
        let qd = vec![0.9_f32, -0.5, 1.3, 0.2];

        let st = PlanarChainState {
            q: q.clone(),
            qdot: qd.clone(),
        };
        let ke_direct = st.energy(&cfg); // gravity = 0 → pure KE

        let m = mass_matrix_crba(&q, &cfg);
        let n = cfg.n as usize;
        let mut ke_matrix = 0.0_f32;
        for i in 0..n {
            for j in 0..n {
                ke_matrix += qd[i] * m[i][j] * qd[j];
            }
        }
        ke_matrix *= 0.5;

        assert!(
            (ke_direct - ke_matrix).abs() < 1e-4 * ke_direct.abs().max(1.0),
            "KE mismatch: kinematic {ke_direct} vs ½q̇ᵀMq̇ {ke_matrix}"
        );
    }
}
