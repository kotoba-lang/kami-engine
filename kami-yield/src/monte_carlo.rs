/// Monte Carlo simulation engine with configurable parameter distributions.
use serde::{Deserialize, Serialize};

/// Monte Carlo simulation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarloConfig {
    /// Number of simulation runs.
    pub num_runs: u32,
    /// PRNG seed for reproducibility.
    pub seed: u64,
    /// Parameters to vary.
    pub parameters: Vec<McParameter>,
}

/// A parameter to sweep in Monte Carlo simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McParameter {
    /// Parameter name.
    pub name: String,
    /// Nominal value.
    pub nominal: f64,
    /// Statistical distribution.
    pub distribution: Distribution,
}

/// Statistical distribution for parameter variation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Distribution {
    /// Gaussian (normal) distribution.
    Gaussian { sigma: f64 },
    /// Uniform distribution.
    Uniform { min: f64, max: f64 },
    /// Log-normal distribution.
    LogNormal { mu: f64, sigma: f64 },
}

/// Result of Monte Carlo simulation for one output metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarloResult {
    /// Parameter or metric name.
    pub parameter_name: String,
    /// All sampled values.
    pub values: Vec<f64>,
    /// Mean of values.
    pub mean: f64,
    /// Standard deviation of values.
    pub std_dev: f64,
    /// Minimum value.
    pub min: f64,
    /// Maximum value.
    pub max: f64,
    /// Fraction of runs passing specification [0.0, 1.0].
    pub yield_pass: f64,
}

/// Simple LCG pseudo-random number generator.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Return a uniform random f64 in [0, 1).
    fn next_f64(&mut self) -> f64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Generate a standard normal sample using Box-Muller transform.
    fn next_gaussian(&mut self) -> f64 {
        let u1 = self.next_f64().max(1e-15);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Sample a single value from a distribution.
fn sample(rng: &mut Lcg, nominal: f64, dist: &Distribution) -> f64 {
    match dist {
        Distribution::Gaussian { sigma } => nominal + sigma * rng.next_gaussian(),
        Distribution::Uniform { min, max } => min + (max - min) * rng.next_f64(),
        Distribution::LogNormal { mu, sigma } => (mu + sigma * rng.next_gaussian()).exp(),
    }
}

/// Run Monte Carlo simulation.
///
/// For each run, all parameters are sampled from their distributions and
/// passed to `eval_fn`. The output value is checked against `[spec_min, spec_max]`.
pub fn run_monte_carlo(
    config: &MonteCarloConfig,
    eval_fn: fn(&[f64]) -> f64,
    spec_min: f64,
    spec_max: f64,
) -> Vec<MonteCarloResult> {
    let mut rng = Lcg::new(config.seed);
    let n = config.num_runs as usize;
    let np = config.parameters.len();

    // Sample all parameter values.
    let mut param_samples: Vec<Vec<f64>> = vec![Vec::with_capacity(n); np];
    let mut output_values: Vec<f64> = Vec::with_capacity(n);

    for _ in 0..n {
        let inputs: Vec<f64> = config
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let v = sample(&mut rng, p.nominal, &p.distribution);
                param_samples[i].push(v);
                v
            })
            .collect();
        output_values.push(eval_fn(&inputs));
    }

    // Build results for each parameter and the output.
    let mut results = Vec::with_capacity(np + 1);

    for (i, p) in config.parameters.iter().enumerate() {
        results.push(compute_stats(
            &p.name,
            &param_samples[i],
            spec_min,
            spec_max,
        ));
    }

    results.push(compute_stats("output", &output_values, spec_min, spec_max));
    results
}

fn compute_stats(name: &str, values: &[f64], spec_min: f64, spec_max: f64) -> MonteCarloResult {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let std_dev = variance.sqrt();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let pass_count = values
        .iter()
        .filter(|&&v| v >= spec_min && v <= spec_max)
        .count();
    let yield_pass = pass_count as f64 / n;

    MonteCarloResult {
        parameter_name: name.to_string(),
        values: values.to_vec(),
        mean,
        std_dev,
        min,
        max,
        yield_pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_monte_carlo_mean_near_nominal() {
        let config = MonteCarloConfig {
            num_runs: 10000,
            seed: 42,
            parameters: vec![McParameter {
                name: "resistance".to_string(),
                nominal: 100.0,
                distribution: Distribution::Gaussian { sigma: 5.0 },
            }],
        };
        let results = run_monte_carlo(&config, |p| p[0], 80.0, 120.0);
        let output = results.last().unwrap();
        assert!(
            (output.mean - 100.0).abs() < 2.0,
            "Mean should be near 100, got {}",
            output.mean
        );
        assert!(
            output.std_dev > 3.0 && output.std_dev < 8.0,
            "Std dev should be near 5, got {}",
            output.std_dev
        );
    }

    #[test]
    fn yield_calculation() {
        let config = MonteCarloConfig {
            num_runs: 1000,
            seed: 123,
            parameters: vec![McParameter {
                name: "vth".to_string(),
                nominal: 0.4,
                distribution: Distribution::Gaussian { sigma: 0.02 },
            }],
        };
        let results = run_monte_carlo(&config, |p| p[0], 0.35, 0.45);
        let output = results.last().unwrap();
        assert!(
            output.yield_pass > 0.95,
            "Yield should be high for 2.5-sigma spec, got {}",
            output.yield_pass
        );
    }
}
