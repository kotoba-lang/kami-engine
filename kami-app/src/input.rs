//! Input modes and handler trait.
//!
//! Handlers install DOM listeners (web) or winit callbacks (native) and
//! each frame `poll(camera, dt)` translates accumulated events into
//! camera state (position / yaw / pitch). Games needing richer input
//! (gesture, gamepad) implement their own `InputHandler`.

use crate::camera::Camera;

#[derive(Debug, Clone)]
pub enum InputMode {
    None,
    WasdFps,
    OrbitMouse,
    SideScroll,
    GraphPan,
    Touch,
}

impl InputMode {
    /// Native / test fallback — no DOM bindings.
    pub fn into_handler(self) -> Box<dyn InputHandler> {
        Box::new(NullInput)
    }

    /// Web binding — receives the canvas so pointer-lock and mouse
    /// listeners target it correctly (keyboard stays on window).
    #[cfg(target_family = "wasm")]
    pub fn into_handler_web(self, canvas: &web_sys::HtmlCanvasElement) -> Box<dyn InputHandler> {
        match self {
            InputMode::WasdFps => Box::new(wasd::WasdFps::attach(canvas)),
            InputMode::OrbitMouse => Box::new(orbit::OrbitMouse::attach(canvas)),
            _ => Box::new(NullInput),
        }
    }
}

pub trait InputHandler {
    fn poll(&mut self, camera: &mut Camera, dt: f32);
}

pub struct NullInput;
impl InputHandler for NullInput {
    fn poll(&mut self, _camera: &mut Camera, _dt: f32) {}
}

#[cfg(target_family = "wasm")]
mod wasd {
    use super::{Camera, InputHandler};
    use glam::Vec3;
    use std::cell::Cell;
    use std::f32::consts::FRAC_PI_2;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    #[derive(Default)]
    struct State {
        // Keys
        w: Cell<bool>,
        a: Cell<bool>,
        s: Cell<bool>,
        d: Cell<bool>,
        space: Cell<bool>,
        shift: Cell<bool>,
        up: Cell<bool>,
        down: Cell<bool>,
        left: Cell<bool>,
        right: Cell<bool>,
        // Mouse accumulators (consumed each frame)
        mdx: Cell<f32>,
        mdy: Cell<f32>,
        locked: Cell<bool>,
        /// Edge-triggered left-click while locked. Set on mousedown,
        /// the InputHandler forwards it to Camera::action_edge each
        /// `poll()` and resets.
        action_pending: Cell<bool>,
        /// Right-click edge (Camera.action2_edge).
        action2_pending: Cell<bool>,
    }

    /// WASD first-person handler with pointer-lock mouse look.
    ///
    /// Controls
    ///   click canvas    request pointer lock
    ///   Escape          release lock (browser default)
    ///   W/S / ↑↓        forward / back (yaw-relative, horizontal)
    ///   A/D / ←→        strafe
    ///   Space / Shift   up / down (world vertical)
    ///   mouse move      yaw (horizontal) + pitch (vertical, clamped)
    pub struct WasdFps {
        state: Rc<State>,
        _closures: Vec<Closure<dyn FnMut(web_sys::Event)>>,
        _mouse_closure: Closure<dyn FnMut(web_sys::MouseEvent)>,
        _key_down: Closure<dyn FnMut(web_sys::KeyboardEvent)>,
        _key_up: Closure<dyn FnMut(web_sys::KeyboardEvent)>,
        _click: Closure<dyn FnMut(web_sys::MouseEvent)>,
    }

    impl WasdFps {
        pub fn attach(canvas: &web_sys::HtmlCanvasElement) -> Self {
            let window = web_sys::window().expect("no window");
            let document = window.document().expect("no document");
            let state = Rc::new(State::default());

            // ── keyboard ──
            let s_kd = state.clone();
            let key_down = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
                update_key(&s_kd, &e.code(), true);
            }) as Box<dyn FnMut(_)>);
            window
                .add_event_listener_with_callback("keydown", key_down.as_ref().unchecked_ref())
                .ok();

            let s_ku = state.clone();
            let key_up = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
                update_key(&s_ku, &e.code(), false);
            }) as Box<dyn FnMut(_)>);
            window
                .add_event_listener_with_callback("keyup", key_up.as_ref().unchecked_ref())
                .ok();

            // ── click → request pointer lock ──
            let canvas_cl = canvas.clone();
            let click = Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
                canvas_cl.request_pointer_lock();
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback("click", click.as_ref().unchecked_ref())
                .ok();

            // ── pointerlockchange (document) ──
            let s_lock = state.clone();
            let doc_cl = document.clone();
            let canvas_cl2 = canvas.clone();
            let lock_change = Closure::wrap(Box::new(move |_e: web_sys::Event| {
                let locked = doc_cl
                    .pointer_lock_element()
                    .map(|el| {
                        let target: &web_sys::Element = canvas_cl2.as_ref();
                        el.is_same_node(Some(target))
                    })
                    .unwrap_or(false);
                s_lock.locked.set(locked);
            }) as Box<dyn FnMut(_)>);
            document
                .add_event_listener_with_callback(
                    "pointerlockchange",
                    lock_change.as_ref().unchecked_ref(),
                )
                .ok();

            // ── mousedown (document; left button only, and only when
            // pointer is locked so an off-canvas click doesn't trigger
            // mining by accident) ──
            let s_md = state.clone();
            let mouse_down = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                if !s_md.locked.get() {
                    return;
                }
                match e.button() {
                    0 => s_md.action_pending.set(true),  // left = mine
                    2 => s_md.action2_pending.set(true), // right = place
                    _ => {}
                }
            }) as Box<dyn FnMut(_)>);
            document
                .add_event_listener_with_callback("mousedown", mouse_down.as_ref().unchecked_ref())
                .ok();
            std::mem::forget(mouse_down);

            // Block the right-click context menu globally while the app
            // is active — otherwise place-action summons the OS menu.
            let ctx_menu = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                e.prevent_default();
            }) as Box<dyn FnMut(_)>);
            document
                .add_event_listener_with_callback("contextmenu", ctx_menu.as_ref().unchecked_ref())
                .ok();
            std::mem::forget(ctx_menu);

            // ── mousemove (document; fires with movementX/Y while locked) ──
            let s_mm = state.clone();
            let mouse = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                if s_mm.locked.get() {
                    s_mm.mdx.set(s_mm.mdx.get() + e.movement_x() as f32);
                    s_mm.mdy.set(s_mm.mdy.get() + e.movement_y() as f32);
                }
            }) as Box<dyn FnMut(_)>);
            document
                .add_event_listener_with_callback("mousemove", mouse.as_ref().unchecked_ref())
                .ok();

            Self {
                state,
                _closures: vec![lock_change],
                _mouse_closure: mouse,
                _key_down: key_down,
                _key_up: key_up,
                _click: click,
            }
        }
    }

    fn update_key(s: &Rc<State>, code: &str, down: bool) {
        match code {
            "KeyW" => s.w.set(down),
            "KeyA" => s.a.set(down),
            "KeyS" => s.s.set(down),
            "KeyD" => s.d.set(down),
            "Space" => s.space.set(down),
            "ShiftLeft" | "ShiftRight" => s.shift.set(down),
            "ArrowUp" => s.up.set(down),
            "ArrowDown" => s.down.set(down),
            "ArrowLeft" => s.left.set(down),
            "ArrowRight" => s.right.set(down),
            _ => {}
        }
    }

    impl InputHandler for WasdFps {
        fn poll(&mut self, camera: &mut Camera, dt: f32) {
            const SPEED: f32 = 20.0;
            const SENSITIVITY: f32 = 0.0025;

            // Forward pending action (left-click edge).
            if self.state.action_pending.replace(false) {
                camera.action_edge = true;
            }
            if self.state.action2_pending.replace(false) {
                camera.action2_edge = true;
            }

            // Consume mouse deltas → yaw/pitch.
            let dx = self.state.mdx.replace(0.0);
            let dy = self.state.mdy.replace(0.0);
            if dx != 0.0 || dy != 0.0 {
                camera.yaw += dx * SENSITIVITY;
                camera.pitch =
                    (camera.pitch - dy * SENSITIVITY).clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
            }

            // Yaw-relative horizontal forward/right.
            let forward = Vec3::new(camera.yaw.sin(), 0.0, -camera.yaw.cos());
            let right = Vec3::new(camera.yaw.cos(), 0.0, camera.yaw.sin());
            let mut delta = Vec3::ZERO;
            if self.state.w.get() || self.state.up.get() {
                delta += forward;
            }
            if self.state.s.get() || self.state.down.get() {
                delta -= forward;
            }
            if self.state.d.get() || self.state.right.get() {
                delta += right;
            }
            if self.state.a.get() || self.state.left.get() {
                delta -= right;
            }
            // Space: fly-up in zero-gravity mode, edge-triggered jump
            // when grounded in gravity mode.
            if camera.gravity > 0.0 {
                if self.state.space.get() && camera.grounded {
                    camera.vel_y = camera.jump_impulse;
                    camera.grounded = false;
                }
            } else {
                if self.state.space.get() {
                    delta.y += 1.0;
                }
                if self.state.shift.get() {
                    delta.y -= 1.0;
                }
            }
            if delta != Vec3::ZERO {
                camera.move_world += delta.normalize() * SPEED * dt;
            }
        }
    }
}

#[cfg(target_family = "wasm")]
mod orbit {
    //! `OrbitMouse` — drag to orbit, wheel to zoom, click (without
    //! drag) to fire `action_edge` for picking. Tracks mouse NDC for
    //! `Camera::ray_from_ndc`.

    use super::{Camera, InputHandler};
    use std::cell::Cell;
    use std::f32::consts::FRAC_PI_2;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    #[derive(Default)]
    struct State {
        dx: Cell<f32>,
        dy: Cell<f32>,
        wheel: Cell<f32>,
        dragging: Cell<bool>,
        drag_distance: Cell<f32>,
        action_pending: Cell<bool>,
        action2_pending: Cell<bool>,
        ndc_x: Cell<f32>,
        ndc_y: Cell<f32>,
    }

    pub struct OrbitMouse {
        state: Rc<State>,
        _mouse_move: Closure<dyn FnMut(web_sys::MouseEvent)>,
        _mouse_down: Closure<dyn FnMut(web_sys::MouseEvent)>,
        _mouse_up: Closure<dyn FnMut(web_sys::MouseEvent)>,
        _context_menu: Closure<dyn FnMut(web_sys::MouseEvent)>,
        _wheel: Closure<dyn FnMut(web_sys::WheelEvent)>,
    }

    impl OrbitMouse {
        pub fn attach(canvas: &web_sys::HtmlCanvasElement) -> Self {
            let state = Rc::new(State::default());

            let to_ndc =
                |e: &web_sys::MouseEvent, canvas: &web_sys::HtmlCanvasElement| -> (f32, f32) {
                    let rect = canvas.get_bounding_client_rect();
                    let w = rect.width() as f32;
                    let h = rect.height() as f32;
                    if w <= 0.0 || h <= 0.0 {
                        return (0.0, 0.0);
                    }
                    let cx = (e.client_x() as f32) - rect.left() as f32;
                    let cy = (e.client_y() as f32) - rect.top() as f32;
                    let x = (cx / w) * 2.0 - 1.0;
                    let y = 1.0 - (cy / h) * 2.0;
                    (x, y)
                };

            // mousemove — track NDC always; accumulate drag delta when button held.
            let s_mm = state.clone();
            let canvas_mm = canvas.clone();
            let mouse_move = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                let (nx, ny) = to_ndc(&e, &canvas_mm);
                s_mm.ndc_x.set(nx);
                s_mm.ndc_y.set(ny);
                if s_mm.dragging.get() {
                    let dx = e.movement_x() as f32;
                    let dy = e.movement_y() as f32;
                    s_mm.dx.set(s_mm.dx.get() + dx);
                    s_mm.dy.set(s_mm.dy.get() + dy);
                    s_mm.drag_distance
                        .set(s_mm.drag_distance.get() + dx.abs() + dy.abs());
                }
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback("mousemove", mouse_move.as_ref().unchecked_ref())
                .ok();

            // mousedown — begin drag.
            let s_md = state.clone();
            let canvas_md = canvas.clone();
            let mouse_down = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                let (nx, ny) = to_ndc(&e, &canvas_md);
                s_md.ndc_x.set(nx);
                s_md.ndc_y.set(ny);
                if e.button() == 0 {
                    s_md.dragging.set(true);
                    s_md.drag_distance.set(0.0);
                }
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback("mousedown", mouse_down.as_ref().unchecked_ref())
                .ok();

            // mouseup (window) — end drag; if total movement < threshold, fire click.
            let s_mu = state.clone();
            let mouse_up = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                if e.button() == 0 && s_mu.dragging.get() {
                    s_mu.dragging.set(false);
                    if s_mu.drag_distance.get() < 6.0 {
                        s_mu.action_pending.set(true);
                    }
                }
                if e.button() == 2 {
                    s_mu.action2_pending.set(true);
                }
            }) as Box<dyn FnMut(_)>);
            let window = web_sys::window().expect("window");
            window
                .add_event_listener_with_callback("mouseup", mouse_up.as_ref().unchecked_ref())
                .ok();

            // contextmenu — suppress browser menu so right-click can fire action2.
            let context_menu = Closure::wrap(Box::new(|e: web_sys::MouseEvent| {
                e.prevent_default();
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback(
                    "contextmenu",
                    context_menu.as_ref().unchecked_ref(),
                )
                .ok();

            // wheel — zoom; suppress page scroll.
            let s_wh = state.clone();
            let wheel = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
                e.prevent_default();
                let dy = e.delta_y() as f32;
                s_wh.wheel.set(s_wh.wheel.get() + dy);
            }) as Box<dyn FnMut(_)>);
            canvas
                .add_event_listener_with_callback("wheel", wheel.as_ref().unchecked_ref())
                .ok();

            Self {
                state,
                _mouse_move: mouse_move,
                _mouse_down: mouse_down,
                _mouse_up: mouse_up,
                _context_menu: context_menu,
                _wheel: wheel,
            }
        }
    }

    impl InputHandler for OrbitMouse {
        fn poll(&mut self, camera: &mut Camera, _dt: f32) {
            const DRAG_SENSITIVITY: f32 = 0.006;
            let dx = self.state.dx.replace(0.0);
            let dy = self.state.dy.replace(0.0);
            if dx != 0.0 || dy != 0.0 {
                camera.yaw += dx * DRAG_SENSITIVITY;
                camera.pitch = (camera.pitch + dy * DRAG_SENSITIVITY)
                    .clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
            }
            let wheel = self.state.wheel.replace(0.0);
            if wheel != 0.0 {
                let scale = (wheel * 0.0015).exp();
                camera.orbit_distance = (camera.orbit_distance * scale).clamp(0.05, 5000.0);
            }
            camera.mouse_ndc_x = self.state.ndc_x.get();
            camera.mouse_ndc_y = self.state.ndc_y.get();
            if self.state.action_pending.replace(false) {
                camera.action_edge = true;
            }
            if self.state.action2_pending.replace(false) {
                camera.action2_edge = true;
            }
        }
    }
}
