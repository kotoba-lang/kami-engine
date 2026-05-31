//! The autopilot: closes perception -> planning -> control into one
//! `step()` and runs a small driving state machine.

use glam::Vec2;
use kami_sensor_sim::LidarReturn;

use crate::classes::{VehicleClass, VehicleLimits};
use crate::control::{curvature_speed_limit, PurePursuit, SpeedController};
use crate::perception::{forward_clearance, OccupancyGrid};
use crate::planner;
use crate::types::{Command, Pose2};

/// High-level driving state, surfaced for telemetry/HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveState {
    /// No goal set.
    Idle,
    /// Tracking the path at the speed envelope.
    Cruise,
    /// Slowing for curvature or a near obstacle.
    Slow,
    /// Reactive emergency stop (obstacle inside braking distance).
    Stop,
    /// Planner found no route this tick.
    Blocked,
    /// Within goal tolerance.
    Arrived,
}

/// Tunable autopilot parameters.
#[derive(Debug, Clone)]
pub struct AutopilotConfig {
    pub limits: VehicleLimits,
    /// Occupancy-grid half-extent (m) and resolution (m/cell).
    pub grid_half_extent: f32,
    pub grid_res: f32,
    /// Lidar **sensor-frame** height band kept as obstacles (m). Returns whose
    /// `point_sensor.z` falls outside are dropped — this rejects the ground
    /// sweep (≈ −mount_height) and overhead clutter. Centre near 0 for a
    /// planar ring mounted at obstacle height.
    pub z_band: (f32, f32),
    /// Replan every N steps (also replans on a blocked path).
    pub replan_period: u32,
    /// Distance to goal counted as arrived (m).
    pub goal_tol: f32,
    /// Forward half-cone for emergency clearance checks (rad).
    pub emergency_cone: f32,
    /// Comfort lateral-accel cap for curvature speed (m/s²).
    pub lateral_accel: f32,
    /// Safety-margin factor on the kinematic braking distance.
    pub brake_margin: f32,
}

impl AutopilotConfig {
    pub fn for_class(class: VehicleClass) -> Self {
        let limits = class.limits();
        Self {
            limits,
            grid_half_extent: 60.0,
            grid_res: 0.5,
            z_band: (-1.0, 1.5),
            replan_period: 20,
            goal_tol: limits.footprint_radius.max(1.0),
            emergency_cone: 0.35,
            lateral_accel: 3.0,
            brake_margin: 1.6,
        }
    }
}

/// Stateful autonomy driver. One per vehicle.
pub struct Autopilot {
    pub cfg: AutopilotConfig,
    pub state: DriveState,
    grid: OccupancyGrid,
    home: Vec2,
    path: Vec<Vec2>,
    goal: Option<Vec2>,
    pursuit: PurePursuit,
    speed_ctl: SpeedController,
    steps_since_replan: u32,
}

impl Autopilot {
    pub fn new(cfg: AutopilotConfig, start: Pose2) -> Self {
        let grid = OccupancyGrid::centered(start.pos(), cfg.grid_half_extent, cfg.grid_res);
        let pursuit = PurePursuit {
            lookahead: 4.0,
            lookahead_gain: 0.4,
            turn_radius_ref: cfg.limits.turn_radius_ref,
        };
        Self {
            cfg,
            state: DriveState::Idle,
            grid,
            home: start.pos(),
            path: Vec::new(),
            goal: None,
            pursuit,
            speed_ctl: SpeedController::new(0.6, 0.05, 0.02),
            steps_since_replan: u32::MAX, // force a plan on first goal
        }
    }

    pub fn set_goal(&mut self, goal: Vec2) {
        // Size the occupancy grid to cover the whole home→goal corridor plus a
        // margin, so the planner can always reach the goal cell and route
        // laterally around obstacles.
        let center = (self.home + goal) * 0.5;
        let margin = self.cfg.limits.footprint_radius * 4.0 + 10.0;
        let half = (self.home.distance(goal) * 0.5 + margin).max(self.cfg.grid_half_extent);
        self.grid = OccupancyGrid::centered(center, half, self.cfg.grid_res);

        self.goal = Some(goal);
        self.steps_since_replan = u32::MAX;
        self.state = DriveState::Cruise;
    }

    pub fn path(&self) -> &[Vec2] {
        &self.path
    }

    pub fn grid(&self) -> &OccupancyGrid {
        &self.grid
    }

    /// One control tick. `pose`/`speed` are the plant's current state; `lidar`
    /// is this tick's sweep with the sensor at `sensor` (planar pose).
    pub fn step(
        &mut self,
        pose: Pose2,
        speed: f32,
        lidar: &[LidarReturn],
        sensor: Pose2,
        dt: f32,
    ) -> Command {
        let Some(goal) = self.goal else {
            self.state = DriveState::Idle;
            return Command::stop();
        };

        // Arrival.
        if pose.pos().distance(goal) <= self.cfg.goal_tol {
            self.state = DriveState::Arrived;
            return Command::stop();
        }

        // 1. Perception — accumulate the sweep into the persistent map.
        self.grid.ingest_lidar(lidar, sensor, self.cfg.z_band);

        // 2. Reactive emergency stop, independent of the planner.
        let stop_dist = self.braking_distance(speed);
        if let Some(clear) = forward_clearance(lidar, self.cfg.emergency_cone, self.cfg.z_band)
            && clear <= stop_dist + self.cfg.limits.footprint_radius
        {
            self.state = DriveState::Stop;
            self.speed_ctl.reset();
            return Command::stop();
        }

        // 3. (Re)plan if needed.
        self.steps_since_replan = self.steps_since_replan.saturating_add(1);
        let need_replan = self.path.len() < 2 || self.steps_since_replan >= self.cfg.replan_period;
        if need_replan {
            let inflated = self.grid.inflated(self.cfg.limits.footprint_radius);
            match planner::plan(&inflated, pose.pos(), goal) {
                Some(p) if p.len() >= 2 => {
                    self.path = p;
                    self.steps_since_replan = 0;
                }
                _ => {
                    // Keep the prior path if we still have one; else stop.
                    if self.path.len() < 2 {
                        self.state = DriveState::Blocked;
                        return Command::stop();
                    }
                }
            }
        }

        // 4. Lateral: pure pursuit.
        let (steer, target_idx) = self.pursuit.steer(pose, speed, &self.path);

        // 5. Longitudinal: min of cruise / curvature / goal-approach /
        //    obstacle-proximity caps.
        let mut target_speed = self.cfg.limits.max_speed;
        target_speed =
            target_speed.min(curvature_speed_limit(&self.path, target_idx, self.cfg.lateral_accel));
        // Decelerate to (near) rest at the goal: v ≤ √(2·d_max·distance).
        let d_goal = pose.pos().distance(goal);
        target_speed = target_speed.min((2.0 * self.cfg.limits.max_decel * d_goal).sqrt());
        if let Some(clear) = forward_clearance(lidar, self.cfg.emergency_cone * 2.0, self.cfg.z_band)
        {
            // Linear taper: full speed at >2x brake dist, zero at brake dist.
            let near = self.braking_distance(self.cfg.limits.max_speed);
            let t = ((clear - stop_dist) / (near + 1e-3)).clamp(0.0, 1.0);
            target_speed = target_speed.min(t * self.cfg.limits.max_speed);
        }

        self.state = if target_speed < self.cfg.limits.max_speed * 0.6 {
            DriveState::Slow
        } else {
            DriveState::Cruise
        };

        let (throttle, brake) = self.speed_ctl.update(target_speed, speed, dt);
        let mut cmd = Command { throttle, brake, steer, handbrake: 0.0 };
        cmd.clamp();
        cmd
    }

    /// Kinematic stopping distance `v² / (2·d_max)` with a safety margin.
    fn braking_distance(&self, speed: f32) -> f32 {
        self.cfg.brake_margin * speed * speed / (2.0 * self.cfg.limits.max_decel.max(0.1))
    }
}
