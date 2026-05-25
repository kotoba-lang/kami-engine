//! Joint-space trajectory generators — smooth motion between configurations.
//!
//! Standard robotics primitives: cubic and quintic polynomial trajectories
//! between two joint vectors with matching boundary derivatives, and a
//! min-jerk shortcut (quintic with zero boundary accelerations). Plus a
//! waypoint sequencer that strings polynomial segments end-to-end.
//!
//! All trajectories implement `JointTrajectory::sample(t) -> (q, qdot, qddot)`
//! so the upstream ArticulationController (iter 12) can consume them
//! step-by-step. The combination IK (iter 9) → Trajectory (this iter) →
//! Controller (iter 12) closes the full kinematic motion stack.
//!
//! References:
//!   - Spong et al., Robot Modeling and Control §5 (trajectory planning).
//!   - Flash & Hogan 1985 (minimum-jerk).

use serde::{Deserialize, Serialize};

/// Trait for any joint-space trajectory: returns position + velocity +
/// acceleration at time `t` (clamped to `[0, duration()]`).
pub trait JointTrajectory: Send + Sync {
    fn duration(&self) -> f32;
    /// `(q, qdot, qddot)` at time `t`.
    fn sample(&self, t: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>);
    fn dof(&self) -> usize;
}

// ── Cubic polynomial: a0 + a1·t + a2·t² + a3·t³ ──────────────────────────
//
// Boundary conditions: q(0)=q0, q(T)=qf, qdot(0)=qd0, qdot(T)=qdf.
// Coefficients (closed-form):
//   a0 = q0
//   a1 = qd0
//   a2 = (3·(qf−q0) − (2·qd0 + qdf)·T) / T²
//   a3 = (2·(q0−qf) + (qd0 + qdf)·T) / T³

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CubicPolynomialTrajectory {
    pub q0: Vec<f32>,
    pub qf: Vec<f32>,
    pub qd0: Vec<f32>,
    pub qdf: Vec<f32>,
    pub duration: f32,
    /// Per-joint coefficients [a0, a1, a2, a3].
    coeffs: Vec<[f32; 4]>,
}

impl CubicPolynomialTrajectory {
    pub fn new(q0: Vec<f32>, qf: Vec<f32>, qd0: Vec<f32>, qdf: Vec<f32>, duration: f32) -> Self {
        assert_eq!(q0.len(), qf.len());
        assert_eq!(q0.len(), qd0.len());
        assert_eq!(q0.len(), qdf.len());
        assert!(duration > 0.0, "trajectory duration must be positive");
        let t = duration;
        let t2 = t * t;
        let t3 = t2 * t;
        let coeffs = (0..q0.len())
            .map(|i| {
                let a0 = q0[i];
                let a1 = qd0[i];
                let a2 = (3.0 * (qf[i] - q0[i]) - (2.0 * qd0[i] + qdf[i]) * t) / t2;
                let a3 = (2.0 * (q0[i] - qf[i]) + (qd0[i] + qdf[i]) * t) / t3;
                [a0, a1, a2, a3]
            })
            .collect();
        CubicPolynomialTrajectory { q0, qf, qd0, qdf, duration, coeffs }
    }

    /// Stop-to-stop cubic: start and end velocities both zero.
    pub fn stop_to_stop(q0: Vec<f32>, qf: Vec<f32>, duration: f32) -> Self {
        let zeros = vec![0.0; q0.len()];
        Self::new(q0, qf, zeros.clone(), zeros, duration)
    }
}

impl JointTrajectory for CubicPolynomialTrajectory {
    fn duration(&self) -> f32 {
        self.duration
    }
    fn dof(&self) -> usize {
        self.q0.len()
    }
    fn sample(&self, t: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let t = t.clamp(0.0, self.duration);
        let n = self.q0.len();
        let mut q = vec![0.0_f32; n];
        let mut qd = vec![0.0_f32; n];
        let mut qdd = vec![0.0_f32; n];
        for i in 0..n {
            let c = &self.coeffs[i];
            q[i] = c[0] + c[1] * t + c[2] * t * t + c[3] * t * t * t;
            qd[i] = c[1] + 2.0 * c[2] * t + 3.0 * c[3] * t * t;
            qdd[i] = 2.0 * c[2] + 6.0 * c[3] * t;
        }
        (q, qd, qdd)
    }
}

// ── Quintic polynomial (boundary positions + velocities + accelerations) ─
//
// q(t) = a0 + a1·t + a2·t² + a3·t³ + a4·t⁴ + a5·t⁵
//
// With boundary conditions at t=0 and t=T for (q, qdot, qddot), the
// coefficients are:
//   a0 = q0
//   a1 = qd0
//   a2 = qdd0 / 2
//   a3 = (20·(qf−q0) − (8·qdf+12·qd0)·T − (3·qdd0−qddf)·T²) / (2·T³)
//   a4 = (30·(q0−qf) + (14·qdf+16·qd0)·T + (3·qdd0−2·qddf)·T²) / (2·T⁴)
//   a5 = (12·(qf−q0) − 6·(qdf+qd0)·T − (qdd0−qddf)·T²) / (2·T⁵)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuinticPolynomialTrajectory {
    pub q0: Vec<f32>,
    pub qf: Vec<f32>,
    pub qd0: Vec<f32>,
    pub qdf: Vec<f32>,
    pub qdd0: Vec<f32>,
    pub qddf: Vec<f32>,
    pub duration: f32,
    coeffs: Vec<[f32; 6]>,
}

impl QuinticPolynomialTrajectory {
    pub fn new(
        q0: Vec<f32>,
        qf: Vec<f32>,
        qd0: Vec<f32>,
        qdf: Vec<f32>,
        qdd0: Vec<f32>,
        qddf: Vec<f32>,
        duration: f32,
    ) -> Self {
        let n = q0.len();
        assert!(n == qf.len() && n == qd0.len() && n == qdf.len()
                && n == qdd0.len() && n == qddf.len());
        assert!(duration > 0.0);
        let t = duration;
        let t2 = t * t;
        let t3 = t2 * t;
        let t4 = t3 * t;
        let t5 = t4 * t;
        let coeffs = (0..n)
            .map(|i| {
                let a0 = q0[i];
                let a1 = qd0[i];
                let a2 = qdd0[i] / 2.0;
                let a3 = (20.0 * (qf[i] - q0[i])
                    - (8.0 * qdf[i] + 12.0 * qd0[i]) * t
                    - (3.0 * qdd0[i] - qddf[i]) * t2)
                    / (2.0 * t3);
                let a4 = (30.0 * (q0[i] - qf[i])
                    + (14.0 * qdf[i] + 16.0 * qd0[i]) * t
                    + (3.0 * qdd0[i] - 2.0 * qddf[i]) * t2)
                    / (2.0 * t4);
                let a5 = (12.0 * (qf[i] - q0[i])
                    - 6.0 * (qdf[i] + qd0[i]) * t
                    - (qdd0[i] - qddf[i]) * t2)
                    / (2.0 * t5);
                [a0, a1, a2, a3, a4, a5]
            })
            .collect();
        QuinticPolynomialTrajectory { q0, qf, qd0, qdf, qdd0, qddf, duration, coeffs }
    }

    /// Min-jerk trajectory (Flash & Hogan 1985): quintic with all boundary
    /// velocities and accelerations zero. Minimises the integral of squared
    /// jerk over the move duration; widely used for human-like reaching.
    pub fn min_jerk(q0: Vec<f32>, qf: Vec<f32>, duration: f32) -> Self {
        let z = vec![0.0_f32; q0.len()];
        Self::new(q0, qf, z.clone(), z.clone(), z.clone(), z, duration)
    }
}

impl JointTrajectory for QuinticPolynomialTrajectory {
    fn duration(&self) -> f32 {
        self.duration
    }
    fn dof(&self) -> usize {
        self.q0.len()
    }
    fn sample(&self, t: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let t = t.clamp(0.0, self.duration);
        let n = self.q0.len();
        let mut q = vec![0.0_f32; n];
        let mut qd = vec![0.0_f32; n];
        let mut qdd = vec![0.0_f32; n];
        let t2 = t * t;
        let t3 = t2 * t;
        let t4 = t3 * t;
        let t5 = t4 * t;
        for i in 0..n {
            let c = &self.coeffs[i];
            q[i] = c[0] + c[1] * t + c[2] * t2 + c[3] * t3 + c[4] * t4 + c[5] * t5;
            qd[i] = c[1] + 2.0 * c[2] * t + 3.0 * c[3] * t2 + 4.0 * c[4] * t3 + 5.0 * c[5] * t4;
            qdd[i] = 2.0 * c[2] + 6.0 * c[3] * t + 12.0 * c[4] * t2 + 20.0 * c[5] * t3;
        }
        (q, qd, qdd)
    }
}

// ── Waypoint trajectory (sequence of cubic segments) ─────────────────────
//
// Pieces a sequence of joint waypoints with per-segment durations into a
// continuous trajectory. Velocity at intermediate waypoints is computed as
// the centred difference  v_k = (q_{k+1} − q_{k−1}) / (t_k + t_{k+1})
// (Catmull-Rom-style); the endpoints get zero velocity (stop-to-stop).

#[derive(Debug, Clone)]
pub struct WaypointTrajectory {
    segments: Vec<CubicPolynomialTrajectory>,
    cum_t: Vec<f32>, // cumulative segment durations: [0, t0, t0+t1, ...]
    dof: usize,
}

impl WaypointTrajectory {
    pub fn from_waypoints(
        waypoints: Vec<Vec<f32>>,
        segment_durations: Vec<f32>,
    ) -> Self {
        assert!(waypoints.len() >= 2, "need at least 2 waypoints");
        assert_eq!(segment_durations.len(), waypoints.len() - 1);
        let dof = waypoints[0].len();
        assert!(waypoints.iter().all(|w| w.len() == dof));
        assert!(segment_durations.iter().all(|t| *t > 0.0));

        // Centred-difference intermediate velocities.
        let n = waypoints.len();
        let mut vels: Vec<Vec<f32>> = vec![vec![0.0; dof]; n];
        for k in 1..(n - 1) {
            let dt_left = segment_durations[k - 1];
            let dt_right = segment_durations[k];
            for j in 0..dof {
                vels[k][j] =
                    (waypoints[k + 1][j] - waypoints[k - 1][j]) / (dt_left + dt_right);
            }
        }
        // Endpoint velocities = 0 (stop-to-stop overall).
        // (vels[0] and vels[n-1] already 0)

        let mut segments = Vec::with_capacity(n - 1);
        let mut cum_t = Vec::with_capacity(n);
        cum_t.push(0.0);
        let mut acc = 0.0_f32;
        for k in 0..(n - 1) {
            let seg = CubicPolynomialTrajectory::new(
                waypoints[k].clone(),
                waypoints[k + 1].clone(),
                vels[k].clone(),
                vels[k + 1].clone(),
                segment_durations[k],
            );
            segments.push(seg);
            acc += segment_durations[k];
            cum_t.push(acc);
        }

        WaypointTrajectory { segments, cum_t, dof }
    }
}

impl JointTrajectory for WaypointTrajectory {
    fn duration(&self) -> f32 {
        *self.cum_t.last().unwrap()
    }
    fn dof(&self) -> usize {
        self.dof
    }
    fn sample(&self, t: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let t = t.clamp(0.0, self.duration());
        // Binary search the segment index. Linear is fine for small N.
        let mut seg_idx = self.segments.len() - 1;
        for k in 0..self.segments.len() {
            if t <= self.cum_t[k + 1] {
                seg_idx = k;
                break;
            }
        }
        let t_local = t - self.cum_t[seg_idx];
        self.segments[seg_idx].sample(t_local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    // ── Cubic ────────────────────────────────────────────────────────

    #[test]
    fn cubic_boundary_conditions_exact() {
        // q from 0 → π/2 over 2 s, zero boundary velocities.
        let t = CubicPolynomialTrajectory::stop_to_stop(
            vec![0.0], vec![std::f32::consts::FRAC_PI_2], 2.0,
        );
        let (q0, qd0, _) = t.sample(0.0);
        let (qf, qdf, _) = t.sample(2.0);
        assert!(approx(q0[0], 0.0, 1e-6));
        assert!(approx(qf[0], std::f32::consts::FRAC_PI_2, 1e-5));
        assert!(approx(qd0[0], 0.0, 1e-6));
        assert!(approx(qdf[0], 0.0, 1e-5));
    }

    #[test]
    fn cubic_clamps_outside_duration() {
        let t = CubicPolynomialTrajectory::stop_to_stop(vec![0.0], vec![1.0], 2.0);
        let (q_before, _, _) = t.sample(-1.0);
        let (q_after, _, _) = t.sample(5.0);
        assert!(approx(q_before[0], 0.0, 1e-6));
        assert!(approx(q_after[0], 1.0, 1e-5));
    }

    #[test]
    fn cubic_velocity_is_derivative_of_position() {
        let t = CubicPolynomialTrajectory::stop_to_stop(vec![0.0], vec![1.0], 1.0);
        let dt = 1e-4_f32;
        let times: Vec<f32> = vec![0.1, 0.3, 0.5, 0.7, 0.9];
        for tt in times {
            let (q_p, qd_a, _) = t.sample(tt);
            let (q_p2, _, _) = t.sample(tt + dt);
            let fd = (q_p2[0] - q_p[0]) / dt;
            assert!(approx(qd_a[0], fd, 1e-2), "t={tt}: analytic={}, fd={}", qd_a[0], fd);
        }
    }

    #[test]
    fn cubic_with_nonzero_boundary_velocity() {
        let t = CubicPolynomialTrajectory::new(
            vec![0.0], vec![1.0], vec![0.5], vec![-0.3], 1.0,
        );
        let (_, qd0, _) = t.sample(0.0);
        let (_, qdf, _) = t.sample(1.0);
        assert!(approx(qd0[0], 0.5, 1e-5));
        assert!(approx(qdf[0], -0.3, 1e-5));
    }

    // ── Quintic ──────────────────────────────────────────────────────

    #[test]
    fn quintic_boundary_pos_vel_acc_exact() {
        let t = QuinticPolynomialTrajectory::new(
            vec![0.0], vec![1.0],
            vec![0.2], vec![-0.1],
            vec![0.3], vec![-0.4],
            2.0,
        );
        let (q0, qd0, qdd0) = t.sample(0.0);
        let (qf, qdf, qddf) = t.sample(2.0);
        assert!(approx(q0[0], 0.0, 1e-5));
        assert!(approx(qf[0], 1.0, 1e-4));
        assert!(approx(qd0[0], 0.2, 1e-5));
        assert!(approx(qdf[0], -0.1, 1e-4));
        assert!(approx(qdd0[0], 0.3, 1e-4));
        assert!(approx(qddf[0], -0.4, 1e-3));
    }

    #[test]
    fn min_jerk_endpoints_zero_velocity_and_acceleration() {
        let t = QuinticPolynomialTrajectory::min_jerk(vec![0.0], vec![1.0], 1.0);
        let (_, qd0, qdd0) = t.sample(0.0);
        let (_, qdf, qddf) = t.sample(1.0);
        assert!(approx(qd0[0], 0.0, 1e-6));
        assert!(approx(qdf[0], 0.0, 1e-5));
        assert!(approx(qdd0[0], 0.0, 1e-6));
        assert!(approx(qddf[0], 0.0, 1e-5));
    }

    #[test]
    fn min_jerk_midpoint_is_halfway_for_symmetric_move() {
        let t = QuinticPolynomialTrajectory::min_jerk(vec![0.0], vec![1.0], 1.0);
        let (q_half, _, _) = t.sample(0.5);
        // Quintic min-jerk profile passes through 0.5 at t = T/2.
        assert!(approx(q_half[0], 0.5, 1e-4));
    }

    #[test]
    fn quintic_velocity_is_derivative_of_position() {
        let t = QuinticPolynomialTrajectory::min_jerk(vec![0.0], vec![1.0], 1.0);
        let dt = 1e-4_f32;
        for tt in [0.1_f32, 0.3, 0.5, 0.7, 0.9] {
            let (q_p, qd_a, _) = t.sample(tt);
            let (q_p2, _, _) = t.sample(tt + dt);
            let fd = (q_p2[0] - q_p[0]) / dt;
            assert!(approx(qd_a[0], fd, 1e-2), "t={tt}: analytic={}, fd={}", qd_a[0], fd);
        }
    }

    // ── Waypoint sequencer ──────────────────────────────────────────

    #[test]
    fn waypoint_traj_visits_each_waypoint() {
        let waypoints = vec![
            vec![0.0, 0.0],
            vec![1.0, 0.5],
            vec![0.5, -0.3],
            vec![0.0, 0.0],
        ];
        let durations = vec![1.0, 1.0, 1.0];
        let traj = WaypointTrajectory::from_waypoints(waypoints.clone(), durations);
        assert!(approx(traj.duration(), 3.0, 1e-6));
        // Sample at cumulative-time boundaries.
        for (k, t) in [0.0_f32, 1.0, 2.0, 3.0].iter().enumerate() {
            let (q, _, _) = traj.sample(*t);
            for j in 0..2 {
                assert!(
                    approx(q[j], waypoints[k][j], 1e-3),
                    "k={k}, j={j}: got {}, expected {}",
                    q[j], waypoints[k][j]
                );
            }
        }
    }

    #[test]
    fn waypoint_endpoint_velocities_zero() {
        let traj = WaypointTrajectory::from_waypoints(
            vec![vec![0.0], vec![1.0], vec![0.5]],
            vec![1.0, 1.0],
        );
        let (_, qd_start, _) = traj.sample(0.0);
        let (_, qd_end, _) = traj.sample(2.0);
        assert!(approx(qd_start[0], 0.0, 1e-5));
        assert!(approx(qd_end[0], 0.0, 1e-5));
    }

    // ── ArticulationController integration ──────────────────────────

    #[test]
    fn controller_tracks_trajectory_to_target() {
        use crate::controllers::{ArticulationAction, ArticulationController};
        use crate::world::World;
        use kami_articulated::parse_urdf;
        const CARTPOLE_URDF: &str =
            include_str!("../../../../70-tools/e7m-sim/scenes/cartpole/cartpole.urdf");

        let sys = parse_urdf(CARTPOLE_URDF).unwrap();
        let mut w = World::default();
        let h = w.add_articulation(sys).unwrap();
        let mut ctrl = ArticulationController::new(2, 200.0, 20.0, 100.0);

        // Min-jerk traj: cart from 0 → 1.0 m over 3 s; pole stays at 0 target.
        let traj = QuinticPolynomialTrajectory::min_jerk(vec![0.0, 0.0], vec![1.0, 0.0], 3.0);
        let dt = w.dt;
        let n_steps = (traj.duration() / dt).ceil() as usize;
        for k in 0..n_steps {
            let t = k as f32 * dt;
            let (q_target, qd_target, _) = traj.sample(t);
            ctrl.apply_action(
                w.get_mut(h).unwrap(),
                &ArticulationAction {
                    joint_positions: Some(q_target),
                    joint_velocities: Some(qd_target),
                    joint_efforts: None,
                },
            );
            w.step();
        }
        let q = w.get(h).unwrap().joint_positions();
        // Cart reaches the goal within 0.05 m.
        assert!((q[0] - 1.0).abs() < 0.05, "cart at trajectory end: x={}", q[0]);
    }
}
