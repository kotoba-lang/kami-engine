//! batched — multi-environment articulation view (Isaac Sim tensor semantics).
//!
//! Real NVIDIA Isaac Sim 4.x / Isaac Lab is **tensor-first**: an
//! `ArticulationView` over `num_envs` returns `[num_envs, n_dof]` arrays and
//! `set_joint_efforts` takes the same shape — that batched data model is the
//! whole point of GPU RL. The single-env `isaac_api::ArticulationView` matched
//! the *names* but not that shape. `ArticulationBatch` provides the **batched
//! tensor shape** (`[num_envs, n_dof]`, env-major flat `Vec<f32>`), so code
//! written against Isaac's batched API runs unchanged.
//!
//! Honest scope: execution is a CPU loop over envs (the GPU compute-batch exists
//! only for cartpole / double-pendulum via `wgpu_backend`). It is API-SHAPE
//! parity for the general articulation solver, not yet GPU-parallel.

use crate::articulation3d::{Articulation3dConfig, Articulation3dState};

/// A batch of `num_envs` independent copies of one articulation, exposed with
/// Isaac Sim `ArticulationView` tensor shapes (`[num_envs, n_dof]`).
pub struct ArticulationBatch {
    cfg: Articulation3dConfig,
    states: Vec<Articulation3dState>,
    efforts: Vec<f32>, // [num_envs * n_dof], env-major
}

impl ArticulationBatch {
    /// `Articulation` cloned across `num_envs` environments (all zeroed).
    pub fn new(cfg: Articulation3dConfig, num_envs: usize) -> Self {
        let num_envs = num_envs.max(1);
        let ndof = cfg.ndof;
        let states = (0..num_envs)
            .map(|_| Articulation3dState::zeros(ndof))
            .collect();
        Self {
            cfg,
            states,
            efforts: vec![0.0; num_envs * ndof],
        }
    }

    pub fn num_envs(&self) -> usize {
        self.states.len()
    }

    pub fn num_dof(&self) -> usize {
        self.cfg.ndof
    }

    /// Isaac `set_joint_efforts(efforts)` — `efforts` is `[num_envs, n_dof]`
    /// (env-major flat). Stored and applied on the next `step()`.
    pub fn set_joint_efforts(&mut self, efforts: &[f32]) {
        let n = self.efforts.len().min(efforts.len());
        self.efforts[..n].copy_from_slice(&efforts[..n]);
    }

    /// Isaac `set_joint_positions(positions)` — `[num_envs, n_dof]` (resets).
    pub fn set_joint_positions(&mut self, positions: &[f32]) {
        let ndof = self.cfg.ndof;
        for (e, st) in self.states.iter_mut().enumerate() {
            for j in 0..ndof {
                if let Some(&v) = positions.get(e * ndof + j) {
                    st.q[j] = v;
                }
            }
            for v in st.qdot.iter_mut() {
                *v = 0.0;
            }
        }
    }

    /// Isaac `world.step()` — advance every environment by `dt` using the stored
    /// per-env efforts (CPU loop; same integrator as the single-env path).
    pub fn step(&mut self) {
        let ndof = self.cfg.ndof;
        for (e, st) in self.states.iter_mut().enumerate() {
            let tau = &self.efforts[e * ndof..(e + 1) * ndof];
            self.cfg.step(st, tau);
        }
    }

    /// Isaac `get_joint_positions()` → `[num_envs, n_dof]` (env-major flat).
    pub fn get_joint_positions(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.num_envs() * self.cfg.ndof);
        for st in &self.states {
            out.extend_from_slice(&st.q);
        }
        out
    }

    /// Isaac `get_joint_velocities()` → `[num_envs, n_dof]`.
    pub fn get_joint_velocities(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.num_envs() * self.cfg.ndof);
        for st in &self.states {
            out.extend_from_slice(&st.qdot);
        }
        out
    }

    /// Isaac `world.reset()` — zero every environment.
    pub fn reset(&mut self) {
        let ndof = self.cfg.ndof;
        for st in &mut self.states {
            *st = Articulation3dState::zeros(ndof);
        }
        for v in self.efforts.iter_mut() {
            *v = 0.0;
        }
    }
}

/// PhysX-named facade — a thin clean-room mirror of the PhysX 5 reduced-coordinate
/// articulation surface (`PxScene` / `PxArticulationReducedCoordinate`). Names
/// only; all dynamics are the KAMI solver (no NVIDIA code). Partial: the cache /
/// link-incoming-joint-force API is not mirrored.
pub mod px {
    pub use crate::batched::ArticulationBatch as PxArticulationReducedCoordinate;
    pub use crate::isaac_api::IsaacWorld as PxScene;

    use super::ArticulationBatch;

    #[allow(non_snake_case)]
    impl ArticulationBatch {
        /// `PxArticulationReducedCoordinate` cache-style joint-force set.
        pub fn setJointEfforts(&mut self, efforts: &[f32]) {
            self.set_joint_efforts(efforts);
        }
        /// PhysX `PxScene::simulate(dt)` + `fetchResults` (one substep here).
        pub fn simulate(&mut self) {
            self.step();
        }
        /// PhysX cache read of generalized positions.
        pub fn getJointPositions(&self) -> Vec<f32> {
            self.get_joint_positions()
        }
        pub fn getJointVelocities(&self) -> Vec<f32> {
            self.get_joint_velocities()
        }
        /// PhysX `getNbShapes`-style: number of parallel scenes (envs).
        pub fn getNbEnvs(&self) -> usize {
            self.num_envs()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a minimal 1-DOF revolute articulation (parsed via the URDF path).
    fn pendulum_cfg() -> Articulation3dConfig {
        let urdf = r#"<robot name="p">
<link name="base"><inertial><mass value="1.0"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="j1" type="revolute"><parent link="base"/><child link="l1"/><origin xyz="0 0 0.1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="50" velocity="10"/><dynamics damping="0.0"/></joint>
<link name="l1"><inertial><origin xyz="0 0 0.2"/><mass value="0.5"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;
        let sys = kami_articulated::parse_urdf(urdf).expect("urdf");
        Articulation3dConfig::from_articulated_system(&sys, glam::Vec3::ZERO, 1.0 / 240.0)
    }

    #[test]
    fn batch_shape_is_num_envs_by_ndof() {
        let cfg = pendulum_cfg();
        let ndof = cfg.ndof;
        let b = ArticulationBatch::new(cfg, 8);
        assert_eq!(b.num_envs(), 8);
        assert_eq!(b.num_dof(), ndof);
        assert_eq!(b.get_joint_positions().len(), 8 * ndof);
        assert!(b.get_joint_positions().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn envs_diverge_under_per_env_efforts() {
        let cfg = pendulum_cfg();
        let ndof = cfg.ndof;
        let n = 4;
        let mut b = ArticulationBatch::new(cfg, n);
        // distinct (small) effort per env so they diverge but stay below the
        // joint limit (no gravity → torque integrates cleanly, θ ∝ τ).
        let efforts: Vec<f32> = (0..n).flat_map(|e| vec![(e as f32) * 0.1; ndof]).collect();
        b.set_joint_efforts(&efforts);
        for _ in 0..50 {
            b.step();
        }
        let q = b.get_joint_positions();
        // env 0 (zero effort) stays ~0; higher-effort envs move more, monotonically.
        assert!(q[0].abs() < 1e-3, "env0 moved: {}", q[0]);
        let mag: Vec<f32> = (0..n).map(|e| q[e * ndof].abs()).collect();
        assert!(mag[1] < mag[2] && mag[2] < mag[3], "not monotone: {mag:?}");
        assert!(q.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn batched_env_matches_the_standalone_single_articulation() {
        // The batch must not change physics: env k stepped with effort e must be
        // bit-for-bit identical to a standalone Articulation3dState stepped with
        // the same cfg + e. Uses gravity (non-trivial trajectory) and verifies a
        // NON-target env stays independent (no cross-env contamination).
        let cfg = Articulation3dConfig {
            gravity: glam::Vec3::new(0.0, 0.0, -9.81),
            ..pendulum_cfg()
        };
        let ndof = cfg.ndof;
        let q0 = 0.4_f32;
        let tau = 0.7_f32;

        // standalone reference.
        let mut ref_st = Articulation3dState::zeros(ndof);
        for v in ref_st.q.iter_mut() {
            *v = q0;
        }
        for _ in 0..120 {
            cfg.step(&mut ref_st, &vec![tau; ndof]);
        }

        // batch: env 1 mirrors the reference; envs 0/2 run different efforts.
        let n = 3;
        let mut b = ArticulationBatch::new(cfg, n);
        let mut pos = vec![0.0_f32; n * ndof];
        for j in 0..ndof {
            pos[ndof + j] = q0; // env 1 only
        }
        b.set_joint_positions(&pos);
        let mut eff = vec![0.0_f32; n * ndof];
        for j in 0..ndof {
            eff[j] = -0.3; // env 0
            eff[ndof + j] = tau; // env 1 (target)
            eff[2 * ndof + j] = 0.5; // env 2
        }
        b.set_joint_efforts(&eff);
        for _ in 0..120 {
            b.step();
        }

        let q = b.get_joint_positions();
        let qd = b.get_joint_velocities();
        for j in 0..ndof {
            assert!(
                (q[ndof + j] - ref_st.q[j]).abs() < 1e-6,
                "env1 q[{j}] {} != standalone {}",
                q[ndof + j],
                ref_st.q[j]
            );
            assert!(
                (qd[ndof + j] - ref_st.qdot[j]).abs() < 1e-6,
                "env1 qdot[{j}] drift"
            );
        }
        // the other envs evolved differently → no shared/aliased state.
        assert!(
            (q[0] - q[ndof]).abs() > 1e-4 && (q[2 * ndof] - q[ndof]).abs() > 1e-4,
            "non-target envs not independent"
        );
    }

    #[test]
    fn physx_facade_aliases_delegate() {
        use px::PxArticulationReducedCoordinate;
        let cfg = pendulum_cfg();
        let mut art: PxArticulationReducedCoordinate = ArticulationBatch::new(cfg, 3);
        assert_eq!(art.getNbEnvs(), 3);
        art.setJointEfforts(&vec![1.0; 3 * art.num_dof()]);
        for _ in 0..50 {
            art.simulate();
        }
        let q = art.getJointPositions();
        assert_eq!(q.len(), 3 * art.num_dof());
        assert!(q.iter().all(|v| v.is_finite()) && q.iter().any(|&v| v.abs() > 1e-4));
    }
}
