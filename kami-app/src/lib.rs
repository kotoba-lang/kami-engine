//! kami-app — Game SDK Builder for KAMI Engine.
//!
//! Composable API for assembling games from engine primitives. Each game
//! crate (kami-app-isekai, kami-app-quarry-walk, ...) consumes this API
//! and exposes its own wasm-bindgen entry point.
//!
//! ```ignore
//! use kami_app::{KamiApp, CameraMode, InputMode};
//!
//! #[wasm_bindgen]
//! pub async fn run_my_game(canvas_id: &str) -> Result<(), JsValue> {
//!     KamiApp::new_web(canvas_id).await?
//!         .with_camera(CameraMode::FirstPerson { spawn: [0.0, 2.0, 0.0].into() })
//!         .with_input(InputMode::WasdFps)
//!         .with_pipeline(my_voxel_pipeline())
//!         .on_update(|world, dt| { /* tick */ })
//!         .run()
//!         .await
//!         .map_err(|e| JsValue::from_str(&e.to_string()))
//! }
//! ```
//!
//! # Responsibility boundary
//!
//! | Layer | Owns | Must not own |
//! |---|---|---|
//! | `kami-app` | Builder, lifecycle (RAF + tick), trait definitions (Scene/RenderPipeline/InputHandler), ECS world | game-specific logic, web_sys glue (beyond new_web) |
//! | `kami-render` | GPU bootstrap (`RenderContext`), pipeline primitives | scene semantics, game loop |
//! | `kami-app-{game}` | scene parse, game tick, pipeline composition, wasm_bindgen export | engine primitives |
//! | `kami-web` | wasm-bindgen bridge, canvas acquisition helpers, panic hook, re-exports | `run_with_*` entries (legacy, being migrated) |

use glam::Vec3;
use hecs::World;
use kami_render::{BootstrapError, RenderContext};
use thiserror::Error;

pub mod camera;
pub mod depth;
pub mod input;
pub mod pipeline;
pub mod scene;

pub use camera::{Camera, CameraMode};
pub use depth::DepthTarget;
pub use input::{InputHandler, InputMode};
pub use pipeline::RenderPipeline;
pub use scene::Scene;

// Re-export kami-render essentials so game crates only need to depend on kami-app.
pub use kami_render::{Backend, RenderContext as GpuCtx};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("bootstrap: {0}")]
    Bootstrap(#[from] BootstrapError),
    #[error("missing canvas '{0}'")]
    Canvas(String),
    #[error("other: {0}")]
    Other(String),
}

/// Main app type. Build with `new_web` / `new_native`, chain `.with_*`,
/// finalize with `.run()`.
pub struct KamiApp {
    ctx: RenderContext,
    depth: DepthTarget,
    #[cfg(target_family = "wasm")]
    canvas: web_sys::HtmlCanvasElement,
    world: World,
    camera: Camera,
    input: Box<dyn InputHandler>,
    pipelines: Vec<Box<dyn RenderPipeline>>,
    tick_hooks: Vec<Box<dyn FnMut(&mut World, &mut Camera, f32)>>,
    label: String,
    // Moving-average FPS tracker (EMA). Updated each tick_once.
    fps_ema: f32,
    hud_publish: bool,
    /// Floor collision probe. Given world XZ (Y ignored) returns the
    /// solid-surface Y at that column, or `None` for no collision. The
    /// camera is clamped each tick to `max(camera.y, floor_y + eye_height)`.
    floor_probe: Option<std::rc::Rc<dyn Fn(glam::Vec3) -> Option<f32>>>,
    eye_height: f32,
    /// Wall collider probe. Returns `true` if the AABB `[min, max]`
    /// overlaps a solid region. `tick_once` uses it to axis-sweep the
    /// player AABB — each frame the pending `move_world` is split into
    /// X/Y/Z steps and rejected per-axis if the step would intersect.
    collider_probe: Option<std::rc::Rc<dyn Fn(glam::Vec3, glam::Vec3) -> bool>>,
    /// Half-extent of the player capsule in XZ (meters). Total body
    /// width = 2 × this. Default 0.35 (0.7m wide, ~shoulder width).
    player_radius: f32,
}

impl KamiApp {
    /// Bootstrap from a browser canvas id.
    ///
    /// Uses `kami_render::RenderContext::for_web_surface` which enforces
    /// the unified `Backends::BROWSER_WEBGPU | GL` +
    /// `Limits::downlevel_webgl2_defaults` policy.
    #[cfg(target_family = "wasm")]
    pub async fn new_web(canvas_id: &str) -> Result<Self, RuntimeError> {
        let window = web_sys::window().ok_or_else(|| RuntimeError::Other("no window".into()))?;
        let document = window
            .document()
            .ok_or_else(|| RuntimeError::Other("no document".into()))?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| RuntimeError::Canvas(canvas_id.into()))?;
        let canvas: web_sys::HtmlCanvasElement = canvas
            .dyn_into()
            .map_err(|_| RuntimeError::Other("not a canvas".into()))?;
        // DPR-aware drawing buffer. CSS size stays as author-specified;
        // drawing buffer scales to device pixels so WebGPU / WebGL2
        // render at native resolution.
        let dpr = window.device_pixel_ratio().max(1.0) as f32;
        let width = ((canvas.client_width() as f32 * dpr).max(1.0)) as u32;
        let height = ((canvas.client_height() as f32 * dpr).max(1.0)) as u32;
        canvas.set_width(width);
        canvas.set_height(height);

        let canvas_for_input = canvas.clone();
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let ctx = RenderContext::for_web_surface(target, width, height, "kami-app").await?;
        let depth = DepthTarget::new(&ctx.device, width, height);

        Ok(Self {
            ctx,
            depth,
            canvas: canvas_for_input,
            world: World::new(),
            camera: Camera::default_for(width as f32 / height as f32),
            input: Box::new(input::NullInput),
            pipelines: Vec::new(),
            tick_hooks: Vec::new(),
            label: "kami-app".into(),
            fps_ema: 60.0,
            hud_publish: false,
            floor_probe: None,
            eye_height: 1.8,
            collider_probe: None,
            player_radius: 0.35,
        })
    }

    /// Bootstrap from a pre-constructed `RenderContext` (native / winit or tests).
    #[cfg(not(target_family = "wasm"))]
    pub fn from_context(ctx: RenderContext) -> Self {
        let aspect = ctx.width as f32 / ctx.height as f32;
        let depth = DepthTarget::new(&ctx.device, ctx.width, ctx.height);
        Self {
            ctx,
            depth,
            world: World::new(),
            camera: Camera::default_for(aspect),
            input: Box::new(input::NullInput),
            pipelines: Vec::new(),
            tick_hooks: Vec::new(),
            label: "kami-app".into(),
            fps_ema: 60.0,
            hud_publish: false,
            floor_probe: None,
            eye_height: 1.8,
            collider_probe: None,
            player_radius: 0.35,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Enable per-frame HUD snapshot on `window.__kami_hud_{label}`.
    /// HTML can poll it via `setInterval` to display camera / FPS /
    /// backend without embedding shader logic on the JS side.
    pub fn with_hud_publish(mut self, enable: bool) -> Self {
        self.hud_publish = enable;
        self
    }

    /// Install a floor collision probe. The closure is called each frame
    /// with the camera's XZ position; if it returns `Some(surface_y)`,
    /// the camera's Y is clamped to `surface_y + eye_height` when it
    /// would otherwise drop below.
    ///
    /// ```ignore
    /// let voxels = VoxelChunkAdapter::streaming(...);
    /// let probe_handle = voxels.clone();
    /// app.with_floor_probe(move |p| probe_handle.sample_floor(p))
    /// ```
    pub fn with_floor_probe<F>(mut self, f: F) -> Self
    where
        F: Fn(glam::Vec3) -> Option<f32> + 'static,
    {
        self.floor_probe = Some(std::rc::Rc::new(f));
        self
    }

    /// Eye offset above the floor surface (meters). Default 1.8.
    pub fn with_eye_height(mut self, h: f32) -> Self {
        self.eye_height = h;
        self
    }

    /// Install a wall collider probe. `probe(min, max) -> bool` returns
    /// `true` if the axis-aligned box intersects a solid region. The
    /// app performs 3-axis sweep per tick so walking into a wall stops
    /// horizontal movement but preserves sliding along the free axis.
    pub fn with_collider_probe<F>(mut self, f: F) -> Self
    where
        F: Fn(glam::Vec3, glam::Vec3) -> bool + 'static,
    {
        self.collider_probe = Some(std::rc::Rc::new(f));
        self
    }

    pub fn with_player_radius(mut self, r: f32) -> Self {
        self.player_radius = r;
        self
    }

    /// Enable vertical gravity + jumping. Pass `9.8` for earth-like,
    /// `0.0` to disable (fly mode — default). Sets `Camera.gravity`.
    pub fn with_gravity(mut self, g: f32) -> Self {
        self.camera.gravity = g;
        self
    }

    pub fn with_jump_impulse(mut self, j: f32) -> Self {
        self.camera.jump_impulse = j;
        self
    }

    /// Build scene entities into the ECS world.
    pub fn with_scene<S: Scene + 'static>(mut self, s: S) -> Self {
        s.build(&mut self.world);
        self
    }

    /// Configure camera mode. See `CameraMode` variants for FPS / Orbit / Ortho.
    pub fn with_camera(mut self, mode: CameraMode) -> Self {
        self.camera.configure(mode);
        self
    }

    /// Configure input handler. On web, the stored canvas is used for
    /// pointer-lock + mouse events; keyboard listens on window.
    pub fn with_input(mut self, mode: InputMode) -> Self {
        #[cfg(target_family = "wasm")]
        {
            self.input = mode.into_handler_web(&self.canvas);
        }
        #[cfg(not(target_family = "wasm"))]
        {
            self.input = mode.into_handler();
        }
        self
    }

    /// Register a render pipeline. Called in order during each frame.
    pub fn with_pipeline<P: RenderPipeline + 'static>(mut self, p: P) -> Self {
        self.pipelines.push(Box::new(p));
        self
    }

    /// Register a game tick hook. Multiple hooks run sequentially each frame.
    pub fn on_update<F: FnMut(&mut World, &mut Camera, f32) + 'static>(mut self, f: F) -> Self {
        self.tick_hooks.push(Box::new(f));
        self
    }

    /// Access ECS world for direct entity manipulation before `run()`.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn backend(&self) -> Backend {
        self.ctx.backend
    }

    /// Access the underlying `RenderContext`. Pipelines need this to
    /// build their wgpu `RenderPipeline` + bind groups at construction
    /// time (e.g. `SkyAdapter::new(app.render_context())`).
    pub fn render_context(&self) -> &RenderContext {
        &self.ctx
    }

    /// Run the main loop. On web, drives `requestAnimationFrame`. On native,
    /// blocks until the window closes.
    ///
    /// Each frame:
    ///   1. poll input
    ///   2. run tick hooks (game logic)
    ///   3. update camera uniform
    ///   4. for each pipeline: prepare + record into command encoder
    ///   5. submit + present
    #[cfg(target_family = "wasm")]
    pub async fn run(mut self) -> Result<(), RuntimeError> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        log::info!(
            "[{label}] running on backend={:?}",
            self.ctx.backend,
            label = self.label
        );

        let window = web_sys::window().ok_or_else(|| RuntimeError::Other("no window".into()))?;
        let perf = window
            .performance()
            .ok_or_else(|| RuntimeError::Other("no performance".into()))?;

        // Prepare pipelines once
        for p in self.pipelines.iter_mut() {
            p.prepare(&self.ctx, &self.camera, &self.world);
        }

        // Self-referential rAF closure
        let state = std::rc::Rc::new(std::cell::RefCell::new(self));

        // ── window.resize → app.resize(dpr-scaled) ──
        {
            let state_rs = state.clone();
            let resize_cb = Closure::wrap(Box::new(move |_e: web_sys::Event| {
                let Some(w) = web_sys::window() else { return };
                let dpr = w.device_pixel_ratio().max(1.0) as f32;
                let Ok(app) = state_rs.try_borrow_mut() else {
                    return;
                };
                // `KamiApp::resize` needs mutable self; the canvas is
                // stored on the app itself, so we read client dims
                // through it.
                let cw = app.canvas.client_width() as f32 * dpr;
                let ch = app.canvas.client_height() as f32 * dpr;
                let nw = cw.max(1.0) as u32;
                let nh = ch.max(1.0) as u32;
                drop(app);
                if let Ok(mut app) = state_rs.try_borrow_mut() {
                    app.canvas.set_width(nw);
                    app.canvas.set_height(nh);
                    app.resize(nw, nh);
                }
            }) as Box<dyn FnMut(_)>);
            window
                .add_event_listener_with_callback("resize", resize_cb.as_ref().unchecked_ref())
                .ok();
            // Leak — lives for page lifetime.
            std::mem::forget(resize_cb);
        }

        let last_ts = std::rc::Rc::new(std::cell::RefCell::new(perf.now()));
        let cb: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let cb_clone = cb.clone();
        let window_inner = window.clone();

        *cb.borrow_mut() = Some(Closure::wrap(Box::new(move || {
            let now = web_sys::window()
                .and_then(|w| w.performance())
                .map(|p| p.now())
                .unwrap_or(0.0);
            let mut last = last_ts.borrow_mut();
            let dt = ((now - *last) as f32 / 1000.0).clamp(0.0, 0.1);
            *last = now;
            drop(last);

            if let Ok(mut app) = state.try_borrow_mut() {
                app.tick_once(dt);
            }

            if let Some(cb) = cb_clone.borrow().as_ref() {
                let _ = window_inner.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }) as Box<dyn FnMut()>));

        window
            .request_animation_frame(cb.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .map_err(|_| RuntimeError::Other("rAF failed".into()))?;

        // Keep closure alive for page lifetime
        std::mem::forget(cb);
        Ok(())
    }

    #[cfg(not(target_family = "wasm"))]
    pub async fn run(mut self) -> Result<(), RuntimeError> {
        // Minimal native driver for tests. Real native entry should use winit.
        for p in self.pipelines.iter_mut() {
            p.prepare(&self.ctx, &self.camera, &self.world);
        }
        self.tick_once(1.0 / 60.0);
        Ok(())
    }

    /// Single frame: poll input → tick game → render pipelines. Exposed
    /// for tests and embedded scenarios (React/Vue host that drives its own
    /// scheduler).
    pub fn tick_once(&mut self, dt: f32) {
        // EMA FPS: lerp 0.05 toward instantaneous 1/dt (skip div-by-zero).
        if dt > 1e-4 {
            let inst = (1.0 / dt).min(240.0);
            self.fps_ema += (inst - self.fps_ema) * 0.05;
        }
        self.input.poll(&mut self.camera, dt);
        for h in self.tick_hooks.iter_mut() {
            h(&mut self.world, &mut self.camera, dt);
        }

        // Gravity integration (before sweep so collider catches floor).
        // vel_y is capped to terminal velocity (-40 m/s) to avoid NaN
        // from missed clamps during long falls.
        if self.camera.gravity > 0.0 {
            self.camera.vel_y -= self.camera.gravity * dt;
            if self.camera.vel_y < -40.0 {
                self.camera.vel_y = -40.0;
            }
            self.camera.move_world.y += self.camera.vel_y * dt;
        }

        // 3-axis AABB sweep: if a collider probe is installed, drain
        // `camera.move_world` and apply it component-wise so sliding
        // along walls works (block single axis, keep the other two).
        if let Some(probe) = self.collider_probe.clone() {
            let mv = std::mem::replace(&mut self.camera.move_world, glam::Vec3::ZERO);
            if mv != glam::Vec3::ZERO {
                let r = self.player_radius;
                let eye = self.eye_height;
                let inner = self.camera.as_render_mut();
                let mut pos = inner.position;
                // Body AABB: [pos.x-r, pos.y-eye, pos.z-r] .. [pos.x+r, pos.y, pos.z+r]
                let try_axis = |pos: glam::Vec3, delta: glam::Vec3| -> glam::Vec3 {
                    let cand = pos + delta;
                    let min = glam::Vec3::new(cand.x - r, cand.y - eye, cand.z - r);
                    let max = glam::Vec3::new(cand.x + r, cand.y, cand.z + r);
                    if probe(min, max) { pos } else { cand }
                };
                pos = try_axis(pos, glam::Vec3::new(mv.x, 0.0, 0.0));
                pos = try_axis(pos, glam::Vec3::new(0.0, mv.y, 0.0));
                pos = try_axis(pos, glam::Vec3::new(0.0, 0.0, mv.z));
                let delta = pos - inner.position;
                inner.position = pos;
                inner.target += delta;
            }
        }

        self.camera.update(dt, &self.world);
        // Floor collision: after camera movement + target recalc, if
        // probe says there's a floor below the eye, lift the eye to
        // `floor + eye_height`. Keeps the player from clipping through
        // voxel ground without a full swept-AABB solver.
        if let Some(probe) = self.floor_probe.clone() {
            let gravity_on = self.camera.gravity > 0.0;
            let p = self.camera.as_render().uniform().position;
            let p = glam::Vec3::from_array(p);
            let clamp = probe(p);
            let inner = self.camera.as_render_mut();
            match clamp {
                Some(floor_y) => {
                    let min_y = floor_y + self.eye_height;
                    if p.y < min_y {
                        let dy = min_y - p.y;
                        inner.position.y = min_y;
                        inner.target.y += dy;
                        self.camera.vel_y = 0.0;
                        self.camera.grounded = true;
                    } else if gravity_on {
                        // Above the floor; grounded only if exactly at
                        // resting height (within a small epsilon).
                        self.camera.grounded = (p.y - min_y).abs() < 0.05;
                    }
                }
                None => {
                    if gravity_on {
                        self.camera.grounded = false;
                    }
                }
            }
        }
        #[cfg(target_family = "wasm")]
        {
            if self.hud_publish {
                self.publish_hud();
            }
        }

        let Ok(frame) = self.ctx.surface.get_current_texture() else {
            log::warn!("[{}] surface frame unavailable", self.label);
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kami-app.frame"),
            });

        // Defensive pre-clear: always run a safety clear pass before the
        // pipelines record. If any pipeline relies on `LoadOp::Clear` for
        // its first attachment (e.g. SkyAdapter clearing to sky-blue),
        // it will overwrite this. But if zero pipelines run, or if a
        // pipeline records with `LoadOp::Load` (preserving previous),
        // this safety net keeps the surface from staying as undefined
        // garbage / opaque grey on Chrome's default WebGPU swap chain.
        {
            let clear_color = if self.pipelines.is_empty() {
                wgpu::Color {
                    r: 0.05,
                    g: 0.06,
                    b: 0.10,
                    a: 1.0,
                }
            } else {
                wgpu::Color {
                    r: 0.55,
                    g: 0.70,
                    b: 0.86,
                    a: 1.0,
                } // sky blue fallback
            };
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kami-app.preclear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        if !self.pipelines.is_empty() {
            for p in self.pipelines.iter_mut() {
                p.prepare(&self.ctx, &self.camera, &self.world);
            }
            for p in self.pipelines.iter() {
                p.record(
                    &self.ctx,
                    &mut encoder,
                    &view,
                    self.depth.view(),
                    &self.camera,
                    &self.world,
                );
            }
        }

        self.ctx.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    /// Handle a viewport resize — forwards to `RenderContext::resize`,
    /// rebuilds the shared `DepthTarget`, and re-aspects the camera.
    pub fn resize(&mut self, w: u32, h: u32) {
        self.ctx.resize(w, h);
        self.depth.resize(&self.ctx.device, w, h);
        self.camera.set_aspect(w as f32 / h as f32);
    }

    /// Write a camera/FPS snapshot to `window.__kami_hud_{label}`.
    /// Called each frame when `hud_publish` is on.
    #[cfg(target_family = "wasm")]
    fn publish_hud(&self) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let u = self.camera.as_render().uniform();
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &"x".into(), &(u.position[0] as f64).into());
        let _ = js_sys::Reflect::set(&obj, &"y".into(), &(u.position[1] as f64).into());
        let _ = js_sys::Reflect::set(&obj, &"z".into(), &(u.position[2] as f64).into());
        let _ = js_sys::Reflect::set(&obj, &"yaw".into(), &(self.camera.yaw as f64).into());
        let _ = js_sys::Reflect::set(&obj, &"pitch".into(), &(self.camera.pitch as f64).into());
        let _ = js_sys::Reflect::set(&obj, &"fps".into(), &(self.fps_ema as f64).into());
        let _ = js_sys::Reflect::set(
            &obj,
            &"backend".into(),
            &format!("{:?}", self.ctx.backend).into(),
        );
        let key = format!("__kami_hud_{}", self.label);
        let _ = js_sys::Reflect::set(&window, &key.into(), &obj);
    }
}

#[cfg(target_family = "wasm")]
use wasm_bindgen::JsCast;

/// Convenience re-export for downstream game crates.
#[cfg(target_family = "wasm")]
pub use wasm_bindgen;

/// `Vec3`-like spawn position alias used by `CameraMode::FirstPerson`.
pub type Position = Vec3;
