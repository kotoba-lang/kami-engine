//! Water plane: flat mesh at sea level with wave animation parameters.
//!
//! Decima-style water: Gerstner wave sum on GPU, Fresnel reflection/refraction,
//! specular highlight from sun. CPU generates the base grid; GPU does the animation.

use bytemuck::{Pod, Zeroable};

/// Water vertex: position (3) + uv (2) = 5 floats (20 bytes).
/// Wave displacement is computed in the vertex shader via Gerstner waves.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct WaterVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// Gerstner wave parameters (uploaded as uniform array).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GerstnerWave {
    /// Wave direction (normalized XZ).
    pub direction: [f32; 2],
    /// Amplitude (world units).
    pub amplitude: f32,
    /// Wavelength (world units).
    pub wavelength: f32,
    /// Speed (world units / second).
    pub speed: f32,
    /// Steepness Q [0, 1] — 0 = sine, 1 = sharp crest.
    pub steepness: f32,
    pub _pad: [f32; 2],
}

/// Water configuration.
pub struct WaterConfig {
    /// Sea level Y coordinate.
    pub sea_level: f32,
    /// Grid extent in world units (centered at origin).
    pub extent: f32,
    /// Grid resolution (vertices per axis).
    pub resolution: u32,
    /// Gerstner wave set.
    pub waves: Vec<GerstnerWave>,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            sea_level: 18.0,
            extent: 512.0,
            resolution: 128,
            waves: default_waves(),
        }
    }
}

/// Generate 4 Gerstner waves from wind direction + speed.
///
/// Decima-style: primary wave aligned with wind, 3 subsidiary waves at a cone
/// spread (±15°, ±30°, ±60°). Amplitude and wavelength scale with wind speed
/// following simplified Beaufort sea-state mapping.
///
/// `wind_dir`: normalized wind direction (XZ plane).
/// `wind_speed`: wind speed in m/s (Beaufort ~3 = 5 m/s, ~5 = 10 m/s).
/// `gust`: gust multiplier [1.0, 2.0] — amplifies amplitude only.
pub fn waves_from_wind(wind_dir: [f32; 2], wind_speed: f32, gust: f32) -> Vec<GerstnerWave> {
    // Normalize wind direction
    let len = (wind_dir[0] * wind_dir[0] + wind_dir[1] * wind_dir[1])
        .sqrt()
        .max(1e-6);
    let wd = [wind_dir[0] / len, wind_dir[1] / len];

    // Wind perpendicular (for rotating cone)
    let perp = [-wd[1], wd[0]];

    // Beaufort mapping (simplified):
    // - Base amplitude ∝ wind_speed^2 / gravity (fully developed sea)
    // - Base wavelength ∝ wind_speed^2 / gravity
    // - Phase speed = sqrt(g * wavelength / (2π)) (deep-water dispersion)
    let g = 9.81f32;
    let u_sq = wind_speed.max(0.5).powi(2);
    let base_amp = (u_sq / g * 0.08).clamp(0.1, 4.0);
    let base_wavelength = (u_sq / g * 4.0).clamp(8.0, 200.0);

    // Helper: rotate `d` by angle (radians)
    let rot = |d: [f32; 2], theta: f32| -> [f32; 2] {
        let c = theta.cos();
        let s = theta.sin();
        [d[0] * c - d[1] * s, d[0] * s + d[1] * c]
    };

    // Wave speed from deep-water dispersion: c = sqrt(g * λ / 2π)
    let speed_for = |wavelength: f32| (g * wavelength / std::f32::consts::TAU).sqrt();

    // Wave 1: primary, along wind, largest
    let w1 = GerstnerWave {
        direction: wd,
        amplitude: base_amp * gust,
        wavelength: base_wavelength,
        speed: speed_for(base_wavelength),
        steepness: 0.35,
        _pad: [0.0; 2],
    };

    // Wave 2: +30° spread, 0.5x wavelength
    let wl2 = base_wavelength * 0.5;
    let w2 = GerstnerWave {
        direction: rot(wd, 0.52), // ~30°
        amplitude: base_amp * 0.55 * gust,
        wavelength: wl2,
        speed: speed_for(wl2),
        steepness: 0.3,
        _pad: [0.0; 2],
    };

    // Wave 3: -15° spread, 0.25x wavelength (ripple)
    let wl3 = base_wavelength * 0.25;
    let w3 = GerstnerWave {
        direction: rot(wd, -0.26), // ~-15°
        amplitude: base_amp * 0.3 * gust,
        wavelength: wl3,
        speed: speed_for(wl3),
        steepness: 0.4,
        _pad: [0.0; 2],
    };

    // Wave 4: nearly perpendicular to wind, 0.12x wavelength (fine chop)
    let wl4 = base_wavelength * 0.12;
    let _ = perp; // kept for reference
    let w4 = GerstnerWave {
        direction: rot(wd, 1.05), // ~60°
        amplitude: base_amp * 0.15 * gust,
        wavelength: wl4,
        speed: speed_for(wl4),
        steepness: 0.2,
        _pad: [0.0; 2],
    };

    vec![w1, w2, w3, w4]
}

/// Default ocean wave set (calm seas, Beaufort 3 breeze from the east).
pub fn default_waves() -> Vec<GerstnerWave> {
    vec![
        GerstnerWave {
            direction: [0.8, 0.6],
            amplitude: 0.8,
            wavelength: 60.0,
            speed: 12.0,
            steepness: 0.4,
            _pad: [0.0; 2],
        },
        GerstnerWave {
            direction: [-0.3, 0.95],
            amplitude: 0.4,
            wavelength: 30.0,
            speed: 8.0,
            steepness: 0.3,
            _pad: [0.0; 2],
        },
        GerstnerWave {
            direction: [0.5, -0.87],
            amplitude: 0.2,
            wavelength: 15.0,
            speed: 5.0,
            steepness: 0.5,
            _pad: [0.0; 2],
        },
        GerstnerWave {
            direction: [-0.7, -0.7],
            amplitude: 0.1,
            wavelength: 8.0,
            speed: 3.0,
            steepness: 0.2,
            _pad: [0.0; 2],
        },
    ]
}

/// Generate a flat water grid mesh.
///
/// Returns (vertices, indices). Wave animation is done in the vertex shader.
pub fn generate_water_mesh(config: &WaterConfig) -> (Vec<WaterVertex>, Vec<u32>) {
    let res = config.resolution;
    let half = config.extent * 0.5;
    let step = config.extent / res as f32;

    let mut vertices = Vec::with_capacity(((res + 1) * (res + 1)) as usize);
    let mut indices = Vec::with_capacity((res * res * 6) as usize);

    for row in 0..=res {
        for col in 0..=res {
            let x = -half + col as f32 * step;
            let z = -half + row as f32 * step;
            vertices.push(WaterVertex {
                position: [x, config.sea_level, z],
                uv: [col as f32 / res as f32, row as f32 / res as f32],
            });
        }
    }

    let row_verts = res + 1;
    for row in 0..res {
        for col in 0..res {
            let tl = row * row_verts + col;
            let tr = tl + 1;
            let bl = (row + 1) * row_verts + col;
            let br = bl + 1;
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_mesh_size() {
        let cfg = WaterConfig {
            resolution: 64,
            ..Default::default()
        };
        let (verts, idxs) = generate_water_mesh(&cfg);
        assert_eq!(verts.len(), 65 * 65);
        assert_eq!(idxs.len(), 64 * 64 * 6);
    }

    #[test]
    fn default_waves_valid() {
        let waves = default_waves();
        assert_eq!(waves.len(), 4);
        for w in &waves {
            assert!(w.amplitude > 0.0);
            assert!(w.wavelength > 0.0);
        }
    }

    #[test]
    fn wind_waves_scale_with_speed() {
        // Calm (1 m/s) should produce tiny amplitude
        let calm = waves_from_wind([1.0, 0.0], 1.0, 1.0);
        // Stormy (20 m/s) should produce large amplitude
        let storm = waves_from_wind([1.0, 0.0], 20.0, 1.0);
        assert!(
            storm[0].amplitude > calm[0].amplitude * 5.0,
            "storm amp {} should be much larger than calm {}",
            storm[0].amplitude,
            calm[0].amplitude
        );
        assert!(
            storm[0].wavelength > calm[0].wavelength * 3.0,
            "storm wavelength {} should be longer than calm {}",
            storm[0].wavelength,
            calm[0].wavelength
        );
    }

    #[test]
    fn wind_waves_align_with_direction() {
        let waves = waves_from_wind([0.6, 0.8], 10.0, 1.0);
        // Primary wave direction should match wind direction
        let dot = waves[0].direction[0] * 0.6 + waves[0].direction[1] * 0.8;
        assert!(dot > 0.99, "primary wave should align with wind: dot={dot}");
    }

    #[test]
    fn wind_waves_gust_amplifies() {
        let calm = waves_from_wind([1.0, 0.0], 5.0, 1.0);
        let gusty = waves_from_wind([1.0, 0.0], 5.0, 1.8);
        assert!(
            gusty[0].amplitude > calm[0].amplitude * 1.7,
            "gust should amplify: calm={} gusty={}",
            calm[0].amplitude,
            gusty[0].amplitude
        );
    }
}
