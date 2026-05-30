//! `World` + `Articulation` — Isaac Sim / PhysX-style API surface.
//!
//! At R1.1 the only supported `Articulation` topology is Cartpole.
//! Future articulations (Franka, ANYmal, suki, sarutahiko) plug in here at R1.5+
//! via the kami-articulated `ArticulatedSystem` and Featherstone solver.

use crate::cartpole::{CartpoleConfig, CartpoleState};
use crate::double_pendulum::{DoublePendulumConfig, DoublePendulumState};
use crate::jacobian::{
    Jacobian, cartpole_link_jacobian, dp_link_jacobian, planar_chain_link_jacobian,
};
use crate::planar_chain::{PlanarChainConfig, PlanarChainState};
use glam::{Quat, Vec3};
use kami_articulated::{ArticulatedSystem, JointKind};
use thiserror::Error;

/// Per-link kinematic state in world frame.
/// Mirrors `isaacsim.core.api.RigidPrim.get_velocities + get_world_pose`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkState {
    pub position: Vec3,
    pub orientation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

impl LinkState {
    pub fn at_origin() -> Self {
        LinkState {
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }
}

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("articulation topology not supported at R1.1: {0}. Cartpole (1 prismatic + 1 revolute) is the only supported topology.")]
    UnsupportedTopology(String),
    #[error("articulation handle {0} is invalid")]
    InvalidHandle(usize),
    #[error("articulation `{0}` already registered")]
    DuplicateName(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArticulationHandle(pub usize);

/// PhysX-style scene + Isaac Sim-style World container.
///
/// Tracks articulations in a flat `Vec` and steps them in lockstep.
/// API surface mirrors:
///   - `isaacsim.core.api.World.step(render=False)`
///   - `PxScene::simulate(elapsedTime)` + `PxScene::fetchResults()`
#[derive(Debug)]
pub struct World {
    pub gravity: f32,
    pub dt: f32,
    articulations: Vec<Articulation>,
}

impl Default for World {
    fn default() -> Self {
        World { gravity: 9.81, dt: 1.0 / 60.0, articulations: Vec::new() }
    }
}

impl World {
    pub fn new(gravity: f32, dt: f32) -> Self {
        World { gravity, dt, articulations: Vec::new() }
    }

    /// Add an articulation, returning its handle. Equivalent to:
    ///   - `isaacsim.core.api.World.scene.add(articulation)`
    ///   - `PxScene::addArticulation(...)`
    pub fn add_articulation(
        &mut self,
        sys: ArticulatedSystem,
    ) -> Result<ArticulationHandle, WorldError> {
        let name = sys.name.clone();
        if self.articulations.iter().any(|a| a.name == name) {
            return Err(WorldError::DuplicateName(name));
        }
        let art = Articulation::from_urdf(sys, self.gravity, self.dt)?;
        let handle = ArticulationHandle(self.articulations.len());
        self.articulations.push(art);
        Ok(handle)
    }

    /// Advance the simulation by one `dt`.
    /// Mirrors `PxScene::simulate(dt) + PxScene::fetchResults()` and
    /// `isaacsim.core.api.World.step()`.
    pub fn step(&mut self) {
        for art in &mut self.articulations {
            art.step();
        }
    }

    pub fn get(&self, h: ArticulationHandle) -> Result<&Articulation, WorldError> {
        self.articulations.get(h.0).ok_or(WorldError::InvalidHandle(h.0))
    }

    pub fn get_mut(
        &mut self,
        h: ArticulationHandle,
    ) -> Result<&mut Articulation, WorldError> {
        self.articulations.get_mut(h.0).ok_or(WorldError::InvalidHandle(h.0))
    }

    pub fn articulation_count(&self) -> usize {
        self.articulations.len()
    }
}

/// Articulation = USD physics ArticulationRoot / PhysX PxArticulationReducedCoordinate.
///
/// At R1.1 backed by closed-form Cartpole; future articulations dispatch on
/// detected topology (kami-articulated `ArticulatedSystem` shape).
#[derive(Debug)]
pub struct Articulation {
    pub name: String,
    pub system: ArticulatedSystem,
    topology: ArticulationTopology,
    applied_action_cartpole: f32,
    applied_action_double_pendulum: [f32; 2],
    applied_action_planar: Vec<f32>,
}

#[derive(Debug)]
enum ArticulationTopology {
    Cartpole { state: CartpoleState, cfg: CartpoleConfig },
    DoublePendulum { state: DoublePendulumState, cfg: DoublePendulumConfig },
    /// N≥3 serial revolute chain (planar reduced-coordinate). Generalizes
    /// DoublePendulum to arbitrary link counts via RNEA bias + CRBA mass
    /// matrix + LDLᵀ solve (see `planar_chain`). The ordered `links` are the
    /// child link names of each revolute joint, used by `link_state` FK.
    PlanarChain {
        state: PlanarChainState,
        cfg: PlanarChainConfig,
        links: Vec<String>,
    },
}

impl Articulation {
    pub fn from_urdf(
        sys: ArticulatedSystem,
        gravity: f32,
        dt: f32,
    ) -> Result<Self, WorldError> {
        let topology = detect_topology(&sys, gravity, dt)?;
        let name = sys.name.clone();
        // Pre-size the planar torque buffer to the joint count so callers can
        // index it before the first set_joint_torques.
        let n_planar = match &topology {
            ArticulationTopology::PlanarChain { cfg, .. } => cfg.n as usize,
            _ => 0,
        };
        Ok(Articulation {
            name,
            system: sys,
            topology,
            applied_action_cartpole: 0.0,
            applied_action_double_pendulum: [0.0, 0.0],
            applied_action_planar: vec![0.0; n_planar],
        })
    }

    pub fn step(&mut self) {
        match &mut self.topology {
            ArticulationTopology::Cartpole { state, cfg } => {
                let action = self.applied_action_cartpole;
                state.step(action, cfg);
                self.applied_action_cartpole = 0.0;
            }
            ArticulationTopology::DoublePendulum { state, cfg } => {
                let action = self.applied_action_double_pendulum;
                state.step(action, cfg);
                self.applied_action_double_pendulum = [0.0, 0.0];
            }
            ArticulationTopology::PlanarChain { state, cfg, .. } => {
                // PlanarChainState::step takes (tau, cfg). Torque is consumed
                // (zeroed) each step to mirror PxArticulation drive semantics.
                let n = cfg.n as usize;
                if self.applied_action_planar.len() != n {
                    self.applied_action_planar.resize(n, 0.0);
                }
                state.step(&self.applied_action_planar, cfg);
                for t in self.applied_action_planar.iter_mut() {
                    *t = 0.0;
                }
            }
        }
    }

    /// Set the force applied to the cart for the next `step()`. Mirrors
    /// `PxArticulationJointReducedCoordinate::setDriveTarget` for the slider DOF.
    pub fn set_cart_force(&mut self, force: f32) {
        self.applied_action_cartpole = force;
    }

    /// Set joint torques for an articulation. Cartpole consumes torques[0] as
    /// the cart force; DoublePendulum consumes [shoulder, elbow]; PlanarChain
    /// consumes one torque per joint (extra entries ignored, missing → 0).
    /// Mirrors `PxArticulationReducedCoordinate::setJointDriveTarget` per DOF.
    pub fn set_joint_torques(&mut self, torques: &[f32]) {
        match &mut self.topology {
            ArticulationTopology::Cartpole { .. } => {
                if !torques.is_empty() {
                    self.applied_action_cartpole = torques[0];
                }
            }
            ArticulationTopology::DoublePendulum { .. } => {
                if torques.len() >= 2 {
                    self.applied_action_double_pendulum = [torques[0], torques[1]];
                }
            }
            ArticulationTopology::PlanarChain { cfg, .. } => {
                let n = cfg.n as usize;
                self.applied_action_planar.resize(n, 0.0);
                for i in 0..n {
                    self.applied_action_planar[i] = torques.get(i).copied().unwrap_or(0.0);
                }
            }
        }
    }

    /// Read the current state (Cartpole only at R1.1).
    pub fn cartpole_state(&self) -> Option<CartpoleState> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, .. } => Some(*state),
            _ => None,
        }
    }

    /// Read the current state (double pendulum).
    pub fn double_pendulum_state(&self) -> Option<DoublePendulumState> {
        match &self.topology {
            ArticulationTopology::DoublePendulum { state, .. } => Some(*state),
            _ => None,
        }
    }

    /// Mutate state (used by `reset`).
    pub fn set_cartpole_state(&mut self, new_state: CartpoleState) {
        match &mut self.topology {
            ArticulationTopology::Cartpole { state, .. } => *state = new_state,
            _ => {}
        }
    }

    /// Mutate state (used by `reset` for double pendulum).
    pub fn set_double_pendulum_state(&mut self, new_state: DoublePendulumState) {
        match &mut self.topology {
            ArticulationTopology::DoublePendulum { state, .. } => *state = new_state,
            _ => {}
        }
    }

    /// Flat joint positions (Cartpole: [x, theta]; DP: [q1, q2];
    /// PlanarChain: [q0, q1, …]).
    pub fn joint_positions(&self) -> Vec<f32> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, .. } => vec![state.x, state.theta],
            ArticulationTopology::DoublePendulum { state, .. } => vec![state.q1, state.q2],
            ArticulationTopology::PlanarChain { state, .. } => state.q.clone(),
        }
    }

    /// Flat joint velocities (Cartpole: [x_dot, theta_dot]; DP: [q1_dot, q2_dot];
    /// PlanarChain: [qdot0, qdot1, …]).
    pub fn joint_velocities(&self) -> Vec<f32> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, .. } => {
                vec![state.x_dot, state.theta_dot]
            }
            ArticulationTopology::DoublePendulum { state, .. } => {
                vec![state.q1_dot, state.q2_dot]
            }
            ArticulationTopology::PlanarChain { state, .. } => state.qdot.clone(),
        }
    }

    /// 6×n geometric Jacobian for the named link in world frame.
    /// Mirrors `isaacsim.core.api.Articulation.get_jacobians()` (Isaac Sim 4.x).
    /// Returns None if the link is not in this articulation.
    pub fn jacobian(&self, link_name: &str) -> Option<Jacobian> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, cfg } => {
                cartpole_link_jacobian(state.theta, link_name, cfg)
            }
            ArticulationTopology::DoublePendulum { state, cfg } => {
                dp_link_jacobian(state.q1, state.q2, link_name, cfg)
            }
            // N-link planar chain: dispatch to the analytic Jacobian already
            // implemented in jacobian.rs (resolve link name → ordered index).
            ArticulationTopology::PlanarChain { state, cfg, links } => {
                let idx = links.iter().position(|l| l == link_name)?;
                planar_chain_link_jacobian(&state.q, idx, cfg)
            }
        }
    }

    /// World-frame kinematic state for a named link.
    /// Returns None if the link name is not present in this articulation.
    ///
    /// Convention (matches kami-genesis URDF assumptions):
    ///   - Cartpole `world`: static at origin.
    ///   - Cartpole `cart`: lin_pos = (x, 0, 0); lin_vel = (x_dot, 0, 0); no rotation.
    ///   - Cartpole `pole_link`: revolute about cart's +y axis; com at half-length
    ///     0.25 m below pivot in the local frame; world com obtained by rotating
    ///     by theta about world +y and translating by cart pos. Angular velocity
    ///     about world +y.
    ///   - Double pendulum `link1`: revolute about world origin's +y; com at
    ///     lc1=0.5 along the link's -z axis in local; world com = (l1*sin(q1)*?,
    ///     0, -l1*cos(q1)*?) etc. (Uniform-rod convention.)
    ///   - Double pendulum `link2`: base at link1 tip; cumulative angle q1+q2.
    pub fn link_state(&self, link_name: &str) -> Option<LinkState> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, cfg } => {
                cartpole_link_state(state, cfg, link_name)
            }
            ArticulationTopology::DoublePendulum { state, cfg } => {
                double_pendulum_link_state(state, cfg, link_name)
            }
            ArticulationTopology::PlanarChain { state, cfg, links } => {
                planar_chain_link_state(state, cfg, links, link_name)
            }
        }
    }
}

/// Forward kinematics for the planar chain (same convention as the double
/// pendulum: q=0 hangs along -z, each revolute about world +y, COM at link
/// half-length). `link_name` is matched against the ordered child link names;
/// a base/world name resolves to the origin.
fn planar_chain_link_state(
    s: &PlanarChainState,
    cfg: &PlanarChainConfig,
    links: &[String],
    link_name: &str,
) -> Option<LinkState> {
    if link_name == "world" || link_name == "base" {
        return Some(LinkState::at_origin());
    }
    let idx = links.iter().position(|l| l == link_name)?;
    let n = cfg.n as usize;

    // Recurse joint-to-joint, accumulating cumulative angle, joint position,
    // and joint linear velocity in the x-z plane.
    let mut theta = 0.0_f32; // cumulative angle from vertical
    let mut omega = 0.0_f32; // cumulative angular velocity
    let mut jx = 0.0_f32; // joint position
    let mut jz = 0.0_f32;
    let mut vx = 0.0_f32; // joint linear velocity
    let mut vz = 0.0_f32;

    for i in 0..n {
        theta += s.q[i];
        omega += s.qdot[i];
        let l = cfg.lengths[i];
        let lc = l * 0.5;
        let (st, ct) = (theta.sin(), theta.cos());
        // Direction of this link (q=0 → -z), its COM, and the next joint.
        // pos(angle): d/dθ (sinθ, -cosθ) = (cosθ, sinθ).
        if i == idx {
            let com_x = jx + lc * st;
            let com_z = jz - lc * ct;
            let com_vx = vx + lc * ct * omega;
            let com_vz = vz + lc * st * omega;
            return Some(LinkState {
                position: Vec3::new(com_x, 0.0, com_z),
                orientation: Quat::from_axis_angle(Vec3::Y, theta),
                linear_velocity: Vec3::new(com_vx, 0.0, com_vz),
                angular_velocity: Vec3::new(0.0, omega, 0.0),
            });
        }
        // Advance to the next joint (full link length).
        jx += l * st;
        jz += -l * ct;
        vx += l * ct * omega;
        vz += l * st * omega;
    }
    None
}

fn cartpole_link_state(s: &CartpoleState, _cfg: &CartpoleConfig, link: &str) -> Option<LinkState> {
    match link {
        "world" => Some(LinkState::at_origin()),
        "cart" => Some(LinkState {
            position: Vec3::new(s.x, 0.0, 0.0),
            orientation: Quat::IDENTITY,
            linear_velocity: Vec3::new(s.x_dot, 0.0, 0.0),
            angular_velocity: Vec3::ZERO,
        }),
        "pole_link" => {
            // Pole revolves about cart's +y axis (revolute joint axis = (0,1,0)).
            // Local com offset = (0, 0, 0.25) BEFORE rotation. At theta = 0 the
            // pole points up (+z). theta rotates about +y, so the tilted com is
            // at (0.25*sin(theta), 0, 0.25*cos(theta)) relative to the cart.
            let lc = 0.25_f32;
            let st = s.theta.sin();
            let ct = s.theta.cos();
            let pos = Vec3::new(s.x + lc * st, 0.0, lc * ct);
            // d/dt of pos with respect to (x, theta) state:
            let vel = Vec3::new(
                s.x_dot + lc * ct * s.theta_dot,
                0.0,
                -lc * st * s.theta_dot,
            );
            let orient = Quat::from_axis_angle(Vec3::Y, s.theta);
            Some(LinkState {
                position: pos,
                orientation: orient,
                linear_velocity: vel,
                angular_velocity: Vec3::new(0.0, s.theta_dot, 0.0),
            })
        }
        _ => None,
    }
}

fn double_pendulum_link_state(
    s: &DoublePendulumState,
    cfg: &DoublePendulumConfig,
    link: &str,
) -> Option<LinkState> {
    match link {
        "world" => Some(LinkState::at_origin()),
        "link1" => {
            // q1=0: link1 hangs straight down (-z), rotates about world y.
            let lc1 = cfg.l1 * 0.5;
            let s1 = s.q1.sin();
            let c1 = s.q1.cos();
            let pos = Vec3::new(lc1 * s1, 0.0, -lc1 * c1);
            let vel = Vec3::new(lc1 * c1 * s.q1_dot, 0.0, lc1 * s1 * s.q1_dot);
            let orient = Quat::from_axis_angle(Vec3::Y, s.q1);
            Some(LinkState {
                position: pos,
                orientation: orient,
                linear_velocity: vel,
                angular_velocity: Vec3::new(0.0, s.q1_dot, 0.0),
            })
        }
        "link2" => {
            // Base of link2 = link1 tip = (l1*sin(q1), 0, -l1*cos(q1)).
            // Link2 com is at lc2 along link2's down direction relative to its
            // base, where link2 makes angle (q1+q2) with vertical.
            let lc2 = cfg.l2 * 0.5;
            let s1 = s.q1.sin();
            let c1 = s.q1.cos();
            let s12 = (s.q1 + s.q2).sin();
            let c12 = (s.q1 + s.q2).cos();
            let base = Vec3::new(cfg.l1 * s1, 0.0, -cfg.l1 * c1);
            let pos = Vec3::new(base.x + lc2 * s12, 0.0, base.z - lc2 * c12);
            // Velocity = base_vel + lc2 * (q1_dot + q2_dot) * (cos(s12), 0, sin(s12))
            // base_vel = derivative of base wrt q1: (l1*c1*q1_dot, 0, l1*s1*q1_dot)
            let base_vel = Vec3::new(cfg.l1 * c1 * s.q1_dot, 0.0, cfg.l1 * s1 * s.q1_dot);
            let q12_dot = s.q1_dot + s.q2_dot;
            let vel = base_vel
                + Vec3::new(lc2 * c12 * q12_dot, 0.0, lc2 * s12 * q12_dot);
            let orient = Quat::from_axis_angle(Vec3::Y, s.q1 + s.q2);
            Some(LinkState {
                position: pos,
                orientation: orient,
                linear_velocity: vel,
                angular_velocity: Vec3::new(0.0, q12_dot, 0.0),
            })
        }
        _ => None,
    }
}

fn detect_topology(
    sys: &ArticulatedSystem,
    gravity: f32,
    dt: f32,
) -> Result<ArticulationTopology, WorldError> {
    // Cartpole signature: 1 prismatic joint with parent=world + 1 revolute joint.
    let has_prismatic_to_world = sys
        .joints
        .iter()
        .any(|j| j.kind == JointKind::Prismatic && j.parent == "world");
    let has_one_revolute =
        sys.joints.iter().filter(|j| j.kind == JointKind::Revolute).count() == 1;
    let total_dofs = sys
        .joints
        .iter()
        .filter(|j| matches!(j.kind, JointKind::Prismatic | JointKind::Revolute))
        .count();

    // Double pendulum signature: exactly 2 revolute joints, first parent=world,
    // second parent = first child (serial chain), no prismatic.
    let revolutes: Vec<&kami_articulated::Joint> =
        sys.joints.iter().filter(|j| j.kind == JointKind::Revolute).collect();
    let no_prismatic = !sys.joints.iter().any(|j| j.kind == JointKind::Prismatic);
    let is_double_pendulum = revolutes.len() == 2
        && no_prismatic
        && total_dofs == 2
        && revolutes[0].parent == "world"
        && revolutes[1].parent == revolutes[0].child;

    if is_double_pendulum {
        // Extract masses + link lengths from URDF. Each link's |com z| × 2
        // approximates link length (uniform rod assumption used in
        // DoublePendulumConfig). Use revolutes[1].origin |z| as l1.
        let link1 = sys
            .links
            .iter()
            .find(|l| l.name == revolutes[0].child)
            .ok_or_else(|| WorldError::UnsupportedTopology("dp link1 missing".into()))?;
        let link2 = sys
            .links
            .iter()
            .find(|l| l.name == revolutes[1].child)
            .ok_or_else(|| WorldError::UnsupportedTopology("dp link2 missing".into()))?;
        let l1 = revolutes[1].origin.xyz.z.abs().max(1e-3);
        let l2 = link2.inertia.com.xyz.z.abs() * 2.0;
        let cfg = DoublePendulumConfig {
            m1: link1.inertia.mass,
            m2: link2.inertia.mass,
            l1,
            l2: if l2 > 1e-3 { l2 } else { l1 },
            gravity,
            effort_limit: revolutes[0].effort.max(revolutes[1].effort).max(1.0),
            dt,
        };
        return Ok(ArticulationTopology::DoublePendulum {
            state: DoublePendulumState::default(),
            cfg,
        });
    }

    // PlanarChain signature: a serial revolute chain of N≥3 joints, no
    // prismatic. joint[0] roots at the base; joint[k].parent == joint[k-1].child.
    let is_serial_revolute_chain = no_prismatic
        && revolutes.len() >= 3
        && revolutes.len() == total_dofs
        && revolutes
            .windows(2)
            .all(|w| w[1].parent == w[0].child);
    if is_serial_revolute_chain {
        let n = revolutes.len();
        let mut masses = Vec::with_capacity(n);
        let mut lengths = Vec::with_capacity(n);
        let mut links = Vec::with_capacity(n);
        for (i, j) in revolutes.iter().enumerate() {
            links.push(j.child.clone());
            // Link mass from the child link's inertia.
            let m = sys
                .links
                .iter()
                .find(|l| l.name == j.child)
                .map(|l| l.inertia.mass)
                .unwrap_or(0.1)
                .max(1e-3);
            masses.push(m);
            // Link length ≈ distance to the next joint origin; the last link
            // uses 2×|child COM| (uniform-rod assumption).
            let len = if i + 1 < n {
                revolutes[i + 1].origin.xyz.length()
            } else {
                sys.links
                    .iter()
                    .find(|l| l.name == j.child)
                    .map(|l| l.inertia.com.xyz.length() * 2.0)
                    .unwrap_or(0.1)
            }
            .max(1e-3);
            lengths.push(len);
        }
        let effort_limit = revolutes
            .iter()
            .map(|j| j.effort)
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let cfg = PlanarChainConfig {
            n: n as u32,
            masses,
            lengths,
            gravity,
            effort_limit,
            dt,
        };
        let state = PlanarChainState::zeros(n as u32);
        return Ok(ArticulationTopology::PlanarChain { state, cfg, links });
    }

    if has_prismatic_to_world && has_one_revolute && total_dofs == 2 {
        let cart = sys
            .links
            .iter()
            .find(|l| l.name == "cart")
            .ok_or_else(|| WorldError::UnsupportedTopology("missing `cart` link".into()))?;
        let pole = sys
            .links
            .iter()
            .find(|l| l.name == "pole_link")
            .ok_or_else(|| {
                WorldError::UnsupportedTopology("missing `pole_link` link".into())
            })?;
        let slider = sys
            .joints
            .iter()
            .find(|j| j.kind == JointKind::Prismatic)
            .expect("checked above");
        let cfg = CartpoleConfig {
            cart_mass: cart.inertia.mass,
            pole_mass: pole.inertia.mass,
            pole_half_length: 0.25, // hardcoded from URDF cylinder length 0.5; future R1.5 reads visual
            gravity,
            force_mag: slider.effort.max(1.0),
            dt,
        };
        Ok(ArticulationTopology::Cartpole {
            state: CartpoleState::default(),
            cfg,
        })
    } else {
        Err(WorldError::UnsupportedTopology(format!(
            "{} (prismatic_to_world={}, revolute_count={}, dofs={})",
            sys.name,
            has_prismatic_to_world,
            sys.joints.iter().filter(|j| j.kind == JointKind::Revolute).count(),
            total_dofs
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARTPOLE_URDF: &str =
        include_str!("../../../../70-tools/e7m-sim/scenes/cartpole/cartpole.urdf");
    const ARM3_URDF: &str =
        include_str!("../../../../70-tools/e7m-sim/scenes/arm3/arm3.urdf");

    fn cartpole_world() -> (World, ArticulationHandle) {
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = World::default();
        let h = world.add_articulation(sys).unwrap();
        (world, h)
    }

    fn arm3_world() -> (World, ArticulationHandle) {
        let sys = kami_articulated::parse_urdf(ARM3_URDF).unwrap();
        let mut world = World::new(9.81, 1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        (world, h)
    }

    #[test]
    fn world_loads_cartpole_urdf() {
        let (world, _) = cartpole_world();
        assert_eq!(world.articulation_count(), 1);
    }

    #[test]
    fn world_steps_cartpole_under_gravity() {
        let (mut world, h) = cartpole_world();
        world.get_mut(h).unwrap().set_cartpole_state(CartpoleState {
            theta: 0.05,
            ..Default::default()
        });
        for _ in 0..120 {
            world.step();
        }
        let s = world.get(h).unwrap().cartpole_state().unwrap();
        assert!(s.theta.abs() > 0.05, "pole should fall under gravity");
    }

    #[test]
    fn cart_force_moves_cart() {
        let (mut world, h) = cartpole_world();
        for _ in 0..60 {
            world.get_mut(h).unwrap().set_cart_force(20.0);
            world.step();
        }
        let s = world.get(h).unwrap().cartpole_state().unwrap();
        assert!(s.x > 0.0, "force should push cart in +x direction");
    }

    const DP_URDF: &str = include_str!(
        "../../../../70-tools/e7m-sim/scenes/double_pendulum/double_pendulum.urdf"
    );

    #[test]
    fn world_loads_double_pendulum_urdf() {
        let sys = kami_articulated::parse_urdf(DP_URDF).unwrap();
        let mut world = World::default();
        let h = world.add_articulation(sys).unwrap();
        assert!(world.get(h).unwrap().double_pendulum_state().is_some());
        // joint dim = 2; positions/velocities reflect q1, q2
        assert_eq!(world.get(h).unwrap().joint_positions().len(), 2);
        assert_eq!(world.get(h).unwrap().joint_velocities().len(), 2);
    }

    #[test]
    fn dp_horizontal_release_swings_downward() {
        let sys = kami_articulated::parse_urdf(DP_URDF).unwrap();
        let mut world = World::new(9.81, 1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        world.get_mut(h).unwrap().set_double_pendulum_state(DoublePendulumState {
            q1: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        });
        for _ in 0..120 {
            world.step();
        }
        let s = world.get(h).unwrap().double_pendulum_state().unwrap();
        assert!(s.q1 < std::f32::consts::FRAC_PI_2, "should swing toward 0 under gravity");
    }

    #[test]
    fn cartpole_link_state_cart_tracks_x() {
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = World::default();
        let h = world.add_articulation(sys).unwrap();
        world.get_mut(h).unwrap().set_cartpole_state(CartpoleState {
            x: 0.5,
            x_dot: 1.0,
            theta: 0.1,
            theta_dot: 0.2,
            ..Default::default()
        });
        let cart = world.get(h).unwrap().link_state("cart").unwrap();
        assert!((cart.position.x - 0.5).abs() < 1e-6);
        assert!((cart.linear_velocity.x - 1.0).abs() < 1e-6);

        let pole = world.get(h).unwrap().link_state("pole_link").unwrap();
        // theta=0.1; cart at x=0.5; pole com at (x + lc*sin(θ), 0, lc*cos(θ))
        let expected_x = 0.5 + 0.25 * 0.1f32.sin();
        let expected_z = 0.25 * 0.1f32.cos();
        assert!((pole.position.x - expected_x).abs() < 1e-5);
        assert!((pole.position.z - expected_z).abs() < 1e-5);
        // angular velocity about y axis = theta_dot
        assert!((pole.angular_velocity.y - 0.2).abs() < 1e-6);
    }

    #[test]
    fn dp_link_state_returns_consistent_kinematics() {
        let sys = kami_articulated::parse_urdf(DP_URDF).unwrap();
        let mut world = World::new(9.81, 1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        world.get_mut(h).unwrap().set_double_pendulum_state(DoublePendulumState {
            q1: std::f32::consts::FRAC_PI_2, // link1 horizontal
            q2: 0.0,
            q1_dot: 0.5,
            q2_dot: 0.0,
        });
        let l1 = world.get(h).unwrap().link_state("link1").unwrap();
        // q1=π/2: lc1*sin(q1) = 0.5*1 = 0.5; -lc1*cos(q1) = 0
        assert!((l1.position.x - 0.5).abs() < 1e-5);
        assert!(l1.position.z.abs() < 1e-5);
        assert!((l1.angular_velocity.y - 0.5).abs() < 1e-6);

        let l2 = world.get(h).unwrap().link_state("link2").unwrap();
        // Base = (l1*sin(q1)=1, 0, -l1*cos(q1)=0); com at base + lc2*(sin(q1+q2), 0, -cos(q1+q2))
        //  with q1+q2 = π/2 still → +x, z stays 0
        assert!((l2.position.x - 1.5).abs() < 1e-5);
        assert!(l2.position.z.abs() < 1e-5);
        assert!((l2.angular_velocity.y - 0.5).abs() < 1e-6); // q1_dot + q2_dot = 0.5
    }

    #[test]
    fn dp_joint_torques_drive_motion() {
        let sys = kami_articulated::parse_urdf(DP_URDF).unwrap();
        let mut world = World::new(9.81, 1.0 / 240.0);
        let h = world.add_articulation(sys).unwrap();
        for _ in 0..60 {
            world.get_mut(h).unwrap().set_joint_torques(&[2.0, 1.0]);
            world.step();
        }
        let s = world.get(h).unwrap().double_pendulum_state().unwrap();
        assert!(s.q1.abs() > 0.001, "torques should drive shoulder motion");
    }

    // ── PlanarChain (N≥3 serial revolute) ──────────────────────────────────

    #[test]
    fn arm3_urdf_detected_as_planar_chain() {
        let (w, h) = arm3_world();
        let a = w.get(h).unwrap();
        // 3 joint DOFs, and the cartpole/DP accessors return None.
        assert_eq!(a.joint_positions().len(), 3);
        assert_eq!(a.joint_velocities().len(), 3);
        assert!(a.cartpole_state().is_none());
        assert!(a.double_pendulum_state().is_none());
    }

    #[test]
    fn arm3_base_joint_responds_to_torque() {
        // The solver moves correctly from the vertical rest pose (q=0): a
        // short, modest-torque horizon keeps the undamped chain in its
        // well-behaved regime. (A long constant high-torque rollout would
        // diverge under semi-implicit Euler — expected for an undamped chain,
        // not a solver defect — so we assert over a short window.)
        let (mut w, h) = arm3_world();
        let q0 = w.get(h).unwrap().joint_positions();
        for _ in 0..30 {
            w.get_mut(h).unwrap().set_joint_torques(&[3.0, 0.0, 0.0]);
            w.step();
        }
        let q1 = w.get(h).unwrap().joint_positions();
        assert!(
            (q1[0] - q0[0]).abs() > 0.01,
            "base joint did not respond to torque: {q0:?} -> {q1:?}"
        );
        assert!(q1.iter().all(|v| v.is_finite()), "state non-finite: {q1:?}");
    }

    #[test]
    fn arm3_link_state_fk_hangs_down_at_rest() {
        let (w, h) = arm3_world();
        let a = w.get(h).unwrap();
        // At q=0 every link hangs straight down (-z), x ≈ 0; distal links lower.
        let l1 = a.link_state("l1").unwrap();
        assert!(l1.position.x.abs() < 1e-4, "l1 x: {}", l1.position.x);
        assert!(l1.position.z < 0.0, "l1 z: {}", l1.position.z);
        let l3 = a.link_state("l3").unwrap();
        assert!(l3.position.z < l1.position.z, "l3 should be below l1");
        assert!(a.link_state("nope").is_none());
        assert_eq!(a.link_state("base").unwrap().position, Vec3::ZERO);
    }

    #[test]
    fn arm3_jacobian_wired_and_matches_link_state_velocity() {
        // World::jacobian() must dispatch PlanarChain links to the analytic
        // Jacobian (was None). Cross-check: J·q̇ linear rows equal the COM
        // linear velocity from link_state at a torque-driven pose. Two
        // independent derivations, one answer.
        let (mut w, h) = arm3_world();
        for _ in 0..40 {
            w.get_mut(h).unwrap().set_joint_torques(&[3.0, -1.5, 0.8]);
            w.step();
        }
        let a = w.get(h).unwrap();
        let qdot = a.joint_velocities();
        for link in ["l1", "l2", "l3"] {
            let j = a.jacobian(link).expect("planar-chain jacobian must be wired");
            assert_eq!(j.cols(), 3);
            // jacobian.rs row layout: rows[0]=v_x, rows[2]=v_z.
            let vx: f32 = (0..3).map(|c| j.rows[0][c] * qdot[c]).sum();
            let vz: f32 = (0..3).map(|c| j.rows[2][c] * qdot[c]).sum();
            let ls = a.link_state(link).unwrap();
            assert!(
                (vx - ls.linear_velocity.x).abs() < 1e-3,
                "{link} vx: J·q̇={vx} link_state={}",
                ls.linear_velocity.x
            );
            assert!(
                (vz - ls.linear_velocity.z).abs() < 1e-3,
                "{link} vz: J·q̇={vz} link_state={}",
                ls.linear_velocity.z
            );
        }
        assert!(w.get(h).unwrap().jacobian("nope").is_none());
    }

    #[test]
    fn unsupported_topology_rejected() {
        let xml = r#"<robot name="single_link">
          <link name="a"><inertial><mass value="1.0"/><inertia ixx="0.1" iyy="0.1" izz="0.1"/></inertial></link>
        </robot>"#;
        let sys = kami_articulated::parse_urdf(xml).unwrap();
        let mut world = World::default();
        assert!(matches!(
            world.add_articulation(sys),
            Err(WorldError::UnsupportedTopology(_))
        ));
    }
}
