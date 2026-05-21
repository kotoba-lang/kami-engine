// Headless wgpu renderer — base color pass + outline pass + PNG readback.
//
// Uses kami_render::OffscreenContext (sole owner of Instance::new per
// ARCHITECTURE.md §1.GPU-bootstrap-policy). One instance is reused across
// `render_multi()` calls for a single MangakaScene.
//
// P2 scope: fullscreen-triangle raymarch against capsule-approximated
// characters + ground plane + sky gradient. Real GPU skinning + mesh upload
// land in P3 alongside LLM cinematography.

#![cfg(not(target_family = "wasm"))]

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use kami_render::OffscreenContext;

use crate::{
    camera::CameraSpec,
    render::{RenderOpts, RenderPasses, RenderResult},
    scene::MangakaScene,
    Result, SceneError,
};

const MAX_CHARS: usize = 16;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

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

/// Headless renderer for a [`MangakaScene`].
///
/// Instances hold long-lived GPU state and are safe to reuse across many
/// `render()` calls. The `Arc` lets callers share a single GPU init across
/// per-character spring simulation fan-out in the Pregel graph.
pub struct MangakaRenderer {
    ctx: Arc<OffscreenContext>,
    base_pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    base_bind_group: wgpu::BindGroup,
    outline_bind_layout: wgpu::BindGroupLayout,
    outline_sampler: wgpu::Sampler,
}

impl MangakaRenderer {
    pub fn new() -> Result<Self> {
        let ctx = pollster::block_on(OffscreenContext::for_offscreen(
            "kami-mangaka-scene",
            COLOR_FORMAT,
        ))
        .map_err(|e| SceneError::Render(format!("offscreen init: {e}")))?;
        let ctx = Arc::new(ctx);
        Self::from_ctx(ctx)
    }

    pub fn from_ctx(ctx: Arc<OffscreenContext>) -> Result<Self> {
        let device = &ctx.device;

        // ── Base render: uniform buffer + bind group + pipeline ──
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mangaka_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let base_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mangaka_base_bind_layout"),
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
        let base_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mangaka_base_bind_group"),
            layout: &base_bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let base_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mangaka_base_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/render.wgsl").into()),
        });
        let base_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mangaka_base_pl"),
            bind_group_layouts: &[&base_bind_layout],
            push_constant_ranges: &[],
        });
        let base_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mangaka_base_pipeline"),
            layout: Some(&base_pl),
            vertex: wgpu::VertexState {
                module: &base_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &base_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
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

        // ── Outline pass: sample base color, run Sobel ──
        let outline_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mangaka_outline_bind_layout"),
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
        let outline_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mangaka_outline_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/outline.wgsl").into()),
        });
        let outline_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mangaka_outline_pl"),
            bind_group_layouts: &[&outline_bind_layout],
            push_constant_ranges: &[],
        });
        let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mangaka_outline_pipeline"),
            layout: Some(&outline_pl),
            vertex: wgpu::VertexState {
                module: &outline_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &outline_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
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
        let outline_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mangaka_outline_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            ctx,
            base_pipeline,
            outline_pipeline,
            uniform_buf,
            base_bind_group,
            outline_bind_layout,
            outline_sampler,
        })
    }

    /// Render a single frame for the configured `scene.camera` (or `cam` override).
    pub fn render(
        &self,
        scene: &MangakaScene,
        cam: Option<CameraSpec>,
        opts: RenderOpts,
    ) -> Result<RenderResult> {
        let camera = cam.or(scene.camera).unwrap_or_default();
        let w = opts.width.max(16);
        let h = opts.height.max(16);

        // ── Build uniforms ──
        let aspect = w as f32 / h as f32;
        let view = Mat4::look_at_rh(camera.eye, camera.target, camera.up);
        let proj = Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, 0.05, 200.0);
        let view_proj = proj * view;
        let view_proj_inv = view_proj.inverse();

        let sun_dir = Vec3::new(-0.3, 0.85, -0.2).normalize();
        let sky_top = [0.30, 0.45, 0.70, 1.0];
        let sky_bottom = [0.78, 0.80, 0.85, 1.0];
        let ground_y = -0.02_f32;
        let ground = [0.34, 0.32, 0.30, ground_y];

        let mut chars = [[0.0_f32; 4]; MAX_CHARS];
        let mut chars_radius = [[0.0_f32; 4]; MAX_CHARS];
        let mut chars_colour = [[0.0_f32; 4]; MAX_CHARS];

        let ids = scene.character_ids();
        let count = ids.len().min(MAX_CHARS);
        for (i, id) in ids.iter().take(MAX_CHARS).enumerate() {
            let Some(ch) = scene.characters.get(id) else {
                continue;
            };
            let t = ch.root_xform.translation;
            // Humanoid capsule: 1.7 m tall, 0.25 m radius, centered at y=0.85.
            chars[i] = [t.x, t.y + 0.85, t.z, 0.55];
            chars_radius[i] = [0.25, 0.0, 0.0, 0.0];
            chars_colour[i] = CHAR_PALETTE[i % CHAR_PALETTE.len()];
        }

        let uniforms = Uniforms {
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
        };

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        // ── Render targets ──
        let base_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mangaka_base_target"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let outline_tex = if opts.passes.contains(RenderPasses::OUTLINE) {
            Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mangaka_outline_target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            }))
        } else {
            None
        };

        let base_view = base_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mangaka_encoder"),
        });

        // Pass 1: base.
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mangaka_base_rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &base_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: sky_top[0] as f64,
                            g: sky_top[1] as f64,
                            b: sky_top[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rp.set_pipeline(&self.base_pipeline);
            rp.set_bind_group(0, &self.base_bind_group, &[]);
            rp.draw(0..3, 0..1);
        }

        // Pass 2: outline (optional).
        if let Some(out_tex) = &outline_tex {
            let out_view = out_tex.create_view(&wgpu::TextureViewDescriptor::default());
            let outline_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mangaka_outline_bg"),
                layout: &self.outline_bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&base_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.outline_sampler),
                    },
                ],
            });
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mangaka_outline_rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rp.set_pipeline(&self.outline_pipeline);
            rp.set_bind_group(0, &outline_bg, &[]);
            rp.draw(0..3, 0..1);
        }

        // ── Readback ──
        let base_png = read_texture_to_png(device, queue, &mut encoder, &base_tex, w, h)?;
        let outline_png = if let Some(out_tex) = &outline_tex {
            Some(read_texture_to_png(device, queue, &mut encoder, out_tex, w, h)?)
        } else {
            None
        };

        queue.submit(Some(encoder.finish()));

        // Issue the map_async callbacks BEFORE the first poll so the device
        // can drive them on the same Wait cycle. P12 fix — the previous
        // ordering (poll → map_async → poll-never) hung the test on macOS
        // because Metal's command queue never delivered the callback to a
        // future poll. Single Wait poll after registering both maps does
        // the trick.
        let base_png = base_png.queue_map(device)?;
        let outline_png = match outline_png {
            Some(o) => Some(o.queue_map(device)?),
            None => None,
        };
        device.poll(wgpu::Maintain::Wait);

        Ok(RenderResult {
            base_png: base_png.finish_read(device)?,
            depth_png: None, // P2.1: dedicated depth-only pass for storyboard reference.
            outline_png: match outline_png {
                Some(o) => Some(o.finish_read(device)?),
                None => None,
            },
            toon_png: None, // P2.1: posterized base for tone fill.
            camera,
        })
    }
}

// ── Texture → buffer → PNG plumbing ──

struct PendingPng {
    buffer: wgpu::Buffer,
    bytes_per_row: u32,
    unpadded_bytes_per_row: u32,
    width: u32,
    height: u32,
}

struct MappedPng {
    buffer: wgpu::Buffer,
    bytes_per_row: u32,
    unpadded_bytes_per_row: u32,
    width: u32,
    height: u32,
    rx: std::sync::mpsc::Receiver<std::result::Result<(), wgpu::BufferAsyncError>>,
}

impl PendingPng {
    /// Register the `map_async` callback. Caller MUST call `device.poll(Wait)`
    /// after this so the callback delivers — see comment in `MangakaRenderer::render`.
    fn queue_map(self, _device: &wgpu::Device) -> Result<MappedPng> {
        let slice = self.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel::<std::result::Result<(), wgpu::BufferAsyncError>>();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        Ok(MappedPng {
            buffer: self.buffer,
            bytes_per_row: self.bytes_per_row,
            unpadded_bytes_per_row: self.unpadded_bytes_per_row,
            width: self.width,
            height: self.height,
            rx,
        })
    }
}

impl MappedPng {
    fn finish_read(self, device: &wgpu::Device) -> Result<Vec<u8>> {
        // Try non-blocking recv first — the device.poll(Wait) before this
        // call usually fires the callback synchronously. If for some
        // reason it didn't (slow Metal queue), pump the device a few more
        // times before giving up.
        let mut attempts = 0;
        let recv_result = loop {
            match self.rx.try_recv() {
                Ok(r) => break r,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    attempts += 1;
                    if attempts > 16 {
                        return Err(SceneError::Render(
                            "map_async callback never fired after 16 polls".into(),
                        ));
                    }
                    device.poll(wgpu::Maintain::Wait);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(SceneError::Render("map_async channel disconnected".into()));
                }
            }
        };
        recv_result.map_err(|e| SceneError::Render(format!("map_async: {e}")))?;

        let slice = self.buffer.slice(..);
        let data = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity(
            (self.unpadded_bytes_per_row as usize) * self.height as usize,
        );
        for row in 0..self.height as usize {
            let start = row * self.bytes_per_row as usize;
            let end = start + self.unpadded_bytes_per_row as usize;
            rgba.extend_from_slice(&data[start..end]);
        }
        drop(data);
        self.buffer.unmap();

        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| SceneError::Render(format!("png header: {e}")))?;
            writer
                .write_image_data(&rgba)
                .map_err(|e| SceneError::Render(format!("png write: {e}")))?;
        }
        Ok(out)
    }
}

fn read_texture_to_png(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    tex: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<PendingPng> {
    // wgpu requires bytes_per_row aligned to 256.
    let bytes_per_pixel = 4_u32;
    let unpadded = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mangaka_readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    Ok(PendingPng {
        buffer,
        bytes_per_row: padded,
        unpadded_bytes_per_row: unpadded,
        width,
        height,
    })
}
