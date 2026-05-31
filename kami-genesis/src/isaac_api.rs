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
//!   - `…::get_jacobians()` → `[6, n_dof]` per link (Isaac:
//!     `[num_envs, num_links, 6, n_dof]`)
//!   - `…::get_world_poses(link)` → `(pos[3], quat_wxyz[4])`
//!
//! Single-environment scope at R1.1 (num_envs = 1). Batched multi-env is a
//! WGSL-backed R1.x extension (see `vectorized`).

use crate::jacobian::Jacobian;
use crate::world::{ArticulationHandle, LinkState, World, WorldError};
use kami_articulated::ArticulatedSystem;

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
}

impl IsaacWorld {
    /// `isaacsim.core.api.World(physics_dt=physics_dt)` — gravity defaults to
    /// 9.81 m/s² along -z, the Isaac default.
    pub fn new(physics_dt: f32) -> Self {
        IsaacWorld {
            inner: World::new(9.81, physics_dt),
            registered: Vec::new(),
        }
    }

    /// `world.scene.add(Articulation(urdf/usd))` — register an articulation
    /// built from an `ArticulatedSystem`. Returns the prim handle.
    pub fn add_articulation(
        &mut self,
        sys: ArticulatedSystem,
    ) -> Result<ArticulationHandle, WorldError> {
        let h = self.inner.add_articulation(sys)?;
        self.registered.push(h);
        Ok(h)
    }

    /// `world.step(render=False)` — advance physics by one `physics_dt`.
    pub fn step(&mut self) {
        self.inner.step();
    }

    /// `world.reset()` — zero all registered articulations' joint state.
    /// (Isaac restores the registered default; R1.1 default is the zero pose.)
    pub fn reset(&mut self) {
        for &h in &self.registered {
            if let Ok(a) = self.inner.get_mut(h) {
                a.reset_to_zero();
            }
        }
    }

    /// Borrow an Isaac-shaped `ArticulationView` for one prim.
    pub fn articulation(&self, h: ArticulationHandle) -> Option<ArticulationView<'_>> {
        self.inner.get(h).ok().map(|_| ArticulationView { world: &self.inner, h })
    }

    /// Mutable view (needed for `set_joint_efforts`).
    pub fn articulation_mut(
        &mut self,
        h: ArticulationHandle,
    ) -> Option<ArticulationViewMut<'_>> {
        // Validate handle first to keep the Option contract honest.
        if self.inner.get(h).is_err() {
            return None;
        }
        Some(ArticulationViewMut { world: &mut self.inner, h })
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
        self.world.get(self.h).map(|a| a.joint_positions()).unwrap_or_default()
    }

    /// `articulation.get_joint_velocities()` → `[n_dof]`.
    pub fn get_joint_velocities(&self) -> Vec<f32> {
        self.world.get(self.h).map(|a| a.joint_velocities()).unwrap_or_default()
    }

    /// `articulation.num_dof` (property).
    pub fn num_dof(&self) -> usize {
        self.get_joint_positions().len()
    }

    /// `articulation.get_jacobians()` for a named link → `[6, n_dof]`.
    /// Isaac returns `[num_envs, num_links, 6, n_dof]`; this is one link, one
    /// env. None if the link is not part of the articulation.
    pub fn get_jacobian(&self, link_name: &str) -> Option<Jacobian> {
        self.world.get(self.h).ok().and_then(|a| a.jacobian(link_name))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARTPOLE_URDF: &str =
        include_str!("../../../../70-tools/e7m-sim/scenes/cartpole/cartpole.urdf");
    const ARM3_URDF: &str =
        include_str!("../../../../70-tools/e7m-sim/scenes/arm3/arm3.urdf");
    const ARM6_URDF: &str =
        include_str!("../../../../70-tools/e7m-sim/scenes/giemon_arm6/giemon_arm6.urdf");

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
            world.articulation_mut(h).unwrap().set_joint_efforts(&[10.0, 0.0]);
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
        let qn = (quat[0] * quat[0] + quat[1] * quat[1] + quat[2] * quat[2] + quat[3] * quat[3])
            .sqrt();
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
            world.articulation_mut(h).unwrap().set_joint_efforts(&[5.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
            world.step();
        }
        let q1 = world.articulation(h).unwrap().get_joint_positions();
        assert!(q1.iter().all(|v| v.is_finite()));
        assert!((q1[0] - q0[0]).abs() > 1e-3, "base joint did not move: {q0:?} -> {q1:?}");

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
    fn isaac_world_reset_zeros_state() {
        let sys = kami_articulated::parse_urdf(ARM3_URDF).unwrap();
        let mut world = IsaacWorld::new(1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        // Drive away from zero, then reset must restore the zero pose.
        for _ in 0..20 {
            world.articulation_mut(h).unwrap().set_joint_efforts(&[5.0, 0.0, 0.0]);
            world.step();
        }
        assert!(world.articulation(h).unwrap().get_joint_positions()[0].abs() > 1e-3);
        world.reset();
        let q = world.articulation(h).unwrap().get_joint_positions();
        let qd = world.articulation(h).unwrap().get_joint_velocities();
        assert!(q.iter().all(|v| v.abs() < 1e-6), "reset q not zero: {q:?}");
        assert!(qd.iter().all(|v| v.abs() < 1e-6), "reset qdot not zero: {qd:?}");
    }
}
