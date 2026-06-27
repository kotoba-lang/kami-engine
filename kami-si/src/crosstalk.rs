use crate::transmission_line::TLineParams;
/// Crosstalk analysis between adjacent transmission lines.
use serde::{Deserialize, Serialize};

/// Type of electromagnetic coupling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CouplingType {
    /// Forward (far-end) crosstalk.
    Forward,
    /// Backward (near-end) crosstalk.
    Backward,
}

/// Result of crosstalk analysis between two nets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrosstalkResult {
    /// Name of the victim net.
    pub victim_net: String,
    /// Name of the aggressor net.
    pub aggressor_net: String,
    /// Forward or backward coupling.
    pub coupling_type: CouplingType,
    /// Peak induced voltage in mV.
    pub peak_mv: f64,
    /// Pulse width of induced noise in ps.
    pub width_ps: f64,
}

/// Analyze crosstalk coupling between a victim and aggressor transmission line.
///
/// Uses simplified capacitive/inductive coupling model. Backward (near-end)
/// crosstalk depends on coupling length relative to signal rise time; forward
/// (far-end) crosstalk depends on the difference in inductive and capacitive
/// coupling coefficients.
pub fn analyze_crosstalk(
    victim: &TLineParams,
    aggressor: &TLineParams,
    coupling_length_mm: f64,
    spacing_mm: f64,
    rise_time_ps: f64,
) -> CrosstalkResult {
    // Coupling coefficient decreases with spacing (simplified exponential model).
    let k_coupling = (-spacing_mm / 0.3).exp();

    // Backward crosstalk coefficient (NEXT).
    let td = victim.delay_ps_per_mm * coupling_length_mm;
    let kb = k_coupling * 0.25;

    // Saturation: backward crosstalk saturates when coupled length > rise_time / (2 * delay).
    let saturation_length = rise_time_ps / (2.0 * victim.delay_ps_per_mm);
    let effective_kb = if coupling_length_mm > saturation_length {
        kb
    } else {
        kb * coupling_length_mm / saturation_length
    };

    // Aggressor amplitude assumed from impedance (1V driver into Z0).
    let aggressor_amplitude_mv = 1000.0 * aggressor.z0_ohm / (aggressor.z0_ohm + 50.0);

    let backward_peak = effective_kb * aggressor_amplitude_mv;
    let backward_width = 2.0 * td;

    // Forward crosstalk coefficient (FEXT) — proportional to coupling_length * delta(Kl-Kc).
    let kf = k_coupling * 0.05 * coupling_length_mm / saturation_length.max(0.1);
    let forward_peak = kf * aggressor_amplitude_mv;
    let forward_width = rise_time_ps;

    // Return the dominant coupling type.
    if backward_peak >= forward_peak {
        CrosstalkResult {
            victim_net: "victim".to_string(),
            aggressor_net: "aggressor".to_string(),
            coupling_type: CouplingType::Backward,
            peak_mv: backward_peak,
            width_ps: backward_width,
        }
    } else {
        CrosstalkResult {
            victim_net: "victim".to_string(),
            aggressor_net: "aggressor".to_string(),
            coupling_type: CouplingType::Forward,
            peak_mv: forward_peak,
            width_ps: forward_width,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosstalk_coupling_decreases_with_spacing() {
        let victim = TLineParams {
            z0_ohm: 50.0,
            delay_ps_per_mm: 7.0,
            loss_db_per_mm: 0.001,
            length_mm: 20.0,
        };
        let aggressor = TLineParams {
            z0_ohm: 50.0,
            delay_ps_per_mm: 7.0,
            loss_db_per_mm: 0.001,
            length_mm: 20.0,
        };

        let close = analyze_crosstalk(&victim, &aggressor, 10.0, 0.15, 50.0);
        let far = analyze_crosstalk(&victim, &aggressor, 10.0, 0.50, 50.0);
        assert!(
            close.peak_mv > far.peak_mv,
            "Closer spacing should have more crosstalk: close={} > far={}",
            close.peak_mv,
            far.peak_mv
        );
    }

    #[test]
    fn crosstalk_has_positive_peak() {
        let victim = TLineParams {
            z0_ohm: 50.0,
            delay_ps_per_mm: 7.0,
            loss_db_per_mm: 0.001,
            length_mm: 20.0,
        };
        let aggressor = TLineParams {
            z0_ohm: 50.0,
            delay_ps_per_mm: 7.0,
            loss_db_per_mm: 0.001,
            length_mm: 20.0,
        };
        let result = analyze_crosstalk(&victim, &aggressor, 10.0, 0.2, 50.0);
        assert!(result.peak_mv > 0.0);
    }
}
