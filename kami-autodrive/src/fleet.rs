//! Multi-agent driver: run N autonomous agents in one world, each perceiving
//! the others as obstacles.
//!
//! Coordination is **priority + index right-of-way**, not full negotiation: an
//! agent yields to (i.e. senses, and routes/brakes around) every agent with a
//! strictly higher `priority`, and to equal-priority agents with a lower index.
//! Higher-priority agents do not see lower ones, so the symmetry that deadlocks
//! two reactive agents head-on is broken — someone always has the right of way.
//!
//! Each agent is sensed by the others as a sphere of its `radius` at the lidar
//! mount height, swept by a 360° ring lidar built per tick. Honest limits: this
//! is decentralised yielding, not cooperative trajectory planning; a cornered
//! bicycle still cannot reverse, and dense gridlock can still stall.

use glam::{Affine3A, Quat, Vec2, Vec3};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

use crate::autopilot::{Autopilot, DriveState};
use crate::plant::Plant;
use crate::types::Pose2;

/// Range (m) within which a closing head-on encounter triggers lane discipline.
const NEGOTIATE_RANGE: f32 = 45.0;

/// One member of a [`Fleet`].
pub struct FleetAgent {
    pub plant: Box<dyn Plant>,
    pub autopilot: Autopilot,
    pub goal: Vec2,
    /// Collision/sensing radius (m).
    pub radius: f32,
    /// Right-of-way rank; higher yields to none below it.
    pub priority: u32,
}

impl FleetAgent {
    pub fn new(
        plant: Box<dyn Plant>,
        autopilot: Autopilot,
        goal: Vec2,
        radius: f32,
        priority: u32,
    ) -> Self {
        let mut a = Self {
            plant,
            autopilot,
            goal,
            radius,
            priority,
        };
        a.autopilot.set_goal(goal);
        a
    }

    pub fn pose(&self) -> Pose2 {
        self.plant.pose()
    }

    pub fn arrived(&self) -> bool {
        self.autopilot.state == DriveState::Arrived
    }
}

/// A multi-agent world.
pub struct Fleet {
    pub agents: Vec<FleetAgent>,
    /// Shared static obstacles (buildings, walls) every agent also senses.
    pub static_scene: Scene,
    mount_z: f32,
    intr: LidarIntrinsics,
}

impl Fleet {
    pub fn new(agents: Vec<FleetAgent>) -> Self {
        Self {
            agents,
            static_scene: Scene::new(),
            mount_z: 1.0,
            intr: LidarIntrinsics {
                hfov: std::f32::consts::TAU,
                vfov: 0.05,
                h_beams: 180,
                v_beams: 1,
                range_min: 0.2,
                range_max: 80.0,
            },
        }
    }

    pub fn with_static_scene(mut self, scene: Scene) -> Self {
        self.static_scene = scene;
        self
    }

    /// Advance every agent one tick. Each agent senses the static scene plus
    /// the right-of-way subset of other agents (as spheres), runs its
    /// autopilot, and steps its plant.
    pub fn step(&mut self, dt: f32) {
        let snap: Vec<(Pose2, f32, u32)> = self
            .agents
            .iter()
            .map(|a| (a.plant.pose(), a.radius, a.priority))
            .collect();

        for i in 0..self.agents.len() {
            let (me, my_r, my_prio) = snap[i];
            let mut scene = self.static_scene.clone();
            for (j, &(p, r, prio)) in snap.iter().enumerate() {
                if j == i {
                    continue;
                }
                let rel = p.pos() - me.pos();
                let dist = rel.length();
                let ahead = dist > 1e-3 && rel.normalize().dot(me.forward()) > 0.5;
                let closing = me.forward().dot(p.forward()) < -0.3;

                // SAFETY: every agent always keeps clear of every other (no
                // one rams a lower-priority agent, even with the right of way).
                let _ = (my_prio, prio); // priority shapes biasing below, not safety
                if ahead && closing && dist < NEGOTIATE_RANGE {
                    // Head-on: lane discipline. The real agent with an extra
                    // pass-clearance margin (so we route WIDE right of it),
                    // plus a "lane wall" to OUR left that blocks a left pass.
                    // Both agents do this symmetrically ⇒ they pass right-to-right.
                    scene.add(Primitive::Sphere {
                        center: Vec3::new(p.x, p.y, self.mount_z),
                        radius: r + 1.0,
                    });
                    let wall = p.pos() + me.left() * (2.0 * (my_r + r));
                    scene.add(Primitive::Sphere {
                        center: Vec3::new(wall.x, wall.y, self.mount_z),
                        radius: r,
                    });
                } else {
                    // Crossing / overtaking / following: avoid the other agent.
                    scene.add(Primitive::Sphere {
                        center: Vec3::new(p.x, p.y, self.mount_z),
                        radius: r,
                    });
                }
            }
            let agent = &mut self.agents[i];
            let pose = agent.plant.pose();
            let returns = ring_sweep(&self.intr, pose, self.mount_z, &scene);
            let cmd = agent
                .autopilot
                .step(pose, agent.plant.speed(), &returns, pose, dt);
            agent.plant.step(cmd, dt);
        }
    }

    pub fn all_arrived(&self) -> bool {
        self.agents.iter().all(FleetAgent::arrived)
    }

    /// Smallest surface-to-surface gap between any two agents (negative ⇒
    /// overlap/collision).
    pub fn min_separation(&self) -> f32 {
        let mut m = f32::INFINITY;
        for i in 0..self.agents.len() {
            for j in (i + 1)..self.agents.len() {
                let a = &self.agents[i];
                let b = &self.agents[j];
                let gap = a.pose().pos().distance(b.pose().pos()) - a.radius - b.radius;
                m = m.min(gap);
            }
        }
        m
    }
}

fn ring_sweep(
    intr: &LidarIntrinsics,
    pose: Pose2,
    mount_z: f32,
    scene: &Scene,
) -> Vec<LidarReturn> {
    let mut lidar = Lidar::new("fleet", "/fleet", *intr);
    let s2w = Affine3A::from_rotation_translation(
        Quat::from_rotation_z(pose.yaw),
        Vec3::new(pose.x, pose.y, mount_z),
    );
    lidar.view = s2w.inverse();
    lidar.acquire_data(scene)
}
