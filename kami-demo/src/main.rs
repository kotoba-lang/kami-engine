//! KAMI Engine Demo.
//!
//! Render:     cargo run -p kami-demo --release
//! Server:     cargo run -p kami-demo --release -- server
//! Client:     cargo run -p kami-demo --release -- client

mod net_demo;

use std::sync::Arc;

use glam::Vec3;
use kami_render::camera::{Camera, LightUniform, MaterialUniform};
use kami_render::mesh;
use kami_render::pipeline;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const INSTANCE_COUNT: u32 = 1000;
const SHADOW_MAP_SIZE: u32 = 2048;

struct Demo {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    time: f32,
}

struct GpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    light_buffer: wgpu::Buffer,
    material_buffer: wgpu::Buffer,
    camera_light_bind_group: wgpu::BindGroup,
    material_bind_group: wgpu::BindGroup,
    shadow_bind_group: wgpu::BindGroup,
    shadow_texture_view: wgpu::TextureView,
    depth_texture_view: wgpu::TextureView,
    camera: Camera,
}

impl Demo {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            time: 0.0,
        }
    }

    fn init_gpu(&mut self, window: Arc<Window>) {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let (device, queue, config) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .expect("no GPU adapter");

            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("kami-demo"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::Performance,
                    },
                    None,
                )
                .await
                .unwrap();

            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            (device, queue, config)
        });

        // Mesh: cube
        let (pos, norm, uv, indices) = mesh::cube();
        let mut interleaved = Vec::with_capacity(pos.len() / 3 * 8);
        for i in 0..pos.len() / 3 {
            interleaved.extend_from_slice(&pos[i * 3..i * 3 + 3]);
            interleaved.extend_from_slice(&norm[i * 3..i * 3 + 3]);
            interleaved.extend_from_slice(&uv[i * 2..i * 2 + 2]);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex"),
            contents: bytemuck::cast_slice(&interleaved),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Instances: 1000 cubes in grid
        let transforms = mesh::grid_instances(INSTANCE_COUNT, 2.5);
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instance"),
            contents: bytemuck::cast_slice(&transforms),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        // Camera + Light uniform
        let camera = Camera::new(config.width as f32 / config.height as f32);
        let light = LightUniform::directional(Vec3::new(-1.0, -2.0, -1.0), Vec3::ONE, 3.0);

        let cam_uniform = camera.uniform();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::bytes_of(&cam_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("light"),
            contents: bytemuck::bytes_of(&light),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Material uniform
        let material = MaterialUniform::default();
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material"),
            contents: bytemuck::bytes_of(&material),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // Bind group layouts
        let camera_light_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera-light-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material-layout"),
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

        // Shadow map
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_texture_view = shadow_texture.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-sampler"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        // Bind groups
        let camera_light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-light-bg"),
            layout: &camera_light_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });

        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material-bg"),
            layout: &material_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }],
        });

        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bg"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        // Depth buffer
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_texture_view = depth_texture.create_view(&Default::default());

        // Pipelines
        let pbr_pipeline = pipeline::create_pbr_pipeline(
            &device,
            config.format,
            &camera_light_layout,
            &material_layout,
            &shadow_layout,
        );
        let shadow_pipeline = pipeline::create_shadow_pipeline(&device, &camera_light_layout);

        self.window = Some(window.clone());
        self.gpu = Some(GpuState {
            device,
            queue,
            surface,
            config,
            pipeline: pbr_pipeline,
            shadow_pipeline,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            instance_buffer,
            camera_buffer,
            light_buffer,
            material_buffer,
            camera_light_bind_group,
            material_bind_group,
            shadow_bind_group,
            shadow_texture_view,
            depth_texture_view,
            camera,
        });
    }

    fn render(&mut self) {
        let gpu = self.gpu.as_mut().unwrap();

        self.time += 1.0 / 60.0;
        gpu.camera.orbit(self.time * 0.3, 0.5, 40.0);

        // Update camera + light uniform
        let cam_uniform = gpu.camera.uniform();
        gpu.queue
            .write_buffer(&gpu.camera_buffer, 0, bytemuck::bytes_of(&cam_uniform));
        let light = LightUniform::directional(Vec3::new(-1.0, -2.0, -1.0), Vec3::ONE, 3.0);
        gpu.queue
            .write_buffer(&gpu.light_buffer, 0, bytemuck::bytes_of(&light));

        let frame = gpu.surface.get_current_texture().unwrap();
        let view = frame.texture.create_view(&Default::default());

        let mut encoder = gpu.device.create_command_encoder(&Default::default());

        // Shadow pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.shadow_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&gpu.shadow_pipeline);
            pass.set_bind_group(0, &gpu.camera_light_bind_group, &[]);
            pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, gpu.instance_buffer.slice(..));
            pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..gpu.index_count, 0, 0..INSTANCE_COUNT);
        }

        // Main PBR pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pbr-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&gpu.pipeline);
            pass.set_bind_group(0, &gpu.camera_light_bind_group, &[]);
            pass.set_bind_group(1, &gpu.material_bind_group, &[]);
            pass.set_bind_group(2, &gpu.shadow_bind_group, &[]);
            pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, gpu.instance_buffer.slice(..));
            pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..gpu.index_count, 0, 0..INSTANCE_COUNT);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        self.window.as_ref().unwrap().request_redraw();
    }
}

impl ApplicationHandler for Demo {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("KAMI Engine — 1000 PBR Cubes")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            self.init_gpu(window);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    gpu.camera.aspect = gpu.config.width as f32 / gpu.config.height as f32;
                    // Recreate depth texture to match new size
                    let depth_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("depth"),
                        size: wgpu::Extent3d {
                            width: gpu.config.width,
                            height: gpu.config.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Depth32Float,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    });
                    gpu.depth_texture_view = depth_texture.create_view(&Default::default());
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("server") => net_demo::run_server(),
        Some("client") => net_demo::run_client(),
        _ => {
            // Default: render demo (1000 cubes)
            let event_loop = EventLoop::new().unwrap();
            let mut demo = Demo::new();
            event_loop.run_app(&mut demo).unwrap();
        }
    }
}
