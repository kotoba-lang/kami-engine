//! Global path planning: A* over the inflated occupancy grid, returned as a
//! world-coordinate polyline.

use glam::Vec2;
use kami_pathfind::astar_grid;

use crate::perception::OccupancyGrid;

/// Plan a collision-free path from `start` to `goal` over `grid`.
///
/// `grid` should already be configuration-space inflated (see
/// [`OccupancyGrid::inflated`]). Start/goal are snapped to the nearest free
/// cell. Returns world-frame waypoints (cell centres), line-of-sight
/// simplified, or `None` if no path exists.
pub fn plan(grid: &OccupancyGrid, start: Vec2, goal: Vec2) -> Option<Vec<Vec2>> {
    let s = grid.nearest_free(start)?;
    let g = grid.nearest_free(goal)?;
    let cost = grid.to_cost_grid();
    let cells = astar_grid(&cost, s, g)?;

    let pts: Vec<Vec2> = cells
        .iter()
        .map(|c| grid.cell_to_world(c.x as usize, c.y as usize))
        .collect();

    Some(simplify(&pts, grid))
}

/// Line-of-sight shortcutting: greedily drop intermediate waypoints whose
/// removal keeps the segment collision-free. Produces a sparse, drivable path.
fn simplify(pts: &[Vec2], grid: &OccupancyGrid) -> Vec<Vec2> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    let mut anchor = 0;
    let mut i = 1;
    while i < pts.len() {
        if i == pts.len() - 1 || !segment_clear(pts[anchor], pts[i + 1], grid) {
            out.push(pts[i]);
            anchor = i;
        }
        i += 1;
    }
    out
}

/// Sample the segment `a..b` at sub-cell spacing; clear iff no sample lands on
/// an occupied cell.
fn segment_clear(a: Vec2, b: Vec2, grid: &OccupancyGrid) -> bool {
    let len = (b - a).length();
    let steps = (len / (grid.res * 0.5)).ceil().max(1.0) as usize;
    for k in 0..=steps {
        let p = a.lerp(b, k as f32 / steps as f32);
        match grid.world_to_cell(p) {
            Some((x, y)) if grid.is_occupied(x, y) => return false,
            None => return false,
            _ => {}
        }
    }
    true
}
