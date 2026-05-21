/// Maze routing — Lee/BFS algorithm on a multi-layer routing grid.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Routing grid definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingGrid {
    pub layers: Vec<String>,
    pub x_pitch: f64,
    pub y_pitch: f64,
    pub num_x: usize,
    pub num_y: usize,
}

/// A routed wire segment on one layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteSegment {
    pub layer: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub width: f64,
}

/// A via connecting two layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteVia {
    pub x: f64,
    pub y: f64,
    pub bottom_layer: String,
    pub top_layer: String,
}

/// A fully routed net.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedNet {
    pub net_name: String,
    pub segments: Vec<RouteSegment>,
    pub vias: Vec<RouteVia>,
}

/// Routing statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStats {
    pub routed_nets: usize,
    pub total_wire_length: f64,
    pub num_vias: usize,
    pub overflow_count: usize,
}

/// Router state tracking grid occupancy and routed nets.
pub struct Router {
    pub grid: RoutingGrid,
    /// Occupancy grid: `occupied[layer][y][x]`
    occupied: Vec<Vec<Vec<bool>>>,
    pub nets: Vec<RoutedNet>,
    pub overflow_count: usize,
}

impl Router {
    pub fn new(grid: RoutingGrid) -> Self {
        let num_layers = grid.layers.len();
        let occupied = vec![vec![vec![false; grid.num_x]; grid.num_y]; num_layers];
        Self { grid, occupied, nets: Vec::new(), overflow_count: 0 }
    }

    /// Route a single net between the given pin grid coordinates using Lee (BFS) algorithm.
    /// `pins` are `(layer_idx, grid_x, grid_y)`.
    pub fn route_net(&mut self, net_name: &str, pins: &[(usize, usize, usize)]) -> Option<RoutedNet> {
        if pins.len() < 2 {
            return None;
        }

        let mut all_segments = Vec::new();
        let mut all_vias = Vec::new();

        // Route sequentially: pin[0]->pin[1], pin[1]->pin[2], ...
        for window in pins.windows(2) {
            let src = window[0];
            let dst = window[1];
            match self.lee_route(src, dst) {
                Some((segs, vias)) => {
                    all_segments.extend(segs);
                    all_vias.extend(vias);
                }
                None => {
                    self.overflow_count += 1;
                    log::warn!("Failed to route net '{}' between {:?} and {:?}", net_name, src, dst);
                }
            }
        }

        let net = RoutedNet {
            net_name: net_name.to_string(),
            segments: all_segments,
            vias: all_vias,
        };
        self.nets.push(net.clone());
        Some(net)
    }

    /// BFS maze routing between two grid points.
    fn lee_route(
        &mut self,
        src: (usize, usize, usize),
        dst: (usize, usize, usize),
    ) -> Option<(Vec<RouteSegment>, Vec<RouteVia>)> {
        let nl = self.grid.layers.len();
        let nx = self.grid.num_x;
        let ny = self.grid.num_y;

        // BFS state: visited[layer][y][x] with parent pointer
        let mut visited = vec![vec![vec![false; nx]; ny]; nl];
        let mut parent: Vec<Vec<Vec<Option<(usize, usize, usize)>>>> =
            vec![vec![vec![None; nx]; ny]; nl];

        let mut queue = VecDeque::new();
        visited[src.0][src.2][src.1] = true;
        queue.push_back(src);

        let found = loop {
            let Some(cur) = queue.pop_front() else { break false };
            if cur == dst {
                break true;
            }
            let (cl, cx, cy) = cur;

            // Neighbors: 4 cardinal on same layer + up/down layer
            let mut neighbors = Vec::new();
            if cx > 0 { neighbors.push((cl, cx - 1, cy)); }
            if cx + 1 < nx { neighbors.push((cl, cx + 1, cy)); }
            if cy > 0 { neighbors.push((cl, cx, cy - 1)); }
            if cy + 1 < ny { neighbors.push((cl, cx, cy + 1)); }
            if cl > 0 { neighbors.push((cl - 1, cx, cy)); }
            if cl + 1 < nl { neighbors.push((cl + 1, cx, cy)); }

            for (nl2, nx2, ny2) in neighbors {
                if !visited[nl2][ny2][nx2] && !self.occupied[nl2][ny2][nx2] {
                    visited[nl2][ny2][nx2] = true;
                    parent[nl2][ny2][nx2] = Some(cur);
                    queue.push_back((nl2, nx2, ny2));
                }
            }
        };

        if !found {
            return None;
        }

        // Backtrace path
        let mut path = Vec::new();
        let mut cur = dst;
        path.push(cur);
        while cur != src {
            let p = parent[cur.0][cur.2][cur.1]?;
            path.push(p);
            cur = p;
        }
        path.reverse();

        // Convert path to segments and vias, mark occupied
        let mut segments = Vec::new();
        let mut vias = Vec::new();

        for w in path.windows(2) {
            let (l1, x1, y1) = w[0];
            let (l2, x2, y2) = w[1];
            self.occupied[l1][y1][x1] = true;

            if l1 != l2 {
                // Via
                vias.push(RouteVia {
                    x: x1 as f64 * self.grid.x_pitch,
                    y: y1 as f64 * self.grid.y_pitch,
                    bottom_layer: self.grid.layers[l1.min(l2)].clone(),
                    top_layer: self.grid.layers[l1.max(l2)].clone(),
                });
            } else {
                segments.push(RouteSegment {
                    layer: self.grid.layers[l1].clone(),
                    x1: x1 as f64 * self.grid.x_pitch,
                    y1: y1 as f64 * self.grid.y_pitch,
                    x2: x2 as f64 * self.grid.x_pitch,
                    y2: y2 as f64 * self.grid.y_pitch,
                    width: self.grid.x_pitch * 0.5,
                });
            }
        }
        // Mark last cell
        let last = *path.last().unwrap();
        self.occupied[last.0][last.2][last.1] = true;

        Some((segments, vias))
    }

    /// Compute routing statistics across all routed nets.
    pub fn routing_stats(&self) -> RoutingStats {
        let mut total_wire_length = 0.0;
        let mut num_vias = 0;
        for net in &self.nets {
            for seg in &net.segments {
                total_wire_length += ((seg.x2 - seg.x1).powi(2) + (seg.y2 - seg.y1).powi(2)).sqrt();
            }
            num_vias += net.vias.len();
        }
        RoutingStats {
            routed_nets: self.nets.len(),
            total_wire_length,
            num_vias,
            overflow_count: self.overflow_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_router_finds_path() {
        let grid = RoutingGrid {
            layers: vec!["M1".into()],
            x_pitch: 1.0,
            y_pitch: 1.0,
            num_x: 10,
            num_y: 10,
        };
        let mut router = Router::new(grid);
        // Route from (0,0,0) to (0,5,5) on layer 0
        let result = router.route_net("net0", &[(0, 0, 0), (0, 5, 5)]);
        assert!(result.is_some());
        let net = result.unwrap();
        assert!(!net.segments.is_empty());
        let stats = router.routing_stats();
        assert_eq!(stats.routed_nets, 1);
        assert!(stats.total_wire_length > 0.0);
        assert_eq!(stats.overflow_count, 0);
    }

    #[test]
    fn router_detects_overflow() {
        let grid = RoutingGrid {
            layers: vec!["M1".into()],
            x_pitch: 1.0,
            y_pitch: 1.0,
            num_x: 3,
            num_y: 1,
        };
        let mut router = Router::new(grid);
        // Block all intermediate cells
        router.occupied[0][0][1] = true;
        let result = router.route_net("blocked", &[(0, 0, 0), (0, 2, 0)]);
        // Should still produce a RoutedNet (possibly with overflow logged)
        assert!(result.is_some());
        assert_eq!(router.overflow_count, 1);
    }
}
