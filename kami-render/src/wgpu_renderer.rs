//! wgpu renderer implementation.
//!
//! Covers all platforms via wgpu's backend selection:
//!   wgpu::Backends::VULKAN  — PC Linux/Win, Android, (PS5 future)
//!   wgpu::Backends::METAL   — macOS, iOS
//!   wgpu::Backends::DX12    — PC Win (fallback)
//!   wgpu::Backends::BROWSER_WEBGPU — Web (Chrome/Edge/Firefox)

#[cfg(feature = "wgpu-backend")]
use crate::{Camera, DrawCmd, MaterialHandle, MaterialUniform, MeshHandle, Renderer};
use kami_core::ipc::Column;
use wgpu::util::DeviceExt;

/// GPU-side mesh data.
struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// GPU-side material: uniform buffer + bind group for Group 1.
struct GpuMaterial {
    _uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    meshes: Vec<GpuMesh>,
    materials: Vec<GpuMaterial>,
    material_layout: Option<wgpu::BindGroupLayout>,
}

impl WgpuRenderer {
    /// Create renderer from existing wgpu device (for integration with windowing).
    pub fn from_device(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            device,
            queue,
            surface: None,
            config: None,
            meshes: Vec::new(),
            materials: Vec::new(),
            material_layout: None,
        }
    }

    /// Create renderer with surface for windowing.
    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Self {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("kami-renderer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .expect("device request failed");

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
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Self {
            device,
            queue,
            surface: Some(surface),
            config: Some(config),
            meshes: Vec::new(),
            materials: Vec::new(),
            material_layout: None,
        }
    }

    /// Set the material bind group layout (must be called before create_material).
    pub fn set_material_layout(&mut self, layout: wgpu::BindGroupLayout) {
        self.material_layout = Some(layout);
    }

    /// Access the device for external resource creation.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Access the queue for buffer writes.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get mesh data for rendering.
    pub fn get_mesh(&self, handle: MeshHandle) -> Option<(&wgpu::Buffer, &wgpu::Buffer, u32)> {
        self.meshes
            .get(handle.0 as usize)
            .map(|m| (&m.vertex_buffer, &m.index_buffer, m.index_count))
    }

    /// Get material bind group for rendering.
    pub fn get_material_bind_group(&self, handle: MaterialHandle) -> Option<&wgpu::BindGroup> {
        self.materials.get(handle.0 as usize).map(|m| &m.bind_group)
    }
}

impl Renderer for WgpuRenderer {
    unsafe fn upload_mesh(
        &mut self,
        positions: &Column,
        _normals: &Column,
        _uvs: &Column,
        indices: &Column,
    ) -> MeshHandle {
        let handle = MeshHandle(self.meshes.len() as u32);

        let vertex_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-vertex"),
                contents: unsafe { positions.as_bytes() },
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let index_bytes = unsafe { indices.as_bytes() };
        let index_count = index_bytes.len() as u32 / 4; // u32 indices
        let index_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-index"),
                contents: index_bytes,
                usage: wgpu::BufferUsages::INDEX,
            });

        self.meshes.push(GpuMesh {
            vertex_buffer: vertex_buf,
            index_buffer: index_buf,
            index_count,
        });

        handle
    }

    fn upload_mesh_interleaved(&mut self, vertices: &[f32], indices: &[u32]) -> MeshHandle {
        let handle = MeshHandle(self.meshes.len() as u32);

        let vertex_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-vertex-interleaved"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let index_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-index-interleaved"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        self.meshes.push(GpuMesh {
            vertex_buffer: vertex_buf,
            index_buffer: index_buf,
            index_count: indices.len() as u32,
        });

        handle
    }

    unsafe fn update_vertex_buffer(&mut self, mesh: MeshHandle, positions: &Column) {
        let idx = mesh.0 as usize;
        if let Some(m) = self.meshes.get(idx) {
            self.queue
                .write_buffer(&m.vertex_buffer, 0, unsafe { positions.as_bytes() });
        }
    }

    fn create_material(&mut self, uniform: MaterialUniform) -> MaterialHandle {
        let handle = MaterialHandle(self.materials.len() as u32);

        let layout = self
            .material_layout
            .as_ref()
            .expect("material_layout must be set before create_material");

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("kami-material"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kami-material-bg"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        self.materials.push(GpuMaterial {
            _uniform_buffer: uniform_buffer,
            bind_group,
        });

        handle
    }

    fn draw(&mut self, _camera: &Camera, _commands: &[DrawCmd]) {
        // Multi-draw is handled externally via get_mesh() / get_material_bind_group()
        // in kami-web and kami-demo render loops.
    }

    fn present(&mut self) {
        if let Some(ref surface) = self.surface {
            if let Ok(frame) = surface.get_current_texture() {
                frame.present();
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }
}
