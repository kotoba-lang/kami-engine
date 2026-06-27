//! Implicit Euler integrator with Conjugate Gradient sparse solver.
//!
//! Standard mass-spring implicit Euler:
//!
//!   (M + dt² K) v_new = M v_old + dt F
//!   x_new = x_old + dt v_new
//!
//! where M is the diagonal mass matrix, K is the stiffness matrix
//! assembled from per-beam local Jacobians, and F is the external
//! force (gravity, ground, tire fx/fy).
//!
//! Unlike XPBD's Gauss-Seidel iteration, implicit Euler with CG handles
//! constraint cycles naturally — the linear system encodes all couplings
//! simultaneously, so cyclic constraint graphs converge in
//! O(√(condition_number)) iterations regardless of the cycle topology.
//!
//! For a sedan with ~84 nodes and ~220 beams, the matrix is 252×252
//! with ~2000 non-zeros. CG typically converges in 30-60 iterations.
//!
//! This module is an *alternate* integrator that the user can swap in
//! via `Vehicle::set_integrator_mode(IntegratorMode::Implicit)`. The
//! default remains XPBD (`vehicle.rs`).

use glam::{Mat3, Vec3};

use crate::beam::{Beam, BeamType};
use crate::node::{Node, NodeId};

/// Conjugate Gradient solver state for the linear system
/// `(M + dt² K) v = b`. Re-used across substeps to amortise allocation.
#[derive(Default, Clone, Debug)]
pub struct CgState {
    pub r: Vec<Vec3>,
    pub p: Vec<Vec3>,
    pub ap: Vec<Vec3>,
    pub max_iters: u32,
    pub tolerance: f32,
}

impl CgState {
    pub fn new(node_count: usize) -> Self {
        Self {
            r: vec![Vec3::ZERO; node_count],
            p: vec![Vec3::ZERO; node_count],
            ap: vec![Vec3::ZERO; node_count],
            max_iters: 60,
            tolerance: 1e-4,
        }
    }
    pub fn ensure_capacity(&mut self, n: usize) {
        if self.r.len() < n {
            self.r.resize(n, Vec3::ZERO);
            self.p.resize(n, Vec3::ZERO);
            self.ap.resize(n, Vec3::ZERO);
        }
    }
}

/// Apply the matrix `M + dt² K` to a vector v and accumulate result in `out`.
/// Sparse: only iterates over the beam list, never builds the full matrix.
fn apply_system_matrix(
    nodes: &[Node],
    beams: &[Beam],
    id_to_idx: &[usize],
    v: &[Vec3],
    out: &mut [Vec3],
    dt: f32,
) {
    let dt2 = dt * dt;
    // Initialise out = M v (diagonal mass).
    for (i, n) in nodes.iter().enumerate() {
        if n.is_fixed() {
            out[i] = Vec3::ZERO;
        } else {
            out[i] = v[i] * n.mass;
        }
    }
    // Add dt² K v: for each beam, K_ij = -k * dir⊗dir (3×3). Beam's
    // contribution to (Kv)_i is (-k * dir⊗dir) * (v_i - v_j).
    for b in beams.iter() {
        if b.broken {
            continue;
        }
        let i1 = match id_to_idx.get(b.n1 as usize) {
            Some(&v) if v != usize::MAX => v,
            _ => continue,
        };
        let i2 = match id_to_idx.get(b.n2 as usize) {
            Some(&v) if v != usize::MAX => v,
            _ => continue,
        };
        let p1 = nodes[i1].position;
        let p2 = nodes[i2].position;
        let delta = p2 - p1;
        let len = delta.length();
        if len < 1e-6 {
            continue;
        }
        let dir = delta / len;
        // For Bounded / Support beams that are idle, skip.
        let rest = b.live_rest_length(0.0);
        match b.beam_type {
            BeamType::Bounded {
                min_ratio,
                max_ratio,
            } => {
                let ratio = len / rest.max(1e-6);
                if ratio >= min_ratio && ratio <= max_ratio {
                    continue;
                }
            }
            BeamType::Support => {
                if len >= rest {
                    continue;
                }
            }
            _ => {}
        }
        let k = b.spring;
        let v_diff = v[i2] - v[i1];
        // Stiffness Jacobian along beam direction (1D approximation):
        // K_local = k * dir ⊗ dir
        let kvd = dir * (k * dir.dot(v_diff));
        out[i1] -= kvd * dt2;
        out[i2] += kvd * dt2;
    }
}

fn dot(a: &[Vec3], b: &[Vec3]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x.dot(*y)).sum()
}

/// Solve `(M + dt² K) v = rhs` via Conjugate Gradient. `v_init` is the
/// starting guess (typically the explicit-Euler velocity update). On
/// return, `v_init` holds the converged velocity.
pub fn cg_solve(
    nodes: &[Node],
    beams: &[Beam],
    id_to_idx: &[usize],
    rhs: &[Vec3],
    v: &mut [Vec3],
    state: &mut CgState,
    dt: f32,
) -> u32 {
    let n = nodes.len();
    state.ensure_capacity(n);
    // r = rhs - A v
    apply_system_matrix(nodes, beams, id_to_idx, v, &mut state.ap, dt);
    for i in 0..n {
        state.r[i] = rhs[i] - state.ap[i];
        state.p[i] = state.r[i];
    }
    let mut rs_old = dot(&state.r, &state.r);
    if rs_old < state.tolerance * state.tolerance {
        return 0;
    }
    for k in 0..state.max_iters {
        apply_system_matrix(nodes, beams, id_to_idx, &state.p, &mut state.ap, dt);
        let p_ap = dot(&state.p, &state.ap);
        if p_ap.abs() < 1e-12 {
            return k;
        }
        let alpha = rs_old / p_ap;
        for i in 0..n {
            v[i] += state.p[i] * alpha;
            state.r[i] -= state.ap[i] * alpha;
        }
        let rs_new = dot(&state.r, &state.r);
        if rs_new < state.tolerance * state.tolerance {
            return k + 1;
        }
        let beta = rs_new / rs_old;
        for i in 0..n {
            state.p[i] = state.r[i] + state.p[i] * beta;
        }
        rs_old = rs_new;
    }
    state.max_iters
}

/// Implicit Euler step.
///
/// Inputs:
///   - `nodes`: current positions + velocities + masses
///   - `beams`: distance constraints contributing to K
///   - `external_forces`: F_external (gravity, tire fx/fy, ground spring)
///   - `dt`: time step
///   - `state`: scratch buffers for CG solver
///
/// Updates `nodes` in place: positions advance by dt × new_velocity.
/// Returns the number of CG iterations used (telemetry).
pub fn implicit_step(
    nodes: &mut [Node],
    beams: &[Beam],
    external_forces: &[Vec3],
    dt: f32,
    state: &mut CgState,
) -> u32 {
    let n = nodes.len();
    state.ensure_capacity(n);
    // Build node-id → index map (defensive: caller may use sparse IDs).
    let n_id_max = nodes.iter().map(|n| n.id as usize).max().unwrap_or(0) + 1;
    let mut id_to_idx = vec![usize::MAX; n_id_max];
    for (i, nn) in nodes.iter().enumerate() {
        id_to_idx[nn.id as usize] = i;
    }

    // RHS = M v_old + dt F_ext.
    // Initial guess for v_new = v_old + dt F_ext / m  (explicit Euler).
    let mut rhs: Vec<Vec3> = Vec::with_capacity(n);
    let mut v_new: Vec<Vec3> = Vec::with_capacity(n);
    for (i, nn) in nodes.iter().enumerate() {
        if nn.is_fixed() {
            rhs.push(Vec3::ZERO);
            v_new.push(Vec3::ZERO);
        } else {
            rhs.push(nn.velocity * nn.mass + external_forces[i] * dt);
            v_new.push(nn.velocity + external_forces[i] * (nn.inv_mass * dt));
        }
    }

    // ── Solve (M + dt² K) v_new = rhs.
    let iters = cg_solve(nodes, beams, &id_to_idx, &rhs, &mut v_new, state, dt);

    // ── Update positions: x_new = x_old + dt v_new.
    for (i, nn) in nodes.iter_mut().enumerate() {
        if nn.is_fixed() {
            continue;
        }
        nn.velocity = v_new[i];
        nn.position += v_new[i] * dt;
    }
    iters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beam::{Beam, DeformParams};

    #[test]
    fn cg_solves_trivial_diagonal_system() {
        // Two nodes, no beams: matrix is just M (diagonal). Solve M v = M v_old.
        let mut nodes = vec![
            Node::new(0, Vec3::ZERO, 1.0),
            Node::new(1, Vec3::new(1.0, 0.0, 0.0), 1.0),
        ];
        nodes[0].velocity = Vec3::new(0.5, 0.0, 0.0);
        nodes[1].velocity = Vec3::new(-0.5, 0.0, 0.0);
        let forces = vec![Vec3::ZERO; 2];
        let mut state = CgState::new(2);
        let iters = implicit_step(&mut nodes, &[], &forces, 0.01, &mut state);
        assert_eq!(iters, 0); // already at solution
        assert!((nodes[0].velocity - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn cg_with_one_beam_does_not_explode() {
        // Two nodes connected by a stiff beam. Implicit Euler should
        // remain bounded even with zero damping (unlike explicit Euler
        // which would oscillate or diverge for very stiff k / dt).
        let mut nodes = vec![
            Node::new(0, Vec3::ZERO, 1.0)
                .with_drag(0.0)
                .with_friction(0.0),
            Node::new(1, Vec3::new(1.10, 0.0, 0.0), 1.0)
                .with_drag(0.0)
                .with_friction(0.0),
        ];
        let mut beam = Beam::new(0, 0, 1, 1.0, 5_000.0, 50.0);
        beam.deform = DeformParams {
            deform_limit: 5.0,
            break_limit: 10.0,
            max_plastic_strain: 0.0,
        };
        let beams = vec![beam];
        let mut state = CgState::new(2);
        let dt = 0.01;
        for _ in 0..200 {
            let forces = vec![Vec3::ZERO; 2];
            implicit_step(&mut nodes, &beams, &forces, dt, &mut state);
        }
        let dist = (nodes[1].position - nodes[0].position).length();
        // Sanity: distance stays bounded (no NaN, no escape to infinity).
        assert!(dist.is_finite() && dist < 100.0);
    }
}
