//! `World` + `Articulation` — Isaac Sim / PhysX-style API surface.
//!
//! At R1.1 the only supported `Articulation` topology is Cartpole.
//! Future articulations (Franka, ANYmal, suki, sarutahiko) plug in here at R1.5+
//! via the kami-articulated `ArticulatedSystem` and Featherstone solver.

use crate::cartpole::{CartpoleConfig, CartpoleState};
use crate::double_pendulum::{DoublePendulumConfig, DoublePendulumState};
use kami_articulated::{ArticulatedSystem, JointKind};
use thiserror::Error;

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
}

#[derive(Debug)]
enum ArticulationTopology {
    Cartpole { state: CartpoleState, cfg: CartpoleConfig },
    DoublePendulum { state: DoublePendulumState, cfg: DoublePendulumConfig },
}

impl Articulation {
    pub fn from_urdf(
        sys: ArticulatedSystem,
        gravity: f32,
        dt: f32,
    ) -> Result<Self, WorldError> {
        let topology = detect_topology(&sys, gravity, dt)?;
        let name = sys.name.clone();
        Ok(Articulation {
            name,
            system: sys,
            topology,
            applied_action_cartpole: 0.0,
            applied_action_double_pendulum: [0.0, 0.0],
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
        }
    }

    /// Set the force applied to the cart for the next `step()`. Mirrors
    /// `PxArticulationJointReducedCoordinate::setDriveTarget` for the slider DOF.
    pub fn set_cart_force(&mut self, force: f32) {
        self.applied_action_cartpole = force;
    }

    /// Set joint torques for a double pendulum articulation [shoulder, elbow].
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

    /// Flat joint positions (Cartpole: [x, theta]; DP: [q1, q2]).
    pub fn joint_positions(&self) -> Vec<f32> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, .. } => vec![state.x, state.theta],
            ArticulationTopology::DoublePendulum { state, .. } => vec![state.q1, state.q2],
        }
    }

    /// Flat joint velocities (Cartpole: [x_dot, theta_dot]; DP: [q1_dot, q2_dot]).
    pub fn joint_velocities(&self) -> Vec<f32> {
        match &self.topology {
            ArticulationTopology::Cartpole { state, .. } => {
                vec![state.x_dot, state.theta_dot]
            }
            ArticulationTopology::DoublePendulum { state, .. } => {
                vec![state.q1_dot, state.q2_dot]
            }
        }
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

    fn cartpole_world() -> (World, ArticulationHandle) {
        let sys = kami_articulated::parse_urdf(CARTPOLE_URDF).unwrap();
        let mut world = World::default();
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
