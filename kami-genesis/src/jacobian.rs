//! Articulation Jacobians — foundation for IK / operational-space control.
//!
//! For each supported topology + named link, returns the 6×n geometric
//! Jacobian `J(q)` such that the link's spatial twist in world frame equals
//! `J(q) · qdot`, where the twist is `[v_x; v_y; v_z; ω_x; ω_y; ω_z]`.
//!
//! Mirrors the public API of `isaacsim.core.api.Articulation.get_jacobians()`
//! (Isaac Sim 4.x).
//!
//! At R1.1+ the topologies all live in the xz-plane with joint axes along
//! world ±y; the Jacobian rows for `v_y`, `ω_x`, `ω_z` are therefore zero
//! and rows 0 (v_x), 2 (v_z), 4 (ω_y) carry all the structure. Full 6-DoF
//! row layout is kept so the upstream API surface is byte-identical.

use crate::cartpole::CartpoleConfig;
use crate::double_pendulum::DoublePendulumConfig;
use crate::planar_chain::PlanarChainConfig;

/// 6×n geometric Jacobian, row-major `[row][col]`.
///
/// Rows: 0 = v_x, 1 = v_y, 2 = v_z, 3 = ω_x, 4 = ω_y, 5 = ω_z (world frame).
/// Columns: one per DOF of the articulation.
#[derive(Debug, Clone, PartialEq)]
pub struct Jacobian {
    pub rows: [Vec<f32>; 6],
}

impl Jacobian {
    pub fn zeros(n: usize) -> Self {
        Jacobian { rows: [vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]] }
    }
    pub fn cols(&self) -> usize {
        self.rows[0].len()
    }
    /// Convenience: row-major `Vec<f32>` of length 6 * n in row order.
    pub fn flatten(&self) -> Vec<f32> {
        let n = self.cols();
        let mut out = Vec::with_capacity(6 * n);
        for r in 0..6 {
            for c in 0..n {
                out.push(self.rows[r][c]);
            }
        }
        out
    }
}

// ─── Cartpole ─────────────────────────────────────────────────────────────
//
// Cartpole DOFs: q = [x, theta]   (prismatic slider + pole revolute about +y).
// Pole convention (matches URDF + world.rs):
//   pole_link com at world (x + lc·sin(θ), 0, lc·cos(θ))  with lc = 0.25.
//   At θ=0 pole points straight up (+z); positive θ rotates toward +x (this
//   corresponds to rotation about world +y, right-hand rule).
//
// Cart link Jacobian (col 0 = ∂/∂x, col 1 = ∂/∂θ):
//   v_x = [1, 0], v_z = [0, 0], ω_y = [0, 0]
// Pole link Jacobian:
//   v_x = [1, lc·cos(θ)], v_z = [0, -lc·sin(θ)], ω_y = [0, 1]

pub fn cartpole_link_jacobian(theta: f32, link: &str, _cfg: &CartpoleConfig) -> Option<Jacobian> {
    match link {
        "world" => Some(Jacobian::zeros(2)),
        "cart" => {
            let mut j = Jacobian::zeros(2);
            j.rows[0] = vec![1.0, 0.0]; // v_x: ∂x/∂x = 1, ∂x/∂θ = 0
            Some(j)
        }
        "pole_link" => {
            let lc = 0.25_f32;
            let st = theta.sin();
            let ct = theta.cos();
            let mut j = Jacobian::zeros(2);
            // v_x: ∂(x + lc sin θ)/∂x = 1, ∂(...)/∂θ = lc cos θ
            j.rows[0] = vec![1.0, lc * ct];
            // v_z: ∂(lc cos θ)/∂x = 0, ∂(lc cos θ)/∂θ = -lc sin θ
            j.rows[2] = vec![0.0, -lc * st];
            // ω_y: ∂(rotation about +y by θ)/∂x = 0, ∂/∂θ = 1
            j.rows[4] = vec![0.0, 1.0];
            Some(j)
        }
        _ => None,
    }
}

// ─── Double pendulum ──────────────────────────────────────────────────────
//
// Two revolute joints, both about world −y (i.e. q=0 means hanging straight
// down). Conventions and formulas mirror world.rs::double_pendulum_link_state.
//
// link1 com:  (lc1·sin(q1), 0, -lc1·cos(q1))     (lc1 = l1/2)
// link2 com:  (l1·sin(q1) + lc2·sin(q1+q2), 0, -l1·cos(q1) - lc2·cos(q1+q2))
//             (lc2 = l2/2)
//
// Since q rotates about world −y, ω in world frame is along −y. The
// `J_ω_y` row therefore carries a negative sign convention.

pub fn dp_link_jacobian(q1: f32, q2: f32, link: &str, cfg: &DoublePendulumConfig) -> Option<Jacobian> {
    match link {
        "world" => Some(Jacobian::zeros(2)),
        "link1" => {
            let lc1 = cfg.l1 * 0.5;
            let s1 = q1.sin();
            let c1 = q1.cos();
            let mut j = Jacobian::zeros(2);
            // v_x = ∂(lc1 sin q1)/∂q1 = lc1 cos q1; ∂/∂q2 = 0
            j.rows[0] = vec![lc1 * c1, 0.0];
            // v_z = ∂(-lc1 cos q1)/∂q1 = lc1 sin q1; ∂/∂q2 = 0
            j.rows[2] = vec![lc1 * s1, 0.0];
            // ω about world +y = +q_dot since the DP rotates "forward" toward +x;
            // matches Articulation::link_state convention (ang_vel.y = q1_dot).
            j.rows[4] = vec![1.0, 0.0];
            Some(j)
        }
        "link2" => {
            let lc2 = cfg.l2 * 0.5;
            let s1 = q1.sin();
            let c1 = q1.cos();
            let s12 = (q1 + q2).sin();
            let c12 = (q1 + q2).cos();
            let mut j = Jacobian::zeros(2);
            // v_x = ∂(l1 sin q1 + lc2 sin(q1+q2))/∂q1 = l1 cos q1 + lc2 cos(q1+q2)
            //      ∂/∂q2 = lc2 cos(q1+q2)
            j.rows[0] = vec![cfg.l1 * c1 + lc2 * c12, lc2 * c12];
            // v_z = ∂(-l1 cos q1 - lc2 cos(q1+q2))/∂q1 = l1 sin q1 + lc2 sin(q1+q2)
            //      ∂/∂q2 = lc2 sin(q1+q2)
            j.rows[2] = vec![cfg.l1 * s1 + lc2 * s12, lc2 * s12];
            // ω_y = cumulative angular velocity = q1_dot + q2_dot
            j.rows[4] = vec![1.0, 1.0];
            Some(j)
        }
        _ => None,
    }
}

// ─── Planar n-link chain ──────────────────────────────────────────────────
//
// `link_index` is 1-based to match the URDF convention "link1", "link2", ...
// The Jacobian columns are q1, q2, ..., qn (length cfg.n).
//
// p_com_k = Σ_{i=0..k-1} l_i d_i  +  lc_k d_k
// where d_i = (sin θ_i, 0, -cos θ_i), θ_i = Σ_{j<=i} q_j (zero-indexed)
//   and  lc_k = l_k / 2 (uniform rod).
//
// ∂p_com_k.x / ∂q_j = Σ_{i ≥ j, i ≤ k-1} l_i · cos(θ_i)  +  (k ≥ j) lc_k · cos(θ_k)
// ∂p_com_k.z / ∂q_j = Σ_{i ≥ j, i ≤ k-1} l_i · sin(θ_i)  +  (k ≥ j) lc_k · sin(θ_k)
// ∂θ_k / ∂q_j       = (k ≥ j) 1 else 0
//
// Note `link_index` here is 0-based for internal indexing (k = 0 means "link1").

pub fn planar_chain_link_jacobian(
    q: &[f32],
    link_index: usize,
    cfg: &PlanarChainConfig,
) -> Option<Jacobian> {
    let n = cfg.n as usize;
    if link_index >= n {
        return None;
    }
    // Precompute cumulative angles θ_i and trig.
    let mut theta = vec![0.0_f32; n];
    let mut cum = 0.0_f32;
    for i in 0..n {
        cum += q[i];
        theta[i] = cum;
    }
    let mut j = Jacobian::zeros(n);
    for col in 0..n {
        if col > link_index {
            continue; // later joints don't move this link
        }
        // Sum contribution from links col..link_index-1 (full-length intermediate links).
        let mut vx = 0.0_f32;
        let mut vz = 0.0_f32;
        for i in col..link_index {
            vx += cfg.lengths[i] * theta[i].cos();
            vz += cfg.lengths[i] * theta[i].sin();
        }
        // Plus the COM offset of link `link_index`.
        let lc = cfg.lengths[link_index] * 0.5;
        vx += lc * theta[link_index].cos();
        vz += lc * theta[link_index].sin();
        j.rows[0][col] = vx;
        j.rows[2][col] = vz;
        j.rows[4][col] = 1.0; // each preceding joint contributes +1 to cumulative angular velocity
    }
    Some(j)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartpole::CartpoleConfig;
    use crate::double_pendulum::{DoublePendulumConfig, DoublePendulumState};

    /// Finite-difference Jacobian helper: takes a function from `n` joint
    /// positions to a 6-vector (lin_xyz + ang_xyz) and returns the numerical
    /// Jacobian.
    fn finite_diff_jacobian<F>(q: &[f32], n: usize, h: f32, mut f: F) -> Jacobian
    where
        F: FnMut(&[f32]) -> [f32; 6],
    {
        let mut j = Jacobian::zeros(n);
        let f0 = f(q);
        let _ = f0; // base value not strictly needed for central diff
        for col in 0..n {
            let mut q_plus = q.to_vec();
            let mut q_minus = q.to_vec();
            q_plus[col] += h;
            q_minus[col] -= h;
            let fp = f(&q_plus);
            let fm = f(&q_minus);
            for row in 0..6 {
                j.rows[row][col] = (fp[row] - fm[row]) / (2.0 * h);
            }
        }
        j
    }

    fn assert_jacobian_close(a: &Jacobian, b: &Jacobian, tol: f32) {
        assert_eq!(a.cols(), b.cols(), "column count mismatch");
        for row in 0..6 {
            for col in 0..a.cols() {
                let diff = (a.rows[row][col] - b.rows[row][col]).abs();
                assert!(
                    diff < tol,
                    "Jacobian mismatch at ({row}, {col}): analytical={}, numerical={}",
                    a.rows[row][col],
                    b.rows[row][col]
                );
            }
        }
    }

    // ─── Cartpole ────────────────────────────────────────────────────────

    #[test]
    fn cartpole_pole_link_jacobian_matches_finite_diff() {
        let cfg = CartpoleConfig::default();
        let theta = 0.3_f32;
        let lc = 0.25_f32;
        // analytical
        let j = cartpole_link_jacobian(theta, "pole_link", &cfg).unwrap();
        // numerical: pose function of (x, theta)
        let fnum = finite_diff_jacobian(&[0.0, theta], 2, 1e-3, |q| {
            let x = q[0];
            let t = q[1];
            // pole com position + orientation about +y (angle = t)
            [x + lc * t.sin(), 0.0, lc * t.cos(), 0.0, t, 0.0]
        });
        assert_jacobian_close(&j, &fnum, 1e-3);
    }

    #[test]
    fn cartpole_cart_jacobian_translation_only() {
        let cfg = CartpoleConfig::default();
        let j = cartpole_link_jacobian(0.0, "cart", &cfg).unwrap();
        assert_eq!(j.rows[0], vec![1.0, 0.0]);
        assert_eq!(j.rows[2], vec![0.0, 0.0]);
        assert_eq!(j.rows[4], vec![0.0, 0.0]);
    }

    #[test]
    fn cartpole_unknown_link_returns_none() {
        let cfg = CartpoleConfig::default();
        assert!(cartpole_link_jacobian(0.0, "nope", &cfg).is_none());
    }

    // ─── Double pendulum ─────────────────────────────────────────────────

    #[test]
    fn dp_link1_jacobian_matches_finite_diff() {
        let cfg = DoublePendulumConfig::default();
        let q1 = 0.3_f32;
        let q2 = -0.4_f32;
        let j = dp_link_jacobian(q1, q2, "link1", &cfg).unwrap();
        let lc1 = cfg.l1 * 0.5;
        let fnum = finite_diff_jacobian(&[q1, q2], 2, 1e-3, |q| {
            // link1 com pose
            let s1 = q[0].sin();
            let c1 = q[0].cos();
            [lc1 * s1, 0.0, -lc1 * c1, 0.0, q[0], 0.0]
        });
        assert_jacobian_close(&j, &fnum, 1e-3);
    }

    #[test]
    fn dp_link2_jacobian_matches_finite_diff() {
        let cfg = DoublePendulumConfig::default();
        let q1 = 0.5_f32;
        let q2 = 0.2_f32;
        let j = dp_link_jacobian(q1, q2, "link2", &cfg).unwrap();
        let lc2 = cfg.l2 * 0.5;
        let l1 = cfg.l1;
        let fnum = finite_diff_jacobian(&[q1, q2], 2, 1e-3, |q| {
            let s1 = q[0].sin();
            let c1 = q[0].cos();
            let s12 = (q[0] + q[1]).sin();
            let c12 = (q[0] + q[1]).cos();
            [
                l1 * s1 + lc2 * s12, 0.0, -l1 * c1 - lc2 * c12,
                0.0, q[0] + q[1], 0.0,
            ]
        });
        assert_jacobian_close(&j, &fnum, 1e-3);
    }

    #[test]
    fn dp_link1_jacobian_at_zero_simple() {
        let cfg = DoublePendulumConfig::default();
        let j = dp_link_jacobian(0.0, 0.0, "link1", &cfg).unwrap();
        // q1=0: lc1·cos(0) = 0.5, lc1·sin(0) = 0
        assert!((j.rows[0][0] - 0.5).abs() < 1e-6);
        assert!(j.rows[0][1].abs() < 1e-6);
        assert_eq!(j.rows[4], vec![1.0, 0.0]);
    }

    // ─── Planar chain ────────────────────────────────────────────────────

    #[test]
    fn planar_chain_n2_matches_dp_jacobian() {
        // PlanarChainConfig::uniform(2) ≡ DoublePendulumConfig::default()
        let chain_cfg = PlanarChainConfig::uniform(2);
        let dp_cfg = DoublePendulumConfig::default();
        let q = [0.3, -0.2];
        let j_chain = planar_chain_link_jacobian(&q, 1, &chain_cfg).unwrap(); // link2 = index 1
        let j_dp = dp_link_jacobian(q[0], q[1], "link2", &dp_cfg).unwrap();
        assert_jacobian_close(&j_chain, &j_dp, 1e-4);
    }

    #[test]
    fn planar_chain_n3_link3_matches_finite_diff() {
        let cfg = PlanarChainConfig::uniform(3);
        let q = [0.2, -0.3, 0.4];
        let j = planar_chain_link_jacobian(&q, 2, &cfg).unwrap(); // link3 (tip)
        let l = cfg.lengths.clone();
        let lc3 = l[2] * 0.5;
        let fnum = finite_diff_jacobian(&q, 3, 1e-3, |qv| {
            let t0 = qv[0];
            let t1 = t0 + qv[1];
            let t2 = t1 + qv[2];
            // link3 com position
            let px =
                l[0] * t0.sin() + l[1] * t1.sin() + lc3 * t2.sin();
            let pz =
                -l[0] * t0.cos() - l[1] * t1.cos() - lc3 * t2.cos();
            [px, 0.0, pz, 0.0, t2, 0.0]
        });
        assert_jacobian_close(&j, &fnum, 1e-3);
    }

    #[test]
    fn planar_chain_later_joint_doesnt_affect_earlier_link() {
        // Jacobian of link1 wrt q2, q3 should be exactly zero.
        let cfg = PlanarChainConfig::uniform(3);
        let q = [0.5, 0.3, -0.2];
        let j = planar_chain_link_jacobian(&q, 0, &cfg).unwrap();
        for col in 1..3 {
            for row in 0..6 {
                assert!(j.rows[row][col].abs() < 1e-12);
            }
        }
    }

    #[test]
    fn jacobian_flatten_layout_is_row_major() {
        let mut j = Jacobian::zeros(2);
        j.rows[0] = vec![1.0, 2.0];
        j.rows[2] = vec![3.0, 4.0];
        let v = j.flatten();
        assert_eq!(v.len(), 12);
        // row 0: [1, 2]; row 1: [0,0]; row 2: [3,4]; rest zero
        assert_eq!(&v[..2], &[1.0, 2.0]);
        assert_eq!(&v[2..4], &[0.0, 0.0]);
        assert_eq!(&v[4..6], &[3.0, 4.0]);
    }

    // Verify Cartpole DP velocity propagation: J·qdot == link linear/angular velocity
    #[test]
    fn dp_jacobian_times_qdot_matches_link_state_velocity() {
        use crate::double_pendulum::DoublePendulumState;
        use crate::world::{World};
        use kami_articulated::parse_urdf;
        const DP_URDF: &str = include_str!(
            "../../../../70-tools/e7m-sim/scenes/double_pendulum/double_pendulum.urdf"
        );

        let q1 = 0.4_f32;
        let q2 = -0.3_f32;
        let qd1 = 0.7_f32;
        let qd2 = -0.5_f32;
        let cfg = DoublePendulumConfig::default();
        let j = dp_link_jacobian(q1, q2, "link2", &cfg).unwrap();
        // J · qdot
        let vx = j.rows[0][0] * qd1 + j.rows[0][1] * qd2;
        let vz = j.rows[2][0] * qd1 + j.rows[2][1] * qd2;
        let wy = j.rows[4][0] * qd1 + j.rows[4][1] * qd2;

        let sys = parse_urdf(DP_URDF).unwrap();
        let mut world = World::new(9.81, 1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        world.get_mut(h).unwrap().set_double_pendulum_state(DoublePendulumState {
            q1, q2, q1_dot: qd1, q2_dot: qd2,
        });
        let ls = world.get(h).unwrap().link_state("link2").unwrap();
        assert!((ls.linear_velocity.x - vx).abs() < 1e-5,
                "vx: link_state={}, J*qdot={}", ls.linear_velocity.x, vx);
        assert!((ls.linear_velocity.z - vz).abs() < 1e-5,
                "vz: link_state={}, J*qdot={}", ls.linear_velocity.z, vz);
        assert!((ls.angular_velocity.y - wy).abs() < 1e-5,
                "wy: link_state={}, J*qdot={}", ls.angular_velocity.y, wy);
    }
}
