//! Arm teleoperation simulation over the Isaac-parity (kami-genesis) world.
//!
//! Closes the loop: controller → [`TeleopMapper`] → [`SafetyEnvelope`] →
//! joint-velocity integration → joint-space PD effort → `IsaacWorld::step` →
//! [`TeleopMetrics`]. Uses only the clean-room `isaacsim.core.api` surface
//! (`set_joint_efforts` / `step` / `get_joint_positions` / `get_world_pose`),
//! so the same control code runs against NVIDIA Isaac Sim unchanged
//! (etzhayyim ADR-2605261800 §D10.1).

use crate::command::TeleopCommand;
use crate::mapping::TeleopMapper;
use crate::metrics::{TeleopAnalysis, TeleopMetrics};
use crate::safety::{SafeState, SafetyConfig, SafetyEnvelope};
use kami_articulated::{JointKind, parse_urdf};
use kami_genesis::{ArticulationHandle, IsaacWorld};
use kami_input::gamepad::GamepadState;

/// Error building / running an arm teleop sim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeleopError {
    /// URDF parse failure.
    Parse(String),
    /// kami-genesis world rejected the articulation.
    World(String),
    /// The articulation reported zero actuated DOF.
    NoDof,
}

impl std::fmt::Display for TeleopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeleopError::Parse(s) => write!(f, "URDF parse error: {s}"),
            TeleopError::World(s) => write!(f, "world error: {s}"),
            TeleopError::NoDof => write!(f, "articulation has no actuated DOF"),
        }
    }
}

impl std::error::Error for TeleopError {}

/// Report from one control tick.
#[derive(Debug, Clone)]
pub struct TickReport {
    pub state: SafeState,
    /// Mean per-DOF tracking error after this tick (rad/m).
    pub tracking_err: f32,
    /// Efforts applied on the final physics substep.
    pub efforts: Vec<f32>,
    /// Whether the applied command was dry-run (R0 = always true).
    pub dry_run: bool,
}

/// A teleoperated articulated arm in the Isaac-parity world.
pub struct ArmTeleopSim {
    world: IsaacWorld,
    handle: ArticulationHandle,
    dof: usize,
    lower: Vec<f32>,
    upper: Vec<f32>,
    vel_limit: Vec<f32>,
    effort_limit: Vec<f32>,
    target_q: Vec<f32>,
    kp: f32,
    kd: f32,
    /// Max lead of the velocity-integrated target over the actual joint angle.
    /// Bounds PD error → stable efforts and natural teleop "the arm follows you".
    max_lag: f32,
    physics_dt: f32,
    ee_link: Option<String>,
    safety: SafetyEnvelope,
    metrics: TeleopMetrics,
}

impl ArmTeleopSim {
    /// Build from a URDF string at `physics_dt` (s). Joint limits, velocity
    /// limits, and effort limits are read from the URDF (with safe fallbacks).
    pub fn from_urdf(urdf: &str, physics_dt: f32) -> Result<Self, TeleopError> {
        let sys = parse_urdf(urdf).map_err(|e| TeleopError::Parse(format!("{e:?}")))?;

        // Movable joints, in URDF order (matches kami-genesis DOF ordering).
        let movable: Vec<_> = sys.joints.iter().filter(|j| j.kind != JointKind::Fixed).collect();
        let mut lower: Vec<f32> = movable.iter().map(|j| finite(j.lower, -std::f32::consts::PI)).collect();
        let mut upper: Vec<f32> = movable.iter().map(|j| finite(j.upper, std::f32::consts::PI)).collect();
        let mut vel_limit: Vec<f32> = movable.iter().map(|j| positive(j.velocity, 3.0)).collect();
        let mut effort_limit: Vec<f32> = movable.iter().map(|j| positive(j.effort, 20.0)).collect();
        let ee_link = sys.links.last().map(|l| l.name.clone());

        let mut world = IsaacWorld::new(physics_dt);
        let handle = world.add_articulation(sys).map_err(|e| TeleopError::World(format!("{e:?}")))?;
        world.reset();

        let dof = world.articulation(handle).map(|v| v.num_dof()).unwrap_or(0);
        if dof == 0 {
            return Err(TeleopError::NoDof);
        }
        // Reconcile limit-vector length with the solver's reported DOF.
        resize(&mut lower, dof, -std::f32::consts::PI);
        resize(&mut upper, dof, std::f32::consts::PI);
        resize(&mut vel_limit, dof, 3.0);
        resize(&mut effort_limit, dof, 20.0);

        Ok(ArmTeleopSim {
            world,
            handle,
            dof,
            lower,
            upper,
            vel_limit,
            effort_limit,
            target_q: vec![0.0; dof],
            kp: 12.0,
            kd: 1.5,
            max_lag: 0.35,
            physics_dt,
            ee_link,
            safety: SafetyEnvelope::new(SafetyConfig::default()),
            metrics: TeleopMetrics::new(),
        })
    }

    /// Override the joint-space PD gains (default kp=60, kd=6).
    pub fn with_gains(mut self, kp: f32, kd: f32) -> Self {
        self.kp = kp;
        self.kd = kd;
        self
    }

    pub fn dof(&self) -> usize {
        self.dof
    }

    pub fn joint_positions(&self) -> Vec<f32> {
        self.world.articulation(self.handle).map(|v| v.get_joint_positions()).unwrap_or_default()
    }

    /// World pose of the end-effector link (`(pos[3], quat_wxyz[4])`).
    pub fn end_effector_pose(&self) -> Option<([f32; 3], [f32; 4])> {
        let link = self.ee_link.as_deref()?;
        self.world.articulation(self.handle)?.get_world_pose(link)
    }

    pub fn safety(&self) -> &SafetyEnvelope {
        &self.safety
    }

    pub fn safety_mut(&mut self) -> &mut SafetyEnvelope {
        &mut self.safety
    }

    pub fn metrics(&self) -> &TeleopMetrics {
        &self.metrics
    }

    pub fn analysis(&self) -> TeleopAnalysis {
        self.metrics.analysis()
    }

    /// Full teleop tick from raw controller state: map → safety-gate → apply →
    /// step → record. `latency_ms` is the measured control round-trip; `dt` is
    /// the control period (s). The caller must call
    /// [`GamepadState::begin_frame`] before feeding this tick's input so the
    /// safety layer's edge-triggered buttons work.
    pub fn drive(
        &mut self,
        gp: &GamepadState,
        mapper: &TeleopMapper,
        latency_ms: f32,
        dt: f32,
    ) -> TickReport {
        let cmd = mapper.map_arm(gp);
        let (gated, state) = self.safety.gate(gp, latency_ms, cmd);
        self.apply(&gated, state, latency_ms, dt)
    }

    /// Apply an already-gated command (e.g. from a recorded demonstration or a
    /// policy) and record metrics under the given [`SafeState`].
    pub fn apply(
        &mut self,
        cmd: &TeleopCommand,
        state: SafeState,
        latency_ms: f32,
        dt: f32,
    ) -> TickReport {
        // Integrate velocity targets into position targets, bounding the lead
        // over the actual angle (stable PD + natural "arm follows you" feel),
        // then clamping to joint limits.
        let q_now = self.joint_positions();
        for i in 0..self.dof {
            let v = cmd
                .joint_vel
                .get(i)
                .copied()
                .unwrap_or(0.0)
                .clamp(-self.vel_limit[i], self.vel_limit[i]);
            let t = (self.target_q[i] + v * dt)
                .clamp(q_now[i] - self.max_lag, q_now[i] + self.max_lag);
            self.target_q[i] = t.clamp(self.lower[i], self.upper[i]);
        }

        // Physics substeps for this control period.
        let substeps = ((dt / self.physics_dt).round() as i64).max(1);
        let mut last_efforts = vec![0.0_f32; self.dof];
        for _ in 0..substeps {
            let (q, qd) = match self.world.articulation(self.handle) {
                Some(v) => (v.get_joint_positions(), v.get_joint_velocities()),
                None => break,
            };
            let mut tau = vec![0.0_f32; self.dof];
            for i in 0..self.dof {
                let e = self.kp * (self.target_q[i] - q[i]) - self.kd * qd[i];
                tau[i] = e.clamp(-self.effort_limit[i], self.effort_limit[i]);
            }
            if let Some(mut v) = self.world.articulation_mut(self.handle) {
                v.set_joint_efforts(&tau);
            }
            self.world.step();
            last_efforts = tau;
        }

        // Metrics: tracking error + closest joint-limit approach.
        let q = self.joint_positions();
        let mut terr = 0.0_f32;
        let mut margin = f32::INFINITY;
        for i in 0..self.dof {
            terr += (self.target_q[i] - q[i]).abs();
            margin = margin.min((q[i] - self.lower[i]).min(self.upper[i] - q[i]));
        }
        terr /= self.dof as f32;
        self.metrics.record(state, terr, &last_efforts, margin, latency_ms);

        TickReport { state, tracking_err: terr, efforts: last_efforts, dry_run: cmd.dry_run }
    }
}

#[inline]
fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

#[inline]
fn positive(v: f32, fallback: f32) -> f32 {
    if v.is_finite() && v > 0.0 { v } else { fallback }
}

#[inline]
fn resize(v: &mut Vec<f32>, n: usize, fill: f32) {
    if v.len() != n {
        v.resize(n, fill);
    }
}
