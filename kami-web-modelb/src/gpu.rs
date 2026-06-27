//! Browser GPU display for the web Model-B dance (compliant: wgpu only, never
//! Canvas2D). A wgpu surface over the page `<canvas>` (via
//! `kami_render::RenderContext::for_web_surface`, the path kami-web uses), plus a
//! fullscreen-quad **blit**: the dance is rasterised on the CPU by
//! `kami_webgpu_rs::render` (the same offscreen path as `dance_png`), uploaded as
//! a texture, and drawn to the surface. So the rendering is the engine's, the
//! display is wgpu — no Canvas2D.
//!
//! wasm32-only (a `<canvas>` + `SurfaceTarget::Canvas` exist only in the browser).

use kami_render::RenderContext;
use wasm_bindgen::JsCast;

const BLIT_WGSL: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VO {
  var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  let xy = p[i];
  var o: VO;
  o.pos = vec4<f32>(xy, 0.0, 1.0);
  o.uv = vec2<f32>((xy.x + 1.0) * 0.5, (1.0 - xy.y) * 0.5);
  return o;
}
@fragment fn fs(o: VO) -> @location(0) vec4<f32> { return textureSample(tex, samp, o.uv); }
"#;

/// A wgpu surface over the page canvas + a fullscreen-quad blit pipeline.
pub struct Gpu {
    ctx: RenderContext,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Gpu {
    /// Acquire `<canvas id=…>`, bring up a wgpu surface, and build the blit pipeline.
    pub async fn new(canvas_id: &str, w: u32, h: u32) -> Result<Gpu, String> {
        let canvas: web_sys::HtmlCanvasElement = web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.get_element_by_id(canvas_id))
            .ok_or_else(|| format!("canvas #{canvas_id} not found"))?
            .dyn_into()
            .map_err(|_| "element is not a <canvas>".to_string())?;
        canvas.set_width(w);
        canvas.set_height(h);
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let ctx = RenderContext::for_web_surface(target, w, h, "kami-web-modelb")
            .await
            .map_err(|e| format!("wgpu surface: {e:?}"))?;

        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("blit"),
                source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
            });
        let bgl = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pl = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(ctx.config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Gpu { ctx, pipeline, bgl, sampler })
    }

    /// Upload a `w×h` RGBA8 pixel buffer (the CPU-rasterised dance) as a texture and
    /// draw it fullscreen to the surface.
    pub fn blit_rgba(&self, rgba: &[u8], w: u32, h: u32) -> Result<(), String> {
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let tex = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.ctx.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
            size,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let frame = self
            .ctx
            .surface
            .get_current_texture()
            .map_err(|e| format!("acquire frame: {e:?}"))?;
        let fview = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &fview,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &bind, &[]);
            rp.draw(0..3, 0..1);
        }
        self.ctx.queue.submit([enc.finish()]);
        frame.present();
        Ok(())
    }
}
