//! kami-os: KAMI Engine OS compositor.
//!
//! Provides a wgpu-based desktop environment with window management,
//! taskbar, app launcher, notification system, and consent modal overlay.
//! Integrates with Magatama host-sdk for agent lifecycle and governance.
//!
//! ## Architecture
//!
//! - **ECS (hecs)**: Windows, notifications, taskbar items as entities
//! - **Compositor**: Z-ordered window rendering via kami-ui-gpu
//! - **Input Router**: Focus-aware dispatch to active window
//! - **Bridge**: kami-bridge for native OS input, kami-knp for device mesh

pub mod compositor;
pub mod file_explorer;
pub mod input_router;
pub mod launcher;
pub mod notification;
pub mod taskbar;
pub mod terminal;
pub mod window;

use hecs::World;
use kami_core::time::GameClock;

/// OS-level ECS world holding all desktop entities.
pub struct OsDesktop {
    /// hecs world containing Window, Notification, TaskbarItem entities.
    pub world: World,
    /// Fixed-timestep clock for compositor (30 fps default — UI, not game).
    pub clock: GameClock,
    /// Compositor state (z-order, focus, drag).
    pub compositor: compositor::CompositorState,
    /// Input router (focus tracking, pointer lock).
    pub input_router: input_router::InputRouterState,
    /// Taskbar state (agent list, budget display, clock).
    pub taskbar: taskbar::TaskbarState,
    /// App launcher state (grid, search filter).
    pub launcher: launcher::LauncherState,
    /// Notification queue (toast stack, consent modals).
    pub notifications: notification::NotificationQueue,
}

impl OsDesktop {
    /// Create a new OS desktop with default 30fps compositor clock.
    pub fn new() -> Self {
        Self {
            world: World::new(),
            clock: GameClock::new(30),
            compositor: compositor::CompositorState::new(),
            input_router: input_router::InputRouterState::new(),
            taskbar: taskbar::TaskbarState::new(),
            launcher: launcher::LauncherState::new(),
            notifications: notification::NotificationQueue::new(),
        }
    }

    /// Open a new window. Returns the window entity ID.
    pub fn open_window(&mut self, config: window::WindowConfig) -> u64 {
        let entity = self.world.spawn((
            window::WindowComponent::from_config(&config),
            window::WindowRect {
                x: config.x,
                y: config.y,
                w: config.w,
                h: config.h,
            },
        ));
        let id = entity.to_bits().into();
        self.compositor.bring_to_front(id);
        self.input_router.set_focus(id);
        id
    }

    /// Close a window by entity ID.
    pub fn close_window(&mut self, window_id: u64) {
        let entity = hecs::Entity::from_bits(window_id).unwrap();
        let _ = self.world.despawn(entity);
        self.compositor.remove(window_id);
        self.input_router.clear_focus_if(window_id);
    }

    /// Focus a window by entity ID.
    pub fn focus_window(&mut self, window_id: u64) {
        self.compositor.bring_to_front(window_id);
        self.input_router.set_focus(window_id);
    }

    /// Push a notification toast.
    pub fn show_notification(
        &mut self,
        title: String,
        body: String,
        level: notification::NotificationLevel,
    ) {
        self.notifications.push_toast(title, body, level, 5000);
    }

    /// Push a consent modal (blocks until user responds).
    pub fn show_consent_modal(&mut self, request: notification::ConsentRequest) -> u32 {
        self.notifications.push_consent(request)
    }

    /// Advance compositor by elapsed nanoseconds. Returns ticks simulated.
    pub fn advance(&mut self, elapsed_ns: u64) -> u32 {
        self.clock.advance(elapsed_ns)
    }
}

impl Default for OsDesktop {
    fn default() -> Self {
        Self::new()
    }
}
