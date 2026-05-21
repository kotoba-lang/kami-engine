/// Thermal analysis for IC packages.

use serde::{Deserialize, Serialize};

/// Thermal analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalResult {
    /// Junction temperature in degrees C.
    pub junction_temp_c: f64,
    /// Case temperature in degrees C.
    pub case_temp_c: f64,
    /// Total power dissipation in W.
    pub power_w: f64,
    /// Ambient temperature in degrees C.
    pub ambient_c: f64,
    /// Effective junction-to-ambient thermal resistance in C/W.
    pub theta_ja: f64,
}

/// Thermal specification inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalSpec {
    /// Total power dissipation in W.
    pub power_w: f64,
    /// Ambient temperature in degrees C.
    pub ambient_c: f64,
    /// Junction-to-case thermal resistance in C/W.
    pub theta_jc: f64,
    /// Case-to-ambient thermal resistance in C/W.
    pub theta_ca: f64,
    /// Airflow velocity in m/s (None for natural convection).
    pub airflow_m_per_s: Option<f64>,
}

/// Calculate junction and case temperatures from thermal specification.
///
/// Uses the simple thermal resistance network:
///   Tj = Ta + P * (theta_jc + theta_ca_effective)
///   Tc = Ta + P * theta_ca_effective
///
/// Forced airflow reduces theta_ca by an empirical factor.
pub fn calculate_thermal(spec: &ThermalSpec) -> ThermalResult {
    let theta_ca_effective = match spec.airflow_m_per_s {
        Some(v) if v > 0.0 => {
            // Empirical reduction: theta_ca decreases roughly as 1/sqrt(velocity).
            spec.theta_ca / (1.0 + v).sqrt()
        }
        _ => spec.theta_ca,
    };

    let theta_ja = spec.theta_jc + theta_ca_effective;
    let case_temp = spec.ambient_c + spec.power_w * theta_ca_effective;
    let junction_temp = spec.ambient_c + spec.power_w * theta_ja;

    ThermalResult {
        junction_temp_c: junction_temp,
        case_temp_c: case_temp,
        power_w: spec.power_w,
        ambient_c: spec.ambient_c,
        theta_ja,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_junction_temp_above_ambient() {
        let spec = ThermalSpec {
            power_w: 2.0,
            ambient_c: 25.0,
            theta_jc: 5.0,
            theta_ca: 20.0,
            airflow_m_per_s: None,
        };
        let result = calculate_thermal(&spec);
        // Tj = 25 + 2 * (5 + 20) = 75 C
        assert!((result.junction_temp_c - 75.0).abs() < 0.01,
            "Expected Tj=75, got {}", result.junction_temp_c);
        assert!(result.case_temp_c > spec.ambient_c);
        assert!(result.junction_temp_c > result.case_temp_c);
    }

    #[test]
    fn airflow_reduces_theta_ja() {
        let base = ThermalSpec {
            power_w: 1.0,
            ambient_c: 25.0,
            theta_jc: 5.0,
            theta_ca: 30.0,
            airflow_m_per_s: None,
        };
        let forced = ThermalSpec {
            airflow_m_per_s: Some(2.0),
            ..base.clone()
        };
        let r_nat = calculate_thermal(&base);
        let r_forced = calculate_thermal(&forced);
        assert!(r_forced.theta_ja < r_nat.theta_ja,
            "Forced airflow should reduce theta_ja: {} < {}", r_forced.theta_ja, r_nat.theta_ja);
    }
}
