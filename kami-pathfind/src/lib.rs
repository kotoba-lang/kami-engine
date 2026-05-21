//! kami-pathfind: A* grid pathfinding + NavMesh.
//!
//! Grid-based A* for tilemaps, NavMesh for open 3D worlds.
//! Designed for NPC navigation in kami-game.

use glam::Vec2;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Grid cell cost (0 = impassable).
pub type CostGrid = Vec<Vec<u8>>; // [y][x], 0 = wall, 1-255 = traversal cost

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug)]
struct Node {
    pos: GridPos,
    g: u32,
    f: u32,
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.cmp(&self.f)
    } // min-heap
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f
    }
}
impl Eq for Node {}

/// A* pathfinding on a 2D grid.
pub fn astar_grid(grid: &CostGrid, start: GridPos, goal: GridPos) -> Option<Vec<GridPos>> {
    let h = grid.len() as i32;
    let w = if h > 0 {
        grid[0].len() as i32
    } else {
        return None;
    };

    let idx = |p: GridPos| -> usize { (p.y * w + p.x) as usize };
    let total = (w * h) as usize;

    let mut g_score = vec![u32::MAX; total];
    let mut came_from = vec![GridPos { x: -1, y: -1 }; total];
    let mut closed = vec![false; total];

    g_score[idx(start)] = 0;
    let mut open = BinaryHeap::new();
    open.push(Node {
        pos: start,
        g: 0,
        f: heuristic(start, goal),
    });

    let dirs: [(i32, i32); 8] = [
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ];

    while let Some(current) = open.pop() {
        if current.pos == goal {
            return Some(reconstruct(came_from, start, goal, w));
        }
        let ci = idx(current.pos);
        if closed[ci] {
            continue;
        }
        closed[ci] = true;

        for &(dx, dy) in &dirs {
            let nx = current.pos.x + dx;
            let ny = current.pos.y + dy;
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let np = GridPos { x: nx, y: ny };
            let ni = idx(np);
            if closed[ni] {
                continue;
            }

            let cost = grid[ny as usize][nx as usize] as u32;
            if cost == 0 {
                continue;
            } // wall

            let move_cost = if dx != 0 && dy != 0 {
                14 * cost
            } else {
                10 * cost
            }; // diagonal = √2 ≈ 1.4
            let ng = current.g + move_cost;
            if ng < g_score[ni] {
                g_score[ni] = ng;
                came_from[ni] = current.pos;
                open.push(Node {
                    pos: np,
                    g: ng,
                    f: ng + heuristic(np, goal),
                });
            }
        }
    }
    None
}

fn heuristic(a: GridPos, b: GridPos) -> u32 {
    let dx = (a.x - b.x).unsigned_abs();
    let dy = (a.y - b.y).unsigned_abs();
    // Octile distance
    let (mn, mx) = if dx < dy { (dx, dy) } else { (dy, dx) };
    (14 * mn + 10 * (mx - mn)) as u32
}

fn reconstruct(came_from: Vec<GridPos>, start: GridPos, goal: GridPos, w: i32) -> Vec<GridPos> {
    let mut path = vec![goal];
    let mut current = goal;
    while current != start {
        let i = (current.y * w + current.x) as usize;
        current = came_from[i];
        path.push(current);
    }
    path.reverse();
    path
}

/// NavMesh triangle for 3D pathfinding.
#[derive(Debug, Clone)]
pub struct NavTriangle {
    pub vertices: [Vec2; 3],
    pub neighbors: [Option<usize>; 3], // adjacent triangle indices
    pub center: Vec2,
    pub cost: f32,
}

/// NavMesh.
pub struct NavMesh {
    pub triangles: Vec<NavTriangle>,
}

impl NavMesh {
    pub fn new() -> Self {
        Self {
            triangles: Vec::new(),
        }
    }

    /// Find which triangle contains a point.
    pub fn locate(&self, p: Vec2) -> Option<usize> {
        self.triangles
            .iter()
            .position(|t| point_in_triangle(p, t.vertices))
    }
}

fn point_in_triangle(p: Vec2, v: [Vec2; 3]) -> bool {
    let d1 = sign(p, v[0], v[1]);
    let d2 = sign(p, v[1], v[2]);
    let d3 = sign(p, v[2], v[0]);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn sign(p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
    (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_astar() {
        let grid = vec![
            vec![1, 1, 1, 1, 1],
            vec![1, 0, 0, 0, 1],
            vec![1, 0, 1, 0, 1],
            vec![1, 1, 1, 0, 1],
            vec![1, 1, 1, 1, 1],
        ];
        let path = astar_grid(&grid, GridPos { x: 0, y: 0 }, GridPos { x: 4, y: 4 });
        assert!(path.is_some());
        let p = path.unwrap();
        assert_eq!(p[0], GridPos { x: 0, y: 0 });
        assert_eq!(*p.last().unwrap(), GridPos { x: 4, y: 4 });
    }
}
