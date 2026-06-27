//! Live native window rendering the demo city via the data-driven executor.
//! `cargo run -p kami-webgpu-rs --example live --target aarch64-apple-darwin`
//!
//! This is the windowed counterpart of the headless golden frames: the same Renderer
//! draws into a wgpu surface each frame. It's the renderer kami-clj-play3d can adopt to
//! become data-driven. (Esc / close to quit.)

use kami_webgpu_rs::{Globals, Instance, Renderer, demo_city};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct App {
    scene: (Globals, Vec<Instance>),
    gpu: Option<Gpu>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(Window::default_attributes().with_title("kami-webgpu-rs — native"))
                .unwrap(),
        );
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).unwrap();
        let (device, queue, format) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .expect("no GPU adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("no device");
            let caps = surface.get_capabilities(&adapter);
            // prefer a non-sRGB format so the shader's manual gamma matches the offscreen path
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| !f.is_srgb())
                .unwrap_or(caps.formats[0]);
            (device, queue, format)
        });
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        let renderer = Renderer::new(device, queue, format, w, h);
        surface.configure(renderer.device(), &config);
        window.request_redraw();
        self.gpu = Some(Gpu {
            window,
            surface,
            config,
            renderer,
        });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                gpu.config.width = size.width.max(1);
                gpu.config.height = size.height.max(1);
                gpu.renderer.resize(gpu.config.width, gpu.config.height);
                gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                gpu.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Ok(frame) = gpu.surface.get_current_texture() {
                    let view = frame.texture.create_view(&Default::default());
                    let (g, insts) = &self.scene;
                    gpu.renderer.draw(&view, g, insts);
                    frame.present();
                    gpu.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let el = EventLoop::new().unwrap();
    let mut app = App {
        scene: demo_city(),
        gpu: None,
    };
    el.run_app(&mut app).unwrap();
}
