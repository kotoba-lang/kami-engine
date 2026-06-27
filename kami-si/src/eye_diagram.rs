/// Eye diagram generation and metrics extraction for serial link analysis.
use serde::{Deserialize, Serialize};

/// Eye diagram quality metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EyeMetrics {
    /// Vertical eye opening in mV.
    pub eye_height_mv: f64,
    /// Horizontal eye opening in ps.
    pub eye_width_ps: f64,
    /// RMS jitter in ps.
    pub jitter_rms_ps: f64,
    /// Peak-to-peak jitter in ps.
    pub jitter_pp_ps: f64,
    /// Estimated bit error rate.
    pub ber_estimate: f64,
}

/// Eye diagram data containing waveform samples and computed metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EyeDiagramData {
    /// Waveform sample points as (time_ps, voltage_mv).
    pub samples: Vec<(f64, f64)>,
    /// Computed eye metrics.
    pub metrics: EyeMetrics,
}

/// Generate eye diagram data for a serial link.
///
/// Produces overlaid bit-period waveform samples with noise and jitter,
/// then computes eye opening metrics.
pub fn generate_eye_data(
    bit_rate_gbps: f64,
    amplitude_mv: f64,
    rise_time_ps: f64,
    noise_rms_mv: f64,
    jitter_rms_ps: f64,
    num_bits: u32,
) -> EyeDiagramData {
    let bit_period_ps = 1000.0 / bit_rate_gbps;
    let samples_per_bit = 64_u32;
    let dt = bit_period_ps / samples_per_bit as f64;

    let mut samples = Vec::with_capacity((num_bits * samples_per_bit) as usize);
    let mut seed: u64 = 0xDEAD_BEEF_CAFE_1234;

    // Simple LCG for deterministic pseudo-random noise/jitter.
    let next_rand = |s: &mut u64| -> f64 {
        *s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // Map to [-1, 1].
        (*s as f64 / u64::MAX as f64) * 2.0 - 1.0
    };

    let half_amp = amplitude_mv / 2.0;
    // Rise/fall time constant for exponential edges.
    let tau = rise_time_ps / 2.2;

    let mut prev_level: f64 = 1.0;
    for bit_idx in 0..num_bits {
        // Pseudo-random bit pattern.
        let bit_val = if (bit_idx.wrapping_mul(7) ^ bit_idx.wrapping_mul(13)) % 3 == 0 {
            -1.0_f64
        } else {
            1.0_f64
        };

        let jitter_offset = next_rand(&mut seed) * jitter_rms_ps;

        for s in 0..samples_per_bit {
            let t = s as f64 * dt + jitter_offset;
            let t_wrapped = t.rem_euclid(bit_period_ps);

            // Transition model: exponential rise/fall.
            let transition = if (bit_val - prev_level).abs() > 0.5 {
                let alpha = 1.0 - (-t_wrapped / tau).exp();
                prev_level + (bit_val - prev_level) * alpha
            } else {
                bit_val
            };

            let noise = next_rand(&mut seed) * noise_rms_mv;
            let voltage = transition * half_amp + noise;

            samples.push((t_wrapped, voltage));
        }
        prev_level = bit_val;
    }

    // Compute metrics from the eye center region.
    let center_start = bit_period_ps * 0.35;
    let center_end = bit_period_ps * 0.65;

    let center_samples: Vec<f64> = samples
        .iter()
        .filter(|(t, _)| *t >= center_start && *t <= center_end)
        .map(|(_, v)| *v)
        .collect();

    let (high_samples, low_samples): (Vec<f64>, Vec<f64>) =
        center_samples.iter().partition(|&&v| v > 0.0);

    let high_mean = if high_samples.is_empty() {
        half_amp
    } else {
        high_samples.iter().sum::<f64>() / high_samples.len() as f64
    };
    let low_mean = if low_samples.is_empty() {
        -half_amp
    } else {
        low_samples.iter().sum::<f64>() / low_samples.len() as f64
    };

    let high_sigma = if high_samples.len() > 1 {
        (high_samples
            .iter()
            .map(|v| (v - high_mean).powi(2))
            .sum::<f64>()
            / (high_samples.len() - 1) as f64)
            .sqrt()
    } else {
        noise_rms_mv
    };

    let low_sigma = if low_samples.len() > 1 {
        (low_samples
            .iter()
            .map(|v| (v - low_mean).powi(2))
            .sum::<f64>()
            / (low_samples.len() - 1) as f64)
            .sqrt()
    } else {
        noise_rms_mv
    };

    let eye_height = (high_mean - 3.0 * high_sigma) - (low_mean + 3.0 * low_sigma);
    let eye_width = bit_period_ps - 6.0 * jitter_rms_ps;
    let jitter_pp = jitter_rms_ps * 6.0;

    // BER estimate from Q-factor.
    let q = if (high_sigma + low_sigma) > 0.0 {
        (high_mean - low_mean) / (high_sigma + low_sigma)
    } else {
        20.0
    };
    let ber = 0.5 * (-q * q / 2.0).exp();

    let metrics = EyeMetrics {
        eye_height_mv: eye_height.max(0.0),
        eye_width_ps: eye_width.max(0.0),
        jitter_rms_ps,
        jitter_pp_ps: jitter_pp,
        ber_estimate: ber,
    };

    EyeDiagramData { samples, metrics }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_height_positive() {
        let data = generate_eye_data(10.0, 800.0, 30.0, 10.0, 5.0, 100);
        assert!(
            data.metrics.eye_height_mv > 0.0,
            "Eye height should be positive, got {}",
            data.metrics.eye_height_mv
        );
        assert!(!data.samples.is_empty());
    }

    #[test]
    fn eye_width_bounded_by_bit_period() {
        let bit_rate = 10.0;
        let data = generate_eye_data(bit_rate, 800.0, 30.0, 10.0, 5.0, 50);
        let bit_period = 1000.0 / bit_rate;
        assert!(
            data.metrics.eye_width_ps <= bit_period,
            "Eye width {} should not exceed bit period {}",
            data.metrics.eye_width_ps,
            bit_period
        );
    }
}
