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
use crate::jacobian::Jacobian;

/// An active implicit-PD drive on a batch: per-DOF gains (broadcast across envs),
/// env-major position targets, and an optional gravity-compensation feedforward.
struct PdDrive {
    /// `[num_envs * n_dof]` env-major position targets.
    targets: Vec<f32>,
    /// Optional `[num_envs * n_dof]` env-major velocity targets. When `Some`,
    /// the damping term tracks them (`kd·(q̇* − q̇)`); when `None`, it damps to
    /// rest (`−kd·q̇`) — matching the single-env `ArticulationController`.
    vel_targets: Option<Vec<f32>>,
    /// `[n_dof]` per-joint stiffness, broadcast across envs.
    kps: Vec<f32>,
    /// `[n_dof]` per-joint damping, broadcast across envs.
    kds: Vec<f32>,
    /// Add the per-env gravity-compensation torque `g(q)` (Isaac's
    /// gravity-compensated actuator), removing the PD steady-state droop `g/kp`.
    /// Ignored when `computed_torque` is set (inverse dynamics already includes
    /// `g(q)`).
    gravity_comp: bool,
    /// Computed-torque (feedback-linearizing) mode: treat the PD output as a
    /// desired acceleration `q̈*` and realize it exactly via inverse dynamics
    /// `τ = M(q)·q̈* + C(q,q̇) + g(q)`, so tracking is invariant to the
    /// configuration-dependent inertia. When false, the PD output is applied
    /// directly as joint torque (the implicit-PD actuator).
    computed_torque: bool,
    /// Optional `[num_envs * n_dof]` acceleration feedforward `q̈_ff`, added to
    /// the PD signal before inverse dynamics in computed-torque mode (the
    /// trajectory's own acceleration → near-exact tracking). Ignored otherwise.
    accel_ff: Option<Vec<f32>>,
}

/// A batch of `num_envs` independent copies of one articulation, exposed with
/// Isaac Sim `ArticulationView` tensor shapes (`[num_envs, n_dof]`).
pub struct ArticulationBatch {
    cfg: Articulation3dConfig,
    states: Vec<Articulation3dState>,
    efforts: Vec<f32>, // [num_envs * n_dof], env-major
    /// Active implicit-PD drive. When set, `step()` recomputes the PD torque
    /// from each env's *current* state every substep (Isaac's implicit
    /// actuator), rather than replaying a stale stored effort. Gains are
    /// per-DOF, broadcast across envs. Cleared by `set_joint_efforts` / `reset`.
    pos_drive: Option<PdDrive>,
    /// DOF names aligned to the q/q̇ index order. From a URDF (`from_urdf`) these
    /// are the actuated **joint** names; from a raw `cfg` (`new`) they fall back
    /// to the child **link** name of each movable body.
    dof_names: Vec<String>,
    /// Per-DOF position limits `[lower, upper]` aligned to the q index order;
    /// `[-inf, inf]` for unlimited (e.g. continuous) joints.
    dof_limits: Vec<[f32; 2]>,
    /// The per-env joint torques handed to the solver on the last `step()`
    /// (`[num_envs * n_dof]`), for `get_applied_joint_efforts`.
    last_efforts: Vec<f32>,
    /// Optional per-env physics configs for domain randomisation (e.g. randomised
    /// gravity). `None` → every env shares `cfg`. Only the *dynamics* path
    /// (`step`) consults these; kinematics (FK / IK / Jacobians) is mass- and
    /// gravity-independent and always uses the shared `cfg`.
    per_env_cfg: Option<Vec<Articulation3dConfig>>,
}

impl ArticulationBatch {
    /// `Articulation` cloned across `num_envs` environments (all zeroed).
    pub fn new(cfg: Articulation3dConfig, num_envs: usize) -> Self {
        let num_envs = num_envs.max(1);
        let ndof = cfg.ndof;
        let states = (0..num_envs)
            .map(|_| Articulation3dState::zeros(ndof))
            .collect();
        // Fallback DOF names from the cfg: each movable body's child link name,
        // placed at that body's DOF index. `from_urdf` overrides with joint names.
        let mut dof_names = vec![String::new(); ndof];
        let mut dof_limits = vec![[f32::NEG_INFINITY, f32::INFINITY]; ndof];
        for b in &cfg.bodies {
            if b.movable() {
                dof_names[b.dof as usize] = b.name.clone();
                if b.has_limit {
                    dof_limits[b.dof as usize] = [b.lower, b.upper];
                }
            }
        }
        Self {
            cfg,
            states,
            efforts: vec![0.0; num_envs * ndof],
            pos_drive: None,
            dof_names,
            dof_limits,
            last_efforts: vec![0.0; num_envs * ndof],
            per_env_cfg: None,
        }
    }

    /// Domain-randomise physics per environment: each env's gravity vector is
    /// scaled by a factor in `gravity_scale` and every body's mass + spatial
    /// inertia by an independent factor in `mass_scale` (both uniform, seeded →
    /// reproducible). Only the dynamics differ per env — kinematics (FK / IK /
    /// Jacobians) is mass/gravity-independent and stays shared. Pass `(1.0, 1.0)`
    /// for a dimension to leave it nominal; `clear_physics_randomization` resets.
    pub fn randomize_physics(
        &mut self,
        seed: u64,
        gravity_scale: (f32, f32),
        mass_scale: (f32, f32),
    ) {
        let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut next = |lo: f32, hi: f32| {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let u = ((s >> 40) as f32) / ((1u64 << 24) as f32); // [0,1)
            lo + (hi - lo) * u
        };
        let cfgs = (0..self.num_envs())
            .map(|_| {
                let mut c = self.cfg.clone();
                c.gravity *= next(gravity_scale.0, gravity_scale.1);
                let mf = next(mass_scale.0, mass_scale.1);
                if (mf - 1.0).abs() > 1e-9 {
                    for b in c.bodies.iter_mut() {
                        b.mass *= mf;
                        for row in b.inertia.iter_mut() {
                            for x in row.iter_mut() {
                                *x *= mf; // spatial inertia is linear in mass
                            }
                        }
                    }
                }
                c
            })
            .collect();
        self.per_env_cfg = Some(cfgs);
    }

    /// Convenience: per-env gravity-only DR (`randomize_physics` with nominal mass).
    pub fn randomize_gravity(&mut self, seed: u64, low: f32, high: f32) {
        self.randomize_physics(seed, (low, high), (1.0, 1.0));
    }

    /// Convenience: per-env mass/inertia-only DR (`randomize_physics`, nominal gravity).
    pub fn randomize_mass(&mut self, seed: u64, low: f32, high: f32) {
        self.randomize_physics(seed, (1.0, 1.0), (low, high));
    }

    /// Drop any per-env physics randomisation (all envs revert to the shared cfg).
    pub fn clear_physics_randomization(&mut self) {
        self.per_env_cfg = None;
    }

    /// Build a `num_envs` batch directly from a parsed URDF — the realistic
    /// Isaac Lab entry point (`Articulation(prim_paths_expr="/World/envs/env_.*/Robot")`
    /// cloning one robot across vectorized environments). Routes through the
    /// general Featherstone `Articulation3dConfig`, so any URDF the single-env
    /// `Spatial3d` path accepts is valid here too.
    pub fn from_urdf(
        sys: &kami_articulated::ArticulatedSystem,
        gravity: glam::Vec3,
        dt: f32,
        num_envs: usize,
    ) -> Self {
        use kami_articulated::JointKind;
        let cfg = Articulation3dConfig::from_articulated_system(sys, gravity, dt);
        let mut batch = Self::new(cfg, num_envs);
        // Override the link-name fallback with the actuated joint names, aligned
        // to the cfg's DOF index (a joint's child link → that body's `.dof`),
        // so dof_names is correct regardless of URDF declaration vs BFS order.
        for j in sys.joints.iter().filter(|j| j.kind != JointKind::Fixed) {
            if let Some(bi) = batch.cfg.body_index(&j.child) {
                let dof = batch.cfg.bodies[bi].dof;
                if dof >= 0 {
                    batch.dof_names[dof as usize] = j.name.clone();
                }
            }
        }
        batch
    }

    pub fn num_envs(&self) -> usize {
        self.states.len()
    }

    pub fn num_dof(&self) -> usize {
        self.cfg.ndof
    }

    /// Isaac `ArticulationView.dof_names` — ordered DOF names aligned to the
    /// joint-position array (joint names for URDF-built batches; child-link
    /// names for raw-cfg batches).
    pub fn dof_names(&self) -> &[String] {
        &self.dof_names
    }

    /// Isaac `ArticulationView.get_dof_index(name)` — DOF index for a name, or
    /// None if it is not a DOF of this articulation.
    pub fn get_dof_index(&self, name: &str) -> Option<usize> {
        self.dof_names.iter().position(|n| n == name)
    }

    /// Isaac `ArticulationView.get_dof_limits()` — per-DOF `[lower, upper]`
    /// position limits aligned to the q index order (`[-inf, inf]` if
    /// unlimited). Broadcast across envs (one robot cloned).
    pub fn get_dof_limits(&self) -> &[[f32; 2]] {
        &self.dof_limits
    }

    /// Isaac `set_joint_efforts(efforts)` — `efforts` is `[num_envs, n_dof]`
    /// (env-major flat). Stored and applied on the next `step()`. Direct effort
    /// control overrides any active PD position-target drive.
    pub fn set_joint_efforts(&mut self, efforts: &[f32]) {
        let n = self.efforts.len().min(efforts.len());
        self.efforts[..n].copy_from_slice(&efforts[..n]);
        self.pos_drive = None;
    }

    /// Isaac Lab `set_joint_position_targets(targets)` — the implicit-PD
    /// actuator with a single global `(kp, kd)`. `targets` is `[num_envs, n_dof]`
    /// (env-major flat). Each `step()` recomputes `τᵢ = kp·(q*ᵢ − qᵢ) − kd·q̇ᵢ`
    /// from that env's *current* state and clamps to the joint effort limit (so
    /// a single call drives correctly across many steps, unlike a one-shot
    /// `set_joint_efforts`). For per-joint stiffness/damping use
    /// [`set_joint_position_targets_with_gains`].
    pub fn set_joint_position_targets(&mut self, targets: &[f32], kp: f32, kd: f32) {
        let ndof = self.cfg.ndof;
        self.set_joint_position_targets_with_gains(targets, &vec![kp; ndof], &vec![kd; ndof]);
    }

    /// As [`set_joint_position_targets`] but with **per-joint** gains: `kps` and
    /// `kds` are length `n_dof`, broadcast across all envs (Isaac Lab's
    /// per-DOF `stiffness` / `damping` arrays). Missing gains default to 0.
    pub fn set_joint_position_targets_with_gains(
        &mut self,
        targets: &[f32],
        kps: &[f32],
        kds: &[f32],
    ) {
        let ndof = self.cfg.ndof;
        let total = self.num_envs() * ndof;
        let mut t = vec![0.0_f32; total];
        let n = total.min(targets.len());
        t[..n].copy_from_slice(&targets[..n]);
        let mut kp_v = vec![0.0_f32; ndof];
        let mut kd_v = vec![0.0_f32; ndof];
        for d in 0..ndof {
            kp_v[d] = kps.get(d).copied().unwrap_or(0.0);
            kd_v[d] = kds.get(d).copied().unwrap_or(0.0);
        }
        // Preserve the gravity-comp / computed-torque modes across a re-target.
        let (gravity_comp, computed_torque) = self
            .pos_drive
            .as_ref()
            .map_or((false, false), |d| (d.gravity_comp, d.computed_torque));
        self.pos_drive = Some(PdDrive {
            targets: t,
            vel_targets: None,
            kps: kp_v,
            kds: kd_v,
            gravity_comp,
            computed_torque,
            accel_ff: None,
        });
    }

    /// Isaac Lab `set_joint_velocity_targets(targets)` — the implicit velocity
    /// actuator. `targets` is `[num_envs, n_dof]` (env-major flat). Each `step()`
    /// recomputes `τᵢ = kd·(q̇*ᵢ − q̇ᵢ)` (no position term) from current state and
    /// clamps to the joint effort limit. Use this for velocity-control RL tasks;
    /// `set_joint_position_targets` is the position-control counterpart.
    pub fn set_joint_velocity_targets(&mut self, targets: &[f32], kd: f32) {
        let ndof = self.cfg.ndof;
        let total = self.num_envs() * ndof;
        let mut v = vec![0.0_f32; total];
        let n = total.min(targets.len());
        v[..n].copy_from_slice(&targets[..n]);
        let (gravity_comp, computed_torque) = self
            .pos_drive
            .as_ref()
            .map_or((false, false), |d| (d.gravity_comp, d.computed_torque));
        self.pos_drive = Some(PdDrive {
            targets: vec![0.0; total], // no position term (kp = 0)
            vel_targets: Some(v),
            kps: vec![0.0; ndof],
            kds: vec![kd; ndof],
            gravity_comp,
            computed_torque,
            accel_ff: None,
        });
    }

    /// Combined position **and** velocity targets — the trajectory-tracking
    /// actuator: `τᵢ = kp·(q*ᵢ − qᵢ) + kd·(q̇*ᵢ − q̇ᵢ)`. The velocity term is a
    /// feedforward that follows a *moving* reference without the steady lag a
    /// rest-damped position target leaves (`set_joint_position_targets` is the
    /// special case `q̇* = 0`). Both arrays are `[num_envs, n_dof]` env-major.
    /// Pair with `set_gravity_compensation` for accurate tracking under gravity.
    pub fn set_joint_position_velocity_targets(
        &mut self,
        pos: &[f32],
        vel: &[f32],
        kp: f32,
        kd: f32,
    ) {
        let ndof = self.cfg.ndof;
        let total = self.num_envs() * ndof;
        let mut p = vec![0.0_f32; total];
        let np = total.min(pos.len());
        p[..np].copy_from_slice(&pos[..np]);
        let mut v = vec![0.0_f32; total];
        let nv = total.min(vel.len());
        v[..nv].copy_from_slice(&vel[..nv]);
        let (gravity_comp, computed_torque) = self
            .pos_drive
            .as_ref()
            .map_or((false, false), |d| (d.gravity_comp, d.computed_torque));
        self.pos_drive = Some(PdDrive {
            targets: p,
            vel_targets: Some(v),
            kps: vec![kp; ndof],
            kds: vec![kd; ndof],
            gravity_comp,
            computed_torque,
            accel_ff: None,
        });
    }

    /// Full computed-torque **trajectory-tracking** actuator: position, velocity
    /// AND acceleration feedforward. In computed-torque mode the realized joint
    /// acceleration is `q̈_ff + kp·(q*−q) + kd·(q̇*−q̇)`, inverse-dynamics'd to
    /// torque — near-exact tracking of a `(q*, q̇*, q̈*)` reference (feedback
    /// linearization + feedforward). Enables computed-torque control if not
    /// already on. All three arrays are `[num_envs, n_dof]` env-major.
    pub fn set_joint_trajectory_targets(
        &mut self,
        pos: &[f32],
        vel: &[f32],
        accel: &[f32],
        kp: f32,
        kd: f32,
    ) {
        let ndof = self.cfg.ndof;
        let total = self.num_envs() * ndof;
        let take = |src: &[f32]| {
            let mut out = vec![0.0_f32; total];
            let n = total.min(src.len());
            out[..n].copy_from_slice(&src[..n]);
            out
        };
        let gravity_comp = self.pos_drive.as_ref().is_some_and(|d| d.gravity_comp);
        self.pos_drive = Some(PdDrive {
            targets: take(pos),
            vel_targets: Some(take(vel)),
            kps: vec![kp; ndof],
            kds: vec![kd; ndof],
            gravity_comp,
            computed_torque: true, // feedforward only makes sense inverse-dynamics'd
            accel_ff: Some(take(accel)),
        });
    }

    /// Toggle gravity-compensation feedforward on the active PD drive — Isaac's
    /// gravity-compensated actuator. With it on, `step()` adds the per-env
    /// gravity torque `g(q)` to the PD command, so the arm holds its target
    /// against gravity without the `g/kp` steady-state droop plain PD leaves.
    /// No-op if no PD drive is active.
    pub fn set_gravity_compensation(&mut self, enabled: bool) {
        if let Some(d) = self.pos_drive.as_mut() {
            d.gravity_comp = enabled;
        }
    }

    /// Switch the active PD drive into computed-torque (feedback-linearizing)
    /// mode: the PD output is interpreted as a desired joint acceleration and
    /// realized exactly via inverse dynamics `τ = M(q)·q̈* + C(q,q̇) + g(q)`, so
    /// tracking no longer droops with configuration-dependent inertia or gravity
    /// (it supersedes `gravity_compensation`). With it off, the PD output is the
    /// joint torque directly. No-op if no PD drive is active.
    pub fn set_computed_torque_control(&mut self, enabled: bool) {
        if let Some(d) = self.pos_drive.as_mut() {
            d.computed_torque = enabled;
        }
    }

    /// Isaac `set_joint_positions(positions)` — `[num_envs, n_dof]` (env-major
    /// flat). Velocities are left untouched, matching Isaac's
    /// `set_joint_positions` and the single-env `Articulation::set_joint_positions`
    /// (use `set_joint_velocities` to set those). Missing entries keep their
    /// current value.
    pub fn set_joint_positions(&mut self, positions: &[f32]) {
        let ndof = self.cfg.ndof;
        for (e, st) in self.states.iter_mut().enumerate() {
            for j in 0..ndof {
                if let Some(&v) = positions.get(e * ndof + j) {
                    st.q[j] = v;
                }
            }
        }
    }

    /// Isaac `set_joint_velocities(velocities)` — `[num_envs, n_dof]` (env-major
    /// flat). Positions are left untouched. Missing entries keep their current
    /// value.
    pub fn set_joint_velocities(&mut self, velocities: &[f32]) {
        let ndof = self.cfg.ndof;
        for (e, st) in self.states.iter_mut().enumerate() {
            for j in 0..ndof {
                if let Some(&v) = velocities.get(e * ndof + j) {
                    st.qdot[j] = v;
                }
            }
        }
    }

    /// Isaac `world.step()` — advance every environment by `dt` (CPU loop, same
    /// integrator as the single-env path). If a PD position-target drive is
    /// active, the per-env torque is recomputed from current state this step;
    /// otherwise the stored per-env efforts are applied. `cfg.step` clamps each
    /// joint to its effort limit.
    pub fn step(&mut self) {
        let ndof = self.cfg.ndof;
        // Per-env dynamics config (domain randomisation) or the shared one.
        // Bind disjoint field refs so the per-env loop can still borrow
        // `self.states` / `self.last_efforts` mutably.
        let base = &self.cfg;
        let per = self.per_env_cfg.as_ref();
        let cfg_for = |e: usize| -> &Articulation3dConfig { per.map_or(base, |v| &v[e]) };
        match &self.pos_drive {
            Some(drive) => {
                for (e, st) in self.states.iter_mut().enumerate() {
                    let q_target = &drive.targets[e * ndof..(e + 1) * ndof];
                    // Per-DOF PD signal: position term kp·(q*−q) plus a damping
                    // term that either tracks a velocity target kd·(q̇*−q̇) or
                    // damps to rest −kd·q̇ when no velocity target is set.
                    let pd: Vec<f32> = (0..ndof)
                        .map(|d| {
                            let pos = drive.kps[d] * (q_target[d] - st.q[d]);
                            let vel = match &drive.vel_targets {
                                Some(vt) => drive.kds[d] * (vt[e * ndof + d] - st.qdot[d]),
                                None => -drive.kds[d] * st.qdot[d],
                            };
                            pos + vel
                        })
                        .collect();
                    let tau = if drive.computed_torque {
                        // Desired acceleration = PD signal + optional trajectory
                        // acceleration feedforward; realize it exactly (inverse
                        // dynamics folds in C(q,q̇) + g(q)).
                        let qddot_des: Vec<f32> = match &drive.accel_ff {
                            Some(aff) => (0..ndof).map(|d| pd[d] + aff[e * ndof + d]).collect(),
                            None => pd,
                        };
                        cfg_for(e).inverse_dynamics(&st.q, &st.qdot, &qddot_des)
                    } else {
                        let mut tau = pd;
                        // Gravity-compensation feedforward g(q) (per env's pose).
                        if drive.gravity_comp {
                            let g = cfg_for(e).gravity_torque(&st.q);
                            for d in 0..ndof {
                                tau[d] += g[d];
                            }
                        }
                        tau
                    };
                    self.last_efforts[e * ndof..(e + 1) * ndof].copy_from_slice(&tau);
                    cfg_for(e).step(st, &tau);
                }
            }
            None => {
                for (e, st) in self.states.iter_mut().enumerate() {
                    let tau = &self.efforts[e * ndof..(e + 1) * ndof];
                    self.last_efforts[e * ndof..(e + 1) * ndof].copy_from_slice(tau);
                    cfg_for(e).step(st, tau);
                }
            }
        }
    }

    /// Per-env position inverse kinematics — Isaac's `compute_inverse_kinematics`.
    /// For each env, solve for joint angles that place `ee_link`'s frame origin
    /// at the world target (`targets` is `[num_envs, 3]`), warm-started from that
    /// env's *current* configuration via damped-least-squares (`lambda`), clamped
    /// to joint limits. Returns `[num_envs, n_dof]` solutions, or `None` if
    /// `ee_link` is unknown. Does not mutate state — feed the result to
    /// `set_joint_positions` (teleport) or `set_joint_position_targets` (drive).
    pub fn solve_ik(
        &self,
        ee_link: &str,
        targets: &[f32],
        iters: usize,
        lambda: f32,
    ) -> Option<Vec<f32>> {
        self.solve_ik_point(ee_link, [0.0, 0.0, 0.0], targets, iters, lambda)
    }

    /// As [`solve_ik`] but for a **tool point** `p_local` fixed in `ee_link`'s
    /// frame (e.g. a gripper tip offset from the link origin) rather than the
    /// link frame origin itself — Isaac's `compute_inverse_kinematics` with a
    /// tool-centre-point. Necessary when the controlled point is not the link's
    /// own joint origin (which, for a chain's distal link, sits at its parent
    /// joint and is positioned by fewer DOFs).
    pub fn solve_ik_point(
        &self,
        ee_link: &str,
        p_local: [f32; 3],
        targets: &[f32],
        iters: usize,
        lambda: f32,
    ) -> Option<Vec<f32>> {
        let link = self.cfg.body_index(ee_link)?;
        let ndof = self.cfg.ndof;
        let pl = glam::Vec3::from(p_local);
        let mut out = vec![0.0_f32; self.num_envs() * ndof];
        for (e, st) in self.states.iter().enumerate() {
            let t = glam::Vec3::new(targets[e * 3], targets[e * 3 + 1], targets[e * 3 + 2]);
            let sol = self
                .cfg
                .solve_position_ik(link, pl, t, &st.q, iters, lambda);
            out[e * ndof..(e + 1) * ndof].copy_from_slice(&sol);
        }
        Some(out)
    }

    /// Per-env full **pose** IK — Isaac's differential IK with an orientation
    /// target. `targets_pos` is `[num_envs, 3]` world positions; `targets_quat`
    /// is `[num_envs, 4]` world orientations in (w, x, y, z) order. Returns
    /// `[num_envs, n_dof]` joint solutions warm-started from current state, or
    /// `None` if `ee_link` is unknown. Needs ≥6 effective DOF for a general pose.
    pub fn solve_ik_pose(
        &self,
        ee_link: &str,
        targets_pos: &[f32],
        targets_quat: &[f32],
        iters: usize,
        lambda: f32,
    ) -> Option<Vec<f32>> {
        let link = self.cfg.body_index(ee_link)?;
        let ndof = self.cfg.ndof;
        let mut out = vec![0.0_f32; self.num_envs() * ndof];
        for (e, st) in self.states.iter().enumerate() {
            let tp = glam::Vec3::new(
                targets_pos[e * 3],
                targets_pos[e * 3 + 1],
                targets_pos[e * 3 + 2],
            );
            // input order (w, x, y, z) → glam (x, y, z, w).
            let tq = glam::Quat::from_xyzw(
                targets_quat[e * 4 + 1],
                targets_quat[e * 4 + 2],
                targets_quat[e * 4 + 3],
                targets_quat[e * 4],
            );
            let sol = self.cfg.solve_pose_ik(link, tp, tq, &st.q, iters, lambda);
            out[e * ndof..(e + 1) * ndof].copy_from_slice(&sol);
        }
        Some(out)
    }

    /// Isaac `ArticulationView.get_applied_joint_efforts()` → `[num_envs, n_dof]`
    /// — the joint torques handed to the solver on the most recent `step()`
    /// (the PD/computed-torque output, or the directly-set efforts). Zeroed
    /// until the first step. Useful for energy / effort-penalty reward terms.
    pub fn get_applied_joint_efforts(&self) -> &[f32] {
        &self.last_efforts
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

    /// Isaac `world.reset()` — zero every environment and clear any active
    /// drive (efforts and PD position targets).
    pub fn reset(&mut self) {
        let ndof = self.cfg.ndof;
        for st in &mut self.states {
            *st = Articulation3dState::zeros(ndof);
        }
        for v in self.efforts.iter_mut().chain(self.last_efforts.iter_mut()) {
            *v = 0.0;
        }
        self.pos_drive = None;
    }

    /// Isaac `ArticulationView.get_jacobians()` for a named link — one
    /// `[6, n_dof]` geometric Jacobian per environment (Isaac shape
    /// `[num_envs, 6, n_dof]` for the single chosen link). Each env's Jacobian
    /// is evaluated at that env's own joint configuration. `None` if the link
    /// is not part of the articulation.
    pub fn get_jacobians(&self, link_name: &str) -> Option<Vec<Jacobian>> {
        let link = self.cfg.body_index(link_name)?;
        Some(
            self.states
                .iter()
                .map(|st| Jacobian {
                    rows: self.cfg.geometric_jacobian(link, &st.q),
                })
                .collect(),
        )
    }

    /// Isaac `ArticulationView.get_world_poses(link)` — per-env world position
    /// and orientation of a named link. Returns `(positions, quats_wxyz)` where
    /// `positions` is `[num_envs]` of `[x,y,z]` and `quats_wxyz` is `[num_envs]`
    /// of `[w,x,y,z]` (Isaac quaternion order). `None` if the link is unknown.
    pub fn get_world_poses(&self, link_name: &str) -> Option<(Vec<[f32; 3]>, Vec<[f32; 4]>)> {
        let link = self.cfg.body_index(link_name)?;
        let mut positions = Vec::with_capacity(self.num_envs());
        let mut quats = Vec::with_capacity(self.num_envs());
        for st in &self.states {
            let (p, quat, _lin, _ang) = self.cfg.link_state_world(link, &st.q, &st.qdot);
            positions.push([p.x, p.y, p.z]);
            quats.push([quat.w, quat.x, quat.y, quat.z]);
        }
        Some((positions, quats))
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

    const PENDULUM_URDF: &str = r#"<robot name="p">
<link name="base"><inertial><mass value="1.0"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="j1" type="revolute"><parent link="base"/><child link="l1"/><origin xyz="0 0 0.1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="50" velocity="10"/><dynamics damping="0.0"/></joint>
<link name="l1"><inertial><origin xyz="0 0 0.2"/><mass value="0.5"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

    #[test]
    fn from_urdf_builds_a_num_envs_batch() {
        // The realistic Isaac Lab entry: clone one URDF robot across N envs.
        let sys = kami_articulated::parse_urdf(PENDULUM_URDF).unwrap();
        let b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 16);
        assert_eq!(b.num_envs(), 16);
        assert_eq!(b.num_dof(), 1);
        assert_eq!(b.get_joint_positions().len(), 16);
        assert!(b.get_joint_positions().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn set_joint_velocities_is_per_env_and_preserves_positions() {
        let sys = kami_articulated::parse_urdf(PENDULUM_URDF).unwrap();
        // No gravity → with q-independent dynamics a seeded velocity integrates
        // position linearly, so each env's displacement tracks its own vel.
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 3);
        let ndof = b.num_dof();

        // Seed distinct positions, then distinct velocities — positions must
        // survive the velocity set (no clobber), mirroring Isaac semantics.
        b.set_joint_positions(&[0.1, 0.2, 0.3]);
        b.set_joint_velocities(&[0.0, 1.0, -1.0]);
        let q_seed = b.get_joint_positions();
        assert_eq!(
            q_seed,
            vec![0.1, 0.2, 0.3],
            "set_joint_velocities clobbered positions"
        );

        for _ in 0..30 {
            b.step();
        }
        let q = b.get_joint_positions();
        // env0 (v=0) barely moves; env1 (v=+1) increases; env2 (v=-1) decreases.
        assert!((q[0] - 0.1).abs() < 1e-3, "env0 drifted: {}", q[0]);
        assert!(q[ndof] > 0.2, "env1 did not advance: {}", q[ndof]);
        assert!(q[2 * ndof] < 0.3, "env2 did not retreat: {}", q[2 * ndof]);
        assert!(q.iter().all(|v| v.is_finite()));
    }

    // A 2-DOF serial chain: the distal link's Jacobian depends on the proximal
    // joint angle, so per-env divergence is observable (unlike a 1-DOF pendulum,
    // whose link origin sits on its own rotation axis → q-invariant Jacobian).
    const TWO_LINK_URDF: &str = r#"<robot name="arm2">
<link name="base"><inertial><mass value="1"/><inertia ixx="0.01" iyy="0.01" izz="0.01" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="j1" type="revolute"><parent link="base"/><child link="l1"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="50" velocity="10"/><dynamics damping="0"/></joint>
<link name="l1"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="j2" type="revolute"><parent link="l1"/><child link="l2"/><origin xyz="0 0 -1"/><axis xyz="0 1 0"/><limit lower="-3.14" upper="3.14" effort="50" velocity="10"/><dynamics damping="0"/></joint>
<link name="l2"><inertial><origin xyz="0 0 -0.5"/><mass value="1"/><inertia ixx="0.02" iyy="0.02" izz="0.001" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;

    #[test]
    fn get_jacobians_is_per_env_and_correct_shape() {
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 3);
        let ndof = b.num_dof();
        assert_eq!(ndof, 2);
        // Distinct proximal angles → distinct distal-link Jacobians per env.
        // [env-major: (j1,j2) per env]
        b.set_joint_positions(&[0.0, 0.0, 0.6, 0.0, 1.2, 0.0]);

        let jac = b.get_jacobians("l2").expect("l2 jacobian");
        assert_eq!(jac.len(), 3, "one Jacobian per env");
        for j in &jac {
            assert_eq!(j.rows.len(), 6);
            assert_eq!(j.cols(), ndof);
        }
        // env0 (q1=0) vs env2 (q1=1.2): the distal link's linear-velocity rows
        // depend on the proximal angle, so at least one entry must differ.
        let differs =
            (0..6).any(|r| (0..ndof).any(|c| (jac[0].rows[r][c] - jac[2].rows[r][c]).abs() > 1e-4));
        assert!(
            differs,
            "per-env Jacobians did not diverge with proximal angle"
        );
        // Unknown link → None (matches single-env get_jacobian).
        assert!(b.get_jacobians("nope").is_none());
    }

    #[test]
    fn get_world_poses_is_per_env_with_unit_quats() {
        let sys = kami_articulated::parse_urdf(PENDULUM_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 3);
        b.set_joint_positions(&[0.0, 0.5, 1.0]);

        let (pos, quats) = b.get_world_poses("l1").expect("l1 pose");
        assert_eq!(pos.len(), 3);
        assert_eq!(quats.len(), 3);
        for q in &quats {
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            assert!((n - 1.0).abs() < 1e-4, "quat not unit: {q:?}");
        }
        // Different joint angles → the link sits at different world orientations,
        // so env0 and env2 quats must differ.
        let dq = (0..4)
            .map(|i| (quats[0][i] - quats[2][i]).abs())
            .fold(0.0, f32::max);
        assert!(dq > 1e-3, "per-env world poses did not diverge");
        assert!(b.get_world_poses("nope").is_none());
    }

    #[test]
    fn position_targets_drive_each_env_to_its_own_target() {
        // Isaac Lab implicit-PD actuator: one set_joint_position_targets call,
        // then many steps — each env converges to its own [n_dof] target with
        // the PD torque recomputed from current state every step (no gravity).
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 3);
        let ndof = b.num_dof();
        assert_eq!(ndof, 2);

        // env-major [num_envs, n_dof] targets.
        let targets = [
            0.0, 0.0, // env0
            0.5, -0.3, // env1
            -0.4, 0.6, // env2
        ];
        b.set_joint_position_targets(&targets, /*kp=*/ 400.0, /*kd=*/ 20.0);
        for _ in 0..4000 {
            b.step();
        }
        let q = b.get_joint_positions();
        for (i, &want) in targets.iter().enumerate() {
            assert!(
                (q[i] - want).abs() < 0.02,
                "dof {i} did not reach target: got {} want {want}",
                q[i]
            );
        }
    }

    #[test]
    fn set_joint_efforts_clears_active_position_drive() {
        // After a PD drive is armed, a direct effort command must take over —
        // zero efforts → the arm holds (no PD pull back to an old target).
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 1);
        let ndof = b.num_dof();
        b.set_joint_position_targets(&[0.8, 0.8], 300.0, 30.0);
        for _ in 0..300 {
            b.step();
        }
        // Now hand control to direct (zero) efforts and seed a known rest pose
        // (zero both q and qdot — set_joint_positions is velocity-preserving).
        b.set_joint_efforts(&vec![0.0; ndof]);
        b.set_joint_positions(&[0.2, 0.2]);
        b.set_joint_velocities(&vec![0.0; ndof]);
        for _ in 0..300 {
            b.step();
        }
        let q = b.get_joint_positions();
        // With the PD drive cleared and zero effort, the rest pose is held
        // (no spring back toward the old 0.8 target).
        assert!(
            (q[0] - 0.2).abs() < 1e-2 && (q[1] - 0.2).abs() < 1e-2,
            "drive not cleared: {q:?}"
        );
    }

    #[test]
    fn dof_names_are_joint_names_aligned_to_dof_order() {
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 4);
        // URDF-built batch → actuated joint names, aligned to the q index order.
        assert_eq!(b.dof_names(), &["j1".to_string(), "j2".to_string()]);
        assert_eq!(b.dof_names().len(), b.num_dof());
        assert_eq!(b.get_dof_index("j1"), Some(0));
        assert_eq!(b.get_dof_index("j2"), Some(1));
        assert_eq!(b.get_dof_index("nope"), None);
    }

    #[test]
    fn solve_ik_places_the_end_effector_at_cartesian_targets() {
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 2);
        let ndof = b.num_dof();

        // Reachable targets = FK of two distinct known configs.
        b.set_joint_positions(&[0.3, 0.4, -0.2, 0.5]);
        let (poses, _q) = b.get_world_poses("l2").unwrap();
        let targets: Vec<f32> = poses.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();

        // Back to the zero pose, then solve IK toward those targets.
        b.reset();
        let sol = b.solve_ik("l2", &targets, 200, 0.01).expect("l2 link");
        assert_eq!(sol.len(), 2 * ndof);

        // Applying the IK solution must put the EE at the targets (FK round-trip).
        b.set_joint_positions(&sol);
        let (reached, _q) = b.get_world_poses("l2").unwrap();
        for e in 0..2 {
            let d = ((reached[e][0] - targets[e * 3]).powi(2)
                + (reached[e][1] - targets[e * 3 + 1]).powi(2)
                + (reached[e][2] - targets[e * 3 + 2]).powi(2))
            .sqrt();
            assert!(
                d < 0.02,
                "env {e} IK miss: {d} m (reached {:?} target {:?})",
                reached[e],
                &targets[e * 3..e * 3 + 3]
            );
        }
        // Unknown link → None.
        assert!(b.solve_ik("nope", &targets, 10, 0.01).is_none());
    }

    const ARM6_URDF: &str = include_str!("../../fixtures/giemon_arm6/giemon_arm6.urdf");

    #[test]
    fn solve_ik_pose_matches_position_and_orientation_on_6dof_arm() {
        // A 6-DOF arm can satisfy a full pose. FK-derive a reachable pose, then
        // pose-IK must recover both the position AND the orientation.
        let sys = kami_articulated::parse_urdf(ARM6_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 1);
        assert_eq!(b.num_dof(), 6);

        b.set_joint_positions(&[0.3, -0.4, 0.5, 0.2, -0.3, 0.4]);
        let (pos, quat) = b.get_world_poses("link6").unwrap();
        let tp = pos[0];
        let tq = quat[0]; // (w,x,y,z)

        b.reset();
        let sol = b.solve_ik_pose("link6", &tp, &tq, 500, 0.02).unwrap();
        assert_eq!(sol.len(), 6);
        b.set_joint_positions(&sol);

        let (pos2, quat2) = b.get_world_poses("link6").unwrap();
        let dpos = ((pos2[0][0] - tp[0]).powi(2)
            + (pos2[0][1] - tp[1]).powi(2)
            + (pos2[0][2] - tp[2]).powi(2))
        .sqrt();
        assert!(dpos < 0.03, "pose IK position miss: {dpos} m");
        // Orientation: |q·q_target| ≈ 1 (same rotation up to sign).
        let dot: f32 = (0..4).map(|i| quat2[0][i] * tq[i]).sum();
        assert!(dot.abs() > 0.99, "pose IK orientation miss: dot {dot}");
        // Unknown link → None.
        assert!(b.solve_ik_pose("nope", &tp, &tq, 10, 0.02).is_none());
    }

    #[test]
    fn solve_ik_point_reaches_a_tool_tip_using_both_dofs() {
        // The distal link's frame origin sits at the elbow (1 DOF positions it);
        // a tool point 0.5 m down the fore link needs BOTH joints. Verify IK to
        // such a tool point lands the tool, not the link origin.
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 1);
        let tool = [0.0, 0.0, -0.5]; // in the fore-link frame
        let tool_v = glam::Vec3::from(tool);

        // Reachable tool target = FK of a known config's tool point.
        b.set_joint_positions(&[0.5, -0.6]);
        let (pos, quat) = b.get_world_poses("l2").unwrap();
        let q = glam::Quat::from_xyzw(quat[0][1], quat[0][2], quat[0][3], quat[0][0]);
        let tool_target = glam::Vec3::from(pos[0]) + q * tool_v;
        let targets = [tool_target.x, tool_target.y, tool_target.z];

        b.reset();
        let sol = b.solve_ik_point("l2", tool, &targets, 300, 0.01).unwrap();
        b.set_joint_positions(&sol);

        // FK round-trip on the TOOL point must hit the target.
        let (pos2, quat2) = b.get_world_poses("l2").unwrap();
        let q2 = glam::Quat::from_xyzw(quat2[0][1], quat2[0][2], quat2[0][3], quat2[0][0]);
        let tool_reached = glam::Vec3::from(pos2[0]) + q2 * tool_v;
        let d = (tool_reached - tool_target).length();
        assert!(
            d < 0.02,
            "tool IK miss: {d} m (reached {tool_reached:?} target {tool_target:?})"
        );
    }

    #[test]
    fn velocity_feedforward_reduces_trajectory_tracking_lag() {
        use crate::{JointTrajectory, QuinticPolynomialTrajectory};
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let dt = 1.0 / 240.0;
        let a = vec![0.0_f32, 0.0];
        let b = vec![0.6_f32, -0.4];
        let dur = 1.0_f32;
        let traj = QuinticPolynomialTrajectory::min_jerk(a, b.clone(), dur);
        let steps = (dur / dt) as usize;
        let (kp, kd) = (150.0_f32, 24.0_f32);

        // Track the min-jerk move with gravity-comped PD, with/without the
        // velocity feedforward; report the worst mid-move joint tracking error.
        let track = |use_ff: bool| -> f32 {
            let mut arm =
                ArticulationBatch::from_urdf(&sys, glam::Vec3::new(0.0, 0.0, -9.81), dt, 1);
            let mut max_err = 0.0_f32;
            for s in 0..=steps {
                let t = s as f32 * dt;
                let (q_des, qd_des, _) = traj.sample(t);
                if use_ff {
                    arm.set_joint_position_velocity_targets(&q_des, &qd_des, kp, kd);
                } else {
                    arm.set_joint_position_targets(&q_des, kp, kd);
                }
                arm.set_gravity_compensation(true);
                arm.step();
                let q = arm.get_joint_positions();
                if s > 20 {
                    let e = (0..2).map(|i| (q[i] - q_des[i]).abs()).fold(0.0, f32::max);
                    max_err = max_err.max(e);
                }
            }
            max_err
        };

        let err_ff = track(true);
        let err_noff = track(false);
        // The velocity feedforward measurably cuts the rest-damped tracking lag
        // (acceleration feedforward, not modelled here, would close the rest).
        assert!(
            err_ff < err_noff * 0.85,
            "vel feedforward should cut lag: ff {err_ff} vs no-ff {err_noff}"
        );
        assert!(
            err_ff < 0.2,
            "feedforward tracking error unreasonably large: {err_ff} rad"
        );
    }

    #[test]
    fn computed_torque_trajectory_tracking_is_near_exact() {
        use crate::{JointTrajectory, QuinticPolynomialTrajectory};
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let dt = 1.0 / 240.0;
        let traj = QuinticPolynomialTrajectory::min_jerk(vec![0.0, 0.0], vec![0.6, -0.4], 1.0);
        let steps = (1.0 / dt) as usize;

        // Computed-torque tracking with full pos+vel+accel feedforward.
        let mut arm = ArticulationBatch::from_urdf(&sys, glam::Vec3::new(0.0, 0.0, -9.81), dt, 1);
        let mut max_err = 0.0_f32;
        for s in 0..=steps {
            let (q, qd, qdd) = traj.sample(s as f32 * dt);
            arm.set_joint_trajectory_targets(&q, &qd, &qdd, 150.0, 24.0);
            arm.step();
            if s > 20 {
                let cur = arm.get_joint_positions();
                max_err = max_err.max((0..2).map(|i| (cur[i] - q[i]).abs()).fold(0.0, f32::max));
            }
        }
        // Feedback linearization + full feedforward → tracking error is tiny,
        // far below the velocity-only-feedforward PD result (~0.13 rad earlier).
        assert!(
            max_err < 0.01,
            "computed-torque tracking not near-exact: {max_err} rad"
        );
    }

    #[test]
    fn per_env_mass_randomization_diverges_the_dynamics() {
        // Zero gravity to isolate mass: under the SAME applied torque, heavier
        // envs accelerate less → the four envs reach different joint angles.
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 4);
        b.randomize_mass(11, 0.5, 2.0);
        let ndof = b.num_dof();
        let efforts: Vec<f32> = (0..4).flat_map(|_| vec![2.0; ndof]).collect();
        b.set_joint_efforts(&efforts);
        for _ in 0..200 {
            b.step();
        }
        let q = b.get_joint_positions();
        assert!(q.iter().all(|v| v.is_finite()));
        let diverged = (1..4).any(|e| (0..ndof).any(|d| (q[e * ndof + d] - q[d]).abs() > 1e-3));
        assert!(diverged, "per-env mass DR produced identical dynamics");

        // Clearing reverts to the shared mass → identical motion again.
        b.clear_physics_randomization();
        b.set_joint_positions(&vec![0.0; 4 * ndof]);
        b.set_joint_velocities(&vec![0.0; 4 * ndof]);
        b.set_joint_efforts(&efforts);
        for _ in 0..200 {
            b.step();
        }
        let q2 = b.get_joint_positions();
        for e in 1..4 {
            for d in 0..ndof {
                assert!(
                    (q2[e * ndof + d] - q2[d]).abs() < 1e-5,
                    "envs differ after clearing mass DR"
                );
            }
        }
    }

    #[test]
    fn per_env_gravity_randomization_diverges_the_dynamics() {
        // Domain randomisation: per-env gravity scaling makes otherwise-identical
        // envs fall at different rates, while kinematics (shape) is unchanged.
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b =
            ArticulationBatch::from_urdf(&sys, glam::Vec3::new(0.0, 0.0, -9.81), 1.0 / 240.0, 4);
        b.randomize_gravity(7, 0.3, 1.7); // wide spread so envs clearly differ
        let ndof = b.num_dof();

        // Release the arm from a tilted pose, passive (zero effort), under gravity.
        b.set_joint_positions(&vec![0.4; 4 * ndof]);
        for _ in 0..200 {
            b.step();
        }
        let q = b.get_joint_positions();
        assert!(q.iter().all(|v| v.is_finite()));
        // Different per-env gravity → the four envs reach different joint angles.
        let env0 = &q[0..ndof];
        let env_diff = (1..4).any(|e| (0..ndof).any(|d| (q[e * ndof + d] - env0[d]).abs() > 1e-3));
        assert!(
            env_diff,
            "per-env gravity DR produced identical dynamics: {q:?}"
        );

        // Clearing reverts to shared gravity → all envs identical again.
        b.clear_physics_randomization();
        b.set_joint_positions(&vec![0.4; 4 * ndof]);
        b.set_joint_velocities(&vec![0.0; 4 * ndof]);
        for _ in 0..200 {
            b.step();
        }
        let q2 = b.get_joint_positions();
        for e in 1..4 {
            for d in 0..ndof {
                assert!(
                    (q2[e * ndof + d] - q2[d]).abs() < 1e-5,
                    "envs differ after clearing DR"
                );
            }
        }
    }

    #[test]
    fn applied_joint_efforts_reflect_the_last_step() {
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 2);
        let ndof = b.num_dof();
        // Zeroed before any step.
        assert!(b.get_applied_joint_efforts().iter().all(|&v| v == 0.0));

        // Direct effort mode: applied efforts mirror what was set.
        let eff = vec![3.0, -1.0, 0.0, 2.0]; // [num_envs=2, ndof=2]
        b.set_joint_efforts(&eff);
        b.step();
        assert_eq!(b.get_applied_joint_efforts().len(), 2 * ndof);
        assert!((b.get_applied_joint_efforts()[0] - 3.0).abs() < 1e-5);
        assert!((b.get_applied_joint_efforts()[3] - 2.0).abs() < 1e-5);

        // PD mode off-target → non-zero computed torque is recorded.
        b.set_joint_positions(&[0.0, 0.0, 0.0, 0.0]);
        b.set_joint_position_targets(&[0.5, 0.5, 0.5, 0.5], 100.0, 20.0);
        b.step();
        assert!(
            b.get_applied_joint_efforts()
                .iter()
                .any(|&v| v.abs() > 1e-3),
            "PD torque not recorded"
        );

        // reset clears them.
        b.reset();
        assert!(b.get_applied_joint_efforts().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn dof_limits_come_from_the_urdf_aligned_to_dof_order() {
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 4);
        let lim = b.get_dof_limits();
        assert_eq!(lim.len(), b.num_dof());
        for (i, l) in lim.iter().enumerate() {
            assert!(
                (l[0] + 3.14).abs() < 1e-3 && (l[1] - 3.14).abs() < 1e-3,
                "dof {i}: {l:?}"
            );
        }
    }

    #[test]
    fn per_joint_gains_drive_only_the_stiff_joint() {
        // kp=0 on joint 1 → it is not pulled to its target (only damped to rest);
        // joint 0 with high kp tracks. Discriminates per-DOF gains from a global.
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 1);
        b.set_joint_position_targets_with_gains(
            &[0.5, 0.5],   // both targets 0.5
            &[400.0, 0.0], // joint 1 has no stiffness
            &[20.0, 5.0],
        );
        for _ in 0..4000 {
            b.step();
        }
        let q = b.get_joint_positions();
        assert!(
            (q[0] - 0.5).abs() < 0.02,
            "stiff joint did not track: {}",
            q[0]
        );
        assert!(
            q[1].abs() < 0.05,
            "zero-stiffness joint moved to target: {}",
            q[1]
        );
    }

    #[test]
    fn gravity_compensation_removes_pd_droop() {
        // Under gravity, plain PD settles with a steady-state droop g/kp. The
        // gravity-compensated actuator must hold the target far more tightly.
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let g = glam::Vec3::new(0.0, 0.0, -9.81);
        let target = [0.6, 0.3];
        // moderate kp so the droop is clearly visible without compensation.
        let (kp, kd) = (80.0, 12.0);

        let settle = |gravity_comp: bool| -> [f32; 2] {
            let mut b = ArticulationBatch::from_urdf(&sys, g, 1.0 / 240.0, 1);
            b.set_joint_position_targets(&target, kp, kd);
            b.set_gravity_compensation(gravity_comp);
            for _ in 0..4000 {
                b.step();
            }
            let q = b.get_joint_positions();
            [q[0], q[1]]
        };

        let plain = settle(false);
        let comp = settle(true);
        let err = |q: [f32; 2]| (q[0] - target[0]).abs() + (q[1] - target[1]).abs();

        assert!(
            err(plain) > 0.03,
            "expected visible droop without comp: {plain:?}"
        );
        assert!(
            err(comp) < 0.01,
            "gravity comp did not hold target: {comp:?}"
        );
        assert!(
            err(comp) < err(plain) * 0.5,
            "comp not clearly better: {comp:?} vs {plain:?}"
        );
    }

    #[test]
    fn velocity_targets_make_each_env_track_its_commanded_velocity() {
        // Isaac Lab implicit velocity actuator: each env's joints converge to
        // their own commanded joint velocity (no gravity → clean tracking).
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let mut b = ArticulationBatch::from_urdf(&sys, glam::Vec3::ZERO, 1.0 / 240.0, 3);
        let ndof = b.num_dof();
        // env-major [num_envs, n_dof] velocity commands.
        let cmd = [
            0.0, 0.0, // env0: hold
            0.5, -0.3, // env1
            -0.4, 0.6, // env2
        ];
        b.set_joint_velocity_targets(&cmd, /*kd=*/ 40.0);
        // Let the velocity loop settle, then measure over a window.
        for _ in 0..400 {
            b.step();
        }
        let qd = b.get_joint_velocities();
        for (i, &want) in cmd.iter().enumerate() {
            assert!(
                (qd[i] - want).abs() < 0.03,
                "dof {i} velocity off: got {} want {want}",
                qd[i]
            );
        }
        // env0 commanded zero velocity stays near rest.
        assert!(
            qd[0].abs() < 0.03 && qd[1].abs() < 0.03,
            "env0 drifted: {:?}",
            &qd[0..ndof]
        );
    }

    #[test]
    fn computed_torque_tracks_exactly_where_plain_pd_droops() {
        // Under gravity, identical gains: computed-torque (feedback-linearizing)
        // drives steady-state error to ~0 by inverting the full dynamics, while
        // plain implicit PD leaves a configuration/gravity droop.
        let sys = kami_articulated::parse_urdf(TWO_LINK_URDF).unwrap();
        let g = glam::Vec3::new(0.0, 0.0, -9.81);
        let target = [0.6, 0.3];
        let (kp, kd) = (100.0, 20.0);

        let settle = |computed_torque: bool| -> f32 {
            let mut b = ArticulationBatch::from_urdf(&sys, g, 1.0 / 240.0, 1);
            b.set_joint_position_targets(&target, kp, kd);
            b.set_computed_torque_control(computed_torque);
            for _ in 0..4000 {
                b.step();
            }
            let q = b.get_joint_positions();
            assert!(q.iter().all(|v| v.is_finite()));
            (q[0] - target[0]).abs() + (q[1] - target[1]).abs()
        };

        let plain = settle(false);
        let ct = settle(true);
        assert!(
            plain > 0.02,
            "expected plain-PD droop under gravity: {plain}"
        );
        assert!(ct < 0.005, "computed torque did not track exactly: {ct}");
        assert!(
            ct < plain * 0.25,
            "computed torque not clearly better: {ct} vs {plain}"
        );
    }
}
