/// Wire bond and flip chip bonding diagram generation.

use serde::{Deserialize, Serialize};

/// Bond interconnect type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BondType {
    /// Gold or copper wire bond.
    WireBond {
        /// Wire diameter in um.
        wire_diameter_um: f64,
        /// Loop height in um.
        loop_height_um: f64,
    },
    /// Solder bump flip chip.
    FlipChip {
        /// Bump pitch in um.
        bump_pitch_um: f64,
        /// Bump diameter in um.
        bump_diameter_um: f64,
    },
    /// Thermo-compression bonding (Cu pillar).
    ThermoCompression,
}

/// A single bond connection between die pad and package pad.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bond {
    /// Pad name or identifier.
    pub pad_name: String,
    /// Die pad X coordinate in mm.
    pub die_x: f64,
    /// Die pad Y coordinate in mm.
    pub die_y: f64,
    /// Package pad X coordinate in mm.
    pub pkg_x: f64,
    /// Package pad Y coordinate in mm.
    pub pkg_y: f64,
    /// Bond type.
    pub bond_type: BondType,
}

/// Complete bonding diagram.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondDiagram {
    /// All bond connections.
    pub bonds: Vec<Bond>,
}

/// Die pad location.
#[derive(Debug, Clone)]
pub struct PadLocation {
    /// Pad name.
    pub name: String,
    /// X coordinate in mm.
    pub x: f64,
    /// Y coordinate in mm.
    pub y: f64,
}

/// Generate a bond diagram connecting die pads to package pads in order.
///
/// Each die pad is matched to the corresponding package pad by index.
/// The bond type is applied uniformly to all connections.
pub fn generate_bond_diagram(
    die_pads: &[PadLocation],
    pkg_pads: &[PadLocation],
    bond_type: BondType,
) -> BondDiagram {
    let count = die_pads.len().min(pkg_pads.len());
    let bonds = (0..count)
        .map(|i| Bond {
            pad_name: die_pads[i].name.clone(),
            die_x: die_pads[i].x,
            die_y: die_pads[i].y,
            pkg_x: pkg_pads[i].x,
            pkg_y: pkg_pads[i].y,
            bond_type: bond_type.clone(),
        })
        .collect();

    BondDiagram { bonds }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_bond_diagram_generation() {
        let die_pads = vec![
            PadLocation { name: "VDD".to_string(), x: 0.1, y: 0.1 },
            PadLocation { name: "GND".to_string(), x: 0.2, y: 0.1 },
            PadLocation { name: "IO0".to_string(), x: 0.3, y: 0.1 },
        ];
        let pkg_pads = vec![
            PadLocation { name: "P1".to_string(), x: 1.0, y: 0.5 },
            PadLocation { name: "P2".to_string(), x: 2.0, y: 0.5 },
            PadLocation { name: "P3".to_string(), x: 3.0, y: 0.5 },
        ];
        let bond_type = BondType::WireBond { wire_diameter_um: 25.0, loop_height_um: 150.0 };
        let diagram = generate_bond_diagram(&die_pads, &pkg_pads, bond_type);
        assert_eq!(diagram.bonds.len(), 3);
        assert_eq!(diagram.bonds[0].pad_name, "VDD");
    }
}
