//! BiomePreset: bundled terrain + splatmap + material color configurations.
//!
//! Each preset defines a cohesive environmental look:
//! - Heightmap FBM params (frequency, octaves, max_height)
//! - Splatmap thresholds (sand/snow lines + rock slope)
//! - Material base + tip colors (for fragment shader)

use crate::heightmap::HeightmapConfig;
use serde::{Deserialize, Serialize};

/// Predefined biome presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiomePreset {
    /// Lush green plains with rolling hills (default).
    Plains,
    /// Rocky quarry / desert mountain: sharp ridges, warm ochre rock,
    /// sparse dry grass. Overcast-friendly palette.
    Quarry,
    /// Arid desert dunes: smooth undulations, sand-dominant.
    Desert,
    /// Snowy tundra: flat with sharp peaks, high snow line.
    Tundra,
}

/// Splatmap generation thresholds for a biome.
#[derive(Debug, Clone, Copy)]
pub struct SplatThresholds {
    pub sand_line: f32,
    pub snow_line: f32,
    pub rock_slope: f32,
}

/// Material palette for fragment shader (4 layers × base + tip).
#[derive(Debug, Clone, Copy)]
pub struct MaterialPalette {
    /// (grass, rock, sand, snow) base colors, RGB [0,1].
    pub base: [[f32; 3]; 4],
    /// (grass, rock, sand, snow) tip/accent colors (fragment can mix with base).
    pub tip: [[f32; 3]; 4],
}

impl BiomePreset {
    pub fn heightmap(&self, seed: f32) -> HeightmapConfig {
        match self {
            BiomePreset::Plains => HeightmapConfig {
                seed,
                max_height: 80.0,
                frequency: 0.008,
                octaves: 7,
                lacunarity: 2.0,
                persistence: 0.5,
            },
            BiomePreset::Quarry => HeightmapConfig {
                // Rolling hills + mid-scale mesas. ~80m main feature wavelength,
                // 7 octaves for detail layering, persistence <0.5 keeps high-freq
                // gentle (prevents jagged noise). Works at 512m world scale.
                seed,
                max_height: 120.0,
                frequency: 0.012,
                octaves: 7,
                lacunarity: 2.1,
                persistence: 0.45,
            },
            BiomePreset::Desert => HeightmapConfig {
                seed,
                max_height: 45.0,
                frequency: 0.006,
                octaves: 5,
                lacunarity: 2.0,
                persistence: 0.45,
            },
            BiomePreset::Tundra => HeightmapConfig {
                seed,
                max_height: 110.0,
                frequency: 0.005,
                octaves: 6,
                lacunarity: 2.0,
                persistence: 0.5,
            },
        }
    }

    pub fn splat_thresholds(&self) -> SplatThresholds {
        match self {
            BiomePreset::Plains => SplatThresholds {
                sand_line: 15.0,
                snow_line: 100.0,
                rock_slope: 0.4,
            },
            BiomePreset::Quarry => SplatThresholds {
                sand_line: 5.0,
                snow_line: 200.0,
                rock_slope: 0.22,
            },
            BiomePreset::Desert => SplatThresholds {
                sand_line: 200.0,
                snow_line: 999.0,
                rock_slope: 0.6,
            },
            BiomePreset::Tundra => SplatThresholds {
                sand_line: 10.0,
                snow_line: 55.0,
                rock_slope: 0.45,
            },
        }
    }

    /// Colors for shader upload (`material_palette_base` + `_tip` = 8 × vec3).
    pub fn palette(&self) -> MaterialPalette {
        match self {
            BiomePreset::Plains => MaterialPalette {
                base: [
                    [0.28, 0.52, 0.15], // grass (green)
                    [0.45, 0.40, 0.35], // rock
                    [0.76, 0.69, 0.50], // sand
                    [0.92, 0.93, 0.95], // snow
                ],
                tip: [
                    [0.42, 0.68, 0.22],
                    [0.55, 0.50, 0.45],
                    [0.85, 0.78, 0.60],
                    [1.00, 1.00, 1.00],
                ],
            },
            BiomePreset::Quarry => MaterialPalette {
                // Warm ochre rock + dry tan grass + grey gravel
                base: [
                    [0.48, 0.44, 0.30], // "grass" → dormant/dry (tan-olive)
                    [0.55, 0.42, 0.28], // rock (warm ochre, sedimentary)
                    [0.62, 0.55, 0.42], // sand (gravel path)
                    [0.85, 0.82, 0.78], // "snow" → dusty top layer
                ],
                tip: [
                    [0.66, 0.58, 0.35],
                    [0.72, 0.55, 0.35],
                    [0.78, 0.70, 0.55],
                    [0.95, 0.92, 0.88],
                ],
            },
            BiomePreset::Desert => MaterialPalette {
                base: [
                    [0.68, 0.55, 0.32],
                    [0.58, 0.42, 0.28],
                    [0.82, 0.70, 0.50],
                    [0.90, 0.82, 0.70],
                ],
                tip: [
                    [0.78, 0.65, 0.40],
                    [0.72, 0.52, 0.35],
                    [0.92, 0.80, 0.58],
                    [1.00, 0.92, 0.78],
                ],
            },
            BiomePreset::Tundra => MaterialPalette {
                base: [
                    [0.32, 0.42, 0.22],
                    [0.40, 0.38, 0.36],
                    [0.70, 0.68, 0.55],
                    [0.95, 0.96, 0.98],
                ],
                tip: [
                    [0.48, 0.58, 0.30],
                    [0.55, 0.50, 0.48],
                    [0.82, 0.78, 0.65],
                    [1.00, 1.00, 1.00],
                ],
            },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BiomePreset::Plains => "plains",
            BiomePreset::Quarry => "quarry",
            BiomePreset::Desert => "desert",
            BiomePreset::Tundra => "tundra",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_biomes_have_palette() {
        let biomes = [
            BiomePreset::Plains,
            BiomePreset::Quarry,
            BiomePreset::Desert,
            BiomePreset::Tundra,
        ];
        for b in &biomes {
            let p = b.palette();
            for rgb in &p.base {
                for &c in rgb {
                    assert!(c >= 0.0 && c <= 1.0);
                }
            }
        }
    }

    #[test]
    fn quarry_has_warm_rock() {
        let p = BiomePreset::Quarry.palette();
        // Rock (index 1) should have R > B (warm)
        assert!(p.base[1][0] > p.base[1][2]);
    }
}
