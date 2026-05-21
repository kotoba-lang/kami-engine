//! Unified GPU bootstrap — single owner of wgpu `Instance` / `Adapter` / `Device`
//! selection across all kami entry points (kami-web, kami-map, kami-demo, app crates).
//!
//! Before this module existed, every WASM entry inlined its own ~40 LoC of
//! `Instance::new` + `request_adapter` + `request_device` + `SurfaceConfiguration`,
//! drifting on Backends and Limits (kami-web used `downlevel_webgl2_defaults`,
//! kami-map used `downlevel_defaults` — the latter silently fails on WebGL2).
//!
//! Owner: kami-render. Callers must not construct `wgpu::Instance` directly.
//!
//! # Backend policy
//!
//! | Target | Backends | Limits |
//! |---|---|---|
//! | `target_family = "wasm"` | `BROWSER_WEBGPU \| GL` (WebGPU preferred, WebGL2 fallback) | `downlevel_webgl2_defaults()` |
//! | native | `PRIMARY` (Vulkan / Metal / DX12) | `downlevel_defaults()` |
//!
//! `Limits::downlevel_webgl2_defaults()` is the WebGL2-compatible subset.
//! Using `downlevel_defaults()` on wasm triggers silent failure when the
//! runtime falls back from WebGPU to WebGL2.

use thiserror::Error;

/// Which concrete backend wgpu selected at runtime.
/// Exposed so callers can log / branch (e.g. disable compute pipelines on WebGL2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    WebGpu,
    WebGl2,
    Vulkan,
    Metal,
    Dx12,
    Other,
}

impl Backend {
    fn from_wgpu(b: wgpu::Backend) -> Self {
        match b {
            wgpu::Backend::BrowserWebGpu => Backend::WebGpu,
            wgpu::Backend::Gl => Backend::WebGl2,
            wgpu::Backend::Vulkan => Backend::Vulkan,
            wgpu::Backend::Metal => Backend::Metal,
            wgpu::Backend::Dx12 => Backend::Dx12,
            _ => Backend::Other,
        }
    }

    /// True when running in a browser. Useful for feature gating.
    pub fn is_web(self) -> bool {
        matches!(self, Backend::WebGpu | Backend::WebGl2)
    }
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("surface creation failed: {0}")]
    Surface(String),
    #[error("no GPU adapter (neither WebGPU nor WebGL2 available)")]
    NoAdapter,
    #[error("device request failed: {0}")]
    Device(String),
}

/// Everything a caller needs to draw: device/queue/surface/config + chosen backend.
///
/// `surface` is `'static` because wgpu stores the target internally; on web the
/// HTMLCanvasElement lives for the page lifetime, on native the window outlives
/// the renderer.
pub struct RenderContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub backend: Backend,
}

impl RenderContext {
    /// Bootstrap a renderer backed by a browser canvas.
    ///
    /// Caller constructs `wgpu::SurfaceTarget::Canvas(canvas)` from
    /// `web_sys::HtmlCanvasElement` (kami-render does not depend on web-sys
    /// to keep non-web builds slim).
    ///
    /// Forces `Backends::BROWSER_WEBGPU | Backends::GL` and
    /// `Limits::downlevel_webgl2_defaults()`. The latter is required: on
    /// WebGL2 path some `downlevel_defaults()` fields exceed WebGL2 caps
    /// and cause `request_device` to silently fall back or fail.
    #[cfg(target_family = "wasm")]
    pub async fn for_web_surface(
        target: wgpu::SurfaceTarget<'static>,
        width: u32,
        height: u32,
        label: &str,
    ) -> Result<Self, BootstrapError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance
            .create_surface(target)
            .map_err(|e| BootstrapError::Surface(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(BootstrapError::NoAdapter)?;
        let backend = Backend::from_wgpu(adapter.get_info().backend);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some(label),
                    required_features: wgpu::Features::empty(),
                    // WebGL2-safe subset; WebGPU path also accepts this.
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| BootstrapError::Device(e.to_string()))?;

        let (config, format) = make_surface_config(&surface, &adapter, width, height);
        surface.configure(&device, &config);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
            format,
            width,
            height,
            backend,
        })
    }

    /// Bootstrap for native (Vulkan / Metal / DX12). Uses `downlevel_defaults()`
    /// which is broader than the web subset.
    #[cfg(not(target_family = "wasm"))]
    pub async fn for_native_surface(
        target: wgpu::SurfaceTarget<'static>,
        width: u32,
        height: u32,
        label: &str,
    ) -> Result<Self, BootstrapError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance
            .create_surface(target)
            .map_err(|e| BootstrapError::Surface(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(BootstrapError::NoAdapter)?;
        let backend = Backend::from_wgpu(adapter.get_info().backend);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some(label),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| BootstrapError::Device(e.to_string()))?;

        let (config, format) = make_surface_config(&surface, &adapter, width, height);
        surface.configure(&device, &config);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
            format,
            width,
            height,
            backend,
        })
    }

    /// Resize + reconfigure surface. Callers should invoke this from a
    /// `ResizeObserver` handler on web, or winit resize event on native.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}

/// Headless render context — wgpu device/queue without a surface.
///
/// Used by server-side renderers (LangGraph pods, batch image jobs) that
/// produce textures, not framebuffers. Keeps the "kami-render is the sole
/// owner of `Instance::new`" invariant (ARCHITECTURE.md §1.GPU-bootstrap-policy)
/// while removing the surface requirement.
///
/// `format` defaults to `Rgba8UnormSrgb` so PNG-encoded readback is gamma-correct.
#[cfg(not(target_family = "wasm"))]
pub struct OffscreenContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub backend: Backend,
}

#[cfg(not(target_family = "wasm"))]
impl OffscreenContext {
    /// Bootstrap a native headless context (Vulkan / Metal / DX12 / GL).
    ///
    /// `format` should match whatever scene pipelines expect (`Rgba8UnormSrgb`
    /// is the canonical choice for PNG readback).
    ///
    /// macOS sidesteps `request_adapter().await` because the Metal backend
    /// hangs inside test threads when the async future cooperates with
    /// pollster (the CPU is idle, no progress — observed during P12). We
    /// use `enumerate_adapters` (sync) to pick the highest-performance
    /// Metal adapter without touching the async path. The runtime image
    /// (Linux pods) keeps the standard async path so the Vulkan side is
    /// unchanged.
    pub async fn for_offscreen(
        label: &str,
        format: wgpu::TextureFormat,
    ) -> Result<Self, BootstrapError> {
        let backends = if cfg!(target_os = "macos") {
            wgpu::Backends::METAL
        } else {
            wgpu::Backends::PRIMARY | wgpu::Backends::GL
        };
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        #[cfg(target_os = "macos")]
        let adapter = {
            // enumerate_adapters is sync — bypasses the Metal/pollster
            // deadlock that hangs `request_adapter().await` on test
            // threads. We pick the first DiscreteGpu / IntegratedGpu and
            // fall back to whatever Metal returned first.
            let mut adapters: Vec<wgpu::Adapter> =
                instance.enumerate_adapters(wgpu::Backends::METAL).into_iter().collect();
            if adapters.is_empty() {
                return Err(BootstrapError::NoAdapter);
            }
            adapters.sort_by_key(|a| match a.get_info().device_type {
                wgpu::DeviceType::DiscreteGpu => 0,
                wgpu::DeviceType::IntegratedGpu => 1,
                wgpu::DeviceType::Cpu => 3,
                wgpu::DeviceType::VirtualGpu => 2,
                wgpu::DeviceType::Other => 4,
            });
            adapters.into_iter().next().unwrap()
        };

        #[cfg(not(target_os = "macos"))]
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(BootstrapError::NoAdapter)?;
        let backend = Backend::from_wgpu(adapter.get_info().backend);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some(label),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| BootstrapError::Device(e.to_string()))?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            format,
            backend,
        })
    }
}

fn make_surface_config(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
    width: u32,
    height: u32,
) -> (wgpu::SurfaceConfiguration, wgpu::TextureFormat) {
    let caps = surface.get_capabilities(adapter);
    let format = caps
        .formats
        .iter()
        .find(|f| f.is_srgb())
        .copied()
        .unwrap_or(caps.formats[0]);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    (config, format)
}
