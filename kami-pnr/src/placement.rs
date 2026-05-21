/// Standard cell placement — greedy left-to-right row-based placement.

use serde::{Deserialize, Serialize};

/// Cell orientation (DEF/LEF convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    N,
    S,
    FN,
    FS,
    W,
    E,
    FW,
    FE,
}

/// A cell that has been assigned a physical location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedCell {
    pub cell_name: String,
    pub instance_name: String,
    pub x: f64,
    pub y: f64,
    pub orientation: Orientation,
    pub row_idx: usize,
}

/// A placement row (site row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRow {
    pub y: f64,
    pub height: f64,
    pub site_width: f64,
    pub num_sites: usize,
}

impl PlacementRow {
    pub fn total_width(&self) -> f64 {
        self.site_width * self.num_sites as f64
    }
}

/// Placement statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementStats {
    pub total_cells: usize,
    pub utilization: f64,
    pub hpwl: f64,
}

/// Container for placed cells and placement rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub rows: Vec<PlacementRow>,
    pub cells: Vec<PlacedCell>,
}

impl Placement {
    pub fn new(rows: Vec<PlacementRow>) -> Self {
        Self {
            rows,
            cells: Vec::new(),
        }
    }

    /// Compute placement statistics including half-perimeter wirelength estimate.
    pub fn placement_stats(&self) -> PlacementStats {
        let total_cells = self.cells.len();
        let total_row_area: f64 = self.rows.iter().map(|r| r.total_width() * r.height).sum();
        // Assume each cell occupies one site_width * row_height
        let cell_area: f64 = self.cells.iter().map(|c| {
            if let Some(row) = self.rows.get(c.row_idx) {
                row.site_width * row.height
            } else {
                0.0
            }
        }).sum();
        let utilization = if total_row_area > 0.0 { cell_area / total_row_area } else { 0.0 };

        // HPWL: simple bounding-box estimate over all cells
        let hpwl = if self.cells.len() >= 2 {
            let min_x = self.cells.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);
            let max_x = self.cells.iter().map(|c| c.x).fold(f64::NEG_INFINITY, f64::max);
            let min_y = self.cells.iter().map(|c| c.y).fold(f64::INFINITY, f64::min);
            let max_y = self.cells.iter().map(|c| c.y).fold(f64::NEG_INFINITY, f64::max);
            (max_x - min_x) + (max_y - min_y)
        } else {
            0.0
        };

        PlacementStats { total_cells, utilization, hpwl }
    }
}

/// A netlist cell to be placed (cell type name + instance name + width in sites).
pub struct NetlistCell {
    pub cell_name: String,
    pub instance_name: String,
    pub width_sites: usize,
}

/// Greedy left-to-right placement: fill rows sequentially, alternating orientation
/// per row for abutment.
pub fn place_cells(netlist: Vec<NetlistCell>, rows: Vec<PlacementRow>) -> Placement {
    let mut placement = Placement::new(rows);
    let mut row_idx = 0;
    let mut site_cursor = 0_usize;

    for cell in netlist {
        // Advance to a row that has space
        while row_idx < placement.rows.len() {
            if site_cursor + cell.width_sites <= placement.rows[row_idx].num_sites {
                break;
            }
            row_idx += 1;
            site_cursor = 0;
        }
        if row_idx >= placement.rows.len() {
            log::warn!("No space for cell '{}', placement overflow", cell.instance_name);
            break;
        }

        let row = &placement.rows[row_idx];
        let orientation = if row_idx % 2 == 0 { Orientation::N } else { Orientation::FS };
        let x = site_cursor as f64 * row.site_width;
        let y = row.y;

        placement.cells.push(PlacedCell {
            cell_name: cell.cell_name,
            instance_name: cell.instance_name,
            x,
            y,
            orientation,
            row_idx,
        });

        site_cursor += cell.width_sites;
    }

    placement
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_to_right_placement() {
        let rows = vec![
            PlacementRow { y: 0.0, height: 10.0, site_width: 1.0, num_sites: 5 },
            PlacementRow { y: 10.0, height: 10.0, site_width: 1.0, num_sites: 5 },
        ];
        let cells: Vec<NetlistCell> = (0..8).map(|i| NetlistCell {
            cell_name: "INV".into(),
            instance_name: format!("U{i}"),
            width_sites: 1,
        }).collect();

        let p = place_cells(cells, rows);
        assert_eq!(p.cells.len(), 8);
        // First 5 in row 0, next 3 in row 1
        assert_eq!(p.cells[4].row_idx, 0);
        assert_eq!(p.cells[5].row_idx, 1);
        assert_eq!(p.cells[5].x, 0.0);
        // Row 0 → N orientation, row 1 → FS
        assert_eq!(p.cells[0].orientation, Orientation::N);
        assert_eq!(p.cells[5].orientation, Orientation::FS);
    }

    #[test]
    fn placement_stats_hpwl() {
        let rows = vec![
            PlacementRow { y: 0.0, height: 10.0, site_width: 1.0, num_sites: 100 },
        ];
        let cells = vec![
            NetlistCell { cell_name: "BUF".into(), instance_name: "U0".into(), width_sites: 1 },
            NetlistCell { cell_name: "BUF".into(), instance_name: "U1".into(), width_sites: 1 },
        ];
        let p = place_cells(cells, rows);
        let stats = p.placement_stats();
        assert_eq!(stats.total_cells, 2);
        // Both in row 0 at x=0 and x=1, same y → HPWL = 1.0
        assert!((stats.hpwl - 1.0).abs() < 1e-9);
    }
}
