//! Damped-least-squares (DLS) inverse-kinematics solver.
//!
//! Iteratively computes a joint configuration `q` such that the named link
//! reaches a target pose in world frame. Uses the Jacobian from `jacobian.rs`
//! (so the IK works for any topology that exposes a Jacobian).
//!
//! Update rule:
//!     δq = J^T (J J^T + λ² I)^-1 · e
//!     q  ← q + step_size · δq
//! where `e = target − current_pose` is the 3-vector (Δx, Δz, Δθ_y) error
//! reduced to the planar plane (other rows of the 6-vector spatial error are
//! identically zero for our planar topologies).
//!
//! Damping `λ` regularises near singularities (det J ≈ 0); larger λ trades
//! convergence speed for robustness near singular configurations.
//!
//! Mirrors the public API surface of
//! `omni.isaac.motion_generation.LulaKinematicsSolver.compute_inverse_kinematics`
//! (Isaac Sim 4.x) at the level of inputs/outputs.

use crate::cartpole::CartpoleConfig;
use crate::cartpole::CartpoleState;
use crate::double_pendulum::DoublePendulumConfig;
use crate::double_pendulum::DoublePendulumState;
use crate::jacobian::{cartpole_link_jacobian, dp_link_jacobian, planar_chain_link_jacobian};
use crate::planar_chain::PlanarChainConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IkOptions {
    /// L2 norm of (Δx, Δz, Δθ_y) below which the iteration is "converged".
    pub tol: f32,
    pub max_iters: u32,
    /// Levenberg damping parameter. 0 = pure pseudo-inverse (fast but
    /// fragile); 0.05 is a reasonable default for most planar arms.
    pub damping_lambda: f32,
    /// Step-size multiplier for δq (Newton-style update; 1.0 = full step).
    pub step_size: f32,
    /// Treat orientation error as a third component of the 3-vector residual.
    pub include_orientation: bool,
}

impl Default for IkOptions {
    fn default() -> Self {
        IkOptions {
            tol: 1e-3,
            max_iters: 200,
            damping_lambda: 0.05,
            step_size: 0.5,
            include_orientation: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IkResult {
    pub q: Vec<f32>,
    pub converged: bool,
    pub iters: u32,
    pub final_error: f32,
}

/// Target pose for the planar topologies: (x, z, θ_y).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TargetPose {
    pub x: f32,
    pub z: f32,
    pub theta_y: f32,
}

/// Topology dispatcher — supplies the forward-pose function and Jacobian.
trait IkBackend {
    /// Forward kinematics for the named link given joint vector q.
    fn link_pose(&self, q: &[f32]) -> Option<TargetPose>;
    /// 3×n planar Jacobian (rows: ∂x/∂q, ∂z/∂q, ∂θ_y/∂q).
    fn planar_jacobian(&self, q: &[f32]) -> Option<Vec<[f32; 3]>>;
    fn dof(&self) -> usize;
}

// ── Cartpole backend ──────────────────────────────────────────────────────

struct CartpoleIkBackend<'a> {
    cfg: &'a CartpoleConfig,
    link_name: &'a str,
}

pub(crate) fn cartpole_pose(x: f32, theta: f32, link: &str) -> Option<TargetPose> {
    match link {
        "world" => Some(TargetPose {
            x: 0.0,
            z: 0.0,
            theta_y: 0.0,
        }),
        "cart" => Some(TargetPose {
            x,
            z: 0.0,
            theta_y: 0.0,
        }),
        "pole_link" => Some(TargetPose {
            x: x + 0.25 * theta.sin(),
            z: 0.25 * theta.cos(),
            theta_y: theta,
        }),
        _ => None,
    }
}

impl<'a> IkBackend for CartpoleIkBackend<'a> {
    fn link_pose(&self, q: &[f32]) -> Option<TargetPose> {
        if q.len() != 2 {
            return None;
        }
        cartpole_pose(q[0], q[1], self.link_name)
    }
    fn planar_jacobian(&self, q: &[f32]) -> Option<Vec<[f32; 3]>> {
        if q.len() != 2 {
            return None;
        }
        let j = cartpole_link_jacobian(q[1], self.link_name, self.cfg)?;
        // Compact 3×n: rows = (linear_x, linear_z, angular_y)
        let cols = j.cols();
        let mut out = Vec::with_capacity(cols);
        for c in 0..cols {
            out.push([j.rows[0][c], j.rows[2][c], j.rows[4][c]]);
        }
        Some(out)
    }
    fn dof(&self) -> usize {
        2
    }
}

// ── Double pendulum backend ───────────────────────────────────────────────

struct DpIkBackend<'a> {
    cfg: &'a DoublePendulumConfig,
    link_name: &'a str,
}

pub(crate) fn dp_pose(
    q1: f32,
    q2: f32,
    cfg: &DoublePendulumConfig,
    link: &str,
) -> Option<TargetPose> {
    match link {
        "world" => Some(TargetPose {
            x: 0.0,
            z: 0.0,
            theta_y: 0.0,
        }),
        "link1" => Some(TargetPose {
            x: (cfg.l1 * 0.5) * q1.sin(),
            z: -(cfg.l1 * 0.5) * q1.cos(),
            theta_y: q1,
        }),
        "link2" => Some(TargetPose {
            x: cfg.l1 * q1.sin() + (cfg.l2 * 0.5) * (q1 + q2).sin(),
            z: -cfg.l1 * q1.cos() - (cfg.l2 * 0.5) * (q1 + q2).cos(),
            theta_y: q1 + q2,
        }),
        // "link2_tip" — useful target reference for IK to the tip.
        "link2_tip" => Some(TargetPose {
            x: cfg.l1 * q1.sin() + cfg.l2 * (q1 + q2).sin(),
            z: -cfg.l1 * q1.cos() - cfg.l2 * (q1 + q2).cos(),
            theta_y: q1 + q2,
        }),
        _ => None,
    }
}

impl<'a> IkBackend for DpIkBackend<'a> {
    fn link_pose(&self, q: &[f32]) -> Option<TargetPose> {
        if q.len() != 2 {
            return None;
        }
        dp_pose(q[0], q[1], self.cfg, self.link_name)
    }
    fn planar_jacobian(&self, q: &[f32]) -> Option<Vec<[f32; 3]>> {
        if q.len() != 2 {
            return None;
        }
        // For link2_tip we need a different Jacobian: derivative of tip
        // position. dp_link_jacobian returns the com Jacobian for link2; we
        // re-derive the tip case in-place.
        if self.link_name == "link2_tip" {
            // Tip position (no com offset) Jacobian:
            //   x = l1*sin(q1) + l2*sin(q1+q2)
            //   z = -l1*cos(q1) - l2*cos(q1+q2)
            //   θ_y = q1 + q2
            // Layout: Vec<[f32; 3]> indexed by column (0=∂/∂q1, 1=∂/∂q2);
            // each entry holds [row_x, row_z, row_theta].
            let s1 = q[0].sin();
            let c1 = q[0].cos();
            let s12 = (q[0] + q[1]).sin();
            let c12 = (q[0] + q[1]).cos();
            let l1 = self.cfg.l1;
            let l2 = self.cfg.l2;
            return Some(vec![
                [l1 * c1 + l2 * c12, l1 * s1 + l2 * s12, 1.0], // ∂/∂q1
                [l2 * c12, l2 * s12, 1.0],                     // ∂/∂q2
            ]);
        }
        let j = dp_link_jacobian(q[0], q[1], self.link_name, self.cfg)?;
        let cols = j.cols();
        let mut out = Vec::with_capacity(cols);
        for c in 0..cols {
            out.push([j.rows[0][c], j.rows[2][c], j.rows[4][c]]);
        }
        Some(out)
    }
    fn dof(&self) -> usize {
        2
    }
}

// ── Planar chain backend ──────────────────────────────────────────────────

struct PlanarChainIkBackend<'a> {
    cfg: &'a PlanarChainConfig,
    /// `link_index` is 0-based.
    link_index: usize,
}

fn planar_chain_com_pose(q: &[f32], k: usize, cfg: &PlanarChainConfig) -> TargetPose {
    let n = cfg.n as usize;
    let mut theta_cum = 0.0_f32;
    let mut p_joint_x = 0.0_f32;
    let mut p_joint_z = 0.0_f32;
    for i in 0..=k.min(n.saturating_sub(1)) {
        theta_cum += q[i];
        if i == k {
            let lc = cfg.lengths[i] * 0.5;
            return TargetPose {
                x: p_joint_x + lc * theta_cum.sin(),
                z: p_joint_z - lc * theta_cum.cos(),
                theta_y: theta_cum,
            };
        }
        let l = cfg.lengths[i];
        p_joint_x += l * theta_cum.sin();
        p_joint_z -= l * theta_cum.cos();
    }
    TargetPose {
        x: p_joint_x,
        z: p_joint_z,
        theta_y: theta_cum,
    }
}

impl<'a> IkBackend for PlanarChainIkBackend<'a> {
    fn link_pose(&self, q: &[f32]) -> Option<TargetPose> {
        if q.len() != self.cfg.n as usize {
            return None;
        }
        Some(planar_chain_com_pose(q, self.link_index, self.cfg))
    }
    fn planar_jacobian(&self, q: &[f32]) -> Option<Vec<[f32; 3]>> {
        if q.len() != self.cfg.n as usize {
            return None;
        }
        let j = planar_chain_link_jacobian(q, self.link_index, self.cfg)?;
        let cols = j.cols();
        let mut out = Vec::with_capacity(cols);
        for c in 0..cols {
            out.push([j.rows[0][c], j.rows[2][c], j.rows[4][c]]);
        }
        Some(out)
    }
    fn dof(&self) -> usize {
        self.cfg.n as usize
    }
}

// ── Solver core ───────────────────────────────────────────────────────────

fn solve_dls(jac3xn: &[[f32; 3]], err3: [f32; 3], lambda: f32) -> Vec<f32> {
    // jac3xn[col] = [row0, row1, row2] for column `col`.  Effectively this is
    // J transposed to column-major. We compute  δq = J^T (J J^T + λ² I)^-1 e.
    let n = jac3xn.len();
    let m = 3;
    // Build A = J J^T (3×3) + λ²·I3.
    let mut a = [[0.0_f32; 3]; 3];
    for r in 0..m {
        for c in 0..m {
            let mut s = 0.0_f32;
            for k in 0..n {
                s += jac3xn[k][r] * jac3xn[k][c];
            }
            a[r][c] = s;
            if r == c {
                a[r][c] += lambda * lambda;
            }
        }
    }
    // Solve (3×3) linear system A · y = e. Use Cramer's rule (small system).
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-12 {
        return vec![0.0; n];
    }
    let inv_det = 1.0 / det;
    // 3×3 inverse via adjugate
    let inv = [
        [
            (a[1][1] * a[2][2] - a[1][2] * a[2][1]) * inv_det,
            -(a[0][1] * a[2][2] - a[0][2] * a[2][1]) * inv_det,
            (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * inv_det,
        ],
        [
            -(a[1][0] * a[2][2] - a[1][2] * a[2][0]) * inv_det,
            (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * inv_det,
            -(a[0][0] * a[1][2] - a[0][2] * a[1][0]) * inv_det,
        ],
        [
            (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * inv_det,
            -(a[0][0] * a[2][1] - a[0][1] * a[2][0]) * inv_det,
            (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * inv_det,
        ],
    ];
    // y = inv · e
    let y = [
        inv[0][0] * err3[0] + inv[0][1] * err3[1] + inv[0][2] * err3[2],
        inv[1][0] * err3[0] + inv[1][1] * err3[1] + inv[1][2] * err3[2],
        inv[2][0] * err3[0] + inv[2][1] * err3[1] + inv[2][2] * err3[2],
    ];
    // δq = J^T · y
    let mut dq = vec![0.0_f32; n];
    for k in 0..n {
        dq[k] = jac3xn[k][0] * y[0] + jac3xn[k][1] * y[1] + jac3xn[k][2] * y[2];
    }
    dq
}

fn pose_error(target: TargetPose, current: TargetPose, include_orientation: bool) -> [f32; 3] {
    let e_x = target.x - current.x;
    let e_z = target.z - current.z;
    let e_th = if include_orientation {
        // Wrap angle error to [-π, π].
        let mut e = target.theta_y - current.theta_y;
        while e > std::f32::consts::PI {
            e -= 2.0 * std::f32::consts::PI;
        }
        while e < -std::f32::consts::PI {
            e += 2.0 * std::f32::consts::PI;
        }
        e
    } else {
        0.0
    };
    [e_x, e_z, e_th]
}

fn norm3(v: &[f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn run_dls<B: IkBackend>(
    backend: &B,
    q_init: &[f32],
    target: TargetPose,
    opts: IkOptions,
) -> IkResult {
    let mut q = q_init.to_vec();
    let mut converged = false;
    let mut iters = 0u32;
    let mut final_err = f32::INFINITY;
    for _ in 0..opts.max_iters {
        let pose = match backend.link_pose(&q) {
            Some(p) => p,
            None => break,
        };
        let err = pose_error(target, pose, opts.include_orientation);
        let err_norm = norm3(&err);
        final_err = err_norm;
        if err_norm < opts.tol {
            converged = true;
            break;
        }
        let jac = match backend.planar_jacobian(&q) {
            Some(j) => j,
            None => break,
        };
        let dq = solve_dls(&jac, err, opts.damping_lambda);
        for i in 0..q.len() {
            q[i] += opts.step_size * dq[i];
        }
        iters += 1;
    }
    IkResult {
        q,
        converged,
        iters,
        final_error: final_err,
    }
}

// ── Public entry points ───────────────────────────────────────────────────

pub fn solve_ik_cartpole(
    cfg: &CartpoleConfig,
    link_name: &str,
    q_init: &[f32; 2],
    target: TargetPose,
    opts: IkOptions,
) -> IkResult {
    let b = CartpoleIkBackend { cfg, link_name };
    run_dls(&b, q_init, target, opts)
}

pub fn solve_ik_dp(
    cfg: &DoublePendulumConfig,
    link_name: &str,
    q_init: &[f32; 2],
    target: TargetPose,
    opts: IkOptions,
) -> IkResult {
    let b = DpIkBackend { cfg, link_name };
    run_dls(&b, q_init, target, opts)
}

pub fn solve_ik_planar_chain(
    cfg: &PlanarChainConfig,
    link_index: usize,
    q_init: &[f32],
    target: TargetPose,
    opts: IkOptions,
) -> IkResult {
    let b = PlanarChainIkBackend { cfg, link_index };
    run_dls(&b, q_init, target, opts)
}

// Used by the Rust IK test suite (forward kinematics ground truth).
#[allow(dead_code)]
pub(crate) fn cartpole_link_pose(s: &CartpoleState, link: &str) -> Option<TargetPose> {
    cartpole_pose(s.x, s.theta, link)
}
#[allow(dead_code)]
pub(crate) fn dp_link_pose(
    s: &DoublePendulumState,
    cfg: &DoublePendulumConfig,
    link: &str,
) -> Option<TargetPose> {
    dp_pose(s.q1, s.q2, cfg, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Cartpole IK ──────────────────────────────────────────────────────

    #[test]
    fn cartpole_cart_ik_to_target_x() {
        // Move the cart to x=1.5 (only the slider matters; pole stays put).
        let cfg = CartpoleConfig::default();
        let target = TargetPose {
            x: 1.5,
            z: 0.0,
            theta_y: 0.0,
        };
        let r = solve_ik_cartpole(&cfg, "cart", &[0.0, 0.0], target, IkOptions::default());
        assert!(
            r.converged,
            "cart IK did not converge (err={})",
            r.final_error
        );
        assert!((r.q[0] - 1.5).abs() < 1e-2, "q[0]={}", r.q[0]);
    }

    // ── Double pendulum IK ───────────────────────────────────────────────

    #[test]
    fn dp_link2_tip_ik_reaches_unique_target() {
        // l1=l2=1, workspace reach = 2.0 (singular when fully extended).
        // Aim slightly inside the workspace at (1.9, 0) — well within reach but
        // far enough from singularity that DLS converges cleanly.
        let cfg = DoublePendulumConfig::default();
        let target = TargetPose {
            x: 1.9,
            z: 0.0,
            theta_y: 0.0,
        };
        let r = solve_ik_dp(&cfg, "link2_tip", &[0.5, 0.5], target, IkOptions::default());
        assert!(
            r.converged,
            "DP tip IK did not converge (err={:.6e})",
            r.final_error
        );
        let pose = dp_pose(r.q[0], r.q[1], &cfg, "link2_tip").unwrap();
        assert!((pose.x - target.x).abs() < 1e-2, "x={}", pose.x);
        assert!((pose.z - target.z).abs() < 1e-2, "z={}", pose.z);
    }

    #[test]
    fn dp_link2_tip_ik_reaches_diagonal_target() {
        // Reachable target at (1, -1) (inside workspace; outer reach = 2).
        let cfg = DoublePendulumConfig::default();
        let target = TargetPose {
            x: 1.0,
            z: -1.0,
            theta_y: 0.0,
        };
        let r = solve_ik_dp(&cfg, "link2_tip", &[0.1, 0.1], target, IkOptions::default());
        assert!(
            r.converged,
            "diag IK didn't converge (err={:.6e})",
            r.final_error
        );
        let pose = dp_pose(r.q[0], r.q[1], &cfg, "link2_tip").unwrap();
        assert!((pose.x - 1.0).abs() < 1e-2);
        assert!((pose.z - (-1.0)).abs() < 1e-2);
    }

    #[test]
    fn dp_unreachable_target_does_not_converge() {
        // Workspace outer radius = l1 + l2 = 2. (5, 0) is unreachable.
        let cfg = DoublePendulumConfig::default();
        let target = TargetPose {
            x: 5.0,
            z: 0.0,
            theta_y: 0.0,
        };
        let r = solve_ik_dp(
            &cfg,
            "link2_tip",
            &[0.1, 0.1],
            target,
            IkOptions {
                max_iters: 50,
                ..IkOptions::default()
            },
        );
        // DLS will move toward the boundary but can't reach; not converged.
        assert!(!r.converged, "should NOT converge to an unreachable target");
        assert!(r.final_error > 0.1, "should have a sizable residual");
    }

    // ── Planar chain IK ──────────────────────────────────────────────────

    #[test]
    fn planar_chain_n3_reaches_target_position() {
        // Reach (1, -1) with N=3; redundant 1-DoF chain, should easily converge.
        let cfg = PlanarChainConfig::uniform(3);
        let target = TargetPose {
            x: 1.0,
            z: -1.0,
            theta_y: 0.0,
        };
        let r = solve_ik_planar_chain(
            &cfg,
            2, // target the tip-most link (link3, index 2)
            &[0.1, 0.1, 0.1],
            target,
            IkOptions::default(),
        );
        assert!(
            r.converged,
            "N=3 IK did not converge (err={:.6e})",
            r.final_error
        );
    }

    #[test]
    fn dp_ik_with_orientation_constraint() {
        let cfg = DoublePendulumConfig::default();
        // Want tip at (1.5, -0.5) AND total angle = π/4 (mostly horizontal).
        let target = TargetPose {
            x: 1.5,
            z: -0.5,
            theta_y: std::f32::consts::FRAC_PI_4,
        };
        let r = solve_ik_dp(
            &cfg,
            "link2_tip",
            &[0.5, 0.5],
            target,
            IkOptions {
                include_orientation: true,
                ..IkOptions::default()
            },
        );
        // With orientation in the residual, the problem is fully constrained
        // (2 DOF, 3 constraints — over-determined). Allow larger residual.
        assert!(r.final_error < 0.2, "err={:.6e}", r.final_error);
    }

    // ── Sanity ───────────────────────────────────────────────────────────

    #[test]
    fn ik_does_not_move_when_already_at_target() {
        let cfg = DoublePendulumConfig::default();
        let q_init = [0.3, -0.2];
        let pose = dp_pose(q_init[0], q_init[1], &cfg, "link2_tip").unwrap();
        let r = solve_ik_dp(&cfg, "link2_tip", &q_init, pose, IkOptions::default());
        assert!(r.converged);
        assert!((r.q[0] - q_init[0]).abs() < 1e-3);
        assert!((r.q[1] - q_init[1]).abs() < 1e-3);
        assert!(r.iters <= 1);
    }
}
