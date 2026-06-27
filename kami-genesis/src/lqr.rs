//! Linear Quadratic Regulator (LQR) controller for the Cartpole around its
//! upright equilibrium (theta = 0).
//!
//! Build pipeline:
//!   1. Finite-difference the existing nonlinear Cartpole step formula at
//!      (x, x_dot, theta, theta_dot) = 0 to recover the discrete-time
//!      linearisation matrices A (4×4) and B (4×1).
//!   2. Solve the Discrete Algebraic Riccati Equation (DARE) iteratively:
//!        P_{k+1} = Q + A^T P_k A − A^T P_k B (R + B^T P_k B)^{-1} B^T P_k A
//!      until ‖P_{k+1} − P_k‖ < tol.
//!   3. The optimal state-feedback gain is
//!        K = (R + B^T P B)^{-1} B^T P A    (1×4 row vector for our 1-D control)
//!   4. Control law: u(s) = −K · (s − s_target), clamped to ±max_effort.
//!
//! Because the control input is scalar, no 4×4 matrix inverse is needed —
//! `R + B^T P B` is a scalar and the rest is pure matrix-vector math.

use crate::cartpole::{CartpoleConfig, CartpoleState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LqrWeights {
    /// Diagonal of Q (4): [x, x_dot, theta, theta_dot]. Larger ⇒ tighter regulation.
    pub q_diag: [f32; 4],
    /// Scalar R: control effort weight. Larger ⇒ smaller control.
    pub r: f32,
}

impl Default for LqrWeights {
    fn default() -> Self {
        // Tuned for stable upright balance: penalise pole angle most, allow
        // moderate cart drift.
        LqrWeights {
            q_diag: [1.0, 0.1, 100.0, 1.0],
            r: 0.1,
        }
    }
}

/// LQR state-feedback gain + max effort clamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LqrController {
    /// 1×4 gain row (K · s = scalar control).
    pub gain: [f32; 4],
    /// Effort clamp magnitude.
    pub max_effort: f32,
    /// Number of DARE iterations actually run (for diagnostics).
    pub dare_iters: u32,
    /// Final |P_{k+1} − P_k|∞ at termination.
    pub dare_residual: f32,
}

impl LqrController {
    /// Build an LQR controller for the given Cartpole config + weights.
    /// Linearises the closed-form `CartpoleState::step` around the upright
    /// equilibrium via finite differences.
    pub fn build(cfg: &CartpoleConfig, weights: LqrWeights) -> Self {
        let (a, b) = linearize_cartpole(cfg, 1e-3);
        let (p, iters, res) = solve_dare(&a, &b, &weights, 1e-6, 1000);
        let gain = compute_gain(&a, &b, &p, weights.r);
        LqrController {
            gain,
            max_effort: cfg.force_mag,
            dare_iters: iters,
            dare_residual: res,
        }
    }

    /// Compute control input u = clamp(−K · s, ±max_effort).
    /// `s = [x, x_dot, theta, theta_dot]` — state relative to the upright
    /// equilibrium (which is the origin in our linearisation).
    pub fn control(&self, s: &CartpoleState) -> f32 {
        let s_vec = [s.x, s.x_dot, s.theta, s.theta_dot];
        let u = -(self.gain[0] * s_vec[0]
            + self.gain[1] * s_vec[1]
            + self.gain[2] * s_vec[2]
            + self.gain[3] * s_vec[3]);
        u.clamp(-self.max_effort, self.max_effort)
    }
}

// ── Finite-difference linearisation ───────────────────────────────────────

fn step_one(s: &CartpoleState, action: f32, cfg: &CartpoleConfig) -> CartpoleState {
    let mut s2 = *s;
    s2.step(action, cfg);
    s2
}

fn state_to_arr(s: &CartpoleState) -> [f32; 4] {
    [s.x, s.x_dot, s.theta, s.theta_dot]
}

fn arr_to_state(a: [f32; 4]) -> CartpoleState {
    CartpoleState {
        x: a[0],
        x_dot: a[1],
        theta: a[2],
        theta_dot: a[3],
    }
}

fn linearize_cartpole(cfg: &CartpoleConfig, eps: f32) -> ([[f32; 4]; 4], [f32; 4]) {
    let s0 = CartpoleState::default();
    let mut a = [[0.0_f32; 4]; 4];
    let mut b = [0.0_f32; 4];

    // A: ∂(step)/∂(state) — column j is the derivative of the next-state
    // 4-vector wrt component j of the input state.
    for j in 0..4 {
        let mut s_plus = state_to_arr(&s0);
        let mut s_minus = state_to_arr(&s0);
        s_plus[j] += eps;
        s_minus[j] -= eps;
        let s_p = arr_to_state(s_plus);
        let s_m = arr_to_state(s_minus);
        let n_p = step_one(&s_p, 0.0, cfg);
        let n_m = step_one(&s_m, 0.0, cfg);
        let n_p_arr = state_to_arr(&n_p);
        let n_m_arr = state_to_arr(&n_m);
        for i in 0..4 {
            a[i][j] = (n_p_arr[i] - n_m_arr[i]) / (2.0 * eps);
        }
    }

    // B: ∂(step)/∂(action) — single column for the scalar action.
    let n_plus = step_one(&s0, eps, cfg);
    let n_minus = step_one(&s0, -eps, cfg);
    let n_p_arr = state_to_arr(&n_plus);
    let n_m_arr = state_to_arr(&n_minus);
    for i in 0..4 {
        b[i] = (n_p_arr[i] - n_m_arr[i]) / (2.0 * eps);
    }

    (a, b)
}

// ── DARE iteration (scalar-control specialisation) ───────────────────────

fn mat4x4_mul(x: &[[f32; 4]; 4], y: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut z = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let mut s = 0.0_f32;
            for k in 0..4 {
                s += x[i][k] * y[k][j];
            }
            z[i][j] = s;
        }
    }
    z
}

fn mat4x4_transpose(x: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut y = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            y[j][i] = x[i][j];
        }
    }
    y
}

fn mat4x4_diag(d: [f32; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        m[i][i] = d[i];
    }
    m
}

fn mat4x4_add(x: &[[f32; 4]; 4], y: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut z = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            z[i][j] = x[i][j] + y[i][j];
        }
    }
    z
}

fn mat4x4_sub(x: &[[f32; 4]; 4], y: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut z = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            z[i][j] = x[i][j] - y[i][j];
        }
    }
    z
}

fn mat4x4_max_abs(x: &[[f32; 4]; 4]) -> f32 {
    let mut m = 0.0_f32;
    for i in 0..4 {
        for j in 0..4 {
            let v = x[i][j].abs();
            if v > m {
                m = v;
            }
        }
    }
    m
}

/// 4×1 column-times-1×4 row outer product.
fn outer4(b: [f32; 4], c: [f32; 4]) -> [[f32; 4]; 4] {
    let mut m = [[0.0_f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            m[i][j] = b[i] * c[j];
        }
    }
    m
}

/// 1×4 × 4×4 row times matrix = 1×4 row.
fn rowmat4(r: [f32; 4], m: &[[f32; 4]; 4]) -> [f32; 4] {
    let mut out = [0.0_f32; 4];
    for j in 0..4 {
        let mut s = 0.0_f32;
        for k in 0..4 {
            s += r[k] * m[k][j];
        }
        out[j] = s;
    }
    out
}

/// 4×4 × 4×1 column = 4×1.
fn matcol4(m: &[[f32; 4]; 4], c: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0_f32; 4];
    for i in 0..4 {
        let mut s = 0.0_f32;
        for k in 0..4 {
            s += m[i][k] * c[k];
        }
        out[i] = s;
    }
    out
}

fn dot4(a: [f32; 4], b: [f32; 4]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

/// Solve the discrete-time algebraic Riccati equation
///   P = Q + A^T P A − A^T P B (R + B^T P B)^{-1} B^T P A
/// via fixed-point iteration, returning (P, iters, |ΔP|∞_final).
fn solve_dare(
    a: &[[f32; 4]; 4],
    b: &[f32; 4],
    weights: &LqrWeights,
    tol: f32,
    max_iters: u32,
) -> ([[f32; 4]; 4], u32, f32) {
    let q = mat4x4_diag(weights.q_diag);
    let mut p = q;
    let a_t = mat4x4_transpose(a);
    let mut last_delta = f32::INFINITY;
    let mut iters = 0_u32;
    for _ in 0..max_iters {
        let pa = mat4x4_mul(&p, a);
        let pb = matcol4(&p, *b); // 4×1
        let btpa = rowmat4(*b, &pa); // 1×4 row
        let btpb = dot4(*b, pb); // scalar
        let scalar_inv = 1.0 / (weights.r + btpb);
        let outer_term = outer4(pb, btpa); // 4×4
        let mut outer_scaled = [[0.0_f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                outer_scaled[i][j] = outer_term[i][j] * scalar_inv;
            }
        }
        let atpa = mat4x4_mul(&a_t, &pa);
        let atpb_outer = mat4x4_mul(&a_t, &outer_scaled);
        let p_new = mat4x4_add(&q, &mat4x4_sub(&atpa, &atpb_outer));
        let delta = mat4x4_max_abs(&mat4x4_sub(&p_new, &p));
        p = p_new;
        iters += 1;
        last_delta = delta;
        if delta < tol {
            break;
        }
    }
    (p, iters, last_delta)
}

fn compute_gain(a: &[[f32; 4]; 4], b: &[f32; 4], p: &[[f32; 4]; 4], r: f32) -> [f32; 4] {
    let pa = mat4x4_mul(p, a);
    let pb = matcol4(p, *b);
    let btpa = rowmat4(*b, &pa); // 1×4
    let btpb = dot4(*b, pb); // scalar
    let scalar_inv = 1.0 / (r + btpb);
    let mut k = [0.0_f32; 4];
    for j in 0..4 {
        k[j] = scalar_inv * btpa[j];
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dare_converges() {
        let cfg = CartpoleConfig::default();
        let lqr = LqrController::build(&cfg, LqrWeights::default());
        assert!(lqr.dare_iters > 0);
        assert!(
            lqr.dare_residual < 1e-3,
            "residual = {:.3e}",
            lqr.dare_residual
        );
    }

    fn gain_magnitudes_and_signs_are_balance_compatible() {
        // Control law: u = −K · s. For Cartpole upright (theta=0=up; +theta
        // tilts toward +x), the cart needs to push +x when the pole leans +x
        // ("chase" the pole to put the base under it). u = −K_theta · theta
        // must be positive when theta > 0, so K_theta < 0. Same logic for
        // K_theta_dot. Magnitudes scale with the heavy Q weighting on theta.
        let cfg = CartpoleConfig::default();
        let lqr = LqrController::build(&cfg, LqrWeights::default());
        assert!(lqr.gain[2] < 0.0, "K_theta = {}", lqr.gain[2]);
        assert!(lqr.gain[3] < 0.0, "K_theta_dot = {}", lqr.gain[3]);
        assert!(
            lqr.gain[2].abs() > 5.0,
            "K_theta magnitude = {}",
            lqr.gain[2].abs()
        );
    }

    #[test]
    fn gain_signs_make_physical_sense() {
        // Renamed via wrapper for back-compat; calls the post-sign-correction body.
        gain_magnitudes_and_signs_are_balance_compatible();
    }

    #[test]
    fn lqr_balances_pole_from_small_perturbation() {
        // Initial state: pole tilted 0.05 rad from upright. LQR should drive
        // it back to the upright equilibrium over many steps.
        let cfg = CartpoleConfig::default();
        let lqr = LqrController::build(&cfg, LqrWeights::default());
        let mut s = CartpoleState {
            theta: 0.05,
            ..Default::default()
        };
        // Run 500 steps (~8 s at 60 Hz).
        let mut max_theta = 0.0_f32;
        for _ in 0..500 {
            let u = lqr.control(&s);
            s.step(u, &cfg);
            if s.theta.abs() > max_theta {
                max_theta = s.theta.abs();
            }
        }
        // Pole should never tip past ±0.2 rad (Cartpole-v1 termination bound).
        assert!(
            max_theta < 0.2,
            "pole tipped past 0.2 rad: max |theta| = {}",
            max_theta
        );
        // After 500 steps the pole should be near upright (within 0.05 rad).
        assert!(s.theta.abs() < 0.05, "final theta = {}", s.theta);
    }

    #[test]
    fn lqr_balances_with_larger_perturbation() {
        // Larger perturbation: theta = 0.1 rad. LQR should still recover.
        let cfg = CartpoleConfig::default();
        let lqr = LqrController::build(&cfg, LqrWeights::default());
        let mut s = CartpoleState {
            theta: 0.1,
            ..Default::default()
        };
        let mut max_theta = 0.0_f32;
        for _ in 0..500 {
            let u = lqr.control(&s);
            s.step(u, &cfg);
            if s.theta.abs() > max_theta {
                max_theta = s.theta.abs();
            }
        }
        assert!(max_theta < 0.2, "max |theta| = {}", max_theta);
    }

    #[test]
    fn lqr_clamps_to_max_effort() {
        // Far-from-equilibrium state should saturate the control output.
        let cfg = CartpoleConfig::default();
        let lqr = LqrController::build(&cfg, LqrWeights::default());
        // theta = 0.5 rad way past the linearisation point → large K·s
        let s = CartpoleState {
            theta: 0.5,
            ..Default::default()
        };
        let u = lqr.control(&s);
        // The raw -K·s would be much larger than ±100 N; ensure clamped.
        assert!(u.abs() <= cfg.force_mag + 1e-3);
    }

    #[test]
    fn lqr_with_vectorized_envs() {
        // Smoke test: LQR control applied to many Cartpole envs in parallel.
        use crate::vectorized::step_vectorized;
        let cfg = CartpoleConfig::default();
        let lqr = LqrController::build(&cfg, LqrWeights::default());
        let n = 256;
        let mut states: Vec<CartpoleState> = (0..n)
            .map(|i| CartpoleState {
                theta: 0.05 + (i as f32) * 1e-4,
                ..Default::default()
            })
            .collect();
        let mut actions = vec![0.0_f32; n];
        let mut max_theta = 0.0_f32;
        for _ in 0..300 {
            for i in 0..n {
                actions[i] = lqr.control(&states[i]);
            }
            step_vectorized(&mut states, &actions, &cfg);
            for s in &states {
                if s.theta.abs() > max_theta {
                    max_theta = s.theta.abs();
                }
            }
        }
        assert!(
            max_theta < 0.2,
            "max |theta| across 256 envs = {}",
            max_theta
        );
    }
}
