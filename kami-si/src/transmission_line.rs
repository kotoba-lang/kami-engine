/// Transmission line parameter calculation for microstrip, stripline, and coplanar waveguide.
use serde::{Deserialize, Serialize};

/// Transmission line parameters computed from geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TLineParams {
    /// Characteristic impedance in ohms.
    pub z0_ohm: f64,
    /// Propagation delay in ps/mm.
    pub delay_ps_per_mm: f64,
    /// Loss in dB/mm.
    pub loss_db_per_mm: f64,
    /// Physical length in mm.
    pub length_mm: f64,
}

/// Transmission line geometry type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TLineType {
    /// Microstrip: trace on top of dielectric above ground plane.
    Microstrip {
        /// Trace width in mm.
        width: f64,
        /// Dielectric height in mm.
        height: f64,
        /// Relative permittivity.
        er: f64,
    },
    /// Stripline: trace between two ground planes.
    Stripline {
        /// Trace width in mm.
        width: f64,
        /// Height to upper ground plane in mm.
        height1: f64,
        /// Height to lower ground plane in mm.
        height2: f64,
        /// Relative permittivity.
        er: f64,
    },
    /// Coplanar waveguide: trace with ground on same layer.
    Coplanar {
        /// Trace width in mm.
        width: f64,
        /// Gap to ground in mm.
        gap: f64,
        /// Dielectric height in mm.
        height: f64,
        /// Relative permittivity.
        er: f64,
    },
}

/// Trace thickness in mm (assumed 1 oz copper = 0.035 mm).
const TRACE_THICKNESS_MM: f64 = 0.035;

/// Speed of light in mm/ps.
const C_MM_PER_PS: f64 = 0.2998;

/// Calculate transmission line parameters from geometry.
///
/// Microstrip Z0 uses the IPC-2141 formula:
///   Z0 = (87 / sqrt(er + 1.41)) * ln(5.98 * h / (0.8 * w + t))
///
/// Stripline and coplanar use simplified closed-form approximations.
pub fn calculate_z0(tline_type: &TLineType, length_mm: f64) -> TLineParams {
    match tline_type {
        TLineType::Microstrip { width, height, er } => {
            let t = TRACE_THICKNESS_MM;
            let z0 = (87.0 / (er + 1.41).sqrt()) * (5.98 * height / (0.8 * width + t)).ln();
            let eff_er =
                (er + 1.0) / 2.0 + (er - 1.0) / (2.0 * (1.0 + 12.0 * height / width).sqrt());
            let delay = eff_er.sqrt() / C_MM_PER_PS;
            let loss = 0.001 * (1.0 + 1.0 / z0); // simplified conductor + dielectric loss
            TLineParams {
                z0_ohm: z0,
                delay_ps_per_mm: delay,
                loss_db_per_mm: loss,
                length_mm,
            }
        }
        TLineType::Stripline {
            width,
            height1,
            height2,
            er,
        } => {
            let b = height1 + height2;
            let t = TRACE_THICKNESS_MM;
            let z0 = (60.0 / er.sqrt())
                * (4.0 * b / (0.67 * std::f64::consts::PI * (0.8 * width + t))).ln();
            let delay = er.sqrt() / C_MM_PER_PS;
            let loss = 0.0008 * (1.0 + 1.0 / z0);
            TLineParams {
                z0_ohm: z0,
                delay_ps_per_mm: delay,
                loss_db_per_mm: loss,
                length_mm,
            }
        }
        TLineType::Coplanar {
            width,
            gap,
            height,
            er,
        } => {
            // Simplified coplanar waveguide impedance.
            let k = width / (width + 2.0 * gap);
            let eff_er = 1.0 + (er - 1.0) / 2.0 * (1.0 / (1.0 + 0.7 * (width / height)).sqrt());
            let z0 = (30.0 * std::f64::consts::PI / eff_er.sqrt()) * (1.0 / k).ln();
            let delay = eff_er.sqrt() / C_MM_PER_PS;
            let loss = 0.0012 * (1.0 + 1.0 / z0);
            TLineParams {
                z0_ohm: z0,
                delay_ps_per_mm: delay,
                loss_db_per_mm: loss,
                length_mm,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microstrip_z0_near_50_ohm() {
        // Standard 50-ohm microstrip: ~0.2mm width, 0.2mm height, FR-4 er=4.3
        let tline = TLineType::Microstrip {
            width: 0.2,
            height: 0.2,
            er: 4.3,
        };
        let params = calculate_z0(&tline, 25.0);
        assert!(
            params.z0_ohm > 30.0 && params.z0_ohm < 80.0,
            "Z0 should be in reasonable range, got {}",
            params.z0_ohm
        );
        assert_eq!(params.length_mm, 25.0);
    }

    #[test]
    fn stripline_z0_positive() {
        let tline = TLineType::Stripline {
            width: 0.15,
            height1: 0.2,
            height2: 0.2,
            er: 4.3,
        };
        let params = calculate_z0(&tline, 10.0);
        assert!(params.z0_ohm > 0.0, "Z0 must be positive");
    }
}
