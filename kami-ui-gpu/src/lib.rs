//! kami-ui-gpu: GPU-rendered UI primitives for wgpu.
//!
//! Renders UI elements (rect, rounded rect, circle, gradient, border)
//! directly on the GPU as instanced quads. No DOM dependency.

use bytemuck::{Pod, Zeroable};

/// UI element instance for instanced quad rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct UiRect {
    pub position: [f32; 2],     // top-left corner (screen px)
    pub size: [f32; 2],         // width, height (screen px)
    pub color: [f32; 4],        // fill RGBA
    pub border_color: [f32; 4], // border RGBA
    pub corner_radius: f32,     // px (0 = sharp)
    pub border_width: f32,      // px (0 = no border)
    pub _pad: [f32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct UiText {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_rect: [f32; 4],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct UiColorGlyph {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_rect: [f32; 4],
}

/// Gradient direction.
#[derive(Debug, Clone, Copy)]
pub enum GradientDir {
    Horizontal,
    Vertical,
    Diagonal,
}

/// UI draw command.
#[derive(Debug, Clone)]
pub enum UiCommand {
    Rect(UiRect),
    Text(UiText),
    ColorGlyph(UiColorGlyph),
    Circle {
        center: [f32; 2],
        radius: f32,
        color: [f32; 4],
    },
    Gradient {
        rect: UiRect,
        color_end: [f32; 4],
        direction: GradientDir,
    },
}

/// UI layer: batched draw commands for a single render pass.
pub struct UiLayer {
    pub commands: Vec<UiCommand>,
    pub screen_width: f32,
    pub screen_height: f32,
}

impl UiLayer {
    pub fn new(w: f32, h: f32) -> Self {
        Self {
            commands: Vec::new(),
            screen_width: w,
            screen_height: h,
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.commands.push(UiCommand::Rect(UiRect {
            position: [x, y],
            size: [w, h],
            color,
            border_color: [0.0; 4],
            corner_radius: 0.0,
            border_width: 0.0,
            _pad: [0.0; 2],
        }));
    }

    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) {
        self.commands.push(UiCommand::Rect(UiRect {
            position: [x, y],
            size: [w, h],
            color,
            border_color: [0.0; 4],
            corner_radius: radius,
            border_width: 0.0,
            _pad: [0.0; 2],
        }));
    }

    pub fn bordered_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        border: [f32; 4],
        bw: f32,
        radius: f32,
    ) {
        self.commands.push(UiCommand::Rect(UiRect {
            position: [x, y],
            size: [w, h],
            color,
            border_color: border,
            corner_radius: radius,
            border_width: bw,
            _pad: [0.0; 2],
        }));
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: [f32; 4]) {
        self.commands.push(UiCommand::Circle {
            center: [cx, cy],
            radius: r,
            color,
        });
    }

    pub fn text(&mut self, position: [f32; 2], size: [f32; 2], uv_rect: [f32; 4], color: [f32; 4]) {
        self.commands.push(UiCommand::Text(UiText {
            position,
            size,
            uv_rect,
            color,
        }));
    }

    pub fn color_glyph(&mut self, position: [f32; 2], size: [f32; 2], uv_rect: [f32; 4]) {
        self.commands.push(UiCommand::ColorGlyph(UiColorGlyph {
            position,
            size,
            uv_rect,
        }));
    }

    /// Flatten all commands to UiRect instances for GPU upload.
    pub fn to_instances(&self) -> Vec<UiRect> {
        self.commands
            .iter()
            .filter_map(|cmd| match cmd {
                UiCommand::Rect(r) => Some(*r),
                UiCommand::Circle {
                    center,
                    radius,
                    color,
                } => Some(UiRect {
                    position: [center[0] - radius, center[1] - radius],
                    size: [radius * 2.0, radius * 2.0],
                    color: *color,
                    border_color: [0.0; 4],
                    corner_radius: *radius,
                    border_width: 0.0,
                    _pad: [0.0; 2],
                }),
                UiCommand::Gradient { rect, .. } => Some(*rect),
                UiCommand::Text(_) => None,
                UiCommand::ColorGlyph(_) => None,
            })
            .collect()
    }

    pub fn to_text_instances(&self) -> Vec<UiText> {
        self.commands
            .iter()
            .filter_map(|cmd| match cmd {
                UiCommand::Text(text) => Some(*text),
                _ => None,
            })
            .collect()
    }

    pub fn to_color_glyph_instances(&self) -> Vec<UiColorGlyph> {
        self.commands
            .iter()
            .filter_map(|cmd| match cmd {
                UiCommand::ColorGlyph(glyph) => Some(*glyph),
                _ => None,
            })
            .collect()
    }
}

// ── Notification / Toast Primitives ────────────
// Reusable toast rendering state for any KAMI app.

/// Toast severity level (affects color).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    /// Get the fill color for this toast level (Nintendo-style pastels).
    pub fn color(&self) -> [f32; 4] {
        match self {
            ToastLevel::Info => [0.36, 0.58, 0.95, 0.95], // soft blue
            ToastLevel::Success => [0.42, 0.82, 0.52, 0.95], // soft green
            ToastLevel::Warning => [0.98, 0.82, 0.28, 0.95], // soft yellow
            ToastLevel::Error => [0.95, 0.42, 0.42, 0.95], // soft red
        }
    }
}

/// A toast notification in the rendering queue.
#[derive(Debug, Clone)]
pub struct Toast {
    /// Title text.
    pub title: String,
    /// Body text.
    pub body: String,
    /// Severity level.
    pub level: ToastLevel,
    /// Time remaining in ms (0 = persistent). Decremented each frame.
    pub remaining_ms: u64,
    /// Y offset for stacking animation (0.0 = final position).
    pub anim_offset_y: f32,
}

/// Toast stack renderer. Manages a FIFO queue of toasts rendered top-right.
pub struct ToastStack {
    /// Active toasts (most recent first).
    pub toasts: Vec<Toast>,
    /// Maximum simultaneous toasts.
    pub max_visible: usize,
    /// Toast width in pixels.
    pub toast_width: f32,
    /// Toast height in pixels.
    pub toast_height: f32,
    /// Gap between toasts.
    pub gap: f32,
    /// Right margin from screen edge.
    pub margin_right: f32,
    /// Top margin from screen edge.
    pub margin_top: f32,
}

impl ToastStack {
    /// Create a toast stack with Nintendo-style defaults.
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            max_visible: 5,
            toast_width: 320.0,
            toast_height: 72.0,
            gap: 8.0,
            margin_right: 16.0,
            margin_top: 16.0,
        }
    }

    /// Push a new toast. Auto-trims if exceeding max_visible.
    pub fn push(&mut self, title: String, body: String, level: ToastLevel, duration_ms: u64) {
        self.toasts.insert(
            0,
            Toast {
                title,
                body,
                level,
                remaining_ms: duration_ms,
                anim_offset_y: -self.toast_height,
            },
        );
        if self.toasts.len() > self.max_visible {
            self.toasts.truncate(self.max_visible);
        }
    }

    /// Advance toast timers and animations. `dt_ms` = frame delta in ms.
    pub fn tick(&mut self, dt_ms: u64) {
        for toast in &mut self.toasts {
            // Animate entrance (spring-like ease toward 0)
            toast.anim_offset_y *= 0.85;
            if toast.anim_offset_y.abs() < 0.5 {
                toast.anim_offset_y = 0.0;
            }
            // Count down
            if toast.remaining_ms > 0 {
                toast.remaining_ms = toast.remaining_ms.saturating_sub(dt_ms);
            }
        }
        // Remove expired toasts
        self.toasts.retain(|t| t.remaining_ms > 0);
    }

    /// Render toasts into a UiLayer. `screen_w` = screen width for positioning.
    pub fn render(&self, layer: &mut UiLayer) {
        let x = layer.screen_width - self.toast_width - self.margin_right;
        for (i, toast) in self.toasts.iter().enumerate() {
            let y =
                self.margin_top + (i as f32) * (self.toast_height + self.gap) + toast.anim_offset_y;
            // Background rounded rect
            layer.rounded_rect(
                x,
                y,
                self.toast_width,
                self.toast_height,
                toast.level.color(),
                12.0,
            );
        }
    }
}

impl Default for ToastStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_stack() {
        let mut stack = ToastStack::new();
        stack.push("Title".into(), "Body".into(), ToastLevel::Success, 3000);
        assert_eq!(stack.toasts.len(), 1);

        stack.tick(1000);
        assert_eq!(stack.toasts[0].remaining_ms, 2000);

        stack.tick(2500);
        assert!(stack.toasts.is_empty(), "toast should have expired");
    }

    #[test]
    fn test_toast_level_colors() {
        assert_eq!(ToastLevel::Info.color()[3], 0.95);
        assert_ne!(ToastLevel::Error.color(), ToastLevel::Success.color());
    }

    #[test]
    fn test_ui_layer() {
        let mut layer = UiLayer::new(1920.0, 1080.0);
        layer.rect(10.0, 10.0, 200.0, 40.0, [1.0, 1.0, 1.0, 0.9]);
        layer.rounded_rect(10.0, 60.0, 200.0, 40.0, [0.2, 0.7, 0.9, 1.0], 12.0);
        layer.circle(100.0, 200.0, 20.0, [1.0, 0.4, 0.4, 1.0]);
        layer.text([12.0, 12.0], [8.0, 16.0], [0.0, 0.0, 0.1, 0.2], [1.0; 4]);
        layer.color_glyph([24.0, 24.0], [16.0, 16.0], [0.0, 0.0, 0.2, 0.2]);
        let instances = layer.to_instances();
        assert_eq!(instances.len(), 3);
        assert_eq!(instances[1].corner_radius, 12.0);
        assert_eq!(layer.to_text_instances().len(), 1);
        assert_eq!(layer.to_color_glyph_instances().len(), 1);
    }
}
