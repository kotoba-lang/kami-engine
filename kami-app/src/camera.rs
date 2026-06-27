//! Camera modes and update logic.
//!
//! Thin layer over `kami_render::Camera` that adds mode-specific update
//! behavior (orbit rotation, FPS physics, ortho zoom).

use glam::{Mat4, Vec3};
use hecs::World;
use kami_render::Camera as RenderCamera;

/// High-level camera behavior selector. Each variant carries its own
/// config. Engine owns the concrete update implementation; game code
/// only picks a mode.
#[derive(Debug, Clone)]
pub enum CameraMode {
    /// First-person walker with yaw/pitch from mouse, WASD translation
    /// via the `InputHandler`.
    FirstPerson { spawn: Vec3, yaw: f32, pitch: f32 },
    /// Fixed target, orbiting around it (drag to rotate, scroll to zoom).
    Orbit {
        target: Vec3,
        distance: f32,
        yaw: f32,
        pitch: f32,
    },
    /// Side-scroller: camera follows an entity tagged `Player`, orthographic.
    SideScroll {
        follow_entity: Option<hecs::Entity>,
        height: f32,
    },
    /// 2D orthographic (graph / map). Pan + zoom handled by input.
    Ortho2D { center: Vec3, extent: f32 },
}

impl Default for CameraMode {
    fn default() -> Self {
        CameraMode::Orbit {
            target: Vec3::ZERO,
            distance: 8.0,
            yaw: 0.0,
            pitch: 0.3,
        }
    }
}

pub struct Camera {
    inner: RenderCamera,
    mode: CameraMode,
    aspect: f32,
    /// FPS yaw (radians) around Y axis. 0 = looking toward -Z.
    pub yaw: f32,
    /// FPS pitch (radians), clamped ±π/2 - epsilon.
    pub pitch: f32,
    /// FPS translation accumulator (set by `InputHandler::poll`).
    pub move_world: Vec3,
    /// Accumulated wall seconds since the app started. Pipelines read
    /// this for time-driven effects (day/night, wind, water waves,
    /// animated shaders). Incremented by `dt` each `update()` call.
    pub time: f32,
    /// Edge-triggered primary action (left-click / "action" button).
    /// `InputHandler` sets this to `true` on press; game tick hook
    /// consumes via `consume_action()` which resets it. Used for
    /// mining / interact / select without game code having to bind DOM
    /// events itself.
    pub action_edge: bool,
    /// Edge-triggered secondary action (right-click / "place" button).
    /// Same consume pattern as `action_edge`.
    pub action2_edge: bool,
    /// Gravity magnitude (m/s²). Default 0 = fly mode (Space/Shift
    /// continuous vertical). `> 0` enables vertical velocity
    /// integration + Space-triggered jump-when-grounded.
    pub gravity: f32,
    /// Current vertical velocity (m/s). Integrated by gravity,
    /// zeroed by floor clamp, bumped by jump.
    pub vel_y: f32,
    /// Set `true` by `KamiApp` floor clamp when the player is resting
    /// on a surface. Cleared when vel_y takes the player above it.
    pub grounded: bool,
    /// Jump impulse (m/s) on grounded Space press. Default 5.5 gives
    /// roughly 1.5 m height at gravity=9.8.
    pub jump_impulse: f32,
    /// Orbit-mode distance from target. Input handlers (e.g.
    /// `OrbitMouse`) mutate this on scroll; `update()` reapplies it
    /// when the camera mode is `Orbit`. Initialised from the `Orbit`
    /// variant's `distance` by `configure()`.
    pub orbit_distance: f32,
    /// Latest mouse position in normalised device coords (x ∈ [-1,1]
    /// right-positive, y ∈ [-1,1] up-positive). Updated by web input
    /// handlers. Used by `ray_from_ndc` to build a world-space pick
    /// ray through the current cursor location.
    pub mouse_ndc_x: f32,
    pub mouse_ndc_y: f32,
}

impl Camera {
    pub fn default_for(aspect: f32) -> Self {
        Self {
            inner: RenderCamera::new(aspect),
            mode: CameraMode::default(),
            aspect,
            yaw: 0.0,
            pitch: 0.0,
            move_world: Vec3::ZERO,
            time: 0.0,
            action_edge: false,
            action2_edge: false,
            gravity: 0.0,
            vel_y: 0.0,
            grounded: false,
            jump_impulse: 5.5,
            orbit_distance: 8.0,
            mouse_ndc_x: 0.0,
            mouse_ndc_y: 0.0,
        }
    }

    /// Build a world-space pick ray from normalised device coords
    /// `(ndc_x, ndc_y)` in `[-1, 1]`. Returns `(origin, dir)` where
    /// `dir` is unit. Uses the current view + projection matrices via
    /// `inverse(proj * view)` transforming NDC near → far plane points.
    pub fn ray_from_ndc(&self, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
        let u = self.inner.uniform();
        let view = glam::Mat4::from_cols_array_2d(&u.view);
        let proj = glam::Mat4::from_cols_array_2d(&u.projection);
        let inv = (proj * view).inverse();
        // WebGPU NDC depth range is [0, 1]; 0 = near, 1 = far.
        let near = inv * glam::Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = inv * glam::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let near_w = Vec3::new(near.x / near.w, near.y / near.w, near.z / near.w);
        let far_w = Vec3::new(far.x / far.w, far.y / far.w, far.z / far.w);
        let dir = (far_w - near_w).normalize_or_zero();
        (near_w, dir)
    }

    /// Returns `true` once per action press. Clears the edge flag so a
    /// held button still produces exactly one trigger per press.
    pub fn consume_action(&mut self) -> bool {
        let v = self.action_edge;
        self.action_edge = false;
        v
    }

    pub fn consume_action2(&mut self) -> bool {
        let v = self.action2_edge;
        self.action2_edge = false;
        v
    }

    /// Camera eye position (world).
    pub fn eye(&self) -> Vec3 {
        Vec3::from_array(self.inner.uniform().position)
    }

    /// Forward unit vector (yaw-relative, pitch-aware).
    pub fn forward(&self) -> Vec3 {
        let cp = self.pitch.cos();
        Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), -cp * self.yaw.cos())
    }

    pub fn configure(&mut self, mode: CameraMode) {
        match &mode {
            CameraMode::FirstPerson { spawn, yaw, pitch } => {
                self.yaw = *yaw;
                self.pitch = *pitch;
                self.inner.position = *spawn;
                self.update_first_person_target();
            }
            CameraMode::Orbit {
                target,
                distance,
                yaw,
                pitch,
            } => {
                self.yaw = *yaw;
                self.pitch = *pitch;
                self.inner.target = *target;
                self.orbit_distance = *distance;
                self.update_orbit_position(*distance);
            }
            CameraMode::SideScroll { height, .. } => {
                self.inner.position = Vec3::new(0.0, *height, 10.0);
                self.inner.target = Vec3::new(0.0, *height, 0.0);
            }
            CameraMode::Ortho2D { center, .. } => {
                self.inner.position = *center + Vec3::new(0.0, 0.0, 10.0);
                self.inner.target = *center;
            }
        }
        self.mode = mode;
    }

    fn update_first_person_target(&mut self) {
        let cp = self.pitch.cos();
        let forward = Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), -cp * self.yaw.cos());
        self.inner.target = self.inner.position + forward;
    }

    fn update_orbit_position(&mut self, distance: f32) {
        let cp = self.pitch.cos();
        let offset =
            Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), cp * self.yaw.cos()) * distance;
        self.inner.position = self.inner.target + offset;
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
        let pos = Vec3::from_array(self.inner.uniform().position);
        let target = self.inner.target;
        let mut cam = RenderCamera::new(aspect);
        cam.position = pos;
        cam.target = target;
        self.inner = cam;
    }

    /// Per-frame update. Reads accumulated translation from
    /// `self.move_world` (written by `InputHandler::poll`) and applies
    /// it to `FirstPerson` or `Orbit` cameras. `move_world` is zeroed
    /// after consumption.
    pub fn update(&mut self, dt: f32, world: &World) {
        self.time += dt;
        match self.mode.clone() {
            CameraMode::FirstPerson { .. } => {
                if self.move_world != Vec3::ZERO {
                    self.inner.position += self.move_world;
                }
                self.update_first_person_target();
            }
            CameraMode::Orbit { .. } => {
                if self.move_world != Vec3::ZERO {
                    self.inner.target += self.move_world;
                }
                if self.orbit_distance <= 0.0 {
                    if let CameraMode::Orbit { distance, .. } = self.mode {
                        self.orbit_distance = distance;
                    }
                }
                self.update_orbit_position(self.orbit_distance);
            }
            CameraMode::SideScroll {
                follow_entity: Some(e),
                height,
            } => {
                if let Ok(tr) = world.get::<&Transform>(e) {
                    self.inner.position = Vec3::new(tr.position.x, height, 10.0);
                    self.inner.target = Vec3::new(tr.position.x, height, 0.0);
                }
            }
            _ => {}
        }
        self.move_world = Vec3::ZERO;
    }

    pub fn as_render(&self) -> &RenderCamera {
        &self.inner
    }

    pub fn as_render_mut(&mut self) -> &mut RenderCamera {
        &mut self.inner
    }

    pub fn view_projection(&self) -> Mat4 {
        let u = self.inner.uniform();
        Mat4::from_cols_array_2d(&u.projection) * Mat4::from_cols_array_2d(&u.view)
    }
}

/// Minimal `Transform` component for SideScroll follow lookup. Games can
/// define their own Transform; this type is only used by the default
/// camera follower.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: glam::Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}
