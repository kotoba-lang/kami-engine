//! Browser GPU host (wasm-bindgen + wgpu) — the concrete `kami:engine/frame`
//! implementation the ClojureScript backend (`kami.backend.browser`) calls.
//!
//! Resources are uploaded once by string id (`register_mesh` / `register_material`
//! / `register_shader`); each frame `submit_frame` decodes the KAMI columnar
//! buffer (camera + instance matrices) plus a small JSON draw-table and renders
//! one instanced pass via wgpu. GPU bootstrap goes through the sanctioned owner
//! `kami_render::RenderContext::for_web_surface` (Authority Rule 1).
//!
//! Compiled for `wasm32-unknown-unknown` with `--features host`.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

use crate::frame;

/// Interleaved vertex: position(3) + normal(3) + uv(2) = 32 bytes, matching
/// `kami_render`'s `upload_mesh_interleaved` convention.
const VERTEX_STRIDE: u64 = 32;

struct GpuMesh {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    index_count: u32,
}

/// Per-draw instance attribute buffers: model matrices (vbuf 1) + RGBA tint
/// (vbuf 2). Both step per-instance; `count` instances are drawn.
struct InstanceBuffers {
    model: wgpu::Buffer,
    tint: wgpu::Buffer,
    count: u32,
}

#[derive(serde::Deserialize)]
struct DrawMeta {
    pipeline: String,
    mesh: String,
    material: String,
    #[allow(dead_code)]
    count: u32,
}

#[derive(serde::Deserialize)]
struct FrameMeta {
    #[serde(default)]
    clear: Option<[f32; 4]>,
    #[serde(default)]
    draws: Vec<DrawMeta>,
}

/// Host state. One per canvas.
#[wasm_bindgen]
pub struct KamiCljHost {
    ctx: kami_render::RenderContext,
    pipeline: wgpu::RenderPipeline,
    camera_bgl: wgpu::BindGroupLayout,
    material_bgl: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    camera_bg: wgpu::BindGroup,
    depth: wgpu::TextureView,
    meshes: HashMap<String, GpuMesh>,
    materials: HashMap<String, wgpu::BindGroup>,
    /// custom clj-authored WGSL pipelines, keyed by shader id (built lazily).
    shaders: HashMap<String, wgpu::RenderPipeline>,
}

#[wasm_bindgen]
impl KamiCljHost {
    /// Bootstrap a host bound to `canvas`. Async (adapter/device request).
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<KamiCljHost, JsValue> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let ctx =
            kami_render::RenderContext::for_web_surface(target, width, height, "kami-clj-host")
                .await
                .map_err(|e| JsValue::from_str(&format!("bootstrap: {e}")))?;

        let camera_bgl = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera-bgl"),
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
        let material_bgl = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material-bgl"),
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

        let camera_buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera-uniform"),
            size: 64, // mat4 view_proj
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        let pipeline = build_pipeline(&ctx, &camera_bgl, &material_bgl, DEFAULT_WGSL, ctx.format);
        let depth = make_depth(&ctx.device, width, height);

        Ok(KamiCljHost {
            ctx,
            pipeline,
            camera_bgl,
            material_bgl,
            camera_buf,
            camera_bg,
            depth,
            meshes: HashMap::new(),
            materials: HashMap::new(),
            shaders: HashMap::new(),
        })
    }

    /// Upload a mesh once under `id`. `vertices` is interleaved pos3+norm3+uv2.
    pub fn register_mesh(&mut self, id: String, vertices: &[f32], indices: &[u32]) {
        let vertex = self
            .ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-clj-mesh-v"),
                contents: bytemuck_cast_f32(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index = self
            .ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-clj-mesh-i"),
                contents: bytemuck_cast_u32(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.meshes.insert(
            id,
            GpuMesh {
                vertex,
                index,
                index_count: indices.len() as u32,
            },
        );
    }

    /// Upload material params under `id`. `params[0..4]` = albedo RGBA (the rest
    /// is reserved; the default shader only reads albedo).
    pub fn register_material(&mut self, id: String, params: &[f32]) {
        let mut albedo = [1.0f32, 1.0, 1.0, 1.0];
        for (i, slot) in albedo.iter_mut().enumerate() {
            if let Some(v) = params.get(i) {
                *slot = *v;
            }
        }
        let buf = self
            .ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-clj-material"),
                contents: bytemuck_cast_f32(&albedo),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bg = self
            .ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kami-clj-material-bg"),
                layout: &self.material_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
        self.materials.insert(id, bg);
    }

    /// Register a clj-authored WGSL shader (from `kami.wgsl/emit`) as a pipeline
    /// under `id`. `layout` is reserved for the bind-group plan.
    pub fn register_shader(&mut self, id: String, wgsl: String, _layout: String) {
        let pipe = build_pipeline(
            &self.ctx,
            &self.camera_bgl,
            &self.material_bgl,
            &wgsl,
            self.ctx.format,
        );
        self.shaders.insert(id, pipe);
    }

    /// Resize the surface + depth target.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.depth = make_depth(&self.ctx.device, width.max(1), height.max(1));
    }

    /// Render one frame. `meta_json` is the `kami.ipc/pack` `:meta` draw-table;
    /// `data` is the KAMI columnar buffer (camera + instance matrices).
    pub fn submit_frame(&mut self, meta_json: &str, data: &[u8]) -> Result<(), JsValue> {
        let meta: FrameMeta = serde_json::from_str(meta_json)
            .map_err(|e| JsValue::from_str(&format!("meta: {e}")))?;
        let fv = frame::decode(data).map_err(|e| JsValue::from_str(&format!("decode: {e:?}")))?;

        // camera: view_proj = proj · view (column-major)
        if let Some((view, proj)) = fv.camera() {
            let vp = mat_mul(&proj, &view);
            self.ctx
                .queue
                .write_buffer(&self.camera_buf, 0, bytemuck_cast_f32(&vp));
        }

        // Build per-draw instance buffers (model mat4 + RGBA tint) from the decoded
        // columns. `fv.draws()` is version-aware: v1 yields model-only blocks, v2
        // pairs `[model, tint]`. A v1 frame (or any draw lacking tint) gets a default
        // opaque-white tint so the pipeline's tint vertex buffer is always present —
        // v1 stays visually identical, v2 modulates albedo by the per-instance tint.
        let draws = fv.draws();
        let mut inst_bufs: Vec<InstanceBuffers> = Vec::with_capacity(draws.len());
        for (model_col, tint_col) in &draws {
            let mats = model_col.mat4s();
            let count = mats.len() as u32;
            let model_flat: Vec<f32> = mats.iter().flat_map(|m| m.iter().copied()).collect();
            let model = self
                .ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("kami-clj-instances"),
                    contents: bytemuck_cast_f32(&model_flat),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let mut tint_flat: Vec<f32> = match tint_col {
                Some(tc) => tc.f16x4s().iter().flat_map(|c| c.iter().copied()).collect(),
                None => Vec::new(),
            };
            tint_flat.resize(4 * count as usize, 1.0); // pad/truncate to one RGBA per instance
            let tint = self
                .ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("kami-clj-instance-tint"),
                    contents: bytemuck_cast_f32(&tint_flat),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            inst_bufs.push(InstanceBuffers {
                model,
                tint,
                count,
            });
        }

        let surface = self
            .ctx
            .surface
            .get_current_texture()
            .map_err(|e| JsValue::from_str(&format!("surface: {e}")))?;
        let view = surface
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let clear = meta.clear.unwrap_or([0.94, 0.917, 0.839, 1.0]); // Nintendo cream
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kami-clj-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kami-clj-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0] as f64,
                            g: clear[1] as f64,
                            b: clear[2] as f64,
                            a: clear[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.camera_bg, &[]);

            for (i, draw) in meta.draws.iter().enumerate() {
                let (Some(mesh), Some(mat_bg), Some(inst)) = (
                    self.meshes.get(&draw.mesh),
                    self.materials.get(&draw.material),
                    inst_bufs.get(i),
                ) else {
                    continue; // unregistered asset or missing column — skip this draw
                };
                let pipe = self.shaders.get(&draw.pipeline).unwrap_or(&self.pipeline);
                pass.set_pipeline(pipe);
                pass.set_bind_group(1, mat_bg, &[]);
                pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                pass.set_vertex_buffer(1, inst.model.slice(..));
                pass.set_vertex_buffer(2, inst.tint.slice(..));
                pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..inst.count);
            }
        }
        self.ctx.queue.submit(Some(encoder.finish()));
        surface.present();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn bytemuck_cast_f32(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
fn bytemuck_cast_u32(v: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// Column-major 4×4 multiply A·B (matches `kami.math/mul`).
fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let el = |m: &[f32; 16], c: usize, r: usize| m[c * 4 + r];
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = el(a, 0, r) * el(b, c, 0)
                + el(a, 1, r) * el(b, c, 1)
                + el(a, 2, r) * el(b, c, 2)
                + el(a, 3, r) * el(b, c, 3);
        }
    }
    out
}

fn make_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("kami-clj-depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_pipeline(
    ctx: &kami_render::RenderContext,
    camera_bgl: &wgpu::BindGroupLayout,
    material_bgl: &wgpu::BindGroupLayout,
    wgsl: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kami-clj-shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
    let layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kami-clj-pl"),
            bind_group_layouts: &[camera_bgl, material_bgl],
            push_constant_ranges: &[],
        });

    // vertex buffer 0: interleaved pos3+norm3+uv2 (stride 32)
    let vbuf = wgpu::VertexBufferLayout {
        array_stride: VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
        ],
    };
    // vertex buffer 1: per-instance mat4 (4 × vec4), step Instance
    let ibuf = wgpu::VertexBufferLayout {
        array_stride: 64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 48,
                shader_location: 6,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
    // vertex buffer 2: per-instance RGBA tint (vec4), step Instance
    let tbuf = wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[wgpu::VertexAttribute {
            offset: 0,
            shader_location: 7,
            format: wgpu::VertexFormat::Float32x4,
        }],
    };

    ctx.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kami-clj-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vbuf, ibuf, tbuf],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
}

/// Default instanced PBR-lite shader (Lambert). group0=camera, group1=material.
const DEFAULT_WGSL: &str = r#"
struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
struct Material { albedo: vec4<f32> };
@group(1) @binding(0) var<uniform> material: Material;

struct VsIn {
  @location(0) pos: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv: vec2<f32>,
  @location(3) m0: vec4<f32>,
  @location(4) m1: vec4<f32>,
  @location(5) m2: vec4<f32>,
  @location(6) m3: vec4<f32>,
  @location(7) tint: vec4<f32>,
};
struct VsOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
  let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
  var out: VsOut;
  out.clip = camera.view_proj * model * vec4<f32>(in.pos, 1.0);
  out.normal = (model * vec4<f32>(in.normal, 0.0)).xyz;
  out.tint = in.tint;
  return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let l = normalize(vec3<f32>(0.4, 1.0, 0.6));
  let diff = max(dot(normalize(in.normal), l), 0.0) * 0.7 + 0.3;
  let base = material.albedo * in.tint;
  return vec4<f32>(base.rgb * diff, base.a);
}
"#;
