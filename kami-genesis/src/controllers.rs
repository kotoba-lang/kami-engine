//! ArticulationController — PD / velocity / effort control surface.
//!
//! Mirrors `isaacsim.core.api.controllers.ArticulationController` (Isaac Sim
//! 4.x). Takes an `ArticulationAction` that may specify any combination of
//! joint_positions (PD target), joint_velocities (PD damping target), or
//! joint_efforts (direct + feedforward) and computes per-step torques.
//!
//! Standard PD control law (when joint_positions set):
//!     τ_i = kp_i · (q_target_i − q_i)
//!         + kd_i · (qdot_target_i − qdot_i)
//!         + tau_ff_i        (feedforward effort)
//!
//! When joint_positions is not set but joint_velocities is:
//!     τ_i = kd_i · (qdot_target_i − qdot_i) + tau_ff_i
//!
//! When only joint_efforts is set: pure direct-effort passthrough.
//!
//! The computed torques are clamped to ±max_effort_i then handed to
//! `Articulation::set_joint_torques(...)`.

use crate::world::Articulation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArticulationAction {
    /// Target joint positions for PD control. `None` = position term disabled.
    pub joint_positions: Option<Vec<f32>>,
    /// Target joint velocities. `None` = use 0 with non-zero kd (damping) when
    /// position term is active, or velocity term entirely disabled otherwise.
    pub joint_velocities: Option<Vec<f32>>,
    /// Feedforward efforts (added on top of PD); also direct-effort mode when
    /// positions+velocities are both None.
    pub joint_efforts: Option<Vec<f32>>,
}

impl ArticulationAction {
    pub fn positions(targets: Vec<f32>) -> Self {
        ArticulationAction {
            joint_positions: Some(targets),
            joint_velocities: None,
            joint_efforts: None,
        }
    }
    pub fn velocities(targets: Vec<f32>) -> Self {
        ArticulationAction {
            joint_positions: None,
            joint_velocities: Some(targets),
            joint_efforts: None,
        }
    }
    pub fn efforts(targets: Vec<f32>) -> Self {
        ArticulationAction {
            joint_positions: None,
            joint_velocities: None,
            joint_efforts: Some(targets),
        }
    }
    pub fn empty(dof: usize) -> Self {
        ArticulationAction {
            joint_positions: None,
            joint_velocities: None,
            joint_efforts: Some(vec![0.0; dof]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArticulationController {
    pub kps: Vec<f32>,
    pub kds: Vec<f32>,
    pub max_efforts: Vec<f32>,
    /// Last applied action (for `get_applied_action()` surface).
    last_action: Option<ArticulationAction>,
    /// Most recent per-joint torques computed in apply_action().
    last_torques: Vec<f32>,
}

impl ArticulationController {
    /// Create with uniform gains.
    pub fn new(dof: usize, kp: f32, kd: f32, max_effort: f32) -> Self {
        ArticulationController {
            kps: vec![kp; dof],
            kds: vec![kd; dof],
            max_efforts: vec![max_effort; dof],
            last_action: None,
            last_torques: vec![0.0; dof],
        }
    }

    pub fn set_gains(&mut self, kps: Vec<f32>, kds: Vec<f32>) {
        assert_eq!(kps.len(), kds.len());
        self.kps = kps;
        self.kds = kds;
    }

    pub fn set_max_efforts(&mut self, max_efforts: Vec<f32>) {
        self.max_efforts = max_efforts;
    }

    pub fn get_gains(&self) -> (&[f32], &[f32]) {
        (&self.kps, &self.kds)
    }

    pub fn get_max_efforts(&self) -> &[f32] {
        &self.max_efforts
    }

    pub fn get_applied_action(&self) -> Option<&ArticulationAction> {
        self.last_action.as_ref()
    }

    pub fn get_last_torques(&self) -> &[f32] {
        &self.last_torques
    }

    /// Compute torques from `action` given current joint state, clamp to
    /// `max_efforts`, set them on the articulation, and stash the action +
    /// torques for `get_applied_action()` / `get_last_torques()`.
    pub fn apply_action(&mut self, art: &mut Articulation, action: &ArticulationAction) {
        let q = art.joint_positions();
        let qdot = art.joint_velocities();
        let dof = q.len();
        assert_eq!(self.kps.len(), dof);
        assert_eq!(self.kds.len(), dof);

        let mut tau = vec![0.0_f32; dof];

        let pos_active = action.joint_positions.is_some();
        let vel_active = action.joint_velocities.is_some();

        // Position term: kp · (q_target − q)
        if let Some(pos) = action.joint_positions.as_ref() {
            assert_eq!(pos.len(), dof);
            for i in 0..dof {
                tau[i] += self.kps[i] * (pos[i] - q[i]);
            }
        }

        // Velocity / damping term:
        //   - if vel set: kd · (qdot_target − qdot)
        //   - elif pos set: kd · (0 − qdot)  (passive damping at zero target velocity)
        //   - else: no kd term
        if let Some(vel) = action.joint_velocities.as_ref() {
            assert_eq!(vel.len(), dof);
            for i in 0..dof {
                tau[i] += self.kds[i] * (vel[i] - qdot[i]);
            }
        } else if pos_active {
            for i in 0..dof {
                tau[i] += self.kds[i] * (0.0 - qdot[i]);
            }
        }

        // Effort term: direct + feedforward.
        if let Some(eff) = action.joint_efforts.as_ref() {
            assert_eq!(eff.len(), dof);
            for i in 0..dof {
                tau[i] += eff[i];
            }
        }

        // If NO control term was set (rare but possible), action becomes zero.
        let _ = (pos_active, vel_active); // ensure variables read; silences warnings

        // Clamp to max effort.
        for i in 0..dof {
            let lim = self.max_efforts[i];
            if tau[i] > lim {
                tau[i] = lim;
            }
            if tau[i] < -lim {
                tau[i] = -lim;
            }
        }

        // Apply to articulation + record.
        art.set_joint_torques(&tau);
        self.last_action = Some(action.clone());
        self.last_torques = tau;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartpole::CartpoleState;
    use crate::double_pendulum::DoublePendulumState;
    use crate::world::World;
    use kami_articulated::parse_urdf;

    const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");
    const DP_URDF: &str = include_str!("../../fixtures/double_pendulum/double_pendulum.urdf");

    fn cartpole_world() -> (World, super::super::world::ArticulationHandle) {
        let sys = parse_urdf(CARTPOLE_URDF).unwrap();
        let mut w = World::default();
        let h = w.add_articulation(sys).unwrap();
        (w, h)
    }

    fn dp_world() -> (World, super::super::world::ArticulationHandle) {
        let sys = parse_urdf(DP_URDF).unwrap();
        let mut w = World::new(9.81, 1.0 / 240.0);
        let h = w.add_articulation(sys).unwrap();
        (w, h)
    }

    #[test]
    fn pd_position_drives_cart_to_target_x() {
        // Cart should converge toward x_target = 0.5 under PD position control.
        // Note: the cartpole URDF has the revolute joint with effort=0 so the
        // pole is un-actuated and may swing freely in response to cart motion —
        // we only verify the slider DOF tracks.
        let (mut w, h) = cartpole_world();
        let mut ctrl = ArticulationController::new(
            2, /*kp=*/ 200.0, /*kd=*/ 20.0, /*max=*/ 100.0,
        );
        let action = ArticulationAction::positions(vec![0.5, 0.0]);
        for _ in 0..600 {
            ctrl.apply_action(w.get_mut(h).unwrap(), &action);
            w.step();
        }
        let pos = w.get(h).unwrap().joint_positions();
        assert!((pos[0] - 0.5).abs() < 0.05, "x converged: got {}", pos[0]);
        assert!(pos[0].is_finite() && pos[1].is_finite());
    }

    #[test]
    fn pd_holds_dp_at_horizontal_pose() {
        // Set DP target q = (π/2, 0) (horizontal) and PD-hold against gravity.
        let (mut w, h) = dp_world();
        // Generous gains: gravity torque on horizontal DP is ~mg·l (~10·1 = 10 N·m
        // for link 1 alone; with link 2 hanging it's more). kp = 500 is enough.
        let mut ctrl = ArticulationController::new(2, 500.0, 50.0, 200.0);
        let target = vec![std::f32::consts::FRAC_PI_2, 0.0];
        let action = ArticulationAction::positions(target.clone());
        // Start at target so settle is fast.
        w.get_mut(h)
            .unwrap()
            .set_double_pendulum_state(DoublePendulumState {
                q1: target[0],
                q2: target[1],
                ..Default::default()
            });
        for _ in 0..1200 {
            ctrl.apply_action(w.get_mut(h).unwrap(), &action);
            w.step();
        }
        let pos = w.get(h).unwrap().joint_positions();
        // Held within 0.15 rad (≈8.6°) of target despite gravity.
        assert!((pos[0] - target[0]).abs() < 0.15, "q1 held: got {}", pos[0]);
        assert!(pos[1].abs() < 0.15, "q2 held: got {}", pos[1]);
    }

    #[test]
    fn velocity_action_damps_to_target_velocity() {
        // Use velocity action to drive cart at constant vx ≈ 0.5 m/s.
        let (mut w, h) = cartpole_world();
        let mut ctrl = ArticulationController::new(2, 0.0, 50.0, 100.0);
        // Don't set kp (set to 0 above) so only velocity term applies.
        let action = ArticulationAction::velocities(vec![0.5, 0.0]);
        for _ in 0..400 {
            ctrl.apply_action(w.get_mut(h).unwrap(), &action);
            w.step();
        }
        let vel = w.get(h).unwrap().joint_velocities();
        // x_dot converges to 0.5 within ±0.05
        assert!((vel[0] - 0.5).abs() < 0.05, "x_dot tracked: got {}", vel[0]);
    }

    #[test]
    fn direct_effort_passthrough_no_pd() {
        // With only joint_efforts set, no PD term; torques pass through to
        // articulation directly. Use an effort that should equal x_dot via
        // F = m * a integration: 5 N on 1.1 kg total mass for 1 s → x_dot ≈ 4.5 m/s.
        let (mut w, h) = cartpole_world();
        let mut ctrl = ArticulationController::new(2, 50.0, 10.0, 100.0); // gains ignored here
        let action = ArticulationAction::efforts(vec![5.0, 0.0]);
        let steps = (1.0 / w.dt) as usize;
        for _ in 0..steps {
            ctrl.apply_action(w.get_mut(h).unwrap(), &action);
            w.step();
        }
        let vel = w.get(h).unwrap().joint_velocities();
        assert!(
            vel[0] > 3.0 && vel[0] < 5.5,
            "x_dot from 5 N effort: got {}",
            vel[0]
        );
        let torques = ctrl.get_last_torques();
        assert!(
            (torques[0] - 5.0).abs() < 1e-5,
            "direct passthrough: got {}",
            torques[0]
        );
    }

    #[test]
    fn effort_clamped_to_max() {
        let (mut w, h) = cartpole_world();
        let mut ctrl = ArticulationController::new(2, 50.0, 5.0, 7.0);
        let action = ArticulationAction::efforts(vec![10_000.0, -10_000.0]);
        ctrl.apply_action(w.get_mut(h).unwrap(), &action);
        let torques = ctrl.get_last_torques();
        assert!((torques[0] - 7.0).abs() < 1e-5);
        assert!((torques[1] + 7.0).abs() < 1e-5);
    }

    #[test]
    fn get_applied_action_returns_last() {
        let (mut w, h) = cartpole_world();
        let mut ctrl = ArticulationController::new(2, 50.0, 5.0, 100.0);
        assert!(ctrl.get_applied_action().is_none());
        let action = ArticulationAction::positions(vec![0.3, 0.0]);
        ctrl.apply_action(w.get_mut(h).unwrap(), &action);
        let recalled = ctrl.get_applied_action().unwrap();
        assert_eq!(recalled.joint_positions, action.joint_positions);
    }

    #[test]
    fn pd_with_feedforward_effort() {
        // Combined: PD position target + feedforward gravity-comp effort.
        // For DP horizontal hold, ff_torque approximates gravity comp.
        let (mut w, h) = dp_world();
        let mut ctrl = ArticulationController::new(2, 500.0, 50.0, 200.0);
        let target = vec![std::f32::consts::FRAC_PI_2, 0.0];
        w.get_mut(h)
            .unwrap()
            .set_double_pendulum_state(DoublePendulumState {
                q1: target[0],
                q2: target[1],
                ..Default::default()
            });
        // Feedforward: rough gravity-comp at horizontal link 1.
        // Gravity torque on q1 at q=(π/2, 0): g1 = (m1+m2)·g·l1·sin(π/2) +
        // m2·g·lc2·sin(π/2). With m=1, g=9.81, l1=lc2_offset=1, lc2=0.5:
        // ≈ 2·9.81·0.5 + 1·9.81·0.5 = 14.715
        let action = ArticulationAction {
            joint_positions: Some(target.clone()),
            joint_velocities: None,
            joint_efforts: Some(vec![14.7, 4.9]),
        };
        for _ in 0..600 {
            ctrl.apply_action(w.get_mut(h).unwrap(), &action);
            w.step();
        }
        let pos = w.get(h).unwrap().joint_positions();
        // With FF the steady-state error is smaller than pure PD.
        assert!((pos[0] - target[0]).abs() < 0.1);
    }
}
