//! Vehicle — the composite soft-body car.
//!
//! Owns the node cloud, beam network, triangles, wheels, powertrain, controls,
//! and integrator config. `step(dt, ground)` advances the simulation one
//! render tick.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::beam::{Beam, BeamId, BreakGroup};
use crate::controls::Controls;
use crate::ground::Ground;
use crate::implicit::{CgState, implicit_step};
use crate::integrator::{IntegratorConfig, substep_count};
use crate::node::{Node, NodeId};
use crate::powertrain::{Powertrain, rad_to_rpm, rpm_to_rad};
use crate::rigid_chassis::RigidChassis;
use crate::triangle::Triangle;
use crate::wheel::{ContactInputs, Wheel, WheelContactMode, pacejka_force, wheel_frame};

/// Integrator algorithm choice for the soft-body inner loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntegratorMode {
    /// XPBD — Extended Position-Based Dynamics. Default. Unconditionally
    /// stable, fast, but suffers slow convergence on cyclic constraint
    /// graphs (mitigated by the rigid-chassis projection layer).
    Xpbd,
    /// Implicit Euler with Conjugate Gradient. Handles cyclic
    /// constraints exactly, naturally dissipative, slower per substep.
    /// No need for rigid-chassis projection.
    Implicit,
}

impl Default for IntegratorMode {
    fn default() -> Self {
        IntegratorMode::Xpbd
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vehicle {
    pub name: String,
    pub nodes: Vec<Node>,
    pub beams: Vec<Beam>,
    pub triangles: Vec<Triangle>,
    pub wheels: Vec<Wheel>,
    pub powertrain: Powertrain,
    pub controls: Controls,
    /// Forward direction in chassis-local frame (used by wheel-frame
    /// resolution before we have a proper rigid-body fit).
    pub chassis_forward: Vec3,
    pub chassis_up: Vec3,
    /// Total mass of all dynamic nodes — cached for telemetry.
    pub total_mass: f32,
    #[serde(skip)]
    pub integrator: IntegratorConfig,
    /// Rolling step counter (telemetry).
    #[serde(skip)]
    pub step_count: u64,
    /// Optional rigid-chassis projector. When `Some`, body + cargo nodes
    /// are projected onto a best-fit rigid transform after the XPBD
    /// constraint loop, eliminating internal frame deformation drift.
    #[serde(skip)]
    pub rigid_chassis: Option<RigidChassis>,
    /// Integrator algorithm choice (XPBD or implicit-Euler+CG).
    pub integrator_mode: IntegratorMode,
    /// Scratch buffers for the implicit solver (re-used across steps).
    #[serde(skip)]
    pub cg_state: CgState,
}

impl Vehicle {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
            beams: Vec::new(),
            triangles: Vec::new(),
            wheels: Vec::new(),
            powertrain: Powertrain::sedan(),
            controls: Controls::default(),
            chassis_forward: Vec3::Z,
            chassis_up: Vec3::Y,
            total_mass: 0.0,
            integrator: IntegratorConfig::default(),
            step_count: 0,
            rigid_chassis: None,
            integrator_mode: IntegratorMode::default(),
            cg_state: CgState::default(),
        }
    }

    pub fn set_integrator_mode(&mut self, mode: IntegratorMode) {
        self.integrator_mode = mode;
    }

    /// Build a rigid-chassis projector from current node positions and
    /// attach it. Call this after the build-time pre-shift so the rest
    /// configuration captures the static-equilibrium pose.
    pub fn enable_rigid_chassis(&mut self) {
        self.rigid_chassis = Some(RigidChassis::build_from(&self.nodes));
    }

    pub fn add_node(&mut self, n: Node) -> NodeId {
        let id = n.id;
        self.nodes.push(n);
        if let Some(last) = self.nodes.last() {
            if last.mass > 0.0 {
                self.total_mass += last.mass;
            }
        }
        id
    }

    pub fn add_beam(&mut self, b: Beam) -> BeamId {
        let id = b.id;
        self.beams.push(b);
        id
    }

    pub fn add_triangle(&mut self, t: Triangle) {
        self.triangles.push(t);
    }

    pub fn add_wheel(&mut self, w: Wheel) {
        self.wheels.push(w);
    }

    /// Centre of mass of all dynamic nodes.
    pub fn center_of_mass(&self) -> Vec3 {
        let mut acc = Vec3::ZERO;
        let mut m = 0.0;
        for n in &self.nodes {
            if n.mass > 0.0 {
                acc += n.position * n.mass;
                m += n.mass;
            }
        }
        if m > 0.0 { acc / m } else { Vec3::ZERO }
    }

    /// Chassis-frame velocity (mass-weighted).
    pub fn body_velocity(&self) -> Vec3 {
        let mut acc = Vec3::ZERO;
        let mut m = 0.0;
        for n in &self.nodes {
            if n.mass > 0.0 {
                acc += n.velocity * n.mass;
                m += n.mass;
            }
        }
        if m > 0.0 { acc / m } else { Vec3::ZERO }
    }

    /// Snap a beam given its fail-group (tearing a panel off all at once).
    pub fn break_group(&mut self, group: BreakGroup) -> u32 {
        let mut n = 0;
        for b in self.beams.iter_mut() {
            if b.break_group == Some(group) {
                b.broken = true;
                n += 1;
            }
        }
        n
    }

    /// Re-attach all beams in a group, resetting their plastic deformation.
    /// Lets the user "repair" parts that were detached or crashed.
    pub fn repair_group(&mut self, group: BreakGroup) -> u32 {
        let mut n = 0;
        for b in self.beams.iter_mut() {
            if b.break_group == Some(group) {
                b.broken = false;
                b.effective_length = b.rest_length;
                b.plastic_strain = 0.0;
                n += 1;
            }
        }
        n
    }

    /// Reset every plastic-deformed beam back to its rest geometry and
    /// undo all breaks. Equivalent to a full body shop respray.
    pub fn repair_all(&mut self) -> u32 {
        let mut n = 0;
        for b in self.beams.iter_mut() {
            if b.broken || b.plastic_strain > 0.0 {
                b.broken = false;
                b.effective_length = b.rest_length;
                b.plastic_strain = 0.0;
                n += 1;
            }
        }
        n
    }

    /// Live chassis-mounted geometry — rebuild forward / up from the wheel
    /// pattern. Cheap: 4 axle endpoints.
    pub fn refresh_chassis_frame(&mut self) {
        if self.wheels.len() < 2 {
            return;
        }
        // Use front-left axle midpoint and rear-left axle midpoint as forward axis.
        let front = self.midpoint_of_axle(0);
        let rear = self.midpoint_of_axle(self.wheels.len() - 2);
        if let (Some(f), Some(r)) = (front, rear) {
            let fwd = (f - r).normalize_or_zero();
            if fwd.length_squared() > 0.0 {
                self.chassis_forward = fwd;
            }
        }
        // Up = body-vertical from any front axle.
        if let (Some(f1), Some(f2)) = (
            self.node_pos(self.wheels[0].axle_n1),
            self.node_pos(self.wheels[0].axle_n2),
        ) {
            let lateral = (f2 - f1).normalize_or_zero();
            let up = lateral.cross(self.chassis_forward).normalize_or_zero();
            if up.length_squared() > 0.0 {
                self.chassis_up = up;
            }
        }
    }

    fn midpoint_of_axle(&self, wheel_idx: usize) -> Option<Vec3> {
        let w = self.wheels.get(wheel_idx)?;
        let p1 = self.node_pos(w.axle_n1)?;
        let p2 = self.node_pos(w.axle_n2)?;
        Some((p1 + p2) * 0.5)
    }

    fn node_pos(&self, id: NodeId) -> Option<Vec3> {
        self.nodes.iter().find(|n| n.id == id).map(|n| n.position)
    }

    pub fn node_index(&self, id: NodeId) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Step the simulation forward by `dt` seconds.
    pub fn step(&mut self, dt: f32, ground: &dyn Ground) {
        self.controls.clamp_inputs();
        let (n, sub_dt) = substep_count(dt, &self.integrator);
        for _ in 0..n {
            self.substep(sub_dt, ground);
        }
        // Rigid chassis projection runs once per frame (not per substep)
        // so the per-substep PBD has time to relax internal stresses
        // before the rigid snap. With 30 % per-frame blend the chassis
        // shape converges over ~5 frames, plenty fast.
        if let Some(rigid) = self.rigid_chassis.clone() {
            rigid.project(&mut self.nodes, dt);
        }
        self.powertrain.gearbox.tick(dt);
        self.refresh_chassis_frame();
        self.step_count += 1;
    }

    fn substep(&mut self, dt: f32, ground: &dyn Ground) {
        match self.integrator_mode {
            IntegratorMode::Xpbd => self.substep_xpbd(dt, ground),
            IntegratorMode::Implicit => self.substep_implicit(dt, ground),
        }
    }

    /// Implicit Euler + CG substep. Handles constraint cycles natively.
    fn substep_implicit(&mut self, dt: f32, ground: &dyn Ground) {
        use crate::node::NodeGroup;

        // ── Phase A: External forces ──
        for n in &mut self.nodes {
            n.force = Vec3::ZERO;
        }
        for n in &mut self.nodes {
            if !n.is_fixed() {
                n.force += self.integrator.gravity * n.mass;
                if n.drag > 0.0 {
                    let v = n.velocity;
                    let speed = v.length();
                    if speed > 0.0 {
                        n.force -= v * (n.drag * speed);
                    }
                }
            }
        }
        self.apply_powertrain_and_tires(dt, ground);
        self.apply_node_ground_contact(ground);

        // Tire vertical via simple per-substep one-sided spring acting
        // through external_forces (CG can't handle unilateral inside the
        // matrix as easily; this gets blended in via the explicit RHS).
        for w in self.wheels.iter() {
            if !w.grounded {
                continue;
            }
            for &id in &[w.axle_n1, w.axle_n2] {
                let i = self.nodes.iter().position(|nn| nn.id == id);
                if let Some(i) = i {
                    let s = ground.sample(self.nodes[i].position.x, self.nodes[i].position.z);
                    let target_y = s.height + w.radius;
                    let pen = target_y - self.nodes[i].position.y;
                    if pen > 0.0 {
                        // Tire effective stiffness 5 MN/m per wheel split
                        // half-half across two axle nodes → 2.5 MN/m
                        // each. With critical damping in the CG solver
                        // this produces ~1 mm static pen.
                        let f_up = pen * 2_500_000.0;
                        self.nodes[i].force.y += f_up;
                    }
                }
            }
        }

        // ── Phase B: Implicit Euler step ──
        let n_count = self.nodes.len();
        let external_forces: Vec<Vec3> = self.nodes.iter().map(|n| n.force).collect();
        let _iters = implicit_step(
            &mut self.nodes,
            &self.beams,
            &external_forces,
            dt,
            &mut self.cg_state,
        );
        let _ = n_count;

        // ── Phase C: Plastic deformation update ──
        let mut beam_lengths: Vec<f32> = Vec::with_capacity(self.beams.len());
        for b in self.beams.iter() {
            let i1 = self.nodes.iter().position(|nn| nn.id == b.n1);
            let i2 = self.nodes.iter().position(|nn| nn.id == b.n2);
            let len = match (i1, i2) {
                (Some(i1), Some(i2)) => {
                    (self.nodes[i2].position - self.nodes[i1].position).length()
                }
                _ => b.current_length,
            };
            beam_lengths.push(len);
        }
        for (b, &len) in self.beams.iter_mut().zip(beam_lengths.iter()) {
            b.update_plastic(len);
        }

        // ── Phase D: Anti-tunnel ground clamp ──
        for n in &mut self.nodes {
            if n.is_fixed() {
                continue;
            }
            if matches!(n.group, NodeGroup::WheelHub | NodeGroup::WheelTire) {
                continue;
            }
            let s = ground.sample(n.position.x, n.position.z);
            if n.position.y < s.height {
                let dy = s.height - n.position.y;
                n.position.y = s.height;
                if n.velocity.y < 0.0 {
                    n.velocity.y = -n.velocity.y * n.restitution.max(0.10);
                }
                let fric = (n.friction * dy * 4.0).clamp(0.0, 0.5);
                n.velocity.x *= 1.0 - fric;
                n.velocity.z *= 1.0 - fric;
            }
        }
    }

    fn substep_xpbd(&mut self, dt: f32, ground: &dyn Ground) {
        use crate::beam::BeamType;
        use crate::node::NodeGroup;

        // ── Phase A: External forces (NOT beams) ──
        for n in &mut self.nodes {
            n.force = Vec3::ZERO;
        }
        for n in &mut self.nodes {
            if !n.is_fixed() {
                n.force += self.integrator.gravity * n.mass;
                if n.drag > 0.0 {
                    let v = n.velocity;
                    let speed = v.length();
                    if speed > 0.0 {
                        n.force -= v * (n.drag * speed);
                    }
                }
            }
        }
        self.apply_powertrain_and_tires(dt, ground);
        self.apply_node_ground_contact(ground);

        // ── Phase B: Predict positions with explicit external forces ──
        let n_count = self.nodes.len();
        let mut predicted: Vec<Vec3> = Vec::with_capacity(n_count);
        for n in self.nodes.iter() {
            if n.is_fixed() {
                predicted.push(n.position);
            } else {
                let v_new = n.velocity + n.force * (n.inv_mass * dt);
                predicted.push(n.position + v_new * dt);
            }
        }

        // ── Phase C: XPBD beam constraint projection ──
        //
        // Each beam is a distance constraint with compliance α = 1/k.
        // λ accumulates across iterations so soft beams still converge.
        // Bounded beams skip projection inside their idle range.
        // Stiff beams (frame, struts, arms) become essentially-rigid
        // distance constraints that no longer trigger Courant blow-up.
        const ITERATIONS: u32 = 30;
        let mut id_to_idx = vec![usize::MAX; self.next_node_id_hint()];
        for (i, n) in self.nodes.iter().enumerate() {
            let id = n.id as usize;
            if id < id_to_idx.len() {
                id_to_idx[id] = i;
            } else {
                id_to_idx.resize(id + 1, usize::MAX);
                id_to_idx[id] = i;
            }
        }
        let mut lambda: Vec<f32> = vec![0.0; self.beams.len()];
        // One tire-vertical lambda per wheel hub axle node (2 per wheel).
        let mut tire_lambda: Vec<f32> = vec![0.0; self.wheels.len() * 2];
        let dt2_inv = 1.0 / (dt * dt);
        // Tire vertical stiffness per axle node. Per-wheel effective
        // k = 2 × 50 = 100 kN/m → static pen ~3.7 cm for a midsize
        // car. This soft tire keeps the hub reliably below ground level
        // even under chassis oscillation, so wheels stay grounded
        // throughout driving. Matches `sedan.rs` pre-shift.
        const TIRE_K: f32 = 50_000.0;
        let tire_alpha_tilde = dt2_inv / TIRE_K;
        // Phase 2.6 — softer per-ring-node spring (deformable tread).
        // Effective per-wheel ring stiffness ≈ N_ring × 25 kN/m. With
        // 12 ring nodes that's 300 kN/m raw, but only ~3-4 nodes are in
        // contact at any moment so the effective contact-patch spring is
        // closer to 80–100 kN/m, similar in order to the hub spring but
        // delivered through the deforming ring.
        const RING_K: f32 = 25_000.0;
        let ring_alpha_tilde = dt2_inv / RING_K;
        // Pre-resolve hub axle node indices so we don't search per-iter.
        let hub_node_idx: Vec<(usize, usize)> = self
            .wheels
            .iter()
            .map(|w| {
                let i1 = *id_to_idx.get(w.axle_n1 as usize).unwrap_or(&usize::MAX);
                let i2 = *id_to_idx.get(w.axle_n2 as usize).unwrap_or(&usize::MAX);
                (i1, i2)
            })
            .collect();
        // Pre-resolve ring node indices for TireRing-mode wheels.
        // Wheels in Hub mode get an empty ring-idx vector → zero ring
        // constraint contribution, preserving classic behaviour.
        let ring_node_idx: Vec<Vec<usize>> = self
            .wheels
            .iter()
            .map(|w| {
                if !matches!(w.contact_mode, WheelContactMode::TireRing) {
                    return Vec::new();
                }
                w.tire_nodes
                    .iter()
                    .filter_map(|id| {
                        let i = *id_to_idx.get(*id as usize).unwrap_or(&usize::MAX);
                        if i == usize::MAX { None } else { Some(i) }
                    })
                    .collect()
            })
            .collect();
        // Per-ring-node lambda (one per ring node across all wheels).
        let total_ring_nodes: usize = ring_node_idx.iter().map(|v| v.len()).sum();
        let mut ring_lambda: Vec<f32> = vec![0.0; total_ring_nodes];
        // Offsets into ring_lambda — wheel `wi` occupies the slice
        // [ring_lambda_offsets[wi]..ring_lambda_offsets[wi+1]).
        let mut ring_lambda_offsets: Vec<usize> = Vec::with_capacity(self.wheels.len() + 1);
        let mut acc = 0usize;
        for v in &ring_node_idx {
            ring_lambda_offsets.push(acc);
            acc += v.len();
        }
        ring_lambda_offsets.push(acc);
        for _ in 0..ITERATIONS {
            // ── Tire vertical constraints (unilateral, per hub axle node) ──
            for (wi, w) in self.wheels.iter().enumerate() {
                let (i1, i2) = hub_node_idx[wi];
                for (local, &i) in [i1, i2].iter().enumerate() {
                    if i == usize::MAX {
                        continue;
                    }
                    let s = ground.sample(predicted[i].x, predicted[i].z);
                    let target_y = s.height + w.radius;
                    let pen = target_y - predicted[i].y;
                    if pen <= 0.0 {
                        // Hub above ground — no tire contact.
                        continue;
                    }
                    let w_node = self.nodes[i].inv_mass;
                    if w_node < 1e-10 {
                        continue;
                    }
                    let lk = wi * 2 + local;
                    // Constraint C = target_y - hub_y (positive = penetrating).
                    // To resolve, we PUSH UP (predicted[i].y += correction).
                    let dlambda =
                        (pen - tire_alpha_tilde * tire_lambda[lk]) / (w_node + tire_alpha_tilde);
                    // Unilateral: lambda must remain non-negative.
                    let new_lambda = (tire_lambda[lk] + dlambda).max(0.0);
                    let actual_dlambda = new_lambda - tire_lambda[lk];
                    tire_lambda[lk] = new_lambda;
                    predicted[i].y += actual_dlambda * w_node;
                }
            }
            // ── Phase 2.6: ring-vertical constraints (TireRing mode only) ──
            // Each ring node carries its own one-sided spring against
            // the ground. Soft per-node spring (RING_K) lets the ring
            // deform: a few nodes carry the load, the rest stay above
            // ground. The hub spring above remains active so the wheel
            // is supported at both rim and tread.
            for wi in 0..self.wheels.len() {
                let ring_idx = &ring_node_idx[wi];
                if ring_idx.is_empty() {
                    continue;
                }
                let off = ring_lambda_offsets[wi];
                for (k, &i) in ring_idx.iter().enumerate() {
                    let s = ground.sample(predicted[i].x, predicted[i].z);
                    let pen = s.height - predicted[i].y;
                    if pen <= 0.0 {
                        continue;
                    }
                    let w_node = self.nodes[i].inv_mass;
                    if w_node < 1e-10 {
                        continue;
                    }
                    let lk = off + k;
                    let dlambda =
                        (pen - ring_alpha_tilde * ring_lambda[lk]) / (w_node + ring_alpha_tilde);
                    let new_lambda = (ring_lambda[lk] + dlambda).max(0.0);
                    let actual_dlambda = new_lambda - ring_lambda[lk];
                    ring_lambda[lk] = new_lambda;
                    predicted[i].y += actual_dlambda * w_node;
                }
            }
            for (bi, b) in self.beams.iter().enumerate() {
                if b.broken {
                    continue;
                }
                let i1 = *id_to_idx.get(b.n1 as usize).unwrap_or(&usize::MAX);
                let i2 = *id_to_idx.get(b.n2 as usize).unwrap_or(&usize::MAX);
                if i1 == usize::MAX || i2 == usize::MAX {
                    continue;
                }
                let w1 = self.nodes[i1].inv_mass;
                let w2 = self.nodes[i2].inv_mass;
                let w_sum = w1 + w2;
                if w_sum < 1e-10 {
                    continue;
                }
                let p1 = predicted[i1];
                let p2 = predicted[i2];
                let delta = p2 - p1;
                let len = delta.length();
                if len < 1e-6 {
                    continue;
                }
                let rest = b.live_rest_length(0.0).max(1e-6);

                // Bounded beams: skip projection when length is inside the
                // idle range. Outside, project to the nearer bound.
                let target = match b.beam_type {
                    BeamType::Bounded {
                        min_ratio,
                        max_ratio,
                    } => {
                        let ratio = len / rest;
                        if ratio >= min_ratio && ratio <= max_ratio {
                            continue;
                        }
                        if ratio < min_ratio {
                            rest * min_ratio
                        } else {
                            rest * max_ratio
                        }
                    }
                    BeamType::Support => {
                        // Compression-only: skip when stretched.
                        if len >= rest {
                            continue;
                        }
                        rest
                    }
                    _ => rest,
                };

                let c = len - target;
                // XPBD lambda update with compliance α/dt² = 1/(k * dt²).
                let alpha_tilde = dt2_inv / b.spring.max(1.0);
                let dlambda = (-c - alpha_tilde * lambda[bi]) / (w_sum + alpha_tilde);
                lambda[bi] += dlambda;
                let dir = delta / len;
                predicted[i1] -= dir * (dlambda * w1);
                predicted[i2] += dir * (dlambda * w2);
            }
        }

        // ── Phase D: Velocity from corrected positions ──
        //
        // v_new = (x_pred - x_old) / dt. The previous version used 0.998
        // per substep which compounded to 0.937 per frame (6.3 % loss),
        // capping cruise speed at ~2 km/h regardless of throttle. Bumped
        // to 0.99995 → only 0.0017 % per frame, equivalent to a tiny
        // air-drag-like dissipation.
        const VEL_DAMPING: f32 = 0.99995;
        for (i, n) in self.nodes.iter_mut().enumerate() {
            if n.is_fixed() {
                continue;
            }
            let new_pos = predicted[i];
            n.velocity = (new_pos - n.position) / dt;
            n.velocity *= VEL_DAMPING;
            n.position = new_pos;
        }

        // ── Phase E: Plastic deformation update post-projection ──
        for b in self.beams.iter_mut() {
            if b.broken {
                continue;
            }
            let i1 = *id_to_idx.get(b.n1 as usize).unwrap_or(&usize::MAX);
            let i2 = *id_to_idx.get(b.n2 as usize).unwrap_or(&usize::MAX);
            if i1 == usize::MAX || i2 == usize::MAX {
                continue;
            }
            let len = (self.nodes[i2].position - self.nodes[i1].position).length();
            b.update_plastic(len);
        }

        // ── Phase F: Anti-tunnel ground clamp (body/cargo only) ──
        for n in &mut self.nodes {
            if n.is_fixed() {
                continue;
            }
            if matches!(n.group, NodeGroup::WheelHub | NodeGroup::WheelTire) {
                continue;
            }
            let s = ground.sample(n.position.x, n.position.z);
            if n.position.y < s.height {
                let dy = s.height - n.position.y;
                n.position.y = s.height;
                if n.velocity.y < 0.0 {
                    n.velocity.y = -n.velocity.y * n.restitution.max(0.10);
                }
                let fric = (n.friction * dy * 4.0).clamp(0.0, 0.5);
                n.velocity.x *= 1.0 - fric;
                n.velocity.z *= 1.0 - fric;
            }
        }
    }

    fn next_node_id_hint(&self) -> usize {
        self.nodes.iter().map(|n| n.id as usize).max().unwrap_or(0) + 1
    }

    fn apply_powertrain_and_tires(&mut self, dt: f32, ground: &dyn Ground) {
        let total_ratio = self.powertrain.gearbox.total_ratio();
        let shifting = self.powertrain.gearbox.shift_progress < 1.0;

        // 1. Compute engine torque and slipping clutch.
        let net_engine_torque = self.powertrain.engine.net_torque(self.controls.throttle);
        // Engine omega next = current + (net_torque - clutch_load) / inertia * dt.
        // We approximate clutch_load by requesting `gearbox_torque = T * ratio`
        // but capped by clutch capacity.
        let gearbox_input_omega = if total_ratio.abs() > 0.0 {
            // Average wheel omega -> back-projected through ratio.
            let avg_omega = self.average_drive_wheel_omega();
            avg_omega * total_ratio
        } else {
            self.powertrain.engine.omega // disengaged
        };

        let clutch_engagement =
            (1.0 - self.controls.clutch_pedal) * if shifting { 0.0 } else { 1.0 };
        self.powertrain.clutch.engagement = clutch_engagement;

        // Requested torque downstream = engine torque pushed through ratio.
        let requested = if total_ratio.abs() > 0.0 {
            net_engine_torque
        } else {
            0.0
        };
        let transmitted = self.powertrain.clutch.transmit(
            self.powertrain.engine.omega,
            gearbox_input_omega,
            requested,
        );

        // Step engine inertia: net_engine_torque - transmitted, plus a
        // kinematic clutch coupling term that pulls engine_omega toward
        // gearbox_input_omega when the clutch is closed. This is the
        // "rigid coupling at low slip" approximation real cars exhibit
        // via the clutch friction plates locking up.
        let engine_load = net_engine_torque - transmitted;
        self.powertrain.engine.omega += engine_load / self.powertrain.engine.inertia * dt;
        let engagement = self.powertrain.clutch.engagement;
        if engagement > 0.05 && total_ratio.abs() > 0.0 {
            // Pull engine omega toward gearbox_input_omega at a rate
            // proportional to engagement. With dt ≈ 0.5 ms and
            // coupling 6/s, lockup happens in ~0.5 s.
            //
            // NOTE 2026-05-06 (Phase 2.8 attempt): gating this block on
            // small slip seemed correct in isolation but it actually
            // reduced realised throttle — `Clutch::transmit` returns
            // `requested` (not `cap × sign(slip)`) when |requested| <=
            // cap, so without the kinematic pull-down the engine has no
            // mechanism to dump its own torque into the driveline. A
            // proper fix is a real friction-slip clutch
            // (transmitted = cap × engagement × sign(slip) when
            // |slip| > lockup band) rolled out together with a slipping-
            // engagement controller for launch. Tracked as Phase 2.8+
            // future work.
            let coupling = 6.0 * engagement;
            let target = gearbox_input_omega;
            let alpha = (coupling * dt).clamp(0.0, 1.0);
            self.powertrain.engine.omega =
                self.powertrain.engine.omega * (1.0 - alpha) + target * alpha;
        }
        // Idle floor — engine never falls below ~50 RPM unless ignition off.
        let min_omega = if self.controls.ignition && self.powertrain.engine.running {
            rpm_to_rad(50.0)
        } else {
            0.0
        };
        if self.powertrain.engine.omega < min_omega {
            self.powertrain.engine.omega = min_omega;
        }
        let max_omega = rpm_to_rad(self.powertrain.engine.max_rpm);
        if self.powertrain.engine.omega > max_omega {
            self.powertrain.engine.omega = max_omega;
        }

        // 2. Distribute torque to wheels.
        let shaft_torque = if total_ratio.abs() > 0.0 {
            transmitted * total_ratio
        } else {
            0.0
        };
        let wheel_omegas = self.fetch_wheel_omegas();
        let split = self.powertrain.distribute(shaft_torque, wheel_omegas);

        // 3. Per-wheel: integrate spin, evaluate Pacejka, apply ground force.
        let chassis_fwd = self.chassis_forward;
        let chassis_up = self.chassis_up;
        let brake_torque_total = self.brake_torque_max();
        let handbrake_torque_total = self.handbrake_torque_max();

        for (i, w) in self.wheels.iter_mut().enumerate() {
            let drive_t = match i {
                0 => split[0].0, // FL
                1 => split[0].1, // FR
                2 => split[1].0, // RL
                3 => split[1].1, // RR
                _ => 0.0,
            };
            w.drive_torque = drive_t;

            // Brake torque: foot brake all 4, handbrake rear only.
            let mut brake_t = brake_torque_total * self.controls.brake;
            if i >= 2 {
                brake_t += handbrake_torque_total * self.controls.handbrake;
            }
            w.brake_torque = brake_t;

            // Steered yaw applied only to front wheels.
            if i < 2 {
                w.steer_angle = self.controls.steer * w.max_steer_angle;
            }

            let (forward, left, _up) = wheel_frame(chassis_fwd, chassis_up, w.steer_angle);

            // Hub centre: midpoint of the two axle nodes.
            let (a1, a2) = match (
                self.nodes.iter().position(|n| n.id == w.axle_n1),
                self.nodes.iter().position(|n| n.id == w.axle_n2),
            ) {
                (Some(a), Some(b)) => (a, b),
                _ => continue,
            };
            let hub_pos = (self.nodes[a1].position + self.nodes[a2].position) * 0.5;
            let hub_vel = (self.nodes[a1].velocity + self.nodes[a2].velocity) * 0.5;

            // Sample ground at the contact point (hub_pos shifted down by radius).
            let sample = ground.sample(hub_pos.x, hub_pos.z);
            let ground_y = sample.height;
            let contact_y = hub_pos.y - w.radius;
            let penetration = ground_y - contact_y;

            if penetration <= 0.0 {
                w.grounded = false;
                // Free spin: brake decelerates, drive accelerates.
                let net_t = drive_t - brake_t * w.angular_velocity.signum();
                w.angular_velocity += net_t / w.spin_inertia.max(0.01) * dt;
                continue;
            }
            w.grounded = true;

            // Tire vertical compliance — softer than the chassis frame
            // so XPBD can balance it inside 10 iterations. 80 kN/m × 5 cm
            // pen = 4 kN/wheel which matches static load of a midsize car.
            let v_normal = -hub_vel.y;
            let fz = (penetration * 80_000.0 + v_normal.max(0.0) * 2_000.0).clamp(0.0, 30_000.0);

            // Velocity components in heading frame.
            let vx = hub_vel.dot(forward);
            let vy = hub_vel.dot(left);
            let vs = w.angular_velocity * w.radius;

            let mut forces = pacejka_force(&w.tire, ContactInputs { fz, vx, vy, vs });
            forces.fx *= sample.grip_modifier * sample.friction_mu;
            forces.fy *= sample.grip_modifier * sample.friction_mu;
            w.last_slip_ratio = forces.slip_ratio;
            w.last_slip_angle = forces.slip_angle;

            // Apply ONLY horizontal Pacejka forces (fx longitudinal, fy
            // lateral) explicitly. Vertical fz is handled inside the
            // XPBD iteration as a unilateral ground constraint, sharing
            // the same projection loop as the suspension beams so the
            // two systems stay in lock-step force balance.
            let f_world = forward * forces.fx + left * forces.fy;
            match w.contact_mode {
                WheelContactMode::Hub => {
                    let half = f_world * 0.5;
                    self.nodes[a1].force += half;
                    self.nodes[a2].force += half;
                }
                WheelContactMode::TireRing => {
                    // 40% retained on the axle pair (engine torque +
                    // chassis coupling); 60% routed through the ring
                    // contact patch so the tire deforms under load and
                    // body-side pillars feel the road through the ring
                    // beams.
                    const HUB_SHARE: f32 = 0.40;
                    const RING_SHARE: f32 = 0.60;
                    let hub_half = f_world * (HUB_SHARE * 0.5);
                    self.nodes[a1].force += hub_half;
                    self.nodes[a2].force += hub_half;

                    // Find the ring node closest to the ground at this
                    // wheel's contact point (lowest sample-relative Y).
                    let mut best_idx: Option<usize> = None;
                    let mut best_pen: f32 = -f32::INFINITY;
                    for &rid in &w.tire_nodes {
                        let i = match self.nodes.iter().position(|nn| nn.id == rid) {
                            Some(i) => i,
                            None => continue,
                        };
                        let np = self.nodes[i].position;
                        let s = ground.sample(np.x, np.z);
                        let pen = s.height - np.y;
                        if pen > best_pen {
                            best_pen = pen;
                            best_idx = Some(i);
                        }
                    }
                    if let Some(centre_i) = best_idx {
                        // Identify the centre node's slot in `tire_nodes`
                        // so we can grab its two angular neighbours.
                        let centre_node_id = self.nodes[centre_i].id;
                        let n_ring = w.tire_nodes.len();
                        let centre_slot = w
                            .tire_nodes
                            .iter()
                            .position(|&id| id == centre_node_id)
                            .unwrap_or(0);
                        let prev_slot = (centre_slot + n_ring - 1) % n_ring;
                        let next_slot = (centre_slot + 1) % n_ring;
                        let prev_id = w.tire_nodes[prev_slot];
                        let next_id = w.tire_nodes[next_slot];
                        let prev_i = self.nodes.iter().position(|nn| nn.id == prev_id);
                        let next_i = self.nodes.iter().position(|nn| nn.id == next_id);
                        // 50 / 25 / 25 split — centre carries most of
                        // the patch, neighbours catch the bleed.
                        self.nodes[centre_i].force += f_world * (RING_SHARE * 0.50);
                        if let Some(pi) = prev_i {
                            self.nodes[pi].force += f_world * (RING_SHARE * 0.25);
                        }
                        if let Some(ni) = next_i {
                            self.nodes[ni].force += f_world * (RING_SHARE * 0.25);
                        }
                    } else {
                        // No ring node near the ground — fall back to
                        // the hub split to keep force balance.
                        let half = f_world * 0.5;
                        self.nodes[a1].force += half;
                        self.nodes[a2].force += half;
                    }
                }
            }

            // Counter-torque on the wheel from longitudinal force:
            // omega_dot = (drive - brake_dir - fx*r) / I.
            let brake_dir = brake_t * w.angular_velocity.signum();
            let road_torque = forces.fx * w.radius;
            let net_t = drive_t - brake_dir - road_torque;
            w.angular_velocity += net_t / w.spin_inertia.max(0.01) * dt;

            // Lock detection: brake torque larger than what physics asks for at v=0.
            if brake_t > 0.0 && vx.abs() < 0.3 {
                w.angular_velocity *= 0.5; // strong damping near zero
            }
        }
    }

    fn average_drive_wheel_omega(&self) -> f32 {
        use crate::powertrain::DrivelineLayout::*;
        let n = self.wheels.len();
        if n == 0 {
            return 0.0;
        }
        match self.powertrain.layout {
            Fwd => avg(&[self.wheels.get(0), self.wheels.get(1)]),
            Rwd => avg(&[self.wheels.get(2), self.wheels.get(3)]),
            Awd { .. } => avg(&[
                self.wheels.get(0),
                self.wheels.get(1),
                self.wheels.get(2),
                self.wheels.get(3),
            ]),
        }
    }

    fn fetch_wheel_omegas(&self) -> [(f32, f32); 2] {
        let g = |i: usize| -> f32 {
            self.wheels
                .get(i)
                .map(|w| w.angular_velocity)
                .unwrap_or(0.0)
        };
        [(g(0), g(1)), (g(2), g(3))]
    }

    fn brake_torque_max(&self) -> f32 {
        // Calibrated so that ~1.0 brake input gives full lock at low speed.
        2400.0
    }

    fn handbrake_torque_max(&self) -> f32 {
        2200.0
    }

    fn apply_node_ground_contact(&mut self, ground: &dyn Ground) {
        use crate::node::NodeGroup;
        // Body/cargo nodes get a stiff bouncy ground contact when they
        // dip into the floor — this catches the chassis if the
        // suspension can't hold it up and prevents the car from sitting
        // on its belly.
        for n in &mut self.nodes {
            if n.is_fixed() {
                continue;
            }
            if !matches!(n.group, NodeGroup::Body | NodeGroup::Cargo) {
                continue;
            }
            let s = ground.sample(n.position.x, n.position.z);
            let pen = s.height - n.position.y;
            if pen < 0.0 {
                continue;
            }
            let stiffness = (200_000.0_f32 * n.mass.max(1.0)).min(2_000_000.0);
            let damping = (1_500.0_f32 * n.mass.max(1.0)).min(15_000.0);
            let v_along_n = n.velocity.dot(s.normal);
            let normal_force = (pen * stiffness - v_along_n * damping).max(0.0);
            n.force += s.normal * normal_force;
            let v_tangent = n.velocity - s.normal * v_along_n;
            let v_t_speed = v_tangent.length();
            if v_t_speed > 1e-3 {
                let max_friction = n.friction * s.friction_mu * normal_force;
                let f_friction = -v_tangent / v_t_speed * max_friction.min(v_t_speed * 200.0);
                n.force += f_friction;
            }
        }
    }

    /// Convenience getter — current engine RPM.
    pub fn engine_rpm(&self) -> f32 {
        rad_to_rpm(self.powertrain.engine.omega)
    }

    /// Convenience getter — current ground speed (m/s).
    pub fn speed(&self) -> f32 {
        self.body_velocity().length()
    }

    /// Pre-warm the simulation with `seconds` of zero-input physics so the
    /// vehicle is at static equilibrium before the user takes the wheel.
    /// Avoids the impulsive load of an uncompressed suspension at frame 1.
    pub fn settle(&mut self, ground: &dyn Ground, seconds: f32) {
        let saved = self.controls;
        self.controls = Controls::coast();
        let mut t = 0.0;
        while t < seconds {
            self.step(1.0 / 240.0, ground);
            t += 1.0 / 240.0;
        }
        self.controls = saved;
    }
}

fn avg(opts: &[Option<&Wheel>]) -> f32 {
    let mut sum = 0.0;
    let mut n = 0;
    for o in opts {
        if let Some(w) = o {
            sum += w.angular_velocity;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum / n as f32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beam::Beam;
    use crate::ground::FlatGround;
    use crate::node::Node;

    #[test]
    fn empty_vehicle_steps_without_panicking() {
        let mut v = Vehicle::new("empty");
        let g = FlatGround::new(0.0);
        v.step(1.0 / 60.0, &g);
        assert_eq!(v.step_count, 1);
    }

    #[test]
    fn falling_node_accelerates_under_gravity() {
        let mut v = Vehicle::new("falling-node");
        v.add_node(Node::new(0, Vec3::new(0.0, 5.0, 0.0), 1.0).with_friction(0.0));
        let g = FlatGround::new(-100.0); // far below, so no contact
        let v0 = v.nodes[0].velocity.y;
        v.step(0.10, &g);
        assert!(v.nodes[0].velocity.y < v0); // moving down (negative y)
        assert!(v.nodes[0].velocity.y < -0.5);
    }

    #[test]
    fn beam_pulls_nodes_back_to_rest_length() {
        use crate::beam::DeformParams;
        let mut v = Vehicle::new("two-node-beam");
        v.add_node(
            Node::new(0, Vec3::ZERO, 1.0)
                .with_friction(0.0)
                .with_drag(0.0),
        );
        v.add_node(
            Node::new(1, Vec3::new(1.05, 0.0, 0.0), 1.0)
                .with_friction(0.0)
                .with_drag(0.0),
        );
        // Critically damped: d = 2*sqrt(k*m). Disable plastic deformation
        // for this elastic-only test.
        let k = 5_000.0_f32;
        let m = 1.0_f32;
        let d = 2.0 * (k * m).sqrt();
        let mut beam = Beam::new(0, 0, 1, 1.0, k, d);
        beam.deform = DeformParams {
            deform_limit: 5.0,
            break_limit: 10.0,
            max_plastic_strain: 0.0,
        };
        v.add_beam(beam);
        v.integrator.gravity = Vec3::ZERO;

        let g = FlatGround::new(-100.0);
        for _ in 0..200 {
            v.step(1.0 / 60.0, &g);
        }
        let dist = (v.nodes[1].position - v.nodes[0].position).length();
        assert!((dist - 1.0).abs() < 0.02, "distance {} not near 1.0", dist);
    }

    #[test]
    fn anchor_node_does_not_move() {
        let mut v = Vehicle::new("anchor");
        v.add_node(Node::anchor(0, Vec3::new(0.0, 1.0, 0.0)));
        let g = FlatGround::new(0.0);
        for _ in 0..10 {
            v.step(1.0 / 60.0, &g);
        }
        assert!((v.nodes[0].position - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn break_group_breaks_only_matching_beams() {
        let mut v = Vehicle::new("break-test");
        v.add_node(Node::new(0, Vec3::ZERO, 1.0));
        v.add_node(Node::new(1, Vec3::new(1.0, 0.0, 0.0), 1.0));
        v.add_node(Node::new(2, Vec3::new(2.0, 0.0, 0.0), 1.0));
        v.add_beam(Beam::new(0, 0, 1, 1.0, 1000.0, 10.0).with_break_group(7));
        v.add_beam(Beam::new(1, 1, 2, 1.0, 1000.0, 10.0).with_break_group(7));
        v.add_beam(Beam::new(2, 0, 2, 2.0, 1000.0, 10.0).with_break_group(99));

        let n = v.break_group(7);
        assert_eq!(n, 2);
        assert!(v.beams[0].broken);
        assert!(v.beams[1].broken);
        assert!(!v.beams[2].broken);
    }
}
