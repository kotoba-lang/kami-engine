//! Wind field: spatially + temporally varying wind vector.
//!
//! Uses FBM noise (2 octaves) to produce "ripple" patterns across the
//! ground plane. A tussock at position (x, z) at time `t` sees a wind
//! vector that is the base wind ± local perturbation — this gives the
//! characteristic *wave* motion of wind through grass/wheat fields.
//!
//! The same algorithm is implemented in WGSL (scene_vegetation.wgsl) so
//! CPU-side simulation (e.g. flag physics) matches GPU-side rendering.

use glam::Vec2;

/// Value noise (hash-based, cosine interpolation).
fn hash2d(x: i32, y: i32) -> f32 {
    let n = x.wrapping_mul(1619).wrapping_add(y.wrapping_mul(31337));
    let n = n.wrapping_mul(n).wrapping_mul(n);
    let n = (n >> 13) ^ n;
    let n = n
        .wrapping_mul(n.wrapping_mul(n.wrapping_mul(60493)).wrapping_add(19990303))
        .wrapping_add(1376312589);
    (n & 0x7fffffff) as f32 / 0x7fffffff as f32
}

fn smoothstep01(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: f32, y: f32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();
    let v00 = hash2d(xi, yi);
    let v10 = hash2d(xi + 1, yi);
    let v01 = hash2d(xi, yi + 1);
    let v11 = hash2d(xi + 1, yi + 1);
    let sx = smoothstep01(xf);
    let sy = smoothstep01(yf);
    let a = v00 * (1.0 - sx) + v10 * sx;
    let b = v01 * (1.0 - sx) + v11 * sx;
    a * (1.0 - sy) + b * sy
}

/// Configuration for the wind field.
#[derive(Debug, Clone, Copy)]
pub struct WindFieldConfig {
    /// Base wind direction (normalized XZ).
    pub base_dir: Vec2,
    /// Base wind speed (m/s).
    pub base_speed: f32,
    /// Global gust multiplier (1.0 = calm, 1.3 = gusty).
    pub gust_mul: f32,
    /// Spatial frequency of the ripple (cycles per world unit).
    /// 1/80 gives ~80m wavelength ripples.
    pub spatial_freq: f32,
    /// Temporal frequency (scrolling speed in "noise units" per second).
    pub temporal_freq: f32,
    /// Amplitude of local perturbation [0, 1]. 0 = uniform wind, 0.5 = ±50% variation.
    pub local_variation: f32,
}

impl Default for WindFieldConfig {
    fn default() -> Self {
        Self {
            base_dir: Vec2::new(1.0, 0.3),
            base_speed: 5.0,
            gust_mul: 1.0,
            spatial_freq: 0.012, // ~83m main ripple wavelength
            temporal_freq: 0.25, // slow temporal drift
            local_variation: 0.5,
        }
    }
}

/// Sample the wind field at world position (x, z) at time `t` (seconds).
///
/// Returns a 2D wind vector (XZ plane) whose magnitude and direction vary
/// with position + time. Base wind drifts over longer scales; local ripples
/// add 2-octave FBM perturbation.
pub fn sample_wind(x: f32, z: f32, t: f32, cfg: &WindFieldConfig) -> Vec2 {
    let dir = cfg.base_dir.normalize_or_zero();
    let perp = Vec2::new(-dir.y, dir.x);

    // Main ripple (large scale)
    let nx1 = x * cfg.spatial_freq + t * cfg.temporal_freq;
    let nz1 = z * cfg.spatial_freq + t * cfg.temporal_freq * 0.7;
    let n1 = value_noise(nx1, nz1) * 2.0 - 1.0; // -1..1

    // Finer ripple (half wavelength)
    let nx2 = x * cfg.spatial_freq * 2.0 + t * cfg.temporal_freq * 1.5 + 13.0;
    let nz2 = z * cfg.spatial_freq * 2.0 + t * cfg.temporal_freq * 1.1 + 7.0;
    let n2 = value_noise(nx2, nz2) * 2.0 - 1.0;

    let magnitude_mod = 1.0 + (n1 * 0.7 + n2 * 0.3) * cfg.local_variation;
    let direction_shift = (n1 * 0.3 + n2 * 0.2) * cfg.local_variation;

    let local_dir = dir + perp * direction_shift;
    let speed = cfg.base_speed * cfg.gust_mul * magnitude_mod.max(0.0);
    local_dir.normalize_or_zero() * speed
}

/// Sample only the signed gust scalar at (x, z, t) — useful for per-instance bending.
/// Returns [-1, +1] representing "less than base" vs "more than base" gust.
pub fn sample_gust_scalar(x: f32, z: f32, t: f32, cfg: &WindFieldConfig) -> f32 {
    let nx = x * cfg.spatial_freq + t * cfg.temporal_freq;
    let nz = z * cfg.spatial_freq + t * cfg.temporal_freq * 0.7;
    value_noise(nx, nz) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_is_deterministic() {
        let cfg = WindFieldConfig::default();
        let a = sample_wind(10.0, 20.0, 3.5, &cfg);
        let b = sample_wind(10.0, 20.0, 3.5, &cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn wind_varies_with_position() {
        let cfg = WindFieldConfig::default();
        let a = sample_wind(0.0, 0.0, 0.0, &cfg);
        let b = sample_wind(50.0, 50.0, 0.0, &cfg);
        assert!((a - b).length() > 0.05, "wind should vary across space");
    }

    #[test]
    fn wind_varies_with_time() {
        let cfg = WindFieldConfig::default();
        let a = sample_wind(10.0, 10.0, 0.0, &cfg);
        let b = sample_wind(10.0, 10.0, 30.0, &cfg);
        assert!((a - b).length() > 0.05, "wind should vary over time");
    }

    #[test]
    fn zero_variation_gives_base() {
        let mut cfg = WindFieldConfig::default();
        cfg.local_variation = 0.0;
        let w = sample_wind(123.0, 456.0, 78.0, &cfg);
        let expected = cfg.base_dir.normalize() * cfg.base_speed;
        assert!(
            (w - expected).length() < 0.01,
            "with 0 variation, should return base wind"
        );
    }
}
