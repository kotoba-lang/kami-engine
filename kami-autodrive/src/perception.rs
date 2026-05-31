//! Perception: lidar returns -> 2-D occupancy grid (+ configuration-space
//! inflation for planning).
//!
//! Consumes `kami_sensor_sim::LidarReturn` directly. Each finite-range beam is
//! projected to the ground plane, height-filtered to drop the ground sweep and
//! overhead clutter, and rasterised into an occupancy grid. The grid is then
//! inflated by the vehicle footprint so the planner can treat the robot as a
//! point.

use glam::{Vec2, Vec3};
use kami_pathfind::{CostGrid, GridPos};
use kami_sensor_sim::{Camera, DepthImage, LidarReturn};

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

    /// Ingest a depth image from a pinhole [`Camera`] (RGB-D / stereo costmap
    /// path, complementary to lidar). Each finite-depth pixel is back-projected
    /// through the intrinsics to a camera-frame point, transformed to the world
    /// (z-up), height-filtered by `world_z_band` (drop ground + overhead), and
    /// rasterised to occupancy.
    pub fn ingest_camera_depth(
        &mut self,
        depth: &DepthImage,
        camera: &Camera,
        world_z_band: (f32, f32),
    ) {
        let intr = camera.intrinsics;
        let cam_to_world = camera.view.inverse();
        for v in 0..depth.height {
            for u in 0..depth.width {
                let d = depth.pixels[(v * depth.width + u) as usize];
                if !d.is_finite() {
                    continue;
                }
                // Pixel + depth -> camera frame (+x right, +y down, +z forward).
                let x = (u as f32 + 0.5 - intr.cx) * d / intr.fx;
                let y = (v as f32 + 0.5 - intr.cy) * d / intr.fy;
                let w = cam_to_world.transform_point3(Vec3::new(x, y, d));
                if w.z < world_z_band.0 || w.z > world_z_band.1 {
                    continue;
                }
                self.mark_world(Vec2::new(w.x, w.y));
            }
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

    /// True iff the straight segment `a..b` stays on free, in-bounds cells
    /// (sampled at sub-cell spacing).
    pub fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let len = (b - a).length();
        let steps = (len / (self.res * 0.5)).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let p = a.lerp(b, k as f32 / steps as f32);
            match self.world_to_cell(p) {
                Some((x, y)) if self.is_occupied(x, y) => return false,
                None => return false,
                _ => {}
            }
        }
        true
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

/// Smallest forward-cone obstacle range from a depth **camera**, for a reactive
/// emergency reflex when running camera-only (no forward lidar). Mirrors
/// [`forward_clearance`]: returns the nearest ground-plane range within a
/// half-angle `cone` of the camera's optical axis and within `world_z_band`
/// (after back-projection), or `None`.
pub fn forward_clearance_camera(
    depth: &DepthImage,
    camera: &Camera,
    cone: f32,
    world_z_band: (f32, f32),
) -> Option<f32> {
    let intr = camera.intrinsics;
    let cam_to_world = camera.view.inverse();
    let mut nearest = f32::INFINITY;
    for v in 0..depth.height {
        for u in 0..depth.width {
            let d = depth.pixels[(v * depth.width + u) as usize];
            if !d.is_finite() {
                continue;
            }
            // Camera frame: +x right, +y down, +z forward.
            let x = (u as f32 + 0.5 - intr.cx) * d / intr.fx;
            // Azimuth off the optical axis in the ground plane.
            if x.atan2(d).abs() > cone {
                continue;
            }
            let y = (v as f32 + 0.5 - intr.cy) * d / intr.fy;
            let w = cam_to_world.transform_point3(Vec3::new(x, y, d));
            if w.z < world_z_band.0 || w.z > world_z_band.1 {
                continue;
            }
            let ground_range = (x * x + d * d).sqrt();
            if ground_range < nearest {
                nearest = ground_range;
            }
        }
    }
    nearest.is_finite().then_some(nearest)
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

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn hit(point_sensor: Vec3, range: f32) -> LidarReturn {
        LidarReturn { range, point_sensor, prim_index: 0 }
    }

    #[test]
    fn cell_world_round_trip() {
        let g = OccupancyGrid::centered(Vec2::new(10.0, -5.0), 20.0, 0.5);
        for &p in &[Vec2::new(10.0, -5.0), Vec2::new(3.5, 2.0), Vec2::new(-7.0, -12.0)] {
            let (cx, cy) = g.world_to_cell(p).expect("inside");
            let c = g.cell_to_world(cx, cy);
            assert!(c.distance(p) <= 0.5 * std::f32::consts::SQRT_2 + 1e-4, "{p:?} -> {c:?}");
        }
    }

    #[test]
    fn out_of_bounds_is_none() {
        let g = OccupancyGrid::centered(Vec2::ZERO, 5.0, 0.5);
        assert!(g.world_to_cell(Vec2::new(100.0, 0.0)).is_none());
    }

    #[test]
    fn mark_and_inflate_and_cost_grid() {
        let mut g = OccupancyGrid::centered(Vec2::ZERO, 10.0, 0.5);
        g.mark_world(Vec2::new(0.0, 0.0));
        let (cx, cy) = g.world_to_cell(Vec2::ZERO).unwrap();
        assert!(g.is_occupied(cx, cy));

        let inflated = g.inflated(1.0); // 2 cells
        let nbr = g.world_to_cell(Vec2::new(0.8, 0.0)).unwrap();
        assert!(inflated.is_occupied(nbr.0, nbr.1), "inflation should reach 0.8 m");
        // Original grid is untouched by inflated().
        assert!(!g.is_occupied(nbr.0, nbr.1));

        let cost = inflated.to_cost_grid();
        assert_eq!(cost[cy][cx], 0, "occupied cell is a wall (cost 0)");
    }

    #[test]
    fn nearest_free_escapes_an_occupied_cell() {
        let mut g = OccupancyGrid::centered(Vec2::ZERO, 10.0, 0.5);
        g.mark_world(Vec2::ZERO);
        let gp = g.nearest_free(Vec2::ZERO).expect("a free neighbour exists");
        assert!(!g.is_occupied(gp.x as usize, gp.y as usize));
    }

    #[test]
    fn forward_clearance_picks_nearest_in_cone() {
        let pose = Pose2::new(0.0, 0.0, 0.0);
        let returns = [
            hit(Vec3::new(8.0, 0.0, 0.0), 8.0),  // ahead, far
            hit(Vec3::new(3.0, 0.2, 0.0), 3.0),  // ahead, near
            hit(Vec3::new(0.0, 5.0, 0.0), 5.0),  // 90° abeam — outside cone
        ];
        let _ = pose;
        let c = forward_clearance(&returns, 0.35, (-1.0, 1.0)).unwrap();
        assert!((c - 3.0).abs() < 0.05, "nearest in cone ≈ 3 m, got {c}");
    }

    #[test]
    fn forward_clearance_height_band_rejects() {
        // A hit well above the band must be dropped (overhead clutter).
        let returns = [hit(Vec3::new(4.0, 0.0, 9.0), 4.0)];
        assert!(forward_clearance(&returns, 0.5, (-1.0, 1.5)).is_none());
    }

    #[test]
    fn camera_depth_back_projects_to_occupancy() {
        use kami_sensor_sim::{Camera, CameraIntrinsics};

        // Camera at (0,0,1) looking down +x (world z-up).
        let intr = CameraIntrinsics::from_hfov(160, 120, 70f32.to_radians());
        let mut cam = Camera::new("c", "/c", intr);
        cam.look_at(Vec3::new(0.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 1.0));

        // A box face at x=8, sampled on a y×z grid within the obstacle band.
        let mut pts = Vec::new();
        let mut y = -1.0;
        while y <= 1.0 {
            let mut z = 0.5;
            while z <= 2.0 {
                pts.push(Vec3::new(8.0, y, z));
                z += 0.1;
            }
            y += 0.1;
        }
        let depth = cam.render_points_to_depth_image(&pts);
        assert!(depth.populated_count() > 0, "camera should see the box");

        let mut grid = OccupancyGrid::centered(Vec2::new(5.0, 0.0), 12.0, 0.25);
        grid.ingest_camera_depth(&depth, &cam, (0.3, 2.5));

        // The back-projected box face should mark occupancy near (8, 0).
        let (cx, cy) = grid.world_to_cell(Vec2::new(8.0, 0.0)).unwrap();
        let mut any = false;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let x = (cx as i32 + dx) as usize;
                let yy = (cy as i32 + dy) as usize;
                if grid.is_occupied(x, yy) {
                    any = true;
                }
            }
        }
        assert!(any, "depth back-projection should mark the box near x=8");
    }

    #[test]
    fn camera_forward_clearance_reports_wall_distance() {
        use kami_sensor_sim::{Camera, CameraIntrinsics};
        let intr = CameraIntrinsics::from_hfov(160, 120, 70f32.to_radians());
        let mut cam = Camera::new("c", "/c", intr);
        cam.look_at(Vec3::new(0.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 1.0));

        // Wall face 8 m ahead, within the obstacle height band.
        let mut pts = Vec::new();
        let mut y = -1.5;
        while y <= 1.5 {
            let mut z = 0.5;
            while z <= 2.0 {
                pts.push(Vec3::new(8.0, y, z));
                z += 0.15;
            }
            y += 0.15;
        }
        let depth = cam.render_points_to_depth_image(&pts);
        let c = forward_clearance_camera(&depth, &cam, 0.35, (0.3, 2.5)).expect("sees the wall");
        assert!((c - 8.0).abs() < 1.0, "should report ≈8 m ahead, got {c}");

        // Overhead-only structure is rejected (no reflex on a gantry).
        let high: Vec<Vec3> = (0..20).map(|i| Vec3::new(8.0, -1.5 + 0.15 * i as f32, 6.0)).collect();
        let depth_high = cam.render_points_to_depth_image(&high);
        assert!(forward_clearance_camera(&depth_high, &cam, 0.35, (0.3, 2.5)).is_none());
    }

    #[test]
    fn camera_depth_height_band_rejects_overhead() {
        use kami_sensor_sim::{Camera, CameraIntrinsics};
        let intr = CameraIntrinsics::from_hfov(160, 120, 70f32.to_radians());
        let mut cam = Camera::new("c", "/c", intr);
        cam.look_at(Vec3::new(0.0, 0.0, 1.0), Vec3::new(10.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 1.0));
        // A gantry far above the band (world z≈6).
        let pts: Vec<Vec3> = (0..20).map(|i| Vec3::new(8.0, -1.0 + 0.1 * i as f32, 6.0)).collect();
        let depth = cam.render_points_to_depth_image(&pts);

        let mut grid = OccupancyGrid::centered(Vec2::new(5.0, 0.0), 12.0, 0.25);
        grid.ingest_camera_depth(&depth, &cam, (0.3, 2.5));
        assert_eq!(grid.to_cost_grid().iter().flatten().filter(|c| **c == 0).count(), 0,
            "overhead structure must not appear in the ground costmap");
    }
}
