//! PPTX Document Renderer — 2D slide editing via wgpu (WebGPU + WebGL2 fallback).
//!
//! Renders PPTX slides using orthographic projection with `kami-ui-gpu` instanced
//! primitives (rects, rounded rects, ellipses, circles) and `kami-text` SDF text.
//!
//! Entry point: [`run_with_document`] — called from JS with a canvas ID and slide JSON.

use glam::Vec2;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// EMU per inch (OOXML native unit).
const EMU_PER_INCH: f32 = 914400.0;
/// Pixels per inch for conversion.
const PX_PER_INCH: f32 = 96.0;

/// Convert EMU to pixels.
fn emu_to_px(emu: f32) -> f32 {
    emu * PX_PER_INCH / EMU_PER_INCH
}

// ---------------------------------------------------------------------------
// JSON input types (matches ooxml-parser.ts output)
// ---------------------------------------------------------------------------

/// Document slide data received from JS.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSlide {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub shapes: Vec<DocumentShape>,
    #[serde(default)]
    pub selected_ids: Vec<String>,
}

/// Shape element on a slide.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentShape {
    pub id: String,
    #[serde(rename = "type")]
    pub shape_type: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub rotation: f32,
    pub fill: Option<String>,
    pub stroke: Option<String>,
    #[serde(default)]
    pub stroke_width: f32,
    #[serde(default)]
    pub corner_radius: Option<f32>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub text_color: Option<String>,
    #[serde(default)]
    pub text_size: Option<f32>,
    #[serde(default)]
    pub text_bold: Option<bool>,
}

// ---------------------------------------------------------------------------
// Color parsing
// ---------------------------------------------------------------------------

/// Parse a 6-char hex color (RRGGBB) to [r, g, b, a] floats.
fn hex_to_rgba(hex: &str) -> [f32; 4] {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return [0.5, 0.5, 0.5, 1.0];
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128) as f32 / 255.0;
    [r, g, b, 1.0]
}

// ---------------------------------------------------------------------------
// UI Layer builder
// ---------------------------------------------------------------------------

/// Build a `kami_ui_gpu::UiLayer` from a slide definition.
fn build_slide_layer(
    slide: &DocumentSlide,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> kami_ui_gpu::UiLayer {
    let sw = emu_to_px(slide.width) * scale;
    let sh = emu_to_px(slide.height) * scale;
    let mut layer = kami_ui_gpu::UiLayer::new(sw + offset_x * 2.0, sh + offset_y * 2.0);

    // Slide background
    let bg = slide
        .background
        .as_deref()
        .map(hex_to_rgba)
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    layer.rect(offset_x, offset_y, sw, sh, bg);

    // Slide border
    layer.bordered_rect(
        offset_x,
        offset_y,
        sw,
        sh,
        [0.0; 4],
        [0.2, 0.2, 0.2, 1.0],
        1.0,
        0.0,
    );

    let selected_set: std::collections::HashSet<&str> =
        slide.selected_ids.iter().map(|s| s.as_str()).collect();

    for shape in &slide.shapes {
        if shape.visible == Some(false) {
            continue;
        }

        let x = offset_x + emu_to_px(shape.x) * scale;
        let y = offset_y + emu_to_px(shape.y) * scale;
        let w = emu_to_px(shape.w) * scale;
        let h = emu_to_px(shape.h) * scale;

        let fill = shape.fill.as_deref().map(hex_to_rgba).unwrap_or([0.0; 4]);
        let has_fill = shape.fill.is_some() && shape.shape_type != "line";

        match shape.shape_type.as_str() {
            "ellipse" => {
                // Approximate ellipse with a high corner-radius rounded rect
                let r = w.min(h) / 2.0;
                if has_fill {
                    layer.rounded_rect(x, y, w, h, fill, r);
                }
                if let Some(stroke_hex) = &shape.stroke {
                    let sc = hex_to_rgba(stroke_hex);
                    let sw = (emu_to_px(shape.stroke_width) * scale).max(1.0);
                    layer.bordered_rect(x, y, w, h, [0.0; 4], sc, sw, r);
                }
            }
            "roundRect" => {
                let r = shape
                    .corner_radius
                    .map(|cr| emu_to_px(cr) * scale)
                    .unwrap_or(w.min(h) * 0.1);
                if has_fill {
                    layer.rounded_rect(x, y, w, h, fill, r);
                }
                if let Some(stroke_hex) = &shape.stroke {
                    let sc = hex_to_rgba(stroke_hex);
                    let sw = (emu_to_px(shape.stroke_width) * scale).max(1.0);
                    layer.bordered_rect(x, y, w, h, [0.0; 4], sc, sw, r);
                }
            }
            "line" => {
                // Render line as a thin rect between (x,y) and (x+w, y+h)
                let sc = shape
                    .stroke
                    .as_deref()
                    .map(hex_to_rgba)
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let lw = (emu_to_px(shape.stroke_width) * scale).max(1.0);
                // Approximate: horizontal line
                layer.rect(x, y + h / 2.0 - lw / 2.0, w, lw, sc);
            }
            "triangle" => {
                // Approximate triangle as a rect with fill (true triangle needs custom shader)
                if has_fill {
                    layer.rect(x, y, w, h, fill);
                }
            }
            _ => {
                // rect, textBox, arrow, freeform → standard rect
                if has_fill {
                    layer.rect(x, y, w, h, fill);
                }
                if let Some(stroke_hex) = &shape.stroke {
                    let sc = hex_to_rgba(stroke_hex);
                    let sw = (emu_to_px(shape.stroke_width) * scale).max(1.0);
                    layer.bordered_rect(x, y, w, h, [0.0; 4], sc, sw, 0.0);
                }
            }
        }

        // Selection indicator
        if selected_set.contains(shape.id.as_str()) {
            let sel_color = [0.29, 0.56, 0.85, 0.8]; // #4a90d9
            layer.bordered_rect(
                x - 2.0,
                y - 2.0,
                w + 4.0,
                h + 4.0,
                [0.0; 4],
                sel_color,
                2.0,
                0.0,
            );

            // 8 resize handles (small white squares with blue border)
            let hs: f32 = 8.0;
            let handle_fill = [1.0, 1.0, 1.0, 1.0];
            let positions = [
                (x - hs / 2.0, y - hs / 2.0),
                (x + w / 2.0 - hs / 2.0, y - hs / 2.0),
                (x + w - hs / 2.0, y - hs / 2.0),
                (x + w - hs / 2.0, y + h / 2.0 - hs / 2.0),
                (x + w - hs / 2.0, y + h - hs / 2.0),
                (x + w / 2.0 - hs / 2.0, y + h - hs / 2.0),
                (x - hs / 2.0, y + h - hs / 2.0),
                (x - hs / 2.0, y + h / 2.0 - hs / 2.0),
            ];
            for (hx, hy) in positions {
                layer.bordered_rect(hx, hy, hs, hs, handle_fill, sel_color, 1.0, 0.0);
            }

            // Rotation handle (circle above top-center)
            let rot_y = y - 30.0;
            layer.circle(x + w / 2.0, rot_y, 5.0, sel_color);
            // Connecting line
            layer.rect(x + w / 2.0 - 0.5, rot_y + 5.0, 1.0, 25.0 - 5.0, sel_color);
        }
    }

    layer
}

// ---------------------------------------------------------------------------
// GPU pipeline for 2D UI rendering
// ---------------------------------------------------------------------------

/// WGSL shader for instanced UI rectangles (SDF rounded rect + border).
const UI_SHADER: &str = r#"
struct UiRect {
    @location(0) position: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) corner_radius: f32,
    @location(5) border_width: f32,
};

struct Uniforms {
    screen_size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) size_px: vec2<f32>,
    @location(4) corner_radius: f32,
    @location(5) border_width: f32,
};

@vertex fn vs(
    @builtin(vertex_index) vi: u32,
    inst: UiRect,
) -> VsOut {
    // Quad: 0=TL, 1=TR, 2=BL, 3=BR (triangle strip)
    let uv = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    let px = inst.position + uv * inst.size;
    let ndc = vec2<f32>(
        px.x / u.screen_size.x * 2.0 - 1.0,
        1.0 - px.y / u.screen_size.y * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    out.color = inst.color;
    out.border_color = inst.border_color;
    out.size_px = inst.size;
    out.corner_radius = inst.corner_radius;
    out.border_width = inst.border_width;
    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment fn fs(v: VsOut) -> @location(0) vec4<f32> {
    let half = v.size_px * 0.5;
    let p = (v.uv - 0.5) * v.size_px;
    let r = min(v.corner_radius, min(half.x, half.y));
    let d = sdf_rounded_rect(p, half, r);

    // Anti-aliased edges
    let aa = fwidth(d);

    if v.border_width > 0.0 {
        let fill_alpha = 1.0 - smoothstep(-aa, aa, d);
        let border_alpha = 1.0 - smoothstep(-aa, aa, d + v.border_width);
        let inner_alpha = 1.0 - smoothstep(-aa, aa, d + v.border_width);
        // Border = outer - inner
        let border_mask = border_alpha * (1.0 - fill_alpha * f32(v.color.a > 0.001));
        let fill_mask = fill_alpha * f32(v.color.a > 0.001);

        if fill_mask + border_mask < 0.001 { discard; }

        // Actually: fill inside, border on edge
        let inner_d = d + v.border_width;
        let inner_fill = 1.0 - smoothstep(-aa, aa, inner_d);
        let outer_fill = 1.0 - smoothstep(-aa, aa, d);
        let border_ring = outer_fill - inner_fill;

        let c = v.color * inner_fill + v.border_color * max(border_ring, 0.0);
        if c.a < 0.001 { discard; }
        return c;
    } else {
        let alpha = 1.0 - smoothstep(-aa, aa, d);
        if alpha < 0.001 { discard; }
        return vec4<f32>(v.color.rgb, v.color.a * alpha);
    }
}
"#;

/// Create the GPU pipeline for UI rect rendering.
fn create_ui_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ui_shader"),
        source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ui_uniforms"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ui_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ui_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<kami_ui_gpu::UiRect>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    // position
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    // size
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 8,
                        shader_location: 1,
                    },
                    // color
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 16,
                        shader_location: 2,
                    },
                    // border_color
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 32,
                        shader_location: 3,
                    },
                    // corner_radius
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 48,
                        shader_location: 4,
                    },
                    // border_width
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 52,
                        shader_location: 5,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Render a PPTX slide to a canvas using wgpu (WebGPU + WebGL2 fallback).
///
/// # Arguments
/// * `canvas_id` — HTML canvas element ID.
/// * `slide_json` — JSON string matching `DocumentSlide` (shapes, dimensions, selection).
///
/// # Returns
/// Resolves when the frame is rendered. Call again for each frame update.
#[wasm_bindgen]
pub async fn render_document_frame(canvas_id: &str, slide_json: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Warn);

    let slide: DocumentSlide = serde_json::from_str(slide_json)
        .map_err(|e| JsValue::from_str(&format!("invalid slide JSON: {e}")))?;

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str(&format!("canvas '{canvas_id}' not found")))?
        .dyn_into::<HtmlCanvasElement>()?;

    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    // Unified bootstrap via kami-render (Backends + Limits owner).
    let target = wgpu::SurfaceTarget::Canvas(canvas.clone());
    let ctx = kami_render::RenderContext::for_web_surface(target, width, height, "pptx-editor")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let device = ctx.device;
    let queue = ctx.queue;
    let surface = ctx.surface;
    let format = ctx.format;

    // Create UI pipeline
    let (pipeline, bind_group_layout) = create_ui_pipeline(&device, format);

    // Uniform buffer: screen size
    let uniform_data = [width as f32, height as f32];
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ui_uniforms"),
        contents: bytemuck::cast_slice(&uniform_data),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ui_bind_group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    // Build UI layer from slide
    let slide_px_w = emu_to_px(slide.width);
    let slide_px_h = emu_to_px(slide.height);
    let scale_x = (width as f32 - 40.0) / slide_px_w;
    let scale_y = (height as f32 - 40.0) / slide_px_h;
    let scale = scale_x.min(scale_y).min(1.0);
    let offset_x = (width as f32 - slide_px_w * scale) / 2.0;
    let offset_y = (height as f32 - slide_px_h * scale) / 2.0;

    let layer = build_slide_layer(&slide, scale, offset_x, offset_y);
    let instances = layer.to_instances();

    if instances.is_empty() {
        return Ok(());
    }

    // Upload instances to GPU
    let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ui_instances"),
        contents: bytemuck::cast_slice(&instances),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // Render frame
    let output = surface
        .get_current_texture()
        .map_err(|e| JsValue::from_str(&format!("surface texture: {e}")))?;
    let view = output.texture.create_view(&Default::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("ui_encoder"),
    });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.1,
                        b: 0.18,
                        a: 1.0,
                    }), // #1a1a2e editor background
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, instance_buffer.slice(..));
        pass.draw(0..4, 0..instances.len() as u32); // 4 vertices per quad (triangle strip), N instances
    }

    queue.submit(std::iter::once(encoder.finish()));
    output.present();

    Ok(())
}

/// Check if WebGPU or WebGL2 is available for document rendering.
#[wasm_bindgen]
pub async fn check_document_gpu() -> bool {
    // Mirror kami-render's web Backends policy.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
        ..Default::default()
    });
    instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .is_some()
}

/// Get GPU adapter info for the document renderer.
#[wasm_bindgen]
pub async fn document_gpu_info() -> Result<String, JsValue> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok_or("no adapter")?;
    let info = adapter.get_info();
    Ok(format!(
        "{} {} ({:?})",
        info.vendor, info.device, info.backend
    ))
}

use wgpu::util::DeviceExt;
