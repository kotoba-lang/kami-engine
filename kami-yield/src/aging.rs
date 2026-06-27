/// Aging and degradation mechanisms estimation (NBTI, PBTI, HCI, TDDB, EM).
use serde::{Deserialize, Serialize};

/// Aging degradation mechanism.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AgingMechanism {
    /// Negative Bias Temperature Instability (PMOS).
    Nbti,
    /// Positive Bias Temperature Instability (NMOS).
    Pbti,
    /// Hot Carrier Injection.
    Hci,
    /// Time-Dependent Dielectric Breakdown.
    Tddb,
    /// Electromigration.
    Em,
}

/// Result of aging estimation for a single mechanism.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgingResult {
    /// Which aging mechanism.
    pub mechanism: AgingMechanism,
    /// Affected parameter name.
    pub parameter_name: String,
    /// Percentage degradation.
    pub degradation_percent: f64,
    /// Time in years.
    pub time_years: f64,
    /// Whether degradation is within acceptable limits.
    pub pass: bool,
}

/// Boltzmann constant in eV/K.
const K_B: f64 = 8.617e-5;

/// Estimate aging degradation using Arrhenius-based models.
///
/// Each mechanism has empirical activation energy and voltage acceleration factors.
/// The degradation percentage is estimated for the given operating conditions
/// and time duration.
pub fn estimate_aging(
    mechanism: AgingMechanism,
    voltage: f64,
    temperature_c: f64,
    time_years: f64,
) -> AgingResult {
    let temp_k = temperature_c + 273.15;
    let time_hours = time_years * 8766.0; // hours per year

    let (param_name, degradation, limit) = match mechanism {
        AgingMechanism::Nbti => {
            // NBTI: Vth shift ~ A * exp(-Ea/kT) * V^gamma * t^n
            let ea = 0.5; // activation energy in eV
            let gamma = 3.0; // voltage acceleration
            let n = 0.25; // time exponent (reaction-diffusion)
            let a = 1e-3;
            let deg =
                a * (-ea / (K_B * temp_k)).exp() * voltage.powf(gamma) * time_hours.powf(n) * 100.0;
            ("Vth_shift".to_string(), deg, 10.0)
        }
        AgingMechanism::Pbti => {
            let ea = 0.4;
            let gamma = 4.0;
            let n = 0.2;
            let a = 5e-4;
            let deg =
                a * (-ea / (K_B * temp_k)).exp() * voltage.powf(gamma) * time_hours.powf(n) * 100.0;
            ("Vth_shift".to_string(), deg, 10.0)
        }
        AgingMechanism::Hci => {
            // HCI: degradation ~ exp(-Ea/kT) * (Vdd/Vdd_nom)^m * t^0.5
            let ea = 0.3;
            let m = 5.0;
            let a = 2e-4;
            let deg = a
                * (-ea / (K_B * temp_k)).exp()
                * (voltage / 0.9).powf(m)
                * time_hours.sqrt()
                * 100.0;
            ("Idsat_degradation".to_string(), deg, 10.0)
        }
        AgingMechanism::Tddb => {
            // TDDB: failure rate ~ exp(-Ea/kT) * exp(gamma * Vox)
            let ea = 0.7;
            let gamma_v = 3.5;
            let a = 1e-6;
            let deg = a
                * (-ea / (K_B * temp_k)).exp()
                * (gamma_v * voltage).exp()
                * time_hours.powf(0.3)
                * 100.0;
            ("dielectric_integrity".to_string(), deg, 5.0)
        }
        AgingMechanism::Em => {
            // EM: Blacks equation — MTTF ~ A * J^(-n) * exp(Ea/kT)
            // Invert: degradation ~ J^n * exp(-Ea/kT) * t
            let ea = 0.7;
            let n = 2.0;
            let a = 1e-8;
            let current_density = voltage * 1e6; // simplified
            let deg =
                a * current_density.powf(n) * (-ea / (K_B * temp_k)).exp() * time_hours * 100.0;
            ("metal_void_growth".to_string(), deg, 20.0)
        }
    };

    AgingResult {
        mechanism,
        parameter_name: param_name,
        degradation_percent: degradation,
        time_years,
        pass: degradation <= limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nbti_increases_with_temperature() {
        let cool = estimate_aging(AgingMechanism::Nbti, 0.9, 25.0, 10.0);
        let hot = estimate_aging(AgingMechanism::Nbti, 0.9, 125.0, 10.0);
        assert!(
            hot.degradation_percent > cool.degradation_percent,
            "NBTI at 125C ({}) should exceed 25C ({})",
            hot.degradation_percent,
            cool.degradation_percent
        );
    }

    #[test]
    fn degradation_increases_with_time() {
        let short = estimate_aging(AgingMechanism::Hci, 0.9, 85.0, 1.0);
        let long = estimate_aging(AgingMechanism::Hci, 0.9, 85.0, 10.0);
        assert!(
            long.degradation_percent > short.degradation_percent,
            "10yr ({}) should degrade more than 1yr ({})",
            long.degradation_percent,
            short.degradation_percent
        );
    }
}
