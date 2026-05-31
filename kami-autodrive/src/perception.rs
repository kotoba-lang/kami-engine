//! Perception: lidar returns -> 2-D occupancy grid (+ configuration-space
//! inflation for planning).
//!
//! Consumes `kami_sensor_sim::LidarReturn` directly. Each finite-range beam is
//! projected to the ground plane, height-filtered to drop the ground sweep and
//! overhead clutter, and rasterised into an occupancy grid. The grid is then
//! inflated by the vehicle footprint so the planner can treat the robot as a
//! point.

use glam::Vec2;
use kami_pathfind::{CostGrid, GridPos};
use kami_sensor_sim::LidarReturn;

use crate::types::Pose2;

/// A dense 2-D occupancy grid centred on `origin`.
///
/// `cells[y * w + x]`: `0` = free, `1` = occupied. Row-major, `+x` east,
/// `+y` north.
#[derive(Debug, Clone)]
pub struct OccupancyGrid {
    /// World position of cell (0,0)'s **centre**.
    pub origin: Vec2,
    /// Metres per cell.
    pub res: f32,
    pub w: usize,
    pub h: usize,
    cells: Vec<u8>,
}

impl OccupancyGrid {
    /// Grid spanning `[center - half, center + half]` on each axis.
    pub fn centered(center: Vec2, half_extent: f32, res: f32) -> Self {
        let n = ((2.0 * half_extent / res).ceil() as usize).max(1);
        let origin = center - Vec2::splat((n as f32 - 1.0) * 0.5 * res);
        Self { origin, res, w: n, h: n, cells: vec![0; n * n] }
    }

    pub fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = 0);
    }

    /// World point -> cell index, or `None` if outside the grid.
    pub fn world_to_cell(&self, p: Vec2) -> Option<(usize, usize)> {
        let rel = (p - self.origin) / self.res;
        let cx = rel.x.round();
        let cy = rel.y.round();
        if cx < 0.0 || cy < 0.0 || cx >= self.w as f32 || cy >= self.h as f32 {
            return None;
        }
        Some((cx as usize, cy as usize))
    }

    /// Cell -> world coordinate of its centre.
    pub fn cell_to_world(&self, x: usize, y: usize) -> Vec2 {
        self.origin + Vec2::new(x as f32, y as f32) * self.res
    }

    pub fn is_occupied(&self, x: usize, y: usize) -> bool {
        self.cells[y * self.w + x] != 0
    }

    pub fn mark_world(&mut self, p: Vec2) {
        if let Some((x, y)) = self.world_to_cell(p) {
            self.cells[y * self.w + x] = 1;
        }
    }

    /// Ingest a lidar sweep. `sensor` is the lidar pose in the world (planar);
    /// `z_band` keeps only hits whose sensor-frame height lies in
    /// `[z_band.0, z_band.1]` (drops the ground plane and overhead returns).
    pub fn ingest_lidar(&mut self, returns: &[LidarReturn], sensor: Pose2, z_band: (f32, f32)) {
        for r in returns {
            if !r.range.is_finite() {
                continue;
            }
            let p = r.point_sensor; // sensor frame: +x fwd, +y left, +z up
            if p.z < z_band.0 || p.z > z_band.1 {
                continue;
            }
            let world = sensor.to_world(Vec2::new(p.x, p.y));
            self.mark_world(world);
        }
    }

    /// Return a configuration-space copy with every occupied cell dilated by
    /// `radius` metres (box dilation). The planner runs on this so it can treat
    /// the vehicle as a point.
    pub fn inflated(&self, radius: f32) -> OccupancyGrid {
        let r = (radius / self.res).ceil() as i32;
        let mut out = self.clone();
        if r <= 0 {
            return out;
        }
        for y in 0..self.h {
            for x in 0..self.w {
                if !self.is_occupied(x, y) {
                    continue;
                }
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy > r * r {
                            continue;
                        }
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < self.w && (ny as usize) < self.h {
                            out.cells[ny as usize * self.w + nx as usize] = 1;
                        }
                    }
                }
            }
        }
        out
    }

    /// View as a `kami_pathfind` cost grid: occupied -> `0` (wall),
    /// free -> `1` (unit cost). Indexed `[y][x]`.
    pub fn to_cost_grid(&self) -> CostGrid {
        (0..self.h)
            .map(|y| {
                (0..self.w)
                    .map(|x| if self.is_occupied(x, y) { 0 } else { 1 })
                    .collect()
            })
            .collect()
    }

    /// Nearest free cell to `p` (spiral search), as a `GridPos`. Used to snap a
    /// start/goal that lands on (or just inside) an inflated obstacle.
    pub fn nearest_free(&self, p: Vec2) -> Option<GridPos> {
        let (cx, cy) = self.world_to_cell(p)?;
        if !self.is_occupied(cx, cy) {
            return Some(GridPos { x: cx as i32, y: cy as i32 });
        }
        let max_r = self.w.max(self.h) as i32;
        for r in 1..max_r {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue; // ring only
                    }
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < self.w && (ny as usize) < self.h
                        && !self.is_occupied(nx as usize, ny as usize)
                    {
                        return Some(GridPos { x: nx, y: ny });
                    }
                }
            }
        }
        None
    }
}

/// Smallest forward-cone obstacle range from a raw lidar sweep, for reactive
/// emergency braking (independent of the grid/planner). Returns the nearest
/// hit distance within a half-angle `cone` of straight-ahead and within
/// `z_band`, or `None`.
pub fn forward_clearance(returns: &[LidarReturn], cone: f32, z_band: (f32, f32)) -> Option<f32> {
    let mut nearest = f32::INFINITY;
    for r in returns {
        if !r.range.is_finite() {
            continue;
        }
        let p = r.point_sensor;
        if p.z < z_band.0 || p.z > z_band.1 {
            continue;
        }
        // azimuth off the forward axis (sensor +x), in the ground plane.
        let az = p.y.atan2(p.x).abs();
        if az <= cone {
            let ground_range = (p.x * p.x + p.y * p.y).sqrt();
            if ground_range < nearest {
                nearest = ground_range;
            }
        }
    }
    nearest.is_finite().then_some(nearest)
}
