//! kami-atmosphere: procedural sky + atmospheric scattering.
//!
//! Sky rendering with Rayleigh/Mie scattering approximation, sun position
//! from time-of-day, fog, wind system, cloud scroll, and wind-field sampling
//! for per-position ripple dynamics.

pub mod wind_field;
pub use wind_field::{WindFieldConfig, sample_gust_scalar, sample_wind};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// Sky parameters for the atmosphere shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SkyUniform {
    /// Sun direction (normalized, world space).
    pub sun_dir: [f32; 3],
    /// Time of day [0, 1] where 0.5 = noon.
    pub time_of_day: f32,
    /// Sun color (HDR, pre-multiplied by intensity).
    pub sun_color: [f32; 3],
    /// Fog density (exponential).
    pub fog_density: f32,
    /// Fog color (matches horizon at current time).
    pub fog_color: [f32; 3],
    /// Sun disk radius (radians, ~0.01).
    pub sun_radius: f32,
}

/// Day/night cycle controller.
pub struct DayNightCycle {
    /// Current time [0, 1) where 0.0 = midnight, 0.5 = noon.
    pub time: f32,
    /// Cycle period in seconds (default: 600 = 10 min).
    pub period: f32,
}

impl Default for DayNightCycle {
    fn default() -> Self {
        Self {
            time: 0.35, // start at morning
            period: 600.0,
        }
    }
}

impl DayNightCycle {
    /// Advance time by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.time = (self.time + dt / self.period) % 1.0;
    }

    /// Compute sun direction from time of day.
    /// Sun rises at time=0.25, peaks at time=0.5, sets at time=0.75.
    pub fn sun_direction(&self) -> Vec3 {
        let angle = (self.time - 0.25) * std::f32::consts::TAU;
        let y = angle.sin();
        let xz = angle.cos();
        Vec3::new(xz * 0.3, y, xz * 0.95).normalize()
    }

    /// Compute sun color based on time (golden hour tinting).
    pub fn sun_color(&self) -> Vec3 {
        let elevation = self.sun_direction().y;
        if elevation < -0.1 {
            // Night: moonlight
            return Vec3::new(0.05, 0.05, 0.15);
        }
        if elevation < 0.1 {
            // Golden hour: warm orange
            let t = ((elevation + 0.1) / 0.2).clamp(0.0, 1.0);
            let sunset = Vec3::new(1.5, 0.5, 0.1);
            let day = Vec3::new(1.0, 0.98, 0.9);
            return sunset.lerp(day, t);
        }
        // Day: warm white
        Vec3::new(1.0, 0.98, 0.9)
    }

    /// Compute fog color (blends sky horizon color).
    pub fn fog_color(&self) -> Vec3 {
        let elevation = self.sun_direction().y;
        if elevation < 0.0 {
            Vec3::new(0.02, 0.02, 0.05)
        } else {
            let t = elevation.clamp(0.0, 1.0);
            let horizon = Vec3::new(0.7, 0.8, 0.95);
            let zenith = Vec3::new(0.4, 0.6, 0.9);
            horizon.lerp(zenith, t * 0.3)
        }
    }

    /// Build SkyUniform for shader upload.
    pub fn to_uniform(&self) -> SkyUniform {
        let dir = self.sun_direction();
        let col = self.sun_color();
        let fog = self.fog_color();
        SkyUniform {
            sun_dir: dir.to_array(),
            time_of_day: self.time,
            sun_color: col.to_array(),
            fog_density: 0.001,
            fog_color: fog.to_array(),
            sun_radius: 0.01,
        }
    }
}

/// Rayleigh scattering approximation for sky color at a view direction.
pub fn rayleigh_sky_color(view_dir: Vec3, sun_dir: Vec3, sun_color: Vec3) -> Vec3 {
    let cos_theta = view_dir.dot(sun_dir).max(0.0);
    let rayleigh = Vec3::new(0.3, 0.5, 1.0);
    let phase = 0.75 * (1.0 + cos_theta * cos_theta);
    let y = view_dir.y.max(0.0);
    let gradient = (-y * 3.0).exp();
    let sky = rayleigh * phase * 0.3 + sun_color * gradient * 0.1;
    let sun_dot = cos_theta.powf(256.0);
    sky + sun_color * sun_dot * 2.0
}

// ════════════════════════════════════════════════════════════════════════
// Wind system — global wind direction + gust + Beaufort scale
// ════════════════════════════════════════════════════════════════════════

/// Wind uniform for shaders (affects water waves, grass, clouds, particles).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct WindUniform {
    /// Wind direction (normalized XZ plane).
    pub direction: [f32; 2],
    /// Base wind speed (m/s).
    pub speed: f32,
    /// Gust intensity [0, 1] — modulates speed randomly.
    pub gust: f32,
    /// Turbulence frequency (affects grass/particle noise).
    pub turbulence: f32,
    /// Current gust multiplier (time-varying, computed by tick).
    pub gust_multiplier: f32,
    pub _pad: [f32; 2],
}

/// Wind controller with gusting.
pub struct WindSystem {
    /// Base wind direction angle (radians from +X toward +Z).
    pub angle: f32,
    /// Base speed (m/s). Beaufort 3-4 = gentle-moderate breeze.
    pub speed: f32,
    /// Gust intensity [0, 1].
    pub gust_intensity: f32,
    /// Internal phase for gust oscillation.
    phase: f32,
}

impl Default for WindSystem {
    fn default() -> Self {
        Self {
            angle: 0.8, // ~46° from east
            speed: 5.0, // gentle breeze (Beaufort 3)
            gust_intensity: 0.3,
            phase: 0.0,
        }
    }
}

impl WindSystem {
    /// Advance wind state by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.phase += dt * 0.7;
        // Slowly vary wind direction
        self.angle += dt * 0.01 * (self.phase * 0.3).sin();
    }

    /// Current gust multiplier (1.0 = calm, up to 1.0 + gust_intensity).
    pub fn gust_multiplier(&self) -> f32 {
        let g1 = (self.phase * 1.7).sin() * 0.5 + 0.5;
        let g2 = (self.phase * 3.1 + 1.3).sin() * 0.3 + 0.5;
        1.0 + self.gust_intensity * g1 * g2
    }

    pub fn direction(&self) -> [f32; 2] {
        [self.angle.cos(), self.angle.sin()]
    }

    pub fn to_uniform(&self) -> WindUniform {
        WindUniform {
            direction: self.direction(),
            speed: self.speed,
            gust: self.gust_intensity,
            turbulence: 1.5,
            gust_multiplier: self.gust_multiplier(),
            _pad: [0.0; 2],
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// Cloud layer — scrolling noise-based cloud coverage
// ════════════════════════════════════════════════════════════════════════

/// Cloud uniform for the sky shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CloudUniform {
    /// Cloud coverage [0, 1] (0 = clear, 1 = overcast).
    pub coverage: f32,
    /// Cloud altitude (world units above sea level).
    pub altitude: f32,
    /// Cloud scroll offset (driven by wind).
    pub scroll_x: f32,
    pub scroll_z: f32,
    /// Cloud density / opacity.
    pub density: f32,
    /// Cloud edge sharpness.
    pub sharpness: f32,
    pub _pad: [f32; 2],
}

/// Cloud layer controller.
pub struct CloudSystem {
    pub coverage: f32,
    pub altitude: f32,
    pub density: f32,
    pub sharpness: f32,
    scroll_x: f32,
    scroll_z: f32,
}

impl Default for CloudSystem {
    fn default() -> Self {
        Self {
            coverage: 0.45,
            altitude: 300.0,
            density: 0.7,
            sharpness: 2.5,
            scroll_x: 0.0,
            scroll_z: 0.0,
        }
    }
}

impl CloudSystem {
    /// Advance clouds by wind.
    pub fn tick(&mut self, wind: &WindSystem, dt: f32) {
        let dir = wind.direction();
        let s = wind.speed * wind.gust_multiplier() * dt * 0.02;
        self.scroll_x += dir[0] * s;
        self.scroll_z += dir[1] * s;
    }

    pub fn to_uniform(&self) -> CloudUniform {
        CloudUniform {
            coverage: self.coverage,
            altitude: self.altitude,
            scroll_x: self.scroll_x,
            scroll_z: self.scroll_z,
            density: self.density,
            sharpness: self.sharpness,
            _pad: [0.0; 2],
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// Weather state — combined atmosphere snapshot
// ════════════════════════════════════════════════════════════════════════

/// Full weather state for one frame.
pub struct Weather {
    pub day_night: DayNightCycle,
    pub wind: WindSystem,
    pub clouds: CloudSystem,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            day_night: DayNightCycle::default(),
            wind: WindSystem::default(),
            clouds: CloudSystem::default(),
        }
    }
}

impl Weather {
    pub fn tick(&mut self, dt: f32) {
        self.day_night.tick(dt);
        self.wind.tick(dt);
        self.clouds.tick(&self.wind, dt);
    }

    /// Overcast preset: near-total cloud coverage, diffuse grey lighting,
    /// moderate wind. Matches volcanic / quarry cinematic atmosphere.
    pub fn overcast() -> Self {
        let mut w = Weather::default();
        w.clouds.coverage = 0.95;
        w.clouds.density = 1.0;
        w.clouds.altitude = 250.0;
        w.clouds.sharpness = 1.2;
        w.wind.speed = 8.0;
        w.wind.gust_intensity = 0.45;
        w.day_night.time = 0.42; // mid-morning, sun behind clouds
        w
    }

    /// Clear sunny preset.
    pub fn clear() -> Self {
        let mut w = Weather::default();
        w.clouds.coverage = 0.25;
        w.clouds.density = 0.6;
        w.wind.speed = 4.0;
        w.day_night.time = 0.4;
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_night_cycle() {
        let mut cycle = DayNightCycle::default();
        cycle.time = 0.5;
        let dir = cycle.sun_direction();
        assert!(dir.y > 0.9, "sun should be near zenith at noon: {dir:?}");
        cycle.time = 0.0;
        let dir = cycle.sun_direction();
        assert!(dir.y < -0.9, "sun below horizon at midnight: {dir:?}");
    }

    #[test]
    fn uniform_valid() {
        let cycle = DayNightCycle::default();
        let u = cycle.to_uniform();
        assert!(u.fog_density > 0.0);
        assert!(u.sun_radius > 0.0);
    }

    #[test]
    fn wind_gust_range() {
        let mut wind = WindSystem::default();
        for _ in 0..100 {
            wind.tick(0.1);
            let g = wind.gust_multiplier();
            assert!(g >= 0.8 && g <= 2.0, "gust out of range: {g}");
        }
    }

    #[test]
    fn cloud_scroll() {
        let mut clouds = CloudSystem::default();
        let wind = WindSystem::default();
        let x0 = clouds.scroll_x;
        clouds.tick(&wind, 10.0);
        assert!(clouds.scroll_x != x0, "clouds should scroll with wind");
    }

    #[test]
    fn weather_tick() {
        let mut w = Weather::default();
        w.tick(1.0);
        assert!(w.wind.phase > 0.0);
        assert!(w.clouds.scroll_x != 0.0 || w.clouds.scroll_z != 0.0);
    }

    #[test]
    fn overcast_preset_is_cloudy() {
        let w = Weather::overcast();
        assert!(w.clouds.coverage > 0.9);
        assert!(w.clouds.density >= 1.0);
    }

    #[test]
    fn clear_preset_is_sunny() {
        let w = Weather::clear();
        assert!(w.clouds.coverage < 0.5);
    }
}
