//! macOS input bridge via Core Graphics event tap.
//!
//! Uses `CGEventTapCreate` for global mouse/keyboard capture,
//! `CGEventPost` for synthetic injection, and `CGGetActiveDisplayList`
//! for screen geometry. Requires Input Monitoring permission in
//! System Settings > Privacy & Security.

use crate::{BridgeError, BridgeEvent, InputBridge, ScreenGeometry};
use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
};
use core_graphics::event_source::CGEventSource;
use core_graphics::event_source::CGEventSourceStateID;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

// Raw FFI for CGEventTap (the `core-graphics` crate marks these private).
mod ffi {
    use std::ffi::c_void;

    pub type CGEventTapCallBack = unsafe extern "C" fn(
        proxy: *mut c_void,
        event_type: u32,
        event: *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void;

    unsafe extern "C" {
        pub fn CGEventTapCreate(
            tap: u32,     // CGEventTapLocation
            place: u32,   // CGEventTapPlacement
            options: u32, // CGEventTapOptions
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> *mut c_void; // CFMachPortRef

        pub fn CGEventTapEnable(tap: *mut c_void, enable: bool);

        pub fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: *mut c_void,
            order: i64,
        ) -> *mut c_void; // CFRunLoopSourceRef

        pub fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);

        pub fn CFRunLoopGetCurrent() -> *mut c_void;
        pub fn CFRunLoopRun();

        pub fn CGEventGetFlags(event: *mut c_void) -> u64;
        pub fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;

        pub fn CGAssociateMouseAndMouseCursorPosition(connected: i32) -> i32;
        pub fn CGDisplayHideCursor(display: u32) -> i32;
        pub fn CGDisplayShowCursor(display: u32) -> i32;

        pub fn CGEventCreateScrollWheelEvent(
            source: *mut c_void,
            units: u32,
            wheel_count: u32,
            wheel1: i32,
            wheel2: i32,
        ) -> *mut c_void;

        pub fn CGEventPost(tap: u32, event: *mut c_void);

        pub fn kCFRunLoopCommonModes() -> *const c_void;
    }
}

/// CGEventTapLocation constants.
const K_CG_HID_EVENT_TAP: u32 = 0;
/// CGEventTapPlacement constants.
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
/// CGEventTapOptions constants.
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

/// CGEventField constants.
const K_CG_MOUSE_EVENT_DELTA_X: u32 = 87;
const K_CG_MOUSE_EVENT_DELTA_Y: u32 = 88;
const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1: u32 = 13;
const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_2: u32 = 14;
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

/// CGEventType constants as u32.
const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
const K_CG_EVENT_OTHER_MOUSE_DRAGGED: u32 = 27;
const K_CG_EVENT_SCROLL_WHEEL: u32 = 22;
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_KEY_UP: u32 = 11;

/// CGEventFlags bit positions.
const K_CG_EVENT_FLAG_SHIFT: u64 = 0x00020000;
const K_CG_EVENT_FLAG_CONTROL: u64 = 0x00040000;
const K_CG_EVENT_FLAG_ALTERNATE: u64 = 0x00080000;
const K_CG_EVENT_FLAG_COMMAND: u64 = 0x00100000;

/// macOS implementation of [`InputBridge`].
pub struct MacOSBridge {
    capturing: Arc<AtomicBool>,
    suppressed: Arc<AtomicBool>,
}

impl MacOSBridge {
    pub fn new() -> Self {
        Self {
            capturing: Arc::new(AtomicBool::new(false)),
            suppressed: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Extract modifier bitmask from raw CGEventFlags.
fn flags_to_modifiers(flags: u64) -> u8 {
    let mut m = 0u8;
    if flags & K_CG_EVENT_FLAG_SHIFT != 0 {
        m |= crate::modifiers::SHIFT;
    }
    if flags & K_CG_EVENT_FLAG_CONTROL != 0 {
        m |= crate::modifiers::CTRL;
    }
    if flags & K_CG_EVENT_FLAG_ALTERNATE != 0 {
        m |= crate::modifiers::ALT;
    }
    if flags & K_CG_EVENT_FLAG_COMMAND != 0 {
        m |= crate::modifiers::META;
    }
    m
}

/// Raw CGEventTap callback.
unsafe extern "C" fn tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void {
    let sender = unsafe { &*(user_info as *const Sender<BridgeEvent>) };
    let flags = unsafe { ffi::CGEventGetFlags(event) };
    let mods = flags_to_modifiers(flags);

    let bridge_event = match event_type {
        K_CG_EVENT_MOUSE_MOVED | K_CG_EVENT_OTHER_MOUSE_DRAGGED => {
            let dx =
                unsafe { ffi::CGEventGetIntegerValueField(event, K_CG_MOUSE_EVENT_DELTA_X) } as f64;
            let dy =
                unsafe { ffi::CGEventGetIntegerValueField(event, K_CG_MOUSE_EVENT_DELTA_Y) } as f64;
            Some(BridgeEvent::MouseMove { dx, dy })
        }
        K_CG_EVENT_LEFT_MOUSE_DOWN => Some(BridgeEvent::MouseButton {
            button: 0,
            pressed: true,
        }),
        K_CG_EVENT_LEFT_MOUSE_UP => Some(BridgeEvent::MouseButton {
            button: 0,
            pressed: false,
        }),
        K_CG_EVENT_RIGHT_MOUSE_DOWN => Some(BridgeEvent::MouseButton {
            button: 1,
            pressed: true,
        }),
        K_CG_EVENT_RIGHT_MOUSE_UP => Some(BridgeEvent::MouseButton {
            button: 1,
            pressed: false,
        }),
        K_CG_EVENT_SCROLL_WHEEL => {
            let dy = unsafe {
                ffi::CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1)
            } as f64;
            let dx = unsafe {
                ffi::CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_2)
            } as f64;
            Some(BridgeEvent::Scroll { dx, dy })
        }
        K_CG_EVENT_KEY_DOWN => {
            let keycode =
                unsafe { ffi::CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) }
                    as u32;
            Some(BridgeEvent::KeyDown {
                keycode,
                modifiers: mods,
            })
        }
        K_CG_EVENT_KEY_UP => {
            let keycode =
                unsafe { ffi::CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) }
                    as u32;
            Some(BridgeEvent::KeyUp {
                keycode,
                modifiers: mods,
            })
        }
        _ => None,
    };

    if let Some(ev) = bridge_event {
        let _ = sender.send(ev);
    }

    // Return the event to pass through (not swallow)
    event
}

impl InputBridge for MacOSBridge {
    fn start_capture(&self) -> Result<Receiver<BridgeEvent>, BridgeError> {
        if self.capturing.swap(true, Ordering::SeqCst) {
            return Err(BridgeError::AlreadyCapturing);
        }

        let (tx, rx) = mpsc::channel();
        let tx_box = Box::new(tx);
        // Store as usize to avoid *mut c_void Send issue across thread boundary.
        let tx_addr = Box::into_raw(tx_box) as usize;

        let event_mask: u64 = (1 << K_CG_EVENT_MOUSE_MOVED)
            | (1 << K_CG_EVENT_LEFT_MOUSE_DOWN)
            | (1 << K_CG_EVENT_LEFT_MOUSE_UP)
            | (1 << K_CG_EVENT_RIGHT_MOUSE_DOWN)
            | (1 << K_CG_EVENT_RIGHT_MOUSE_UP)
            | (1 << K_CG_EVENT_OTHER_MOUSE_DRAGGED)
            | (1 << K_CG_EVENT_SCROLL_WHEEL)
            | (1 << K_CG_EVENT_KEY_DOWN)
            | (1 << K_CG_EVENT_KEY_UP);

        std::thread::spawn(move || unsafe {
            let tx_ptr = tx_addr as *mut c_void;
            let tap = ffi::CGEventTapCreate(
                K_CG_HID_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                event_mask,
                tap_callback,
                tx_ptr,
            );

            if tap.is_null() {
                log::error!("CGEventTapCreate failed — is Input Monitoring enabled?");
                let _ = Box::from_raw(tx_ptr as *mut Sender<BridgeEvent>);
                return;
            }

            let source = ffi::CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let run_loop = ffi::CFRunLoopGetCurrent();

            // kCFRunLoopCommonModes
            let mode = core_foundation::runloop::kCFRunLoopCommonModes;
            ffi::CFRunLoopAddSource(run_loop, source, mode as *const _ as *const c_void);
            ffi::CGEventTapEnable(tap, true);
            ffi::CFRunLoopRun();

            let _ = Box::from_raw(tx_ptr as *mut Sender<BridgeEvent>);
        });

        Ok(rx)
    }

    fn inject(&self, event: &BridgeEvent) -> Result<(), BridgeError> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| BridgeError::OsError("CGEventSource creation failed".into()))?;

        match event {
            BridgeEvent::MouseMove { dx, dy } => {
                let cg = CGEvent::new_mouse_event(
                    source,
                    CGEventType::MouseMoved,
                    CGPoint::new(0.0, 0.0),
                    CGMouseButton::Left,
                )
                .map_err(|_| BridgeError::OsError("CGEvent mouse creation failed".into()))?;
                cg.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, *dx as i64);
                cg.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, *dy as i64);
                cg.post(CGEventTapLocation::HID);
            }
            BridgeEvent::MouseWarp { x, y } => {
                CGDisplay::warp_mouse_cursor_position(CGPoint::new(*x, *y))
                    .map_err(|_| BridgeError::OsError("CGWarpMouseCursorPosition failed".into()))?;
            }
            BridgeEvent::MouseButton { button, pressed } => {
                let (event_type, btn) = match (button, pressed) {
                    (0, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
                    (0, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
                    (1, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
                    (1, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
                    _ => return Ok(()),
                };
                // Get current cursor position for mouse button events
                let cg = CGEvent::new_mouse_event(
                    source,
                    event_type,
                    CGPoint::new(0.0, 0.0), // actual position set by OS
                    btn,
                )
                .map_err(|_| BridgeError::OsError("CGEvent mouse btn failed".into()))?;
                cg.post(CGEventTapLocation::HID);
            }
            BridgeEvent::Scroll { dx, dy } => {
                // Use raw FFI for scroll events (CGEvent wrapper lacks this)
                let scroll_event = unsafe {
                    ffi::CGEventCreateScrollWheelEvent(
                        std::ptr::null_mut(), // source
                        1,                    // kCGScrollEventUnitLine
                        2,                    // wheel count
                        *dy as i32,
                        *dx as i32,
                    )
                };
                if !scroll_event.is_null() {
                    unsafe {
                        ffi::CGEventPost(K_CG_HID_EVENT_TAP, scroll_event);
                        core_foundation::base::CFRelease(scroll_event as *const _);
                    }
                }
            }
            BridgeEvent::KeyDown { keycode, modifiers } => {
                let cg = CGEvent::new_keyboard_event(source, *keycode as u16, true)
                    .map_err(|_| BridgeError::OsError("CGEvent key failed".into()))?;
                cg.set_flags(modifiers_to_flags(*modifiers));
                cg.post(CGEventTapLocation::HID);
            }
            BridgeEvent::KeyUp { keycode, modifiers } => {
                let cg = CGEvent::new_keyboard_event(source, *keycode as u16, false)
                    .map_err(|_| BridgeError::OsError("CGEvent key failed".into()))?;
                cg.set_flags(modifiers_to_flags(*modifiers));
                cg.post(CGEventTapLocation::HID);
            }
        }
        Ok(())
    }

    fn screens(&self) -> Result<Vec<ScreenGeometry>, BridgeError> {
        let max_displays = 16u32;
        let mut display_ids = vec![0u32; max_displays as usize];
        let mut count = 0u32;

        let err = unsafe {
            core_graphics::display::CGGetActiveDisplayList(
                max_displays,
                display_ids.as_mut_ptr(),
                &mut count,
            )
        };
        if err != 0 {
            return Err(BridgeError::OsError(format!(
                "CGGetActiveDisplayList error {err}"
            )));
        }

        let mut screens = Vec::with_capacity(count as usize);
        for &did in &display_ids[..count as usize] {
            let bounds = CGDisplay::new(did).bounds();
            screens.push(ScreenGeometry {
                id: did,
                x: bounds.origin.x as i32,
                y: bounds.origin.y as i32,
                width: bounds.size.width as u32,
                height: bounds.size.height as u32,
                scale_factor: CGDisplay::new(did).pixels_wide() as f64 / bounds.size.width,
            });
        }
        Ok(screens)
    }

    fn warp_cursor(&self, x: f64, y: f64) -> Result<(), BridgeError> {
        CGDisplay::warp_mouse_cursor_position(CGPoint::new(x, y))
            .map_err(|_| BridgeError::OsError("CGWarpMouseCursorPosition failed".into()))?;
        Ok(())
    }

    fn suppress_local(&self) -> Result<(), BridgeError> {
        self.suppressed.store(true, Ordering::SeqCst);
        let err = unsafe { ffi::CGAssociateMouseAndMouseCursorPosition(0) };
        if err != 0 {
            return Err(BridgeError::OsError(
                "CGAssociateMouseAndMouseCursorPosition(false) failed".into(),
            ));
        }
        unsafe {
            ffi::CGDisplayHideCursor(CGDisplay::main().id);
        }
        Ok(())
    }

    fn resume_local(&self) -> Result<(), BridgeError> {
        self.suppressed.store(false, Ordering::SeqCst);
        let err = unsafe { ffi::CGAssociateMouseAndMouseCursorPosition(1) };
        if err != 0 {
            return Err(BridgeError::OsError(
                "CGAssociateMouseAndMouseCursorPosition(true) failed".into(),
            ));
        }
        unsafe {
            ffi::CGDisplayShowCursor(CGDisplay::main().id);
        }
        Ok(())
    }
}

/// Convert modifier bitmask to CGEventFlags.
fn modifiers_to_flags(mods: u8) -> CGEventFlags {
    let mut flags = CGEventFlags::empty();
    if mods & crate::modifiers::SHIFT != 0 {
        flags |= CGEventFlags::CGEventFlagShift;
    }
    if mods & crate::modifiers::CTRL != 0 {
        flags |= CGEventFlags::CGEventFlagControl;
    }
    if mods & crate::modifiers::ALT != 0 {
        flags |= CGEventFlags::CGEventFlagAlternate;
    }
    if mods & crate::modifiers::META != 0 {
        flags |= CGEventFlags::CGEventFlagCommand;
    }
    flags
}
