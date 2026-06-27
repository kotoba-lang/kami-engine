//! kami-postfx: Post-processing effects for wgpu.
//!
//! Bloom, outline (cel-shading), CRT scanlines, vignette, SSAO.
//! Each effect = a fullscreen pass reading previous frame's texture.

use bytemuck::{Pod, Zeroable};

/// Post-processing effect type.
#[derive(Debug, Clone)]
pub enum PostEffect {
    /// Bloom: bright pixel bleed. Nintendo: Splatoon ink glow.
    Bloom {
        threshold: f32,
        intensity: f32,
        radius: f32,
    },
    /// Outline: edge detection (Sobel). Nintendo: cel-shading / Zelda Wind Waker.
    Outline {
        color: [f32; 4],
        width: f32,
        depth_threshold: f32,
    },
    /// Vignette: dark corners.
    Vignette { intensity: f32, radius: f32 },
    /// CRT: scanlines + curvature. Retro feel.
    CRT {
        scanline_intensity: f32,
        curvature: f32,
    },
    /// Color grading: lift/gamma/gain.
    ColorGrade {
        lift: [f32; 3],
        gamma: [f32; 3],
        gain: [f32; 3],
    },
    /// Pixelate: downscale for retro pixel art look.
    Pixelate { pixel_size: f32 },
    /// SSAO: screen-space ambient occlusion. Contact shadows in creases/cavities.
    SSAO {
        radius: f32,
        bias: f32,
        intensity: f32,
        /// Number of sample kernel directions (16/32/64).
        samples: u32,
    },
    /// Depth of Field: bokeh blur based on focal distance.
    DepthOfField {
        focal_distance: f32,
        focal_range: f32,
        bokeh_radius: f32,
        /// 0=gaussian, 1=hexagonal bokeh.
        bokeh_shape: u32,
    },
    /// Screen-Space Reflections: ray-marched reflections on glossy surfaces.
    SSR {
        max_distance: f32,
        /// Ray march step count (32/64/128).
        steps: u32,
        thickness: f32,
        fade_edge: f32,
    },
    /// ACES filmic tonemapping: HDR→LDR with cinematic contrast curve.
    ACESTonemap {
        exposure: f32,
        /// 0=ACES Fitted, 1=ACES Full, 2=Uncharted2, 3=Reinhard.
        curve: u32,
    },
    /// Film grain: photographic noise for cinematic feel.
    FilmGrain {
        intensity: f32,
        /// Grain size in pixels.
        size: f32,
    },
    /// Chromatic aberration: RGB channel offset at screen edges.
    ChromaticAberration {
        intensity: f32,
        /// Number of samples for smooth fringing (3/5/7).
        samples: u32,
    },
    /// God rays: volumetric light scattering from a directional light source.
    GodRays {
        density: f32,
        weight: f32,
        decay: f32,
        exposure: f32,
        /// Light source position in screen space [0,1].
        light_pos: [f32; 2],
    },
}

/// Post-fx pipeline configuration.
#[derive(Debug, Clone)]
pub struct PostFxPipeline {
    pub effects: Vec<PostEffect>,
    pub enabled: bool,
}

impl PostFxPipeline {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            enabled: true,
        }
    }

    pub fn add(&mut self, effect: PostEffect) -> &mut Self {
        self.effects.push(effect);
        self
    }

    /// Nintendo preset: soft bloom + outline.
    pub fn nintendo() -> Self {
        let mut p = Self::new();
        p.add(PostEffect::Bloom {
            threshold: 0.8,
            intensity: 0.3,
            radius: 4.0,
        });
        p.add(PostEffect::Outline {
            color: [0.15, 0.15, 0.15, 1.0],
            width: 1.5,
            depth_threshold: 0.1,
        });
        p.add(PostEffect::Vignette {
            intensity: 0.15,
            radius: 0.8,
        });
        p
    }

    /// Retro pixel art preset.
    pub fn retro() -> Self {
        let mut p = Self::new();
        p.add(PostEffect::Pixelate { pixel_size: 4.0 });
        p.add(PostEffect::CRT {
            scanline_intensity: 0.3,
            curvature: 0.02,
        });
        p
    }

    /// Final Fantasy quality preset: SSAO + SSR + DOF + ACES + bloom + film grain.
    /// Designed for photorealistic character rendering with cinematic atmosphere.
    pub fn final_fantasy() -> Self {
        let mut p = Self::new();
        p.add(PostEffect::SSAO {
            radius: 0.5,
            bias: 0.025,
            intensity: 1.2,
            samples: 64,
        });
        p.add(PostEffect::SSR {
            max_distance: 50.0,
            steps: 64,
            thickness: 0.3,
            fade_edge: 0.15,
        });
        p.add(PostEffect::Bloom {
            threshold: 0.9,
            intensity: 0.15,
            radius: 6.0,
        });
        p.add(PostEffect::DepthOfField {
            focal_distance: 2.5,
            focal_range: 1.5,
            bokeh_radius: 3.0,
            bokeh_shape: 1, // hexagonal
        });
        p.add(PostEffect::GodRays {
            density: 0.96,
            weight: 0.15,
            decay: 0.97,
            exposure: 0.12,
            light_pos: [0.5, 0.3],
        });
        p.add(PostEffect::ACESTonemap {
            exposure: 1.1,
            curve: 0, // ACES Fitted
        });
        p.add(PostEffect::ChromaticAberration {
            intensity: 0.002,
            samples: 5,
        });
        p.add(PostEffect::FilmGrain {
            intensity: 0.03,
            size: 1.5,
        });
        p.add(PostEffect::Vignette {
            intensity: 0.2,
            radius: 0.85,
        });
        p.add(PostEffect::ColorGrade {
            lift: [0.0, -0.01, 0.02], // subtle cool shadows
            gamma: [1.0, 1.0, 0.98],  // neutral mids
            gain: [1.05, 1.02, 1.0],  // warm highlights
        });
        p
    }

    /// Baminiku LiveStage character preset: portrait-focused DOF + warm bloom.
    pub fn baminiku_character() -> Self {
        let mut p = Self::new();
        p.add(PostEffect::SSAO {
            radius: 0.3,
            bias: 0.02,
            intensity: 0.8,
            samples: 32,
        });
        p.add(PostEffect::Bloom {
            threshold: 0.85,
            intensity: 0.2,
            radius: 5.0,
        });
        p.add(PostEffect::DepthOfField {
            focal_distance: 2.0,
            focal_range: 0.8,
            bokeh_radius: 4.0,
            bokeh_shape: 1,
        });
        p.add(PostEffect::ACESTonemap {
            exposure: 1.0,
            curve: 0,
        });
        p.add(PostEffect::Vignette {
            intensity: 0.25,
            radius: 0.8,
        });
        p.add(PostEffect::ColorGrade {
            lift: [0.01, 0.0, -0.01],
            gamma: [1.02, 1.0, 0.98],
            gain: [1.08, 1.04, 1.0],
        });
        p
    }
}

/// Bloom pass parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct BloomParams {
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
    pub _pad: f32,
}

/// Outline pass parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct OutlineParams {
    pub color: [f32; 4],
    pub width: f32,
    pub depth_threshold: f32,
    pub _pad: [f32; 2],
}

/// SSAO pass parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SSAOParams {
    pub radius: f32,
    pub bias: f32,
    pub intensity: f32,
    pub samples: u32,
}

/// Depth of Field pass parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct DepthOfFieldParams {
    pub focal_distance: f32,
    pub focal_range: f32,
    pub bokeh_radius: f32,
    pub bokeh_shape: u32,
}

/// Screen-Space Reflections pass parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SSRParams {
    pub max_distance: f32,
    pub steps: u32,
    pub thickness: f32,
    pub fade_edge: f32,
}

/// ACES filmic tonemapping parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ACESTonemapParams {
    pub exposure: f32,
    pub curve: u32,
    pub _pad: [f32; 2],
}

/// God rays (volumetric light scattering) parameters for GPU uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GodRaysParams {
    pub density: f32,
    pub weight: f32,
    pub decay: f32,
    pub exposure: f32,
    pub light_pos: [f32; 2],
    pub _pad: [f32; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nintendo_preset() {
        let p = PostFxPipeline::nintendo();
        assert_eq!(p.effects.len(), 3);
        assert!(p.enabled);
    }

    #[test]
    fn test_final_fantasy_preset() {
        let p = PostFxPipeline::final_fantasy();
        assert_eq!(p.effects.len(), 10);
        assert!(p.enabled);
    }

    #[test]
    fn test_baminiku_character_preset() {
        let p = PostFxPipeline::baminiku_character();
        assert_eq!(p.effects.len(), 6);
        assert!(p.enabled);
    }
}
