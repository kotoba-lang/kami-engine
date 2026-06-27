// Browser preview surface — wasm-bindgen front for ScenePreview.
//
// Renders the same scene jsonld the LangGraph pod consumes, but onto the
// editor's HTMLCanvasElement instead of a headless texture. The shader
// bundle (render.wgsl) is shared verbatim across native + browser builds.
// Outline pass is omitted on the web preview to keep the per-frame cost
// down — the editor wants <16 ms tick and the inking pass only matters for
// the final PDS-bound render that the pod produces.

#![cfg(target_family = "wasm")]

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use kami_render::RenderContext;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::scene::MangakaScene;

const MAX_CHARS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj_inv: [f32; 16],
    cam_origin: [f32; 4],
    sun_dir: [f32; 4],
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    ground: [f32; 4],
    char_count: [u32; 4],
    chars: [[f32; 4]; MAX_CHARS],
    chars_radius: [[f32; 4]; MAX_CHARS],
    chars_colour: [[f32; 4]; MAX_CHARS],
}

const CHAR_PALETTE: [[f32; 4]; MAX_CHARS] = [
    [0.92, 0.86, 0.78, 1.0],
    [0.78, 0.86, 0.92, 1.0],
    [0.86, 0.78, 0.92, 1.0],
    [0.92, 0.78, 0.78, 1.0],
    [0.78, 0.92, 0.86, 1.0],
    [0.92, 0.92, 0.78, 1.0],
    [0.78, 0.78, 0.92, 1.0],
    [0.92, 0.78, 0.92, 1.0],
    [0.78, 0.92, 0.78, 1.0],
    [0.92, 0.86, 0.92, 1.0],
    [0.86, 0.92, 0.78, 1.0],
    [0.78, 0.86, 0.78, 1.0],
    [0.92, 0.78, 0.86, 1.0],
    [0.78, 0.92, 0.92, 1.0],
    [0.86, 0.86, 0.86, 1.0],
    [0.66, 0.66, 0.66, 1.0],
];

/// Stateful browser preview handle. JS holds one per canvas.
///
/// Typical lifecycle:
/// ```ts
/// import init, { ScenePreview } from "./scene-3d/kami_mangaka_scene.js";
/// await init();
/// const preview = await ScenePreview.create("scene-canvas");
/// preview.load_scene_jsonld(serverSceneJsonld);
/// // RAF loop from JS:
/// requestAnimationFrame(function frame() {
///   preview.render_frame();
///   requestAnimationFrame(frame);
/// });
/// ```
#[wasm_bindgen]
pub struct ScenePreview {
    ctx: RenderContext,
    scene: MangakaScene,
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

#[wasm_bindgen]
impl ScenePreview {
    /// Construct a preview bound to the given canvas. Returned as a Promise
    /// because adapter / device init are async on the browser.
    #[wasm_bindgen]
    pub async fn create(canvas_id: String) -> Result<ScenePreview, JsValue> {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("no document"))?;
        let element = document
            .get_element_by_id(&canvas_id)
            .ok_or_else(|| JsValue::from_str(&format!("canvas #{canvas_id} not found")))?;
        let canvas: web_sys::HtmlCanvasElement = element
            .dyn_into()
            .map_err(|_| JsValue::from_str("element is not a <canvas>"))?;

        let dpr = window.device_pixel_ratio().max(1.0) as f32;
        let css_w = canvas.client_width().max(1) as f32;
        let css_h = canvas.client_height().max(1) as f32;
        let width = (css_w * dpr).max(1.0) as u32;
        let height = (css_h * dpr).max(1.0) as u32;
        canvas.set_width(width);
        canvas.set_height(height);

        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let ctx = RenderContext::for_web_surface(target, width, height, "mangaka-scene-preview")
            .await
            .map_err(|e| JsValue::from_str(&format!("for_web_surface: {e}")))?;

        let (pipeline, uniform_buf, bind_group) = build_pipeline(&ctx.device, ctx.format);

        Ok(Self {
            ctx,
            scene: MangakaScene::new(),
            pipeline,
            uniform_buf,
            bind_group,
        })
    }

    /// Replace the entire scene state from a JSON-LD payload — the same
    /// shape `MangakaScene::to_jsonld()` emits on the server. Returns an
    /// error string if parsing fails.
    #[wasm_bindgen]
    pub fn load_scene_jsonld(&mut self, jsonld: &str) -> Result<(), JsValue> {
        let v: serde_json::Value = serde_json::from_str(jsonld)
            .map_err(|e| JsValue::from_str(&format!("scene jsonld parse: {e}")))?;
        let s = MangakaScene::from_jsonld(&v)
            .map_err(|e| JsValue::from_str(&format!("scene from_jsonld: {e}")))?;
        self.scene = s;
        Ok(())
    }

    /// Round-trip the current scene back out for the editor to ship to the
    /// LangGraph pod.
    #[wasm_bindgen]
    pub fn to_scene_jsonld(&self) -> String {
        self.scene.to_jsonld().to_string()
    }

    /// Orbit camera helper: yaw + pitch (radians) at `distance` metres from
    /// (0, 1.4, 0). For interactive mouse-drag in the editor.
    #[wasm_bindgen]
    pub fn set_orbit_camera(&mut self, yaw_rad: f32, pitch_rad: f32, distance_m: f32) {
        use crate::camera::{CameraSpec, ShotGrammar};
        let r = distance_m.max(0.3);
        let cy = pitch_rad.clamp(-1.4, 1.4);
        let cz = r * cy.cos();
        let cs = cz * yaw_rad.cos();
        let ss = cz * yaw_rad.sin();
        let ey = 1.4 + r * cy.sin();
        let cam = CameraSpec {
            eye: glam::Vec3::new(ss, ey, cs),
            target: glam::Vec3::new(0.0, 1.4, 0.0),
            up: glam::Vec3::Y,
            fov_deg: 35.0,
            roll_deg: 0.0,
            dof: None,
            shot: ShotGrammar::MediumShot,
        };
        self.scene.set_camera(cam);
    }

    /// Resize the surface — call from JS `ResizeObserver` when the canvas
    /// CSS box changes.
    #[wasm_bindgen]
    pub fn resize(&mut self, css_w: u32, css_h: u32, dpr: f32) {
        let w = ((css_w as f32) * dpr.max(1.0)).max(1.0) as u32;
        let h = ((css_h as f32) * dpr.max(1.0)).max(1.0) as u32;
        self.ctx.resize(w, h);
    }

    /// Render one frame to the canvas. JS owns the `requestAnimationFrame`
    /// loop — keeps `ScenePreview` thread-affined to the JS event loop and
    /// avoids reaching for `web_sys::Window::request_animation_frame` from
    /// Rust (which needs `Closure` plumbing).
    #[wasm_bindgen]
    pub fn render_frame(&mut self) -> Result<(), JsValue> {
        let frame = self
            .ctx
            .surface
            .get_current_texture()
            .map_err(|e| JsValue::from_str(&format!("surface acquire: {e}")))?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let uniforms = self.build_uniforms();
        self.ctx
            .queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mangaka_preview_encoder"),
            });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mangaka_preview_rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: uniforms.sky_top[0] as f64,
                            g: uniforms.sky_top[1] as f64,
                            b: uniforms.sky_top[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &self.bind_group, &[]);
            rp.draw(0..3, 0..1);
        }
        self.ctx.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn build_uniforms(&self) -> Uniforms {
        let camera = self.scene.camera.unwrap_or_default();
        let aspect = self.ctx.width.max(1) as f32 / self.ctx.height.max(1) as f32;
        let view = Mat4::look_at_rh(camera.eye, camera.target, camera.up);
        let proj = Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, 0.05, 200.0);
        let view_proj_inv = (proj * view).inverse();

        let sun_dir = Vec3::new(-0.3, 0.85, -0.2).normalize();
        let sky_top = [0.30, 0.45, 0.70, 1.0];
        let sky_bottom = [0.78, 0.80, 0.85, 1.0];
        let ground = [0.34, 0.32, 0.30, -0.02];

        let mut chars = [[0.0_f32; 4]; MAX_CHARS];
        let mut chars_radius = [[0.0_f32; 4]; MAX_CHARS];
        let mut chars_colour = [[0.0_f32; 4]; MAX_CHARS];

        let ids = self.scene.character_ids();
        let count = ids.len().min(MAX_CHARS);
        for (i, id) in ids.iter().take(MAX_CHARS).enumerate() {
            let Some(ch) = self.scene.characters.get(id) else {
                continue;
            };
            let t = ch.root_xform.translation;
            chars[i] = [t.x, t.y + 0.85, t.z, 0.55];
            chars_radius[i] = [0.25, 0.0, 0.0, 0.0];
            chars_colour[i] = CHAR_PALETTE[i % CHAR_PALETTE.len()];
        }

        Uniforms {
            view_proj_inv: view_proj_inv.to_cols_array(),
            cam_origin: [camera.eye.x, camera.eye.y, camera.eye.z, 1.0],
            sun_dir: [sun_dir.x, sun_dir.y, sun_dir.z, 0.0],
            sky_top,
            sky_bottom,
            ground,
            char_count: [count as u32, 0, 0, 0],
            chars,
            chars_radius,
            chars_colour,
        }
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mangaka_preview_uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mangaka_preview_bind_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mangaka_preview_bg"),
        layout: &bind_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buf.as_entire_binding(),
        }],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mangaka_preview_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/render.wgsl").into()),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mangaka_preview_pl"),
        bind_group_layouts: &[&bind_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mangaka_preview_pipeline"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    (pipeline, uniform_buf, bind_group)
}
