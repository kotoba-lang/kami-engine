/// Chip floorplanning — block placement, IO pin assignment, and utilization analysis.
use serde::{Deserialize, Serialize};

/// Block functional type within the floorplan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    Macro,
    StdCellRegion,
    IOPad,
    PowerDomain,
}

/// Side of the die for IO pin placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinSide {
    North,
    South,
    East,
    West,
}

/// A rectangular block in the floorplan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorplanBlock {
    pub name: String,
    pub block_type: BlockType,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fixed: bool,
}

impl FloorplanBlock {
    pub fn area(&self) -> f64 {
        self.width * self.height
    }
}

/// An IO pin on the die perimeter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoPin {
    pub name: String,
    pub side: PinSide,
    pub offset: f64,
}

/// Top-level floorplan containing die dimensions, blocks, and IO pins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Floorplan {
    pub die_width: f64,
    pub die_height: f64,
    pub blocks: Vec<FloorplanBlock>,
    pub io_pins: Vec<IoPin>,
}

impl Floorplan {
    pub fn new(die_width: f64, die_height: f64) -> Self {
        Self {
            die_width,
            die_height,
            blocks: Vec::new(),
            io_pins: Vec::new(),
        }
    }

    pub fn add_block(&mut self, block: FloorplanBlock) {
        self.blocks.push(block);
    }

    pub fn add_io_pin(&mut self, pin: IoPin) {
        self.io_pins.push(pin);
    }

    /// Ratio of total block area to die area.
    pub fn utilization(&self) -> f64 {
        let total_block_area: f64 = self.blocks.iter().map(|b| b.area()).sum();
        let die_area = self.die_width * self.die_height;
        if die_area == 0.0 {
            return 0.0;
        }
        total_block_area / die_area
    }

    /// Check for overlapping blocks. Returns a list of violation descriptions.
    pub fn validate(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for i in 0..self.blocks.len() {
            let a = &self.blocks[i];
            // Check die boundary
            if a.x < 0.0
                || a.y < 0.0
                || a.x + a.width > self.die_width
                || a.y + a.height > self.die_height
            {
                violations.push(format!("Block '{}' extends outside die boundary", a.name));
            }
            for j in (i + 1)..self.blocks.len() {
                let b = &self.blocks[j];
                // Axis-aligned overlap test
                if a.x < b.x + b.width
                    && a.x + a.width > b.x
                    && a.y < b.y + b.height
                    && a.y + a.height > b.y
                {
                    violations.push(format!("Overlap between '{}' and '{}'", a.name, b.name));
                }
            }
        }
        violations
    }
}

/// Simple row-based automatic floorplan: packs blocks left-to-right in rows.
pub fn auto_floorplan(blocks: Vec<FloorplanBlock>, die_width: f64, die_height: f64) -> Floorplan {
    let mut fp = Floorplan::new(die_width, die_height);
    let mut cursor_x = 0.0;
    let mut cursor_y = 0.0;
    let mut row_height = 0.0_f64;

    for mut block in blocks {
        if cursor_x + block.width > die_width {
            // Advance to next row
            cursor_y += row_height;
            cursor_x = 0.0;
            row_height = 0.0;
        }
        block.x = cursor_x;
        block.y = cursor_y;
        cursor_x += block.width;
        row_height = row_height.max(block.height);
        fp.add_block(block);
    }
    fp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utilization_calculation() {
        let mut fp = Floorplan::new(100.0, 100.0);
        fp.add_block(FloorplanBlock {
            name: "ram".into(),
            block_type: BlockType::Macro,
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
            fixed: true,
        });
        fp.add_block(FloorplanBlock {
            name: "logic".into(),
            block_type: BlockType::StdCellRegion,
            x: 50.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
            fixed: false,
        });
        // Two 50x50 blocks = 5000 area, die = 10000
        assert!((fp.utilization() - 0.5).abs() < 1e-9);
        assert!(fp.validate().is_empty());
    }

    #[test]
    fn overlap_detection() {
        let mut fp = Floorplan::new(100.0, 100.0);
        fp.add_block(FloorplanBlock {
            name: "a".into(),
            block_type: BlockType::Macro,
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 60.0,
            fixed: false,
        });
        fp.add_block(FloorplanBlock {
            name: "b".into(),
            block_type: BlockType::Macro,
            x: 50.0,
            y: 50.0,
            width: 40.0,
            height: 40.0,
            fixed: false,
        });
        let v = fp.validate();
        assert!(v.iter().any(|s| s.contains("Overlap")));
    }

    #[test]
    fn auto_floorplan_row_packing() {
        let blocks = vec![
            FloorplanBlock {
                name: "a".into(),
                block_type: BlockType::StdCellRegion,
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
                fixed: false,
            },
            FloorplanBlock {
                name: "b".into(),
                block_type: BlockType::StdCellRegion,
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
                fixed: false,
            },
            FloorplanBlock {
                name: "c".into(),
                block_type: BlockType::StdCellRegion,
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
                fixed: false,
            },
        ];
        let fp = auto_floorplan(blocks, 100.0, 100.0);
        // a at (0,0), b at (40,0), c wraps to (80,0) — still fits since 80+40>100 → next row
        // Actually 40+40=80 fits, then 80+40=120 > 100, so c goes to next row
        assert_eq!(fp.blocks[0].x, 0.0);
        assert_eq!(fp.blocks[1].x, 40.0);
        assert_eq!(fp.blocks[2].x, 0.0);
        assert_eq!(fp.blocks[2].y, 20.0);
    }
}
