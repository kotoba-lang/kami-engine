/// PVT (Process-Voltage-Temperature) corner analysis.
use serde::{Deserialize, Serialize};

/// Process corner classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ProcessCorner {
    /// Typical-Typical (NMOS typical, PMOS typical).
    TT,
    /// Fast-Fast.
    FF,
    /// Slow-Slow.
    SS,
    /// Fast NMOS, Slow PMOS.
    FS,
    /// Slow NMOS, Fast PMOS.
    SF,
}

/// A single PVT corner specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PvtCorner {
    /// Corner name.
    pub name: String,
    /// Process corner.
    pub process: ProcessCorner,
    /// Supply voltage in V.
    pub voltage: f64,
    /// Temperature in degrees C.
    pub temperature_c: f64,
}

/// Result of evaluating a design at one PVT corner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CornerResult {
    /// Corner name.
    pub corner_name: String,
    /// Evaluated metric value.
    pub value: f64,
    /// Whether the value meets specification.
    pub pass: bool,
}

/// Return the 5 standard PVT corners (TT/FF/SS/FS/SF at nominal, best, worst conditions).
pub fn standard_corners() -> Vec<PvtCorner> {
    vec![
        PvtCorner {
            name: "TT_0.9V_25C".to_string(),
            process: ProcessCorner::TT,
            voltage: 0.9,
            temperature_c: 25.0,
        },
        PvtCorner {
            name: "FF_0.99V_-40C".to_string(),
            process: ProcessCorner::FF,
            voltage: 0.99,
            temperature_c: -40.0,
        },
        PvtCorner {
            name: "SS_0.81V_125C".to_string(),
            process: ProcessCorner::SS,
            voltage: 0.81,
            temperature_c: 125.0,
        },
        PvtCorner {
            name: "FS_0.9V_25C".to_string(),
            process: ProcessCorner::FS,
            voltage: 0.9,
            temperature_c: 25.0,
        },
        PvtCorner {
            name: "SF_0.9V_25C".to_string(),
            process: ProcessCorner::SF,
            voltage: 0.9,
            temperature_c: 25.0,
        },
    ]
}

/// Run all corners through an evaluation function and check against specification.
///
/// The `eval_fn` receives (voltage, temperature_c) and returns a metric value.
/// Process corner effects should be embedded in the function or handled externally.
pub fn run_corners(
    corners: &[PvtCorner],
    eval_fn: fn(f64, f64) -> f64,
    spec_min: f64,
    spec_max: f64,
) -> Vec<CornerResult> {
    corners
        .iter()
        .map(|c| {
            let value = eval_fn(c.voltage, c.temperature_c);
            let pass = value >= spec_min && value <= spec_max;
            CornerResult {
                corner_name: c.name.clone(),
                value,
                pass,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_corners_count() {
        let corners = standard_corners();
        assert_eq!(corners.len(), 5, "Should have 5 standard PVT corners");
    }

    #[test]
    fn run_corners_pass_fail() {
        let corners = standard_corners();
        // Simple delay model: delay ~ 1 / (voltage * (1 - 0.002 * temp)).
        let results = run_corners(&corners, |v, t| 1.0 / (v * (1.0 - 0.002 * t)), 0.5, 2.0);
        assert_eq!(results.len(), 5);
        // TT corner at nominal should pass easily.
        assert!(
            results[0].pass,
            "TT corner should pass: value={}",
            results[0].value
        );
    }
}
