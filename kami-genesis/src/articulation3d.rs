//! articulation3d — clean-room 3-D reduced-coordinate articulated rigid-body
//! dynamics (the algorithm class NVIDIA PhysX uses for its `Articulation`).
//!
//! Forward dynamics solve `M(q)·q̈ = τ − C(q,q̇) − g(q)`:
//!   - `C + g` (Coriolis/centrifugal + gravity bias) from **RNEA**
//!     (Recursive Newton-Euler) with `q̈ = 0`,
//!   - `M(q)` (joint-space inertia) from **CRBA** (Composite-Rigid-Body),
//!   - solved with in-place `LDLᵀ`, integrated with semi-implicit (symplectic)
//!     Euler.
//! All in 6-D spatial-vector form (`crate::spatial`), so revolute / prismatic
//! joints about **arbitrary 3-D axes** are handled — not just the planar,
//! single-axis case of `planar_chain`.
//!
//! Correctness is gated by an exact cross-check: a planar chain built here
//! reproduces the independently-validated `planar_chain` `q(t)` trajectory
//! (see tests). No NVIDIA / PhysX / Isaac code is linked or referenced
//! (clean-room, ADR-2605261800 §2(b) N1..N9).
//!
//! Ref: Featherstone, *Rigid Body Dynamics Algorithms* (2008), Tables 5.1
//! (RNEA), 6.2 (CRBA).

use crate::spatial::*;
use glam::{Mat3, Mat4, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JointType3d {
    Fixed,
    Revolute,
    Prismatic,
}

/// One body + the joint connecting it to its parent. Topologically ordered:
/// `parent < self` index (parent before child).
#[derive(Clone, Debug)]
pub struct Body3d {
    pub name: String,
    pub parent: isize, // -1 = base
    pub joint_type: JointType3d,
    pub axis: Vec3,   // unit joint axis, in this body's frame
    pub e_tree: Mat3, // fixed rotation child←parent (from joint <origin> rpy)
    pub r_tree: Vec3, // joint origin in parent frame (from joint <origin> xyz)
    pub inertia: M6,  // spatial inertia about this body's frame origin
    pub mass: f32,
    pub com: Vec3, // COM in body frame
    pub lower: f32,
    pub upper: f32,
    pub has_limit: bool,
    pub effort: f32,  // |τ| clamp; 0 = unlimited
    pub damping: f32, // joint viscous damping
    pub dof: isize,   // index into q/q̇; -1 for Fixed
}

impl Body3d {
    /// Motion subspace `S` (6-vector) in this body's frame.
    fn s(&self) -> Sv {
        match self.joint_type {
            JointType3d::Revolute => sv(self.axis, Vec3::ZERO),
            JointType3d::Prismatic => sv(Vec3::ZERO, self.axis),
            JointType3d::Fixed => ZERO_SV,
        }
    }

    /// Joint transform `X_J(q)`: maps joint-frame motion → this body's frame.
    fn x_joint(&self, q: f32) -> M6 {
        match self.joint_type {
            JointType3d::Revolute => plucker(rot(self.axis, q).transpose(), Vec3::ZERO),
            JointType3d::Prismatic => plucker(Mat3::IDENTITY, self.axis * q),
            JointType3d::Fixed => ident6(),
        }
    }

    fn x_tree(&self) -> M6 {
        plucker(self.e_tree, self.r_tree)
    }

    pub fn movable(&self) -> bool {
        self.joint_type != JointType3d::Fixed
    }
}

fn ident6() -> M6 {
    from_blocks(Mat3::IDENTITY, Mat3::ZERO, Mat3::ZERO, Mat3::IDENTITY)
}

fn rot(axis: Vec3, angle: f32) -> Mat3 {
    if axis.length_squared() < 1e-12 {
        Mat3::IDENTITY
    } else {
        Mat3::from_axis_angle(axis.normalize(), angle)
    }
}

#[derive(Clone, Debug)]
pub struct Articulation3dConfig {
    pub bodies: Vec<Body3d>,
    pub gravity: Vec3,
    pub dt: f32,
    pub ndof: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Articulation3dState {
    pub q: Vec<f32>,
    pub qdot: Vec<f32>,
}

impl Articulation3dState {
    pub fn zeros(ndof: usize) -> Self {
        Self {
            q: vec![0.0; ndof],
            qdot: vec![0.0; ndof],
        }
    }
}

/// Per-body kinematic quantities for one `(q, q̇)`.
struct Kin {
    x: Vec<M6>,           // X_i : parent-body frame → body i frame
    x0_inv: Vec<M6>,      // base ← body i (maps body-i spatial motion to base)
    v: Vec<Sv>,           // spatial velocity of body i, in body i frame
    rwb: Vec<Mat3>,       // world ← body rotation
    pw: Vec<Vec3>,        // body frame origin in world
    com_world: Vec<Vec3>, // COM in world
}

impl Articulation3dConfig {
    pub fn n_bodies(&self) -> usize {
        self.bodies.len()
    }

    fn kinematics(&self, q: &[f32], qdot: &[f32]) -> Kin {
        let nb = self.bodies.len();
        let mut x = vec![ZERO_M6; nb];
        let mut x0_inv = vec![ZERO_M6; nb];
        let mut v = vec![ZERO_SV; nb];
        let mut rwb = vec![Mat3::IDENTITY; nb];
        let mut pw = vec![Vec3::ZERO; nb];
        let mut com_world = vec![Vec3::ZERO; nb];

        for i in 0..nb {
            let b = &self.bodies[i];
            let qi = if b.movable() { q[b.dof as usize] } else { 0.0 };
            let qdi = if b.movable() {
                qdot[b.dof as usize]
            } else {
                0.0
            };
            let xi = mat_mul(&b.x_joint(qi), &b.x_tree());
            x[i] = xi;

            // World pose, consistent with the spatial transforms.
            let (rwp, pwp) = if b.parent < 0 {
                (Mat3::IDENTITY, Vec3::ZERO)
            } else {
                (rwb[b.parent as usize], pw[b.parent as usize])
            };
            let rwj = rwp * b.e_tree.transpose();
            let pwj = pwp + rwp * b.r_tree;
            let (rwi, pwi) = match b.joint_type {
                JointType3d::Revolute => (rwj * rot(b.axis, qi), pwj),
                JointType3d::Prismatic => (rwj, pwj + rwj * (b.axis * qi)),
                JointType3d::Fixed => (rwj, pwj),
            };
            rwb[i] = rwi;
            pw[i] = pwi;
            com_world[i] = pwi + rwi * b.com;

            // Spatial velocity recursion: v_i = X_i v_parent + S_i q̇_i.
            let v_parent = if b.parent < 0 {
                ZERO_SV
            } else {
                v[b.parent as usize]
            };
            let vj = scale_sv(qdi, &b.s());
            v[i] = add_sv(&mat_vec(&xi, &v_parent), &vj);

            // base ← body i transform: inv(X0_i) = inv(X0_parent)·inv(X_i).
            let xi_inv = invert_xi(b, qi);
            x0_inv[i] = if b.parent < 0 {
                xi_inv
            } else {
                mat_mul(&x0_inv[b.parent as usize], &xi_inv)
            };
        }
        Kin {
            x,
            x0_inv,
            v,
            rwb,
            pw,
            com_world,
        }
    }

    /// Inverse-dynamics bias `C(q,q̇) + g(q)` via RNEA with `q̈ = 0`.
    fn rnea_bias(&self, qdot: &[f32], kin: &Kin) -> Vec<f32> {
        let nb = self.bodies.len();
        let mut a = vec![ZERO_SV; nb];
        let mut f = vec![ZERO_SV; nb];
        let a_base = sv(Vec3::ZERO, -self.gravity); // a_0 = −g

        for i in 0..nb {
            let b = &self.bodies[i];
            let qdi = if b.movable() {
                qdot[b.dof as usize]
            } else {
                0.0
            };
            let a_parent = if b.parent < 0 {
                a_base
            } else {
                a[b.parent as usize]
            };
            let vj = scale_sv(qdi, &b.s());
            a[i] = add_sv(
                &mat_vec(&kin.x[i], &a_parent),
                &mat_vec(&crm(&kin.v[i]), &vj),
            );
            let iv = mat_vec(&b.inertia, &kin.v[i]);
            f[i] = add_sv(&mat_vec(&b.inertia, &a[i]), &mat_vec(&crf(&kin.v[i]), &iv));
        }

        let mut tau = vec![0.0_f32; self.ndof];
        for i in (0..nb).rev() {
            let b = &self.bodies[i];
            if b.movable() {
                tau[b.dof as usize] = dot(&b.s(), &f[i]);
            }
            if b.parent >= 0 {
                let up = mat_vec(&transpose(&kin.x[i]), &f[i]);
                f[b.parent as usize] = add_sv(&f[b.parent as usize], &up);
            }
        }
        tau
    }

    /// Joint-space inertia `M(q)` via CRBA.
    fn crba(&self, kin: &Kin) -> Vec<Vec<f32>> {
        let nb = self.bodies.len();
        let n = self.ndof;
        let mut ic: Vec<M6> = self.bodies.iter().map(|b| b.inertia).collect();
        for i in (0..nb).rev() {
            let b = &self.bodies[i];
            if b.parent >= 0 {
                let xt = transpose(&kin.x[i]);
                let term = mat_mul(&mat_mul(&xt, &ic[i]), &kin.x[i]);
                ic[b.parent as usize] = add(&ic[b.parent as usize], &term);
            }
        }

        let mut m = vec![vec![0.0_f32; n]; n];
        for i in 0..nb {
            let bi = &self.bodies[i];
            if !bi.movable() {
                continue;
            }
            let di = bi.dof as usize;
            let mut fcol = mat_vec(&ic[i], &bi.s());
            m[di][di] = dot(&bi.s(), &fcol);
            let mut j = i;
            loop {
                fcol = mat_vec(&transpose(&kin.x[j]), &fcol);
                let p = self.bodies[j].parent;
                if p < 0 {
                    break;
                }
                j = p as usize;
                let bj = &self.bodies[j];
                if bj.movable() {
                    let dj = bj.dof as usize;
                    let val = dot(&bj.s(), &fcol);
                    m[di][dj] = val;
                    m[dj][di] = val;
                }
            }
        }
        m
    }

    /// Free (contact-less) acceleration `q̈` for applied torques `tau_applied`,
    /// plus the mass matrix used (so the contact solver can reuse `M`).
    pub(crate) fn forward_dynamics(
        &self,
        st: &Articulation3dState,
        tau_applied: &[f32],
    ) -> (Vec<f32>, Vec<Vec<f32>>) {
        let kin = self.kinematics(&st.q, &st.qdot);
        let bias = self.rnea_bias(&st.qdot, &kin);
        let m = self.crba(&kin);
        let mut rhs = vec![0.0_f32; self.ndof];
        for b in self.bodies.iter().filter(|b| b.movable()) {
            let di = b.dof as usize;
            let mut t = tau_applied[di];
            if b.effort > 0.0 {
                t = t.clamp(-b.effort, b.effort);
            }
            rhs[di] = t - bias[di] - b.damping * st.qdot[di];
        }
        let qddot = solve_ldlt(&m, &mut rhs).unwrap_or_else(|| vec![0.0; self.ndof]);
        (qddot, m)
    }

    /// Advance one semi-implicit Euler step (no contact).
    pub fn step(&self, st: &mut Articulation3dState, tau_applied: &[f32]) {
        let (qddot, _) = self.forward_dynamics(st, tau_applied);
        self.integrate(st, &qddot);
    }

    /// Joint-space PD torque for position targets — the Isaac "implicit
    /// actuator" (`set_joint_position_targets`): `τᵢ = kp·(q*ᵢ − qᵢ) − kd·q̇ᵢ`.
    /// Feed the result to `step()`, which clamps each joint to its effort limit.
    /// One global `(kp, kd)`; scale the returned vector for per-joint gains.
    pub fn pd_position_torque(
        &self,
        st: &Articulation3dState,
        q_target: &[f32],
        kp: f32,
        kd: f32,
    ) -> Vec<f32> {
        (0..self.ndof)
            .map(|d| {
                let qt = q_target.get(d).copied().unwrap_or(0.0);
                kp * (qt - st.q[d]) - kd * st.qdot[d]
            })
            .collect()
    }

    /// Convenience: one PD position-target step (`pd_position_torque` → `step`).
    pub fn drive_to_targets(
        &self,
        st: &mut Articulation3dState,
        q_target: &[f32],
        kp: f32,
        kd: f32,
    ) {
        let tau = self.pd_position_torque(st, q_target, kp, kd);
        self.step(st, &tau);
    }

    /// Joint torques that hold the arm static against gravity at configuration
    /// `q` — the RNEA bias evaluated at `q̇ = 0` (Coriolis vanishes, so only the
    /// gravity term `g(q)` remains). This is the gravity feedforward of Isaac's
    /// gravity-compensated actuator; add it to a PD command to remove the
    /// steady-state droop `g(q)/kp` that plain PD leaves under gravity.
    pub fn gravity_torque(&self, q: &[f32]) -> Vec<f32> {
        let zero = vec![0.0_f32; self.ndof];
        let kin = self.kinematics(q, &zero);
        self.rnea_bias(&zero, &kin)
    }

    /// Inverse dynamics: the joint torques that realise a desired acceleration
    /// `q̈*` at state `(q, q̇)` — the full RNEA `τ = M(q)·q̈* + C(q,q̇) + g(q)`.
    /// The exact inverse of `forward_dynamics`, and the basis of computed-torque
    /// control (Isaac `compute_inverse_dynamics`). `gravity_torque` is the
    /// special case `q̇ = 0, q̈* = 0`.
    pub fn inverse_dynamics(&self, q: &[f32], qdot: &[f32], qddot_des: &[f32]) -> Vec<f32> {
        let kin = self.kinematics(q, qdot);
        let mut tau = self.rnea_bias(qdot, &kin); // C + g
        let m = self.crba(&kin);
        for i in 0..self.ndof {
            for j in 0..self.ndof {
                tau[i] += m[i][j] * qddot_des.get(j).copied().unwrap_or(0.0);
            }
        }
        tau
    }

    /// Damped-least-squares position IK: joint angles that place the point
    /// `p_local` (fixed in `link`'s frame) at world `target`, starting from
    /// `q_init`. Iterates `q ← q + Jᵀ(JJᵀ+λ²I)⁻¹·(target − p)` using the
    /// (finite-diff-validated) point Jacobian, clamping to joint limits each
    /// step. `lambda` damps singularities (Nakamura/Wampler). Redundant arms
    /// reach the target point via many configurations — only the point is
    /// constrained. Returns the joint solution.
    pub fn solve_position_ik(
        &self,
        link: usize,
        p_local: Vec3,
        target: Vec3,
        q_init: &[f32],
        iters: usize,
        lambda: f32,
    ) -> Vec<f32> {
        let n = self.ndof;
        let mut q = q_init.to_vec();
        q.resize(n, 0.0);
        for _ in 0..iters {
            let (r, t) = self.link_world(&q)[link];
            let p = t + r * p_local;
            let e = target - p;
            if e.length() < 1e-6 {
                break;
            }
            let jac = self.point_jacobian(link, p, &q); // 3×n (column per dof)
            // JJᵀ + λ²I (3×3 SPD).
            let mut a = vec![vec![0.0_f32; 3]; 3];
            for col in jac.iter().take(n) {
                for r0 in 0..3 {
                    for c0 in 0..3 {
                        a[r0][c0] += col[r0] * col[c0];
                    }
                }
            }
            for d in 0..3 {
                a[d][d] += lambda * lambda;
            }
            let mut rhs = vec![e.x, e.y, e.z];
            let y = match solve_ldlt(&a, &mut rhs) {
                Some(y) => y,
                None => break,
            };
            // dq = Jᵀ·y, then clamp to limits.
            for (d, col) in jac.iter().enumerate().take(n) {
                q[d] += col[0] * y[0] + col[1] * y[1] + col[2] * y[2];
            }
            for b in self.bodies.iter().filter(|b| b.movable() && b.has_limit) {
                let di = b.dof as usize;
                q[di] = q[di].clamp(b.lower, b.upper);
            }
        }
        q
    }

    /// Integrate `q̇ += dt·q̈ ; q += dt·q̇` with joint-limit clamping. Exposed so
    /// the contact solver can correct `q̇` before the position update.
    pub(crate) fn integrate(&self, st: &mut Articulation3dState, qddot: &[f32]) {
        let dt = self.dt;
        for b in self.bodies.iter().filter(|b| b.movable()) {
            let di = b.dof as usize;
            st.qdot[di] += dt * qddot[di];
        }
        self.integrate_positions(st);
    }

    pub(crate) fn integrate_positions(&self, st: &mut Articulation3dState) {
        let dt = self.dt;
        for b in self.bodies.iter().filter(|b| b.movable()) {
            let di = b.dof as usize;
            st.q[di] += dt * st.qdot[di];
            if b.has_limit {
                if st.q[di] < b.lower {
                    st.q[di] = b.lower;
                    if st.qdot[di] < 0.0 {
                        st.qdot[di] = 0.0;
                    }
                } else if st.q[di] > b.upper {
                    st.q[di] = b.upper;
                    if st.qdot[di] > 0.0 {
                        st.qdot[di] = 0.0;
                    }
                }
            }
        }
    }

    /// Per-body world transforms (base frame = world).
    pub fn fk_world(&self, q: &[f32]) -> Vec<Mat4> {
        let kin = self.kinematics(q, &vec![0.0; self.ndof]);
        (0..self.bodies.len())
            .map(|i| Mat4::from_rotation_translation(glam::Quat::from_mat3(&kin.rwb[i]), kin.pw[i]))
            .collect()
    }

    /// `(world←body rotation, body-origin in world)` per body.
    pub fn link_world(&self, q: &[f32]) -> Vec<(Mat3, Vec3)> {
        let kin = self.kinematics(q, &vec![0.0; self.ndof]);
        (0..self.bodies.len())
            .map(|i| (kin.rwb[i], kin.pw[i]))
            .collect()
    }

    /// Total mechanical energy `KE + PE`.
    pub fn energy(&self, st: &Articulation3dState) -> f32 {
        let kin = self.kinematics(&st.q, &st.qdot);
        let mut ke = 0.0;
        let mut pe = 0.0;
        for i in 0..self.bodies.len() {
            let b = &self.bodies[i];
            let iv = mat_vec(&b.inertia, &kin.v[i]);
            ke += 0.5 * dot(&kin.v[i], &iv);
            pe += -b.mass * self.gravity.dot(kin.com_world[i]);
        }
        ke + pe
    }

    /// Linear-velocity Jacobian (3×ndof) of a world point `p` rigidly attached
    /// to body `link`. Used by the contact solver.
    pub fn point_jacobian(&self, link: usize, p: Vec3, q: &[f32]) -> Vec<[f32; 3]> {
        let kin = self.kinematics(q, &vec![0.0; self.ndof]);
        let mut cols = vec![[0.0_f32; 3]; self.ndof];
        let mut i = link as isize;
        while i >= 0 {
            let b = &self.bodies[i as usize];
            if b.movable() {
                let s_world = mat_vec(&kin.x0_inv[i as usize], &b.s());
                let w = sv_top(&s_world);
                let v0 = sv_bot(&s_world);
                let vp = v0 + w.cross(p);
                cols[b.dof as usize] = [vp.x, vp.y, vp.z];
            }
            i = b.parent;
        }
        cols
    }

    /// Mass matrix at `q` (Delassus operator for the contact solver).
    pub fn mass_matrix(&self, q: &[f32]) -> Vec<Vec<f32>> {
        let kin = self.kinematics(q, &vec![0.0; self.ndof]);
        self.crba(&kin)
    }

    /// Body index by URDF link name.
    pub fn body_index(&self, name: &str) -> Option<usize> {
        self.bodies.iter().position(|b| b.name == name)
    }

    /// 6×ndof geometric Jacobian of body `link` at its frame origin, in the
    /// base frame: rows `[ωx,ωy,ωz, vx,vy,vz]`. Mirrors Isaac's `get_jacobians`
    /// shape (one link, one env).
    pub fn geometric_jacobian(&self, link: usize, q: &[f32]) -> [Vec<f32>; 6] {
        let kin = self.kinematics(q, &vec![0.0; self.ndof]);
        let p = kin.pw[link];
        let mut rows: [Vec<f32>; 6] = [
            vec![0.0; self.ndof],
            vec![0.0; self.ndof],
            vec![0.0; self.ndof],
            vec![0.0; self.ndof],
            vec![0.0; self.ndof],
            vec![0.0; self.ndof],
        ];
        let mut i = link as isize;
        while i >= 0 {
            let b = &self.bodies[i as usize];
            if b.movable() {
                let sw = mat_vec(&kin.x0_inv[i as usize], &b.s());
                let w = sv_top(&sw);
                let v0 = sv_bot(&sw);
                let vp = v0 + w.cross(p); // shift linear vel to the link origin
                let d = b.dof as usize;
                rows[0][d] = w.x;
                rows[1][d] = w.y;
                rows[2][d] = w.z;
                rows[3][d] = vp.x;
                rows[4][d] = vp.y;
                rows[5][d] = vp.z;
            }
            i = b.parent;
        }
        rows
    }

    /// World pose + spatial velocity of body `link`'s frame origin:
    /// `(position, orientation, linear_velocity, angular_velocity)`.
    pub fn link_state_world(
        &self,
        link: usize,
        q: &[f32],
        qdot: &[f32],
    ) -> (Vec3, glam::Quat, Vec3, Vec3) {
        let kin = self.kinematics(q, qdot);
        let rwb = kin.rwb[link];
        let v = kin.v[link];
        (
            kin.pw[link],
            glam::Quat::from_mat3(&rwb),
            rwb * sv_bot(&v),
            rwb * sv_top(&v),
        )
    }
}

/// URDF rpy (fixed-axis roll-x, pitch-y, yaw-z) → rotation matrix.
fn rpy_to_mat3(rpy: Vec3) -> Mat3 {
    Mat3::from_rotation_z(rpy.z) * Mat3::from_rotation_y(rpy.y) * Mat3::from_rotation_x(rpy.x)
}

impl Articulation3dConfig {
    /// Build from a parsed URDF (`kami_articulated::ArticulatedSystem`).
    ///
    /// The base link (never a joint child) is fixed to the world. Continuous
    /// joints are revolute without limits. Links are emitted parent-before-child
    /// (BFS from the root) so the dynamics recursions stay valid.
    pub fn from_articulated_system(
        sys: &kami_articulated::ArticulatedSystem,
        gravity: Vec3,
        dt: f32,
    ) -> Self {
        use kami_articulated::JointKind;
        use std::collections::HashMap;

        let name_idx: HashMap<&str, usize> = sys
            .links
            .iter()
            .enumerate()
            .map(|(i, l)| (l.name.as_str(), i))
            .collect();
        // joint whose child is link i
        let mut joint_of_link: Vec<Option<usize>> = vec![None; sys.links.len()];
        // children link indices of each link
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); sys.links.len()];
        let child_names: std::collections::HashSet<&str> =
            sys.joints.iter().map(|j| j.child.as_str()).collect();
        for (ji, j) in sys.joints.iter().enumerate() {
            if let (Some(&ci), Some(&pi)) = (
                name_idx.get(j.child.as_str()),
                name_idx.get(j.parent.as_str()),
            ) {
                joint_of_link[ci] = Some(ji);
                children[pi].push(ci);
            }
        }
        let root = sys
            .links
            .iter()
            .position(|l| !child_names.contains(l.name.as_str()))
            .unwrap_or(0);

        // BFS order from root.
        let mut order = Vec::with_capacity(sys.links.len());
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(li) = queue.pop_front() {
            order.push(li);
            for &c in &children[li] {
                queue.push_back(c);
            }
        }

        let mut link_to_body: HashMap<usize, usize> = HashMap::new();
        let mut bodies: Vec<Body3d> = Vec::with_capacity(order.len());
        let mut ndof = 0usize;
        for &li in &order {
            let link = &sys.links[li];
            let inert = &link.inertia;
            let i_diag = Mat3::from_cols(
                Vec3::new(inert.ixx, inert.ixy, inert.ixz),
                Vec3::new(inert.ixy, inert.iyy, inert.iyz),
                Vec3::new(inert.ixz, inert.iyz, inert.izz),
            );
            let r_c = rpy_to_mat3(inert.com.rpy);
            let i_com = r_c * i_diag * r_c.transpose();
            let com = inert.com.xyz;
            let inertia = spatial_inertia(inert.mass, com, i_com);

            let (
                parent,
                joint_type,
                axis,
                e_tree,
                r_tree,
                lower,
                upper,
                has_limit,
                effort,
                damping,
                dof,
            ) = match joint_of_link[li] {
                None => (
                    -1isize,
                    JointType3d::Fixed,
                    Vec3::Z,
                    Mat3::IDENTITY,
                    Vec3::ZERO,
                    0.0,
                    0.0,
                    false,
                    0.0,
                    0.0,
                    -1isize,
                ),
                Some(ji) => {
                    let j = &sys.joints[ji];
                    let parent_body = *link_to_body
                        .get(name_idx.get(j.parent.as_str()).unwrap())
                        .expect("parent emitted before child (BFS)");
                    let (jt, movable) = match j.kind {
                        JointKind::Fixed => (JointType3d::Fixed, false),
                        JointKind::Prismatic => (JointType3d::Prismatic, true),
                        JointKind::Revolute | JointKind::Continuous => {
                            (JointType3d::Revolute, true)
                        }
                    };
                    let axis = if j.axis.length_squared() > 1e-12 {
                        j.axis.normalize()
                    } else {
                        Vec3::Z
                    };
                    let r_o = rpy_to_mat3(j.origin.rpy);
                    let has_limit =
                        movable && matches!(j.kind, JointKind::Revolute | JointKind::Prismatic);
                    let dof = if movable {
                        let d = ndof as isize;
                        ndof += 1;
                        d
                    } else {
                        -1
                    };
                    (
                        parent_body as isize,
                        jt,
                        axis,
                        r_o.transpose(),
                        j.origin.xyz,
                        j.lower,
                        j.upper,
                        has_limit,
                        j.effort.max(0.0),
                        j.damping.max(0.0),
                        dof,
                    )
                }
            };
            link_to_body.insert(li, bodies.len());
            bodies.push(Body3d {
                name: link.name.clone(),
                parent,
                joint_type,
                axis,
                e_tree,
                r_tree,
                inertia,
                mass: inert.mass,
                com,
                lower,
                upper,
                has_limit,
                effort,
                damping,
                dof,
            });
        }

        Articulation3dConfig {
            bodies,
            gravity,
            dt,
            ndof,
        }
    }
}

fn scale_sv(s: f32, v: &Sv) -> Sv {
    let mut o = ZERO_SV;
    for i in 0..6 {
        o[i] = s * v[i];
    }
    o
}
fn add_sv(a: &Sv, b: &Sv) -> Sv {
    let mut o = ZERO_SV;
    for i in 0..6 {
        o[i] = a[i] + b[i];
    }
    o
}

/// inv(X_i) = inv(X_J·X_T) = inv(X_T)·inv(X_J).
fn invert_xi(b: &Body3d, q: f32) -> M6 {
    let xt_inv = plucker_inv(b.e_tree, b.r_tree);
    let xj_inv = match b.joint_type {
        JointType3d::Revolute => plucker_inv(rot(b.axis, q).transpose(), Vec3::ZERO),
        JointType3d::Prismatic => plucker_inv(Mat3::IDENTITY, b.axis * q),
        JointType3d::Fixed => ident6(),
    };
    mat_mul(&xt_inv, &xj_inv)
}

/// In-place `LDLᵀ` solve of the SPD system `A x = b`.
pub(crate) fn solve_ldlt(mat: &[Vec<f32>], b: &mut [f32]) -> Option<Vec<f32>> {
    let n = b.len();
    if mat.len() != n || mat.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut a: Vec<Vec<f32>> = mat.iter().map(|r| r.clone()).collect();
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
    let mut y = vec![0.0_f32; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= a[i][k] * y[k];
        }
        y[i] = s;
    }
    let mut z = y;
    for i in 0..n {
        z[i] /= a[i][i];
    }
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
fn uniform_planar_chain(n: usize, gravity: f32, dt: f32) -> Articulation3dConfig {
    let axis = Vec3::new(0.0, -1.0, 0.0); // matches planar_chain's +θ→+x sense
    let mut bodies = Vec::new();
    for i in 0..n {
        let m = 1.0_f32;
        let l = 1.0_f32;
        let i_perp = m * l * l / 12.0;
        let i_com = Mat3::from_diagonal(Vec3::new(i_perp, i_perp, 0.0));
        let com = Vec3::new(0.0, 0.0, -l / 2.0);
        let inertia = spatial_inertia(m, com, i_com);
        let r_tree = if i == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(0.0, 0.0, -1.0)
        };
        bodies.push(Body3d {
            name: format!("link{i}"),
            parent: i as isize - 1,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree,
            inertia,
            mass: m,
            com,
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof: i as isize,
        });
    }
    Articulation3dConfig {
        bodies,
        gravity: Vec3::new(0.0, 0.0, -gravity),
        dt,
        ndof: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planar_chain::{PlanarChainConfig, PlanarChainState};

    /// THE decisive correctness gate: a 3-D spatial chain restricted to a plane
    /// must reproduce the independently-validated `planar_chain` `q(t)`.
    #[test]
    fn planar_chain_cross_check_matches_2d_solver() {
        for n in 1..=4 {
            let g = 9.81;
            let dt = 1.0 / 240.0;
            let cfg3 = uniform_planar_chain(n, g, dt);
            let cfg2 = PlanarChainConfig {
                n: n as u32,
                masses: vec![1.0; n],
                lengths: vec![1.0; n],
                gravity: g,
                effort_limit: 1.0e9,
                dt,
            };
            let mut q0 = vec![0.0_f32; n];
            let seed = [0.6_f32, -0.4, 0.3, -0.2];
            for i in 0..n {
                q0[i] = seed[i];
            }
            let mut s3 = Articulation3dState {
                q: q0.clone(),
                qdot: vec![0.0; n],
            };
            let mut s2 = PlanarChainState {
                q: q0.clone(),
                qdot: vec![0.0; n],
            };
            let zero = vec![0.0_f32; n];
            for _ in 0..(0.5 / dt) as usize {
                cfg3.step(&mut s3, &zero);
                s2.step(&zero, &cfg2);
            }
            for i in 0..n {
                assert!(
                    (s3.q[i] - s2.q[i]).abs() < 2e-3,
                    "n={n} dof{i}: 3d={} planar={}",
                    s3.q[i],
                    s2.q[i]
                );
            }
        }
    }

    #[test]
    fn single_pendulum_energy_bounded() {
        let g = 9.81;
        let dt = 1.0 / 240.0;
        let cfg = uniform_planar_chain(1, g, dt);
        let mut s = Articulation3dState {
            q: vec![0.05],
            qdot: vec![0.0],
        };
        let e0 = cfg.energy(&s);
        for _ in 0..(1.637 / dt) as usize {
            cfg.step(&mut s, &[0.0]);
        }
        let e1 = cfg.energy(&s);
        assert!(
            (e1 - e0).abs() / e0.abs().max(1.0) < 0.05,
            "drift e0={e0} e1={e1}"
        );
    }

    #[test]
    fn three_d_axes_conserve_energy_without_gravity() {
        // Genuinely 3-D arm (axes z, y, x), no gravity → energy ~constant.
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.2, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.2, 0.0, 0.0), 2),
            ],
            gravity: Vec3::ZERO,
            dt: 1.0 / 480.0,
            ndof: 3,
        };
        let mut s = Articulation3dState {
            q: vec![0.3, -0.5, 0.7],
            qdot: vec![1.0, -0.8, 0.5],
        };
        let e0 = cfg.energy(&s);
        for _ in 0..2000 {
            cfg.step(&mut s, &[0.0, 0.0, 0.0]);
        }
        let e1 = cfg.energy(&s);
        assert!(s.q.iter().all(|x| x.is_finite()));
        assert!(
            (e1 - e0).abs() / e0.abs().max(1.0) < 0.02,
            "energy drift e0={e0} e1={e1}"
        );
    }

    #[test]
    fn pd_position_targets_drive_the_3d_arm_to_target() {
        // Isaac set_joint_position_targets on the 3-D solver: PD torque drives
        // every joint of a genuinely 3-D (z/y/x) arm to its target. Gravity off
        // so a well-damped PD has the static equilibrium q=target, q̇=0.
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0, // unlimited so PD converges cleanly
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.3, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.3, 0.0, 0.0), 2),
            ],
            gravity: Vec3::ZERO,
            dt: 1.0 / 240.0,
            ndof: 3,
        };
        let mut st = Articulation3dState::zeros(3);
        let target = vec![0.5_f32, -0.4, 0.3];
        let (kp, kd) = (40.0, 8.0);
        for _ in 0..3000 {
            cfg.drive_to_targets(&mut st, &target, kp, kd);
        }
        for d in 0..3 {
            assert!(
                (st.q[d] - target[d]).abs() < 1e-2,
                "joint {d}: {} vs target {}",
                st.q[d],
                target[d]
            );
        }
        assert!(
            st.qdot.iter().all(|v| v.abs() < 1e-2),
            "not settled at rest"
        );
    }

    #[test]
    fn position_ik_reaches_a_reachable_cartesian_target() {
        // Target defined as the FK of a known config, so it is reachable; solve
        // from a different start and confirm the controlled point reaches it.
        // The solution joints need not equal the original (redundant arm) — only
        // the Cartesian point is constrained.
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.3, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.3, 0.0, 0.0), 2),
            ],
            gravity: Vec3::ZERO,
            dt: 1.0 / 240.0,
            ndof: 3,
        };
        let link = 2;
        let p_local = Vec3::new(0.15, 0.0, 0.0);
        let q_true = vec![0.5_f32, -0.3, 0.4];
        let (rt, tt) = cfg.link_world(&q_true)[link];
        let target = tt + rt * p_local;

        let q_sol = cfg.solve_position_ik(link, p_local, target, &[0.0, 0.0, 0.0], 300, 0.02);
        let (rs, ts) = cfg.link_world(&q_sol)[link];
        let reached = ts + rs * p_local;
        assert!(
            (reached - target).length() < 1e-3,
            "IK did not reach target: {reached:?} vs {target:?}"
        );
    }

    #[test]
    fn inverse_dynamics_round_trips_through_forward_dynamics() {
        // τ = ID(q, q̇, q̈*) fed back to forward dynamics must reproduce q̈* — the
        // M·q̈ + C + g identity that ties the two solvers together. Exercised with
        // gravity on, nonzero q̇ (Coriolis active) and a nontrivial desired
        // acceleration, so M, C and g all contribute (the full coupled case).
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0, // unlimited → no clamp, exact round-trip
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.3, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.3, 0.0, 0.0), 2),
            ],
            gravity: Vec3::new(0.0, 0.0, -9.81),
            dt: 1.0 / 240.0,
            ndof: 3,
        };
        let q = vec![0.3_f32, -0.5, 0.7];
        let qd = vec![0.9_f32, -0.4, 0.6];
        let qdd_des = vec![1.5_f32, -2.0, 0.8];

        let tau = cfg.inverse_dynamics(&q, &qd, &qdd_des);
        let st = Articulation3dState {
            q: q.clone(),
            qdot: qd.clone(),
        };
        let (qdd, _m) = cfg.forward_dynamics(&st, &tau);
        for d in 0..3 {
            assert!(
                (qdd[d] - qdd_des[d]).abs() < 1e-3,
                "dof {d}: forward {} vs desired {}",
                qdd[d],
                qdd_des[d]
            );
        }
    }

    #[test]
    fn gravity_compensation_removes_pd_droop() {
        // Under gravity, plain PD settles with steady-state error g(q)/kp on the
        // gravity-loaded joints. Adding the RNEA gravity feedforward τ = g(q) + PD
        // cancels the bias → the arm holds the target with near-zero error.
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof,
        };
        let make = || Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.3, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.3, 0.0, 0.0), 2),
            ],
            gravity: Vec3::new(0.0, 0.0, -9.81),
            dt: 1.0 / 240.0,
            ndof: 3,
        };
        let target = vec![0.4_f32, -0.3, 0.5];
        let (kp, kd) = (30.0, 6.0);
        let max_err = |st: &Articulation3dState| {
            (0..3)
                .map(|d| (st.q[d] - target[d]).abs())
                .fold(0.0_f32, f32::max)
        };

        // plain PD — sags under gravity.
        let cfg = make();
        let mut a = Articulation3dState::zeros(3);
        for _ in 0..3000 {
            cfg.drive_to_targets(&mut a, &target, kp, kd);
        }
        let err_pd = max_err(&a);

        // PD + gravity feedforward.
        let mut b = Articulation3dState::zeros(3);
        for _ in 0..3000 {
            let g = cfg.gravity_torque(&b.q);
            let pd = cfg.pd_position_torque(&b, &target, kp, kd);
            let tau: Vec<f32> = (0..3).map(|d| g[d] + pd[d]).collect();
            cfg.step(&mut b, &tau);
        }
        let err_gc = max_err(&b);

        assert!(
            err_pd > 0.02,
            "test too weak: plain PD barely drooped ({err_pd})"
        );
        assert!(
            err_gc < 0.25 * err_pd,
            "gravity comp did not help: pd={err_pd} gc={err_gc}"
        );
        assert!(err_gc < 1e-2, "gravity-comp pose not accurate: {err_gc}");
    }

    #[test]
    fn point_jacobian_matches_finite_difference_fk() {
        // The 3-D linear-velocity Jacobian of a world point rigidly attached to a
        // link must equal the finite difference of that point's world position
        // w.r.t. each joint angle. jacobian.rs has this cross-check for the 2-D
        // analytic Jacobians; articulation3d's point_jacobian (the column IK /
        // contact use, Isaac get_jacobians linear rows) lacked it. An OFFSET point
        // exercises both the origin-velocity and the ω×p rotational parts.
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.3, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.3, 0.0, 0.0), 2),
            ],
            gravity: Vec3::ZERO,
            dt: 1.0 / 240.0,
            ndof: 3,
        };
        let q = vec![0.3_f32, -0.5, 0.7];
        let link = 2;
        // world point = link-2 frame origin + an offset fixed in the link frame.
        let (r0, t0) = cfg.link_world(&q)[link];
        let offset_local = Vec3::new(0.2, 0.1, -0.05);
        let p = t0 + r0 * offset_local;

        let jac = cfg.point_jacobian(link, p, &q);
        let h = 1e-4;
        for d in 0..3 {
            let mut qp = q.clone();
            qp[d] += h;
            let (r1, t1) = cfg.link_world(&qp)[link];
            let world_p = t1 + r1 * offset_local;
            let fd = (world_p - p) / h;
            let col = Vec3::new(jac[d][0], jac[d][1], jac[d][2]);
            assert!(
                (fd - col).length() < 2e-2,
                "dof {d}: finite-diff {fd:?} vs jacobian {col:?}"
            );
        }
    }

    #[test]
    fn geometric_jacobian_times_qdot_matches_link_twist() {
        // J(q)·q̇ must equal the link's spatial twist (ω, v_origin) from the
        // forward-kinematics velocity recursion — validates BOTH the angular rows
        // and the ω×p-shifted linear rows of the 6-row geometric Jacobian (Isaac
        // get_jacobians), independently of point_jacobian (iter 17 checked only
        // the linear column via finite differences).
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::splat(0.02));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.0, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.0, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.3, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.3, 0.0, 0.0), 2),
            ],
            gravity: Vec3::ZERO,
            dt: 1.0 / 240.0,
            ndof: 3,
        };
        let q = vec![0.3_f32, -0.5, 0.7];
        let qd = vec![1.1_f32, -0.8, 0.6];
        let link = 2;

        let j = cfg.geometric_jacobian(link, &q);
        let mut tw = [0.0_f32; 6];
        for (r, row) in j.iter().enumerate() {
            for d in 0..cfg.ndof {
                tw[r] += row[d] * qd[d];
            }
        }
        let w_jac = Vec3::new(tw[0], tw[1], tw[2]);
        let v_jac = Vec3::new(tw[3], tw[4], tw[5]);

        let (_pos, _rot, v_fk, w_fk) = cfg.link_state_world(link, &q, &qd);
        assert!(
            (w_jac - w_fk).length() < 1e-3,
            "angular: {w_jac:?} vs {w_fk:?}"
        );
        assert!(
            (v_jac - v_fk).length() < 1e-3,
            "linear: {v_jac:?} vs {v_fk:?}"
        );
    }

    #[test]
    fn mass_matrix_matches_kinetic_energy_3d() {
        // The spatial-algebra CRBA mass matrix must agree with the independent
        // 6-D spatial-velocity energy recursion: with gravity off, energy() is
        // pure KE = ½ Σ vᵢᵀ Iᵢ vᵢ, which by definition equals ½·q̇ᵀ M(q) q̇.
        // Genuinely out-of-plane axes (z, y, x) + COM offsets exercise the full
        // 3-D composite-inertia path, not the planar special case.
        let m = 1.0;
        let i_com = Mat3::from_diagonal(Vec3::new(0.02, 0.03, 0.015));
        let mk = |parent: isize, axis: Vec3, r: Vec3, dof: isize| Body3d {
            name: "l".into(),
            parent,
            joint_type: JointType3d::Revolute,
            axis,
            e_tree: Mat3::IDENTITY,
            r_tree: r,
            inertia: spatial_inertia(m, Vec3::new(0.1, 0.05, 0.0), i_com),
            mass: m,
            com: Vec3::new(0.1, 0.05, 0.0),
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![
                mk(-1, Vec3::Z, Vec3::ZERO, 0),
                mk(0, Vec3::Y, Vec3::new(0.2, 0.0, 0.0), 1),
                mk(1, Vec3::X, Vec3::new(0.2, 0.0, 0.0), 2),
            ],
            gravity: Vec3::ZERO,
            dt: 1.0 / 480.0,
            ndof: 3,
        };
        let q = vec![0.3_f32, -0.5, 0.7];
        let qd = vec![1.1_f32, -0.8, 0.6];
        let st = Articulation3dState {
            q: q.clone(),
            qdot: qd.clone(),
        };
        let ke_direct = cfg.energy(&st); // gravity = 0 → pure KE

        let mm = cfg.mass_matrix(&q);
        let mut ke_matrix = 0.0_f32;
        for i in 0..cfg.ndof {
            for j in 0..cfg.ndof {
                ke_matrix += qd[i] * mm[i][j] * qd[j];
            }
        }
        ke_matrix *= 0.5;

        assert!(
            (ke_direct - ke_matrix).abs() < 1e-4 * ke_direct.abs().max(1.0),
            "3D KE mismatch: spatial recursion {ke_direct} vs ½q̇ᵀMq̇ {ke_matrix}"
        );
    }
}
