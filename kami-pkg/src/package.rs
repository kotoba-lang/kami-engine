/// IC package type definitions and estimation.
use serde::{Deserialize, Serialize};

/// IC package type with geometry parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageType {
    /// Quad Flat Package.
    QFP { pin_count: u32, pitch_mm: f64 },
    /// Ball Grid Array.
    BGA { rows: u32, cols: u32, pitch_mm: f64 },
    /// Chip Scale Package.
    CSP { rows: u32, cols: u32, pitch_mm: f64 },
    /// Wafer Level Chip Scale Package.
    WLCSP {
        bump_rows: u32,
        bump_cols: u32,
        bump_pitch_um: f64,
    },
    /// System in Package.
    SiP,
    /// 2.5D chiplet with silicon interposer.
    Chiplet2_5D,
    /// 3D stacked chiplet.
    Chiplet3D,
}

/// Complete package specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// Package name / part number.
    pub name: String,
    /// Package type.
    pub pkg_type: PackageType,
    /// Body dimensions (x, y, z) in mm.
    pub body_size_mm: (f64, f64, f64),
    /// Die dimensions (x, y) in mm.
    pub die_size_mm: (f64, f64),
    /// Total pin / ball count.
    pub pin_count: u32,
    /// Junction-to-case thermal resistance in C/W.
    pub thermal_resistance_jc: f64,
    /// Junction-to-ambient thermal resistance in C/W.
    pub thermal_resistance_ja: f64,
}

/// Estimate package body size and thermal properties from type and die size.
///
/// Body dimensions include clearance around the die. Thermal resistances
/// are estimated from package area using simplified empirical models.
pub fn estimate_package(pkg_type: PackageType, die_size_mm: (f64, f64)) -> Package {
    let (die_x, die_y) = die_size_mm;

    let (pin_count, body_x, body_y, body_z) = match &pkg_type {
        PackageType::QFP {
            pin_count,
            pitch_mm,
        } => {
            let side_pins = pin_count / 4;
            let body_side = side_pins as f64 * pitch_mm + 2.0;
            (*pin_count, body_side, body_side, 1.4)
        }
        PackageType::BGA {
            rows,
            cols,
            pitch_mm,
        } => {
            let count = rows * cols;
            let bx = *cols as f64 * pitch_mm + 1.0;
            let by = *rows as f64 * pitch_mm + 1.0;
            (count, bx, by, 1.2)
        }
        PackageType::CSP {
            rows,
            cols,
            pitch_mm,
        } => {
            let count = rows * cols;
            let bx = *cols as f64 * pitch_mm + 0.5;
            let by = *rows as f64 * pitch_mm + 0.5;
            (count, bx, by, 0.8)
        }
        PackageType::WLCSP {
            bump_rows,
            bump_cols,
            bump_pitch_um,
        } => {
            let count = bump_rows * bump_cols;
            let pitch_mm = bump_pitch_um / 1000.0;
            let bx = die_x + pitch_mm;
            let by = die_y + pitch_mm;
            (count, bx, by, 0.5)
        }
        PackageType::SiP => {
            let bx = die_x * 2.5;
            let by = die_y * 2.5;
            (256, bx, by, 2.0)
        }
        PackageType::Chiplet2_5D => {
            let bx = die_x * 3.0;
            let by = die_y * 2.0;
            (512, bx, by, 1.5)
        }
        PackageType::Chiplet3D => {
            let bx = die_x * 1.5;
            let by = die_y * 1.5;
            (1024, bx, by, 2.5)
        }
    };

    // Thermal estimation: theta_jc ~ 0.5 + 20 / area, theta_ja ~ theta_jc + 30 / area.
    let area = body_x * body_y;
    let theta_jc = 0.5 + 20.0 / area;
    let theta_ja = theta_jc + 30.0 / area;

    let name = match &pkg_type {
        PackageType::QFP { pin_count, .. } => format!("QFP-{pin_count}"),
        PackageType::BGA { rows, cols, .. } => format!("BGA-{}", rows * cols),
        PackageType::CSP { rows, cols, .. } => format!("CSP-{}", rows * cols),
        PackageType::WLCSP {
            bump_rows,
            bump_cols,
            ..
        } => format!("WLCSP-{}", bump_rows * bump_cols),
        PackageType::SiP => "SiP".to_string(),
        PackageType::Chiplet2_5D => "Chiplet-2.5D".to_string(),
        PackageType::Chiplet3D => "Chiplet-3D".to_string(),
    };

    Package {
        name,
        pkg_type,
        body_size_mm: (body_x, body_y, body_z),
        die_size_mm,
        pin_count,
        thermal_resistance_jc: theta_jc,
        thermal_resistance_ja: theta_ja,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bga_pin_count_correct() {
        let pkg = estimate_package(
            PackageType::BGA {
                rows: 16,
                cols: 16,
                pitch_mm: 0.8,
            },
            (5.0, 5.0),
        );
        assert_eq!(pkg.pin_count, 256);
        assert!(pkg.body_size_mm.0 > 5.0, "Body should be larger than die");
    }

    #[test]
    fn qfp_pin_count_preserved() {
        let pkg = estimate_package(
            PackageType::QFP {
                pin_count: 144,
                pitch_mm: 0.5,
            },
            (4.0, 4.0),
        );
        assert_eq!(pkg.pin_count, 144);
    }
}
