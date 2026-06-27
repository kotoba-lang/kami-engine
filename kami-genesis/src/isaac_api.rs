//! `isaac_api` — clean-room `isaacsim.core.api` surface over kami-genesis.
//!
//! ADR-2605261800 §D10.1 / §2(b) N1..N9 NEVER: this is a **clean-room API
//! mirror**. Method names and call/return shapes match the public, documented
//! surface of NVIDIA Isaac Sim 4.x (`isaacsim.core.api.World` +
//! `isaacsim.core.api.articulations.Articulation`) so application code written
//! against Isaac runs unchanged — but NO NVIDIA library, header, or binary is
//! linked or referenced. All dynamics are solved by the KAMI-native
//! reduced-coordinate solver in `world` / `planar_chain` / `cartpole` / etc.
//!
//! Mirrored surface (Isaac 4.x names, batched-array semantics):
//!   - `World::new(physics_dt)` ~ `isaacsim.core.api.World(physics_dt=…)`
//!   - `World::add_articulation` ~ `world.scene.add(Articulation(...))`
//!   - `World::reset` ~ `world.reset()`
//!   - `World::step` ~ `world.step(render=False)`
//!   - `ArticulationView::get_joint_positions` → `[n_dof]` (Isaac returns
//!     `[num_envs, n_dof]`; single-env here is the `[n_dof]` row)
//!   - `…::get_joint_velocities`
//!   - `…::set_joint_efforts(efforts)` ~ `apply_action(ArticulationAction(...))`
//!   - `…::set_joint_positions(positions)` / `set_joint_velocities(velocities)`
//!     (seed/teleport state — the RL `reset()`-to-distribution path)
//!   - `World::{current_time, current_time_step_index, get_physics_dt}`
//!   - `articulation.get_articulation_controller()` → PD/velocity/effort drive
//!     (`apply_action(ArticulationAction)` + `set_gains` / `set_max_efforts`)
//!   - `…::get_jacobians()` → `[6, n_dof]` per link (Isaac:
//!     `[num_envs, num_links, 6, n_dof]`)
//!   - `…::get_world_poses(link)` → `(pos[3], quat_wxyz[4])`
//!
//! Single-environment scope at R1.1 (num_envs = 1). Batched multi-env is a
//! WGSL-backed R1.x extension (see `vectorized`).

use crate::controllers::{ArticulationAction, ArticulationController};
use crate::jacobian::Jacobian;
use crate::world::{Articulation, ArticulationHandle, LinkState, World, WorldError};
use kami_articulated::ArticulatedSystem;
use std::collections::HashMap;

/// Clean-room mirror of `isaacsim.core.api.World`.
///
/// Wraps the kami-genesis `World` and exposes the Isaac-shaped lifecycle
/// (`reset` / `step`) plus an `ArticulationView`-style accessor per handle.
pub struct IsaacWorld {
    inner: World,
    /// Snapshot taken at `reset()` so a subsequent `reset()` restores it,
    /// matching Isaac's `world.reset()` returning to the registered default
    /// joint state. (R1.1: zero state per topology; recorded handles only.)
    registered: Vec<ArticulationHandle>,
    /// `physics_dt` passed at construction; exposed via `get_physics_dt()`.
    physics_dt: f32,
    /// Number of `step()`s since the last `reset()`. Mirrors Isaac's
    /// `world.current_time_step_index`.
    step_index: usize,
    /// One `ArticulationController` per registered prim, created on `add`.
    /// Mirrors Isaac's `articulation.get_articulation_controller()`.
    controllers: HashMap<ArticulationHandle, ArticulationController>,
}

impl IsaacWorld {
    /// `isaacsim.core.api.World(physics_dt=physics_dt)` — gravity defaults to
    /// 9.81 m/s² along -z, the Isaac default.
    pub fn new(physics_dt: f32) -> Self {
        IsaacWorld {
            inner: World::new(9.81, physics_dt),
            registered: Vec::new(),
            physics_dt,
            step_index: 0,
            controllers: HashMap::new(),
        }
    }

    /// `world.get_physics_dt()` — the physics timestep in seconds.
    pub fn get_physics_dt(&self) -> f32 {
        self.physics_dt
    }

    /// `world.current_time_step_index` — number of physics steps taken since
    /// the last `reset()`.
    pub fn current_time_step_index(&self) -> usize {
        self.step_index
    }

    /// `world.current_time` — elapsed simulated time in seconds since the last
    /// `reset()` (`current_time_step_index * physics_dt`).
    pub fn current_time(&self) -> f32 {
        self.step_index as f32 * self.physics_dt
    }

    /// `world.scene.add(Articulation(urdf/usd))` — register an articulation
    /// built from an `ArticulatedSystem`. Returns the prim handle.
    pub fn add_articulation(
        &mut self,
        sys: ArticulatedSystem,
    ) -> Result<ArticulationHandle, WorldError> {
        let h = self.inner.add_articulation(sys)?;
        // Create the articulation's controller, sized to its DOF, and seed its
        // drive parameters from the URDF the way Isaac loads them from the
        // USD/URDF drive API: per-DOF effort limit → `max_efforts`, joint
        // damping → `kd`. Stiffness (`kp`) has no standard URDF field, so it
        // stays 0 until `set_gains(...)` (or, later, a USD drive `stiffness`).
        // A URDF effort limit of 0 is treated as "unspecified" → no clamp.
        let mut ctrl = ArticulationController::new(0, 0.0, 0.0, f32::MAX);
        if let Ok(a) = self.inner.get(h) {
            let params = a.dof_drive_params();
            let dof = params.len();
            let max_efforts = params
                .iter()
                .map(|&(effort, _)| if effort > 0.0 { effort } else { f32::MAX })
                .collect();
            let kds = params.iter().map(|&(_, damping)| damping).collect();
            ctrl = ArticulationController::new(dof, 0.0, 0.0, f32::MAX);
            ctrl.set_max_efforts(max_efforts);
            ctrl.set_gains(vec![0.0; dof], kds);
        }
        self.controllers.insert(h, ctrl);
        self.registered.push(h);
        Ok(h)
    }

    /// `articulation.get_articulation_controller()` — borrow the PD/effort
    /// controller bound to one prim. Drives via `apply_action`, configurable
    /// via `set_gains` / `set_max_efforts`. None if the handle is unknown.
    pub fn get_articulation_controller(
        &mut self,
        h: ArticulationHandle,
    ) -> Option<ArticulationControllerView<'_>> {
        // Disjoint field borrows: `controllers` and `inner` are distinct fields,
        // so both may be borrowed mutably for the returned view's lifetime.
        let ctrl = self.controllers.get_mut(&h)?;
        let art = self.inner.get_mut(h).ok()?;
        Some(ArticulationControllerView { ctrl, art })
    }

    /// `world.step(render=False)` — advance physics by one `physics_dt`.
    pub fn step(&mut self) {
        self.inner.step();
        self.step_index += 1;
    }

    /// `world.reset()` — zero all registered articulations' joint state.
    /// (Isaac restores the registered default; R1.1 default is the zero pose.)
    pub fn reset(&mut self) {
        for &h in &self.registered {
            if let Ok(a) = self.inner.get_mut(h) {
                a.reset_to_zero();
            }
        }
        self.step_index = 0;
    }

    /// Borrow an Isaac-shaped `ArticulationView` for one prim.
    pub fn articulation(&self, h: ArticulationHandle) -> Option<ArticulationView<'_>> {
        self.inner.get(h).ok().map(|_| ArticulationView {
            world: &self.inner,
            h,
        })
    }

    /// Mutable view (needed for `set_joint_efforts`).
    pub fn articulation_mut(&mut self, h: ArticulationHandle) -> Option<ArticulationViewMut<'_>> {
        // Validate handle first to keep the Option contract honest.
        if self.inner.get(h).is_err() {
            return None;
        }
        Some(ArticulationViewMut {
            world: &mut self.inner,
            h,
        })
    }

    /// Escape hatch: the underlying kami-genesis world (non-Isaac surface).
    pub fn kami_world(&self) -> &World {
        &self.inner
    }
}

/// Clean-room mirror of `isaacsim.core.api.articulations.Articulation`
/// (read accessors). Isaac returns `[num_envs, n_dof]`; single-env → `[n_dof]`.
pub struct ArticulationView<'a> {
    world: &'a World,
    h: ArticulationHandle,
}

impl ArticulationView<'_> {
    /// `articulation.get_joint_positions()` → `[n_dof]`.
    pub fn get_joint_positions(&self) -> Vec<f32> {
        self.world
            .get(self.h)
            .map(|a| a.joint_positions())
            .unwrap_or_default()
    }

    /// `articulation.get_joint_velocities()` → `[n_dof]`.
    pub fn get_joint_velocities(&self) -> Vec<f32> {
        self.world
            .get(self.h)
            .map(|a| a.joint_velocities())
            .unwrap_or_default()
    }

    /// `articulation.num_dof` (property).
    pub fn num_dof(&self) -> usize {
        self.get_joint_positions().len()
    }

    /// `articulation.dof_names` (property) — ordered actuated-joint names, one
    /// per DOF, aligned with `get_joint_positions()`.
    pub fn dof_names(&self) -> Vec<String> {
        self.world
            .get(self.h)
            .map(|a| a.dof_names())
            .unwrap_or_default()
    }

    /// `articulation.get_dof_index(dof_name)` — DOF index for a joint name, or
    /// None if it is not an actuated joint of this articulation.
    pub fn get_dof_index(&self, dof_name: &str) -> Option<usize> {
        self.world
            .get(self.h)
            .ok()
            .and_then(|a| a.dof_index(dof_name))
    }

    /// `articulation.get_dof_limits()` — per-DOF position limits `[lower, upper]`
    /// aligned to `dof_names()` / the joint-position array.
    pub fn get_dof_limits(&self) -> Vec<[f32; 2]> {
        self.world
            .get(self.h)
            .map(|a| a.dof_limits().iter().map(|&(l, u)| [l, u]).collect())
            .unwrap_or_default()
    }

    /// `articulation.get_jacobians()` for a named link → `[6, n_dof]`.
    /// Isaac returns `[num_envs, num_links, 6, n_dof]`; this is one link, one
    /// env. None if the link is not part of the articulation.
    pub fn get_jacobian(&self, link_name: &str) -> Option<Jacobian> {
        self.world
            .get(self.h)
            .ok()
            .and_then(|a| a.jacobian(link_name))
    }

    /// `RigidPrimView.get_world_poses(link)` → `(position[3], quat_wxyz[4])`.
    /// Isaac quaternion order is (w, x, y, z).
    pub fn get_world_pose(&self, link_name: &str) -> Option<([f32; 3], [f32; 4])> {
        self.world
            .get(self.h)
            .ok()
            .and_then(|a| a.link_state(link_name))
            .map(|ls: LinkState| {
                let p = ls.position;
                let q = ls.orientation;
                ([p.x, p.y, p.z], [q.w, q.x, q.y, q.z])
            })
    }

    /// `RigidPrimView.get_velocities(link)` → `(linear[3], angular[3])` in the
    /// world frame (Isaac Sim 4.x). Pairs with `get_world_pose` to feed a body's
    /// full kinematic state to a downstream sensor (e.g. an IMU). `None` if the
    /// link is not part of this articulation.
    pub fn get_world_velocity(&self, link_name: &str) -> Option<([f32; 3], [f32; 3])> {
        self.world
            .get(self.h)
            .ok()
            .and_then(|a| a.link_state(link_name))
            .map(|ls: LinkState| {
                let v = ls.linear_velocity;
                let w = ls.angular_velocity;
                ([v.x, v.y, v.z], [w.x, w.y, w.z])
            })
    }
}

/// Mutable Isaac articulation view: effort/action application.
pub struct ArticulationViewMut<'a> {
    world: &'a mut World,
    h: ArticulationHandle,
}

impl ArticulationViewMut<'_> {
    /// `articulation.set_joint_efforts(efforts)` — one torque/force per DOF,
    /// consumed on the next `world.step()` (PxArticulation drive semantics).
    /// Equivalent to `apply_action(ArticulationAction(joint_efforts=...))`.
    pub fn set_joint_efforts(&mut self, efforts: &[f32]) {
        if let Ok(a) = self.world.get_mut(self.h) {
            a.set_joint_torques(efforts);
        }
    }

    /// `articulation.set_joint_positions(positions)` — seed/teleport the joint
    /// positions (velocities untouched). The RL `reset()`-to-distribution path.
    /// DOF order matches `get_joint_positions()`; entries beyond `num_dof` are
    /// ignored, missing DOFs keep their current value.
    pub fn set_joint_positions(&mut self, positions: &[f32]) {
        if let Ok(a) = self.world.get_mut(self.h) {
            a.set_joint_positions(positions);
        }
    }

    /// `articulation.set_joint_velocities(velocities)` — set the joint
    /// velocities (positions untouched). DOF order matches
    /// `get_joint_velocities()`.
    pub fn set_joint_velocities(&mut self, velocities: &[f32]) {
        if let Ok(a) = self.world.get_mut(self.h) {
            a.set_joint_velocities(velocities);
        }
    }
}

/// Clean-room mirror of `isaacsim.core.api.controllers.ArticulationController`
/// bound to one prim. Holds disjoint mutable borrows of the controller and the
/// articulation, so `apply_action` can read joint state and write torques in
/// one call — exactly Isaac's `get_articulation_controller().apply_action(...)`.
pub struct ArticulationControllerView<'a> {
    ctrl: &'a mut ArticulationController,
    art: &'a mut Articulation,
}

impl ArticulationControllerView<'_> {
    /// `controller.apply_action(ArticulationAction(...))` — compute PD + velocity
    /// + feedforward-effort torques from the current joint state, clamp to the
    /// effort limit, and stage them for the next `world.step()`.
    pub fn apply_action(&mut self, action: &ArticulationAction) {
        self.ctrl.apply_action(self.art, action);
    }

    /// `controller.set_gains(kps, kds)` — per-DOF PD stiffness/damping.
    pub fn set_gains(&mut self, kps: Vec<f32>, kds: Vec<f32>) {
        self.ctrl.set_gains(kps, kds);
    }

    /// `controller.set_max_efforts(max_efforts)` — per-DOF torque/force clamp.
    pub fn set_max_efforts(&mut self, max_efforts: Vec<f32>) {
        self.ctrl.set_max_efforts(max_efforts);
    }

    /// `controller.get_gains()` → `(kps, kds)`.
    pub fn get_gains(&self) -> (&[f32], &[f32]) {
        self.ctrl.get_gains()
    }

    /// `controller.get_max_efforts()` → per-DOF torque/force clamp.
    pub fn get_max_efforts(&self) -> &[f32] {
        self.ctrl.get_max_efforts()
    }

    /// `controller.get_applied_action()` — the last action handed to
    /// `apply_action`, or None if none yet.
    pub fn get_applied_action(&self) -> Option<&ArticulationAction> {
        self.ctrl.get_applied_action()
    }

    /// The per-DOF torques computed by the most recent `apply_action` (post
    /// PD law and effort clamp). Useful for logging / reward shaping.
    pub fn get_applied_joint_efforts(&self) -> &[f32] {
        self.ctrl.get_last_torques()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");
    const ARM3_URDF: &str = include_str!("../../fixtures/arm3/arm3.urdf");
    const ARM6_URDF: &str = include_str!("../../fixtures/giemon_arm6/giemon_arm6.urdf");

    #[test]
    fn isaac_world_cartpole_lifecycle() {
        // Mirrors a minimal Isaac script:
        //   world = World(physics_dt=1/60)
        //   art = world.scene.add(Articulation(cartpole))
        //   world.reset(); art.set_joint_efforts([F, 0]); world.step()
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();

        // Isaac cartpole DOF order = [cart_slider, pole_revolute].
        assert_eq!(world.articulation(h).unwrap().num_dof(), 2);
        world.reset();

        let q0 = world.articulation(h).unwrap().get_joint_positions();
        for _ in 0..30 {
            world
                .articulation_mut(h)
                .unwrap()
                .set_joint_efforts(&[10.0, 0.0]);
            world.step();
        }
        let q1 = world.articulation(h).unwrap().get_joint_positions();
        // A steady push moves the cart (DOF 0) along +x.
        assert!(q1[0] > q0[0] + 0.01, "cart did not move: {q0:?} -> {q1:?}");
        assert!(q1.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn isaac_world_pose_and_jacobian_shapes() {
        let sys = kami_articulated::parse_urdf(ARM3_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        let view = world.articulation(h).unwrap();

        // num_dof matches the 3-revolute chain.
        assert_eq!(view.num_dof(), 3);

        // get_world_pose returns (pos[3], quat_wxyz[4]); quat is unit-norm.
        let (pos, quat) = view.get_world_pose("l1").expect("l1 pose");
        let qn =
            (quat[0] * quat[0] + quat[1] * quat[1] + quat[2] * quat[2] + quat[3] * quat[3]).sqrt();
        assert!((qn - 1.0).abs() < 1e-4, "quat not unit: {quat:?}");
        // At rest the first link hangs down (-z); x≈0.
        assert!(pos[0].abs() < 1e-4);
        assert!(pos[2] < 0.0);

        // get_jacobian returns a [6, n_dof] Jacobian.
        let j = view.get_jacobian("l3").expect("l3 jacobian");
        assert_eq!(j.cols(), 3);
        assert_eq!(j.rows.len(), 6);

        // Unknown link → None (Isaac raises; we return Option).
        assert!(view.get_world_pose("nope").is_none());
        assert!(view.get_jacobian("nope").is_none());
    }

    #[test]
    fn isaac_world_drives_6dof_arm_via_spatial_solver() {
        // The 6-DOF giemon arm (no special topology) routes through the
        // Spatial3d fallback and is fully usable via the Isaac surface:
        //   world.scene.add(Articulation(arm6)); set_joint_efforts; step;
        //   get_joint_positions / get_world_pose / get_jacobian.
        let sys = kami_articulated::parse_urdf(ARM6_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();

        assert_eq!(world.articulation(h).unwrap().num_dof(), 6, "6-DOF arm");

        // Drive joint 1 (base yaw) with a steady effort; it must rotate.
        let q0 = world.articulation(h).unwrap().get_joint_positions();
        for _ in 0..60 {
            world
                .articulation_mut(h)
                .unwrap()
                .set_joint_efforts(&[5.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
            world.step();
        }
        let q1 = world.articulation(h).unwrap().get_joint_positions();
        assert!(q1.iter().all(|v| v.is_finite()));
        assert!(
            (q1[0] - q0[0]).abs() > 1e-3,
            "base joint did not move: {q0:?} -> {q1:?}"
        );

        // Isaac-shaped accessors on a named link.
        let view = world.articulation(h).unwrap();
        let (_pos, quat) = view.get_world_pose("link6").expect("link6 pose");
        let qn = (quat.iter().map(|c| c * c).sum::<f32>()).sqrt();
        assert!((qn - 1.0).abs() < 1e-4, "quat not unit: {quat:?}");
        let j = view.get_jacobian("link6").expect("link6 jacobian");
        assert_eq!(j.rows.len(), 6);
        assert_eq!(j.cols(), 6);
    }

    #[test]
    fn isaac_world_velocity_accessor_tracks_motion() {
        // get_world_velocity returns the link's world linear+angular velocity.
        // Drive the cartpole and confirm the cart link's linear velocity grows
        // along +x while at rest it is zero.
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();

        let (v0, _w0) = world
            .articulation(h)
            .unwrap()
            .get_world_velocity("cart")
            .expect("cart vel");
        assert!(v0[0].abs() < 1e-6, "cart should start at rest: {v0:?}");

        for _ in 0..20 {
            world
                .articulation_mut(h)
                .unwrap()
                .set_joint_efforts(&[8.0, 0.0]);
            world.step();
        }
        let (v1, _w1) = world
            .articulation(h)
            .unwrap()
            .get_world_velocity("cart")
            .unwrap();
        assert!(
            v1[0] > 0.01,
            "cart linear velocity should grow under +x force: {v1:?}"
        );
        assert!(v1.iter().all(|c| c.is_finite()));
        // Unknown link → None, matching get_world_pose.
        assert!(
            world
                .articulation(h)
                .unwrap()
                .get_world_velocity("nope")
                .is_none()
        );
    }

    #[test]
    fn isaac_world_reset_zeros_state() {
        let sys = kami_articulated::parse_urdf(ARM3_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        // Drive away from zero, then reset must restore the zero pose.
        for _ in 0..20 {
            world
                .articulation_mut(h)
                .unwrap()
                .set_joint_efforts(&[5.0, 0.0, 0.0]);
            world.step();
        }
        assert!(world.articulation(h).unwrap().get_joint_positions()[0].abs() > 1e-3);
        world.reset();
        let q = world.articulation(h).unwrap().get_joint_positions();
        let qd = world.articulation(h).unwrap().get_joint_velocities();
        assert!(q.iter().all(|v| v.abs() < 1e-6), "reset q not zero: {q:?}");
        assert!(
            qd.iter().all(|v| v.abs() < 1e-6),
            "reset qdot not zero: {qd:?}"
        );
    }

    #[test]
    fn isaac_set_joint_state_seeds_pose_and_velocity() {
        // Mirrors the RL reset()-to-distribution path:
        //   art.set_joint_positions([...]); art.set_joint_velocities([...])
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();

        // Seed an off-zero pole angle and a cart velocity.
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_positions(&[0.2, 0.15]);
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_velocities(&[0.5, 0.0]);

        let q = world.articulation(h).unwrap().get_joint_positions();
        let qd = world.articulation(h).unwrap().get_joint_velocities();
        assert!(
            (q[0] - 0.2).abs() < 1e-6 && (q[1] - 0.15).abs() < 1e-6,
            "pose not set: {q:?}"
        );
        assert!((qd[0] - 0.5).abs() < 1e-6, "vel not set: {qd:?}");

        // set_joint_positions must NOT clobber the velocity we just set.
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_positions(&[0.3, 0.0]);
        let qd2 = world.articulation(h).unwrap().get_joint_velocities();
        assert!(
            (qd2[0] - 0.5).abs() < 1e-6,
            "set_joint_positions clobbered velocity: {qd2:?}"
        );

        // A short/empty array leaves the missing DOFs unchanged.
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_positions(&[0.9]);
        let q3 = world.articulation(h).unwrap().get_joint_positions();
        assert!(
            (q3[0] - 0.9).abs() < 1e-6 && (q3[1] - 0.0).abs() < 1e-6,
            "partial set wrong: {q3:?}"
        );
    }

    #[test]
    fn isaac_set_joint_state_on_spatial_arm() {
        // The generic setters work on the Spatial3d fallback (6-DOF arm) too.
        let sys = kami_articulated::parse_urdf(ARM6_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();

        let target = [0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_positions(&target);
        let q = world.articulation(h).unwrap().get_joint_positions();
        for (got, want) in q.iter().zip(target.iter()) {
            assert!((got - want).abs() < 1e-6, "arm pose not set: {q:?}");
        }
    }

    #[test]
    fn isaac_world_tracks_time_and_step_index() {
        // Mirrors world.current_time / current_time_step_index / get_physics_dt.
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let dt = 1.0 / 60.0;
        let mut world = IsaacWorld::new(dt);
        let h = world.add_articulation(sys).unwrap();

        assert!((world.get_physics_dt() - dt).abs() < 1e-9);
        assert_eq!(world.current_time_step_index(), 0);
        assert!(world.current_time().abs() < 1e-9);

        for _ in 0..10 {
            world
                .articulation_mut(h)
                .unwrap()
                .set_joint_efforts(&[1.0, 0.0]);
            world.step();
        }
        assert_eq!(world.current_time_step_index(), 10);
        assert!(
            (world.current_time() - 10.0 * dt).abs() < 1e-6,
            "time: {}",
            world.current_time()
        );

        // reset() rewinds the clock to zero, matching Isaac.
        world.reset();
        assert_eq!(world.current_time_step_index(), 0);
        assert!(world.current_time().abs() < 1e-9);
    }

    #[test]
    fn isaac_controller_pd_drives_cart_to_target() {
        // Mirrors the canonical Isaac robot-control loop:
        //   ctrl = art.get_articulation_controller()
        //   ctrl.set_gains(kps, kds)
        //   ctrl.apply_action(ArticulationAction(joint_positions=[...]))
        //   world.step()
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();

        let action = ArticulationAction::positions(vec![0.5, 0.0]);
        for _ in 0..600 {
            {
                let mut ctrl = world.get_articulation_controller(h).unwrap();
                ctrl.set_gains(vec![200.0, 0.0], vec![20.0, 0.0]);
                ctrl.apply_action(&action);
            }
            world.step();
        }
        let q = world.articulation(h).unwrap().get_joint_positions();
        assert!(
            (q[0] - 0.5).abs() < 0.05,
            "PD did not reach target x: {q:?}"
        );
    }

    #[test]
    fn isaac_controller_reports_applied_action_and_efforts() {
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();

        // Default gains are zero → effort action passes straight through.
        let mut ctrl = world.get_articulation_controller(h).unwrap();
        assert!(ctrl.get_applied_action().is_none());
        ctrl.set_max_efforts(vec![100.0, 100.0]);
        ctrl.apply_action(&ArticulationAction::efforts(vec![7.0, 0.0]));

        let recalled = ctrl.get_applied_action().expect("action recorded");
        assert_eq!(recalled.joint_efforts.as_deref(), Some(&[7.0, 0.0][..]));
        // Zero gains → torque == feedforward effort, unclamped here.
        assert!((ctrl.get_applied_joint_efforts()[0] - 7.0).abs() < 1e-5);

        let (kps, kds) = ctrl.get_gains();
        assert_eq!(kps, &[0.0, 0.0]);
        assert_eq!(kds, &[0.0, 0.0]);
    }

    #[test]
    fn isaac_controller_unknown_handle_is_none() {
        let mut world = IsaacWorld::new(1.0 / 60.0);
        assert!(
            world
                .get_articulation_controller(ArticulationHandle(99))
                .is_none()
        );
    }

    #[test]
    fn isaac_dof_names_align_with_joint_positions() {
        // Mirrors articulation.dof_names + get_dof_index. Cartpole DOF order is
        // [slider_to_cart (prismatic), cart_to_pole (revolute)].
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();
        let view = world.articulation(h).unwrap();

        let names = view.dof_names();
        assert_eq!(
            names,
            vec!["slider_to_cart".to_string(), "cart_to_pole".to_string()]
        );
        // dof_names length must equal num_dof / the joint-position array length.
        assert_eq!(names.len(), view.num_dof());
        assert_eq!(names.len(), view.get_joint_positions().len());

        // get_dof_index round-trips against dof_names order.
        assert_eq!(view.get_dof_index("slider_to_cart"), Some(0));
        assert_eq!(view.get_dof_index("cart_to_pole"), Some(1));
        assert_eq!(view.get_dof_index("nonexistent"), None);
    }

    #[test]
    fn isaac_dof_limits_come_from_the_urdf() {
        // Cartpole URDF: slider ∈ [-2.4, 2.4], pole ∈ [-π, π].
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();
        let lim = world.articulation(h).unwrap().get_dof_limits();
        assert_eq!(lim.len(), 2);
        assert!(
            (lim[0][0] + 2.4).abs() < 1e-3 && (lim[0][1] - 2.4).abs() < 1e-3,
            "slider: {:?}",
            lim[0]
        );
        assert!(
            (lim[1][0] + 3.14159).abs() < 1e-3 && (lim[1][1] - 3.14159).abs() < 1e-3,
            "pole: {:?}",
            lim[1]
        );
    }

    #[test]
    fn isaac_dof_index_lets_you_target_a_named_joint() {
        // The point of get_dof_index: drive a joint by name without hardcoding
        // its position in the action array. Push the cart via its named DOF.
        let sys = kami_articulated::parse_urdf(ARM6_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();

        let names = world.articulation(h).unwrap().dof_names();
        assert_eq!(names, vec!["j1", "j2", "j3", "j4", "j5", "j6"]);

        let j4 = world
            .articulation(h)
            .unwrap()
            .get_dof_index("j4")
            .expect("j4 exists");
        assert_eq!(j4, 3);

        // Build an effort vector that actuates only the named joint.
        let n = world.articulation(h).unwrap().num_dof();
        let mut efforts = vec![0.0; n];
        efforts[j4] = 4.0;
        let q0 = world.articulation(h).unwrap().get_joint_positions();
        for _ in 0..60 {
            world
                .articulation_mut(h)
                .unwrap()
                .set_joint_efforts(&efforts);
            world.step();
        }
        let q1 = world.articulation(h).unwrap().get_joint_positions();
        assert!(
            (q1[j4] - q0[j4]).abs() > 1e-3,
            "named joint j4 did not move: {q0:?} -> {q1:?}"
        );
    }

    #[test]
    fn isaac_controller_seeds_drive_params_from_urdf() {
        // The auto-created controller picks up the URDF effort limit as
        // `max_efforts` and the joint damping as `kd`, mirroring how Isaac
        // loads drive parameters from the USD/URDF rather than leaving them 0.
        // Cartpole: slider effort=100 (→ clamp 100), pole effort=0 (→ no clamp).
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 60.0);
        let h = world.add_articulation(sys).unwrap();

        let ctrl = world.get_articulation_controller(h).unwrap();
        let max_efforts = ctrl.get_max_efforts();
        assert!(
            (max_efforts[0] - 100.0).abs() < 1e-3,
            "slider effort limit: {max_efforts:?}"
        );
        assert!(
            max_efforts[1].is_infinite() || max_efforts[1] > 1e30,
            "pole = unspecified → no clamp"
        );
    }

    #[test]
    fn isaac_controller_loads_damping_and_clamps_to_effort_limit() {
        // An inline URDF with explicit effort limit + damping → kd, max_efforts.
        const URDF: &str = r#"<?xml version="1.0"?>
<robot name="damped_arm">
  <link name="base"><inertial><mass value="1"/><inertia ixx="0.1" iyy="0.1" izz="0.1"/></inertial></link>
  <link name="l1"><inertial><mass value="1"/><inertia ixx="0.1" iyy="0.1" izz="0.1"/></inertial></link>
  <link name="l2"><inertial><mass value="1"/><inertia ixx="0.1" iyy="0.1" izz="0.1"/></inertial></link>
  <joint name="j1" type="revolute">
    <parent link="base"/><child link="l1"/>
    <origin xyz="0 0 0"/><axis xyz="0 1 0"/>
    <limit lower="-3.14" upper="3.14" effort="7.5" velocity="10"/>
    <dynamics damping="3.0" friction="0"/>
  </joint>
  <joint name="j2" type="revolute">
    <parent link="l1"/><child link="l2"/>
    <origin xyz="0 0 -1"/><axis xyz="0 1 0"/>
    <limit lower="-3.14" upper="3.14" effort="2.0" velocity="10"/>
    <dynamics damping="1.5" friction="0"/>
  </joint>
</robot>"#;
        let sys = kami_articulated::parse_urdf(URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();

        let ctrl = world.get_articulation_controller(h).unwrap();
        // Effort limits loaded verbatim.
        assert_eq!(ctrl.get_max_efforts(), &[7.5, 2.0]);
        // Damping loaded as kd; kp left at 0 (no URDF stiffness field).
        let (kps, kds) = ctrl.get_gains();
        assert_eq!(kps, &[0.0, 0.0]);
        assert_eq!(kds, &[3.0, 1.5]);
    }
}
