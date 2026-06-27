/// Clock Tree Synthesis — balanced buffer tree construction for clock distribution.
use serde::{Deserialize, Serialize};

/// Specification for clock tree construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtsSpec {
    pub clock_name: String,
    pub target_skew_ps: f64,
    pub max_transition_ps: f64,
    pub buffer_cell: String,
}

/// A buffer inserted in the clock tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtsBuffer {
    pub name: String,
    pub cell_type: String,
    pub x: f64,
    pub y: f64,
    pub load_cap: f64,
}

/// A wire segment in the clock tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtsWire {
    pub from: String,
    pub to: String,
    pub length: f64,
    pub layer: String,
}

/// One level of the clock tree (root is level 0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtsLevel {
    pub buffers: Vec<CtsBuffer>,
    pub wire_segments: Vec<CtsWire>,
}

/// The complete clock tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockTree {
    pub root_buffer: CtsBuffer,
    pub levels: Vec<CtsLevel>,
}

/// Statistics for a built clock tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtsStats {
    pub num_buffers: usize,
    pub num_levels: usize,
    pub max_skew_ps: f64,
    pub total_wire_length: f64,
}

impl ClockTree {
    /// Compute clock tree statistics.
    pub fn clock_tree_stats(&self) -> CtsStats {
        let mut num_buffers = 1; // root
        let mut total_wire_length = 0.0;
        let mut max_wire = 0.0_f64;
        let mut min_wire = f64::INFINITY;

        for level in &self.levels {
            num_buffers += level.buffers.len();
            for wire in &level.wire_segments {
                total_wire_length += wire.length;
                max_wire = max_wire.max(wire.length);
                min_wire = min_wire.min(wire.length);
            }
        }

        // Skew estimate: proportional to max wire length difference
        let max_skew_ps = if min_wire.is_finite() {
            (max_wire - min_wire) * 10.0 // rough: 10 ps/unit length
        } else {
            0.0
        };

        CtsStats {
            num_buffers,
            num_levels: self.levels.len(),
            max_skew_ps,
            total_wire_length,
        }
    }
}

/// Build a balanced H-tree clock distribution from sink positions.
///
/// Uses recursive bisection: at each level, split sinks into two groups,
/// place a buffer at the centroid of each group, and wire from parent buffer.
pub fn build_clock_tree(spec: &CtsSpec, sink_positions: &[(f64, f64)]) -> ClockTree {
    if sink_positions.is_empty() {
        return ClockTree {
            root_buffer: CtsBuffer {
                name: format!("{}_root", spec.clock_name),
                cell_type: spec.buffer_cell.clone(),
                x: 0.0,
                y: 0.0,
                load_cap: 0.0,
            },
            levels: Vec::new(),
        };
    }

    // Root at centroid of all sinks
    let (cx, cy) = centroid(sink_positions);
    let root_buffer = CtsBuffer {
        name: format!("{}_root", spec.clock_name),
        cell_type: spec.buffer_cell.clone(),
        x: cx,
        y: cy,
        load_cap: sink_positions.len() as f64 * 0.01, // rough estimate
    };

    let mut levels = Vec::new();
    let mut current_groups: Vec<(String, Vec<(f64, f64)>)> =
        vec![(root_buffer.name.clone(), sink_positions.to_vec())];
    let mut buf_counter = 0_usize;

    // Recursively bisect until each group has <= 4 sinks
    while current_groups.iter().any(|(_, sinks)| sinks.len() > 4) {
        let mut level = CtsLevel {
            buffers: Vec::new(),
            wire_segments: Vec::new(),
        };
        let mut next_groups = Vec::new();

        for (parent_name, sinks) in &current_groups {
            if sinks.len() <= 4 {
                // Pass through to next iteration
                next_groups.push((parent_name.clone(), sinks.clone()));
                continue;
            }
            let (left, right) = bisect_sinks(sinks);
            for half in [&left, &right] {
                let (bx, by) = centroid(half);
                let buf_name = format!("{}_buf{}", spec.clock_name, buf_counter);
                buf_counter += 1;
                let parent_pos = find_buf_pos(&root_buffer, &levels, parent_name);
                let wire_len = ((bx - parent_pos.0).powi(2) + (by - parent_pos.1).powi(2)).sqrt();

                level.buffers.push(CtsBuffer {
                    name: buf_name.clone(),
                    cell_type: spec.buffer_cell.clone(),
                    x: bx,
                    y: by,
                    load_cap: half.len() as f64 * 0.01,
                });
                level.wire_segments.push(CtsWire {
                    from: parent_name.clone(),
                    to: buf_name.clone(),
                    length: wire_len,
                    layer: "M3".into(),
                });
                next_groups.push((buf_name, half.clone()));
            }
        }
        levels.push(level);
        current_groups = next_groups;
    }

    // Final level: wire from leaf buffers to sinks
    let mut leaf_level = CtsLevel {
        buffers: Vec::new(),
        wire_segments: Vec::new(),
    };
    for (parent_name, sinks) in &current_groups {
        let parent_pos = find_buf_pos(&root_buffer, &levels, parent_name);
        for (i, &(sx, sy)) in sinks.iter().enumerate() {
            let wire_len = ((sx - parent_pos.0).powi(2) + (sy - parent_pos.1).powi(2)).sqrt();
            leaf_level.wire_segments.push(CtsWire {
                from: parent_name.clone(),
                to: format!("sink_{parent_name}_{i}"),
                length: wire_len,
                layer: "M2".into(),
            });
        }
    }
    if !leaf_level.wire_segments.is_empty() {
        levels.push(leaf_level);
    }

    ClockTree {
        root_buffer,
        levels,
    }
}

fn centroid(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let sx: f64 = points.iter().map(|p| p.0).sum();
    let sy: f64 = points.iter().map(|p| p.1).sum();
    (sx / n, sy / n)
}

fn bisect_sinks(sinks: &[(f64, f64)]) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
    let mut sorted = sinks.to_vec();
    // Split along the longer axis
    let (min_x, max_x) = sorted
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), p| {
            (mn.min(p.0), mx.max(p.0))
        });
    let (min_y, max_y) = sorted
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), p| {
            (mn.min(p.1), mx.max(p.1))
        });
    if (max_x - min_x) >= (max_y - min_y) {
        sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    } else {
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    }
    let mid = sorted.len() / 2;
    (sorted[..mid].to_vec(), sorted[mid..].to_vec())
}

fn find_buf_pos(root: &CtsBuffer, levels: &[CtsLevel], name: &str) -> (f64, f64) {
    if root.name == name {
        return (root.x, root.y);
    }
    for level in levels {
        for buf in &level.buffers {
            if buf.name == name {
                return (buf.x, buf.y);
            }
        }
    }
    (root.x, root.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cts_buffer_count() {
        let spec = CtsSpec {
            clock_name: "clk".into(),
            target_skew_ps: 50.0,
            max_transition_ps: 100.0,
            buffer_cell: "CLKBUF_X4".into(),
        };
        // 16 sinks in a grid
        let sinks: Vec<(f64, f64)> = (0..16)
            .map(|i| ((i % 4) as f64 * 100.0, (i / 4) as f64 * 100.0))
            .collect();

        let tree = build_clock_tree(&spec, &sinks);
        let stats = tree.clock_tree_stats();
        // Should have root + intermediate buffers
        assert!(stats.num_buffers >= 1);
        assert!(stats.num_levels >= 1);
        assert!(stats.total_wire_length > 0.0);
    }
}
