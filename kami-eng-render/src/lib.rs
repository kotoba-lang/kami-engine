//! kami-eng-render: Engineering-specific 2D/3D rendering primitives.
//!
//! Built on top of kami-render (wgpu) + kami-ui-gpu + kami-text.
//! Provides drawing commands for schematics, PCB, CAD views, waveforms, FEA color maps.

use glam::Vec2;

/// Engineering drawing command (submitted to wgpu renderer).
#[derive(Debug, Clone)]
pub enum EngDrawCmd {
    /// Straight line segment with width.
    Line {
        start: Vec2,
        end: Vec2,
        width: f32,
        color: [f32; 4],
        layer: u32,
    },
    /// Arc segment (center, radius, start/end angle).
    Arc {
        center: Vec2,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        width: f32,
        color: [f32; 4],
    },
    /// Circle (outline or filled).
    Circle {
        center: Vec2,
        radius: f32,
        width: f32,
        color: [f32; 4],
        filled: bool,
    },
    /// Rectangular pad (SMD/TH).
    Pad {
        center: Vec2,
        size: Vec2,
        corner_radius: f32,
        rotation: f32,
        color: [f32; 4],
    },
    /// Dimension annotation (two endpoints + value text).
    Dimension {
        start: Vec2,
        end: Vec2,
        offset: f32,
        value: f32,
        unit: String,
        color: [f32; 4],
    },
    /// Hatching pattern (for cross-section fill).
    Hatch {
        boundary: Vec<Vec2>,
        angle: f32,
        spacing: f32,
        color: [f32; 4],
    },
    /// Grid overlay.
    Grid {
        spacing: f32,
        major_every: u32,
        color: [f32; 4],
        major_color: [f32; 4],
    },
    /// Cursor crosshair.
    Crosshair {
        position: Vec2,
        size: f32,
        color: [f32; 4],
    },
    /// Color-mapped quad (for FEA stress/displacement visualization).
    ColorMapQuad {
        corners: [Vec2; 4],
        values: [f32; 4],
        color_map: ColorMap,
        min_val: f32,
        max_val: f32,
    },
    /// Waveform signal trace.
    WaveformTrace {
        transitions: Vec<(f32, f32)>,
        y_offset: f32,
        height: f32,
        color: [f32; 4],
    },
}

/// Color map for FEA post-processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMap {
    Rainbow,
    Jet,
    Viridis,
    Coolwarm,
    Grayscale,
}

impl ColorMap {
    /// Map a normalized value [0, 1] to RGBA color.
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        match self {
            ColorMap::Jet => {
                let r = (1.5 - (t * 4.0 - 3.0).abs()).clamp(0.0, 1.0);
                let g = (1.5 - (t * 4.0 - 2.0).abs()).clamp(0.0, 1.0);
                let b = (1.5 - (t * 4.0 - 1.0).abs()).clamp(0.0, 1.0);
                [r, g, b, 1.0]
            }
            ColorMap::Coolwarm => {
                let r = (0.5 + t * 0.5).clamp(0.0, 1.0);
                let b = (1.0 - t * 0.5).clamp(0.0, 1.0);
                let g = 1.0 - (2.0 * t - 1.0).abs();
                [r, g, b, 1.0]
            }
            ColorMap::Viridis => {
                let r = (0.267 + t * 0.726).min(1.0);
                let g = (0.004 + t * 0.870).min(1.0);
                let b = (0.329 + (1.0 - t) * 0.341).min(1.0);
                [r, g, b, 1.0]
            }
            ColorMap::Grayscale => [t, t, t, 1.0],
            ColorMap::Rainbow => {
                let h = (1.0 - t) * 270.0;
                let (r, g, b) = hsv_to_rgb(h, 1.0, 1.0);
                [r, g, b, 1.0]
            }
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (r + m, g + m, b + m)
}

/// Engineering viewport state.
pub struct EngViewport {
    pub center: Vec2,
    pub zoom: f32,
    pub rotation: f32,
    pub width: f32,
    pub height: f32,
}

impl EngViewport {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            rotation: 0.0,
            width,
            height,
        }
    }

    /// Convert screen coordinates to world coordinates.
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        let sx = (screen.x - self.width / 2.0) / self.zoom;
        let sy = (screen.y - self.height / 2.0) / self.zoom;
        Vec2::new(sx + self.center.x, sy + self.center.y)
    }

    /// Convert world coordinates to screen coordinates.
    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        let sx = (world.x - self.center.x) * self.zoom + self.width / 2.0;
        let sy = (world.y - self.center.y) * self.zoom + self.height / 2.0;
        Vec2::new(sx, sy)
    }

    pub fn zoom_to_fit(&mut self, min: Vec2, max: Vec2) {
        self.center = (min + max) / 2.0;
        let extent = max - min;
        let zoom_x = self.width / extent.x;
        let zoom_y = self.height / extent.y;
        self.zoom = zoom_x.min(zoom_y) * 0.9;
    }
}

/// Draw command buffer for a frame.
pub struct EngDrawList {
    pub commands: Vec<EngDrawCmd>,
}

impl EngDrawList {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn push(&mut self, cmd: EngDrawCmd) {
        self.commands.push(cmd);
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for EngDrawList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_map_jet_endpoints() {
        let low = ColorMap::Jet.sample(0.0);
        let high = ColorMap::Jet.sample(1.0);
        assert!(low[2] >= 0.5); // blue at low end
        assert!(high[0] >= 0.5); // red at high end
    }

    #[test]
    fn viewport_screen_world_roundtrip() {
        let vp = EngViewport::new(800.0, 600.0);
        let world = Vec2::new(10.0, 20.0);
        let screen = vp.world_to_screen(world);
        let back = vp.screen_to_world(screen);
        assert!((back.x - world.x).abs() < 1e-5);
        assert!((back.y - world.y).abs() < 1e-5);
    }
}
