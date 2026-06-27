//! Ground contact resolution.
//!
//! BeamNG resolves ground contact per-node (every body / tire node tests the
//! terrain heightfield). We follow the same approach because:
//!   * it scales linearly with the soft body without needing a convex hull,
//!   * it makes deformation feel right — a crumpled fender drags on the road
//!     because its nodes get stuck below ground.
//!
//! `Ground` is a trait so the host engine can plug a heightmap, an SDF, or
//! a flat plane. The default implementation is a flat plane at `y = 0`.

use glam::Vec3;

/// Pre-tuned road / surface presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// Dry asphalt — baseline. mu = 1.0, grip = 1.0.
    AsphaltDry,
    /// Wet asphalt — mu 0.7, grip 0.7.
    AsphaltWet,
    /// Loose gravel — mu 0.55, grip 0.55.
    Gravel,
    /// Sand — mu 0.40, grip 0.45 (lots of slip, low rolling speed).
    Sand,
    /// Snow — mu 0.30, grip 0.35.
    Snow,
    /// Ice — mu 0.10, grip 0.10. Skating-rink physics.
    Ice,
    /// Mud — mu 0.35, grip 0.40.
    Mud,
    /// Grass — mu 0.55, grip 0.60.
    Grass,
}

impl SurfaceKind {
    pub fn id(self) -> &'static str {
        match self {
            SurfaceKind::AsphaltDry => "asphalt_dry",
            SurfaceKind::AsphaltWet => "asphalt_wet",
            SurfaceKind::Gravel => "gravel",
            SurfaceKind::Sand => "sand",
            SurfaceKind::Snow => "snow",
            SurfaceKind::Ice => "ice",
            SurfaceKind::Mud => "mud",
            SurfaceKind::Grass => "grass",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "asphalt_wet" => SurfaceKind::AsphaltWet,
            "gravel" => SurfaceKind::Gravel,
            "sand" => SurfaceKind::Sand,
            "snow" => SurfaceKind::Snow,
            "ice" => SurfaceKind::Ice,
            "mud" => SurfaceKind::Mud,
            "grass" => SurfaceKind::Grass,
            _ => SurfaceKind::AsphaltDry,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            SurfaceKind::AsphaltDry => "Dry Asphalt",
            SurfaceKind::AsphaltWet => "Wet Asphalt",
            SurfaceKind::Gravel => "Gravel",
            SurfaceKind::Sand => "Sand",
            SurfaceKind::Snow => "Snow",
            SurfaceKind::Ice => "Ice",
            SurfaceKind::Mud => "Mud",
            SurfaceKind::Grass => "Grass",
        }
    }

    /// (friction_mu, grip_modifier) — the two parameters consumed by
    /// `vehicle.rs` when evaluating Pacejka tire forces.
    pub fn coefficients(self) -> (f32, f32) {
        match self {
            SurfaceKind::AsphaltDry => (1.00, 1.00),
            SurfaceKind::AsphaltWet => (0.70, 0.70),
            SurfaceKind::Gravel => (0.55, 0.55),
            SurfaceKind::Sand => (0.40, 0.45),
            SurfaceKind::Snow => (0.30, 0.35),
            SurfaceKind::Ice => (0.10, 0.10),
            SurfaceKind::Mud => (0.35, 0.40),
            SurfaceKind::Grass => (0.55, 0.60),
        }
    }

    /// Visual tint for renderer overlays (RGB 0-1).
    pub fn tint(self) -> [f32; 3] {
        match self {
            SurfaceKind::AsphaltDry => [0.20, 0.20, 0.22],
            SurfaceKind::AsphaltWet => [0.16, 0.18, 0.24],
            SurfaceKind::Gravel => [0.45, 0.42, 0.38],
            SurfaceKind::Sand => [0.85, 0.75, 0.55],
            SurfaceKind::Snow => [0.95, 0.95, 0.98],
            SurfaceKind::Ice => [0.75, 0.85, 0.95],
            SurfaceKind::Mud => [0.30, 0.22, 0.15],
            SurfaceKind::Grass => [0.30, 0.55, 0.25],
        }
    }
}

/// Sampled ground contact at a query point.
#[derive(Debug, Clone, Copy)]
pub struct GroundSample {
    /// Outward surface normal at the contact point.
    pub normal: Vec3,
    /// Height of the surface at the query (x, z) — `y` of the contact point.
    pub height: f32,
    /// Coulomb friction multiplier of the surface (1.0 = asphalt baseline).
    pub friction_mu: f32,
    /// 1.0 = dry, 0.5 = damp, 0.0 = flooded — modulates Pacejka peak grip.
    pub grip_modifier: f32,
}

pub trait Ground: Send + Sync {
    fn sample(&self, x: f32, z: f32) -> GroundSample;
}

/// Flat-plane ground at a given altitude with a chosen surface kind.
pub struct FlatGround {
    pub height: f32,
    pub friction_mu: f32,
    pub grip_modifier: f32,
    pub surface: SurfaceKind,
}

impl FlatGround {
    pub fn new(height: f32) -> Self {
        Self::with_surface(height, SurfaceKind::AsphaltDry)
    }

    pub fn with_surface(height: f32, surface: SurfaceKind) -> Self {
        let (mu, grip) = surface.coefficients();
        Self {
            height,
            friction_mu: mu,
            grip_modifier: grip,
            surface,
        }
    }
}

impl Ground for FlatGround {
    fn sample(&self, _x: f32, _z: f32) -> GroundSample {
        GroundSample {
            normal: Vec3::Y,
            height: self.height,
            friction_mu: self.friction_mu,
            grip_modifier: self.grip_modifier,
        }
    }
}

/// Heightmap-backed ground. Hosts (e.g. `kami-terrain`) provide the closure.
pub struct ClosureGround<F>
where
    F: Fn(f32, f32) -> GroundSample + Send + Sync,
{
    pub f: F,
}

impl<F> Ground for ClosureGround<F>
where
    F: Fn(f32, f32) -> GroundSample + Send + Sync,
{
    fn sample(&self, x: f32, z: f32) -> GroundSample {
        (self.f)(x, z)
    }
}

/// Multi-zone flat ground — different rectangular regions of the (x, z)
/// plane have different surface kinds. Default surface fills the gaps.
#[derive(Debug, Clone)]
pub struct SurfaceZone {
    pub x_min: f32,
    pub x_max: f32,
    pub z_min: f32,
    pub z_max: f32,
    pub surface: SurfaceKind,
}

#[derive(Debug, Clone)]
pub struct MapGround {
    pub default: SurfaceKind,
    pub zones: Vec<SurfaceZone>,
}

impl MapGround {
    /// Reference test track: a long asphalt road through the centre with
    /// off-road patches of sand / snow / ice / mud / gravel on either
    /// side. Drive forward to traverse them.
    pub fn demo_circuit() -> Self {
        Self {
            default: SurfaceKind::Grass,
            zones: vec![
                // Main asphalt road — long strip down the middle.
                SurfaceZone {
                    x_min: -4.0,
                    x_max: 4.0,
                    z_min: -100.0,
                    z_max: 100.0,
                    surface: SurfaceKind::AsphaltDry,
                },
                // Wet patch on the road (slippery start).
                SurfaceZone {
                    x_min: -4.0,
                    x_max: 4.0,
                    z_min: 8.0,
                    z_max: 20.0,
                    surface: SurfaceKind::AsphaltWet,
                },
                // Ice patch further down the road.
                SurfaceZone {
                    x_min: -4.0,
                    x_max: 4.0,
                    z_min: 30.0,
                    z_max: 42.0,
                    surface: SurfaceKind::Ice,
                },
                // Snow patch at the far end.
                SurfaceZone {
                    x_min: -4.0,
                    x_max: 4.0,
                    z_min: 55.0,
                    z_max: 75.0,
                    surface: SurfaceKind::Snow,
                },
                // Sand area to the right.
                SurfaceZone {
                    x_min: 8.0,
                    x_max: 30.0,
                    z_min: -20.0,
                    z_max: 20.0,
                    surface: SurfaceKind::Sand,
                },
                // Gravel area to the right, further forward.
                SurfaceZone {
                    x_min: 8.0,
                    x_max: 30.0,
                    z_min: 25.0,
                    z_max: 60.0,
                    surface: SurfaceKind::Gravel,
                },
                // Mud area to the left.
                SurfaceZone {
                    x_min: -30.0,
                    x_max: -8.0,
                    z_min: -20.0,
                    z_max: 20.0,
                    surface: SurfaceKind::Mud,
                },
                // Snow area to the left, further forward.
                SurfaceZone {
                    x_min: -30.0,
                    x_max: -8.0,
                    z_min: 25.0,
                    z_max: 60.0,
                    surface: SurfaceKind::Snow,
                },
                // Reverse bay: ice patch behind.
                SurfaceZone {
                    x_min: -4.0,
                    x_max: 4.0,
                    z_min: -45.0,
                    z_max: -30.0,
                    surface: SurfaceKind::Mud,
                },
            ],
        }
    }

    /// Surface at a given position (used by the renderer too).
    pub fn surface_at(&self, x: f32, z: f32) -> SurfaceKind {
        for zone in &self.zones {
            if x >= zone.x_min && x <= zone.x_max && z >= zone.z_min && z <= zone.z_max {
                return zone.surface;
            }
        }
        self.default
    }
}

impl Ground for MapGround {
    fn sample(&self, x: f32, z: f32) -> GroundSample {
        let surface = self.surface_at(x, z);
        let (mu, grip) = surface.coefficients();
        GroundSample {
            normal: Vec3::Y,
            height: 0.0,
            friction_mu: mu,
            grip_modifier: grip,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_ground_returns_constant_height() {
        let g = FlatGround::new(1.5);
        let s = g.sample(0.0, 0.0);
        assert!((s.height - 1.5).abs() < 1e-6);
        assert!((s.normal - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn closure_ground_can_emulate_slope() {
        let g = ClosureGround {
            f: |x, _| GroundSample {
                normal: Vec3::Y,
                height: x * 0.1,
                friction_mu: 1.0,
                grip_modifier: 1.0,
            },
        };
        let s = g.sample(10.0, 0.0);
        assert!((s.height - 1.0).abs() < 1e-3);
    }
}
