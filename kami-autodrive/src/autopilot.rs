//! The autopilot: closes perception -> planning -> control into one
//! `step()` and runs a small driving state machine.

use glam::Vec2;
use kami_sensor_sim::{Camera, DepthImage, LidarReturn};

use crate::classes::{VehicleClass, VehicleLimits};
use crate::control::{curvature_speed_limit, PurePursuit, SpeedController};
use crate::perception::{forward_clearance, forward_clearance_camera, OccupancyGrid};
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
    /// Backing out of a stuck pose (K-turn) before re-planning.
    Recovering,
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
    /// When `true`, the occupancy grid is cleared and rebuilt from the current
    /// sweep every tick, so **moving** obstacles (other agents) are tracked
    /// without smearing a trail. When `false`, the grid accumulates — better
    /// for a static world with occlusion/limited range. A 360° lidar makes the
    /// fresh-each-tick default sound.
    pub dynamic_obstacles: bool,
    /// **World-frame** height band (m above ground) kept from depth-camera
    /// back-projection — rejects the ground plane and overhead structure.
    pub camera_z_band: (f32, f32),
    /// Consecutive stuck ticks (emergency-stop / blocked) before a reverse
    /// K-turn recovery is attempted. **0 disables recovery (the default)** — a
    /// stuck agent just holds a safe stop. Recovery is opt-in: it backs out and
    /// gives up after a bounded number of attempts (never ramming, always
    /// terminating), but does not yet reliably escape arbitrary tight corners.
    pub stuck_limit: u32,
    /// Duration (ticks) of the reverse K-turn once recovery starts.
    pub recovery_ticks: u32,
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
            dynamic_obstacles: true,
            camera_z_band: (0.3, 2.5),
            stuck_limit: 0, // recovery off by default (opt-in)
            recovery_ticks: 60,
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
    /// Latched once the goal is reached, so coasting past `goal_tol` doesn't
    /// un-arrive and send the agent looping back (matters when something keeps
    /// stepping the autopilot after arrival, e.g. a multi-agent `Fleet`).
    arrived: bool,
    pursuit: PurePursuit,
    speed_ctl: SpeedController,
    steps_since_replan: u32,
    /// Consecutive ticks spent stuck (emergency-stop / blocked).
    stuck_ticks: u32,
    /// Remaining ticks of an active reverse K-turn (0 = not recovering).
    recovery_ticks: u32,
    /// Steer held during the reverse K-turn.
    recovery_steer: f32,
    /// Best (smallest) distance-to-goal achieved, for no-progress give-up.
    best_dist: f32,
    /// Recoveries attempted since the last real progress toward the goal.
    recoveries_since_progress: u32,
    /// Telemetry snapshot from the most recent tick.
    last_pose: Pose2,
    last_target_speed: f32,
    last_cross_track: f32,
}

/// Read-only autopilot status for HUD / logging / monitoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    pub state: DriveState,
    /// Straight-line distance from the last pose to the goal (∞ if no goal).
    pub distance_to_goal: f32,
    /// Lateral deviation of the last pose from the planned path (m).
    pub cross_track_error: f32,
    /// Speed the controller was aiming for last tick (m/s).
    pub target_speed: f32,
    /// Number of waypoints in the current plan.
    pub path_waypoints: usize,
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
            arrived: false,
            pursuit,
            speed_ctl: SpeedController::new(0.6, 0.05, 0.02),
            steps_since_replan: u32::MAX, // force a plan on first goal
            stuck_ticks: 0,
            recovery_ticks: 0,
            recovery_steer: 0.0,
            best_dist: f32::INFINITY,
            recoveries_since_progress: 0,
            last_pose: start,
            last_target_speed: 0.0,
            last_cross_track: 0.0,
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
        self.arrived = false;
        self.steps_since_replan = u32::MAX;
        self.stuck_ticks = 0;
        self.recovery_ticks = 0;
        self.best_dist = f32::INFINITY;
        self.recoveries_since_progress = 0;
        self.state = DriveState::Cruise;
    }

    pub fn path(&self) -> &[Vec2] {
        &self.path
    }

    pub fn grid(&self) -> &OccupancyGrid {
        &self.grid
    }

    /// Read-only status snapshot from the most recent tick.
    pub fn telemetry(&self) -> Telemetry {
        let distance_to_goal = match self.goal {
            Some(g) => self.last_pose.pos().distance(g),
            None => f32::INFINITY,
        };
        Telemetry {
            state: self.state,
            distance_to_goal,
            cross_track_error: self.last_cross_track,
            target_speed: self.last_target_speed,
            path_waypoints: self.path.len(),
        }
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
        self.step_multimodal(pose, speed, lidar, &[], sensor, dt)
    }

    /// Multi-modal control tick: fuse the lidar sweep **and** zero or more
    /// pinhole depth cameras into the occupancy map this tick. Reactive
    /// emergency braking still uses the lidar forward cone, so a camera-only
    /// configuration plans/routes but has no sub-planner reflex — pair a camera
    /// with at least a forward lidar for the reactive layer.
    pub fn step_multimodal(
        &mut self,
        pose: Pose2,
        speed: f32,
        lidar: &[LidarReturn],
        cameras: &[(&DepthImage, &Camera)],
        sensor: Pose2,
        dt: f32,
    ) -> Command {
        self.last_pose = pose;
        let Some(goal) = self.goal else {
            self.state = DriveState::Idle;
            return Command::stop();
        };

        // Arrival (latched — stays arrived even if the plant coasts past).
        if self.arrived || pose.pos().distance(goal) <= self.cfg.goal_tol {
            self.arrived = true;
            self.state = DriveState::Arrived;
            return Command::stop();
        }

        // Active K-turn recovery: back out for a fixed window, then force a
        // fresh plan from the (now clearer) pose.
        if self.recovery_ticks > 0 {
            self.recovery_ticks -= 1;
            self.state = DriveState::Recovering;
            if self.recovery_ticks == 0 {
                self.stuck_ticks = 0;
                self.path.clear();
                self.steps_since_replan = u32::MAX;
            }
            return Command::reverse_with(0.6, self.recovery_steer);
        }

        // 1. Perception — refresh (dynamic) or accumulate (static) the map,
        //    fusing lidar + every depth camera into one occupancy grid.
        if self.cfg.dynamic_obstacles {
            self.grid.clear();
        }
        self.grid.ingest_lidar(lidar, sensor, self.cfg.z_band);
        for (depth, camera) in cameras {
            self.grid.ingest_camera_depth(depth, camera, self.cfg.camera_z_band);
        }

        // 2. Reactive emergency stop, independent of the planner — fused over
        //    the lidar forward cone AND every depth camera (so a camera-only
        //    rig still has a reflex).
        let stop_dist = self.braking_distance(speed);
        if let Some(clear) = self.fwd_clearance(lidar, cameras, self.cfg.emergency_cone)
            && clear <= stop_dist + self.cfg.limits.footprint_radius
        {
            self.speed_ctl.reset();
            return self.register_stuck(lidar);
        }

        // 3. (Re)plan if needed: no path, the period elapsed, or the current
        //    path is now blocked (e.g. a moving obstacle drifted onto it).
        //    The block test runs on the raw grid (cheap); inflation — the
        //    expensive step — is deferred to the replan branch only.
        self.steps_since_replan = self.steps_since_replan.saturating_add(1);
        let path_blocked = self.path.len() >= 2
            && self.path.windows(2).any(|w| !self.grid.line_clear(w[0], w[1]));
        let need_replan = self.path.len() < 2
            || path_blocked
            || self.steps_since_replan >= self.cfg.replan_period;
        if need_replan {
            let inflated = self.grid.inflated(self.cfg.limits.footprint_radius);
            match planner::plan(&inflated, pose.pos(), goal) {
                Some(p) if p.len() >= 2 => {
                    self.path = p;
                    self.steps_since_replan = 0;
                }
                _ => {
                    // No route this tick. If the prior path is also blocked,
                    // hold position rather than drive into the obstacle.
                    if self.path.len() < 2 || path_blocked {
                        return self.register_stuck(lidar);
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
        if let Some(clear) = self.fwd_clearance(lidar, cameras, self.cfg.emergency_cone * 2.0) {
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

        // Telemetry: record target speed + cross-track error for this tick.
        self.last_target_speed = target_speed;
        self.last_cross_track = cross_track_error(pose.pos(), &self.path);

        // Progress this tick — clear the stuck count, and on *substantial*
        // progress toward the goal, reset the recovery-attempt budget.
        self.stuck_ticks = 0;
        if d_goal < self.best_dist - 2.0 {
            self.best_dist = d_goal;
            self.recoveries_since_progress = 0;
        }

        let (throttle, brake) = self.speed_ctl.update(target_speed, speed, dt);
        let mut cmd = Command { throttle, brake, steer, handbrake: 0.0, reverse: false };
        cmd.clamp();
        cmd
    }

    /// Count a stuck tick (emergency-stop or no route). Once `stuck_limit`
    /// consecutive stuck ticks accrue, kick off a reverse K-turn toward the
    /// more-open side; otherwise just hold a stop.
    fn register_stuck(&mut self, lidar: &[LidarReturn]) -> Command {
        self.stuck_ticks = self.stuck_ticks.saturating_add(1);
        // Give up recovering after a bounded number of attempts without
        // substantial progress — an impassable obstacle, where repeated K-turns
        // would only oscillate (and risk nosing into it). Hold a stop (Blocked).
        // This budget only resets on real progress, guaranteeing termination.
        let exhausted = self.recoveries_since_progress >= 4;
        if self.cfg.stuck_limit > 0 && self.stuck_ticks >= self.cfg.stuck_limit && !exhausted {
            self.recovery_steer = self.open_side_steer(lidar);
            self.recovery_ticks = self.cfg.recovery_ticks;
            self.recoveries_since_progress += 1;
            self.stuck_ticks = 0;
            self.state = DriveState::Recovering;
            self.recovery_ticks -= 1;
            return Command::reverse_with(0.6, self.recovery_steer);
        }
        self.state = DriveState::Blocked;
        Command::stop()
    }

    /// Pick the reverse steer that swings the nose toward the more-open side.
    /// Compares nearest obstacle range in the left vs right forward quadrants;
    /// reversing with the returned steer rotates the nose toward open space.
    fn open_side_steer(&self, lidar: &[LidarReturn]) -> f32 {
        let (mut left_min, mut right_min) = (f32::INFINITY, f32::INFINITY);
        for r in lidar {
            if !r.range.is_finite() {
                continue;
            }
            let p = r.point_sensor;
            if p.z < self.cfg.z_band.0 || p.z > self.cfg.z_band.1 {
                continue;
            }
            let az = p.y.atan2(p.x); // +left, −right
            let gr = (p.x * p.x + p.y * p.y).sqrt();
            if (0.2..1.4).contains(&az) {
                left_min = left_min.min(gr);
            } else if (-1.4..-0.2).contains(&az) {
                right_min = right_min.min(gr);
            }
        }
        // Open side = farther nearest obstacle. To rotate the nose toward it
        // while reversing (speed < 0 ⇒ yaw_rate = v/L·tanδ), steer the opposite
        // sign: nose-left needs steer right.
        if left_min >= right_min { -1.0 } else { 1.0 }
    }

    /// Nearest forward obstacle range fused over the lidar cone and every depth
    /// camera (min of all available sources), or `None` if nothing is ahead.
    fn fwd_clearance(
        &self,
        lidar: &[LidarReturn],
        cameras: &[(&DepthImage, &Camera)],
        cone: f32,
    ) -> Option<f32> {
        let mut best = forward_clearance(lidar, cone, self.cfg.z_band);
        for (depth, cam) in cameras {
            if let Some(c) = forward_clearance_camera(depth, cam, cone, self.cfg.camera_z_band) {
                best = Some(best.map_or(c, |b| b.min(c)));
            }
        }
        best
    }

    /// Kinematic stopping distance `v² / (2·d_max)` with a safety margin.
    fn braking_distance(&self, speed: f32) -> f32 {
        self.cfg.brake_margin * speed * speed / (2.0 * self.cfg.limits.max_decel.max(0.1))
    }
}

/// Lateral deviation of `p` from a path polyline = min point-to-segment
/// distance over its segments (0 for a path with fewer than 2 points).
fn cross_track_error(p: Vec2, path: &[Vec2]) -> f32 {
    if path.len() < 2 {
        return 0.0;
    }
    path.windows(2)
        .map(|w| point_segment_distance(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}

fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-9 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}
