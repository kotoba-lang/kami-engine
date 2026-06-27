//! Windows input bridge via Win32 low-level hooks.
//!
//! Uses `SetWindowsHookExW` with `WH_MOUSE_LL` / `WH_KEYBOARD_LL` for global capture,
//! `SendInput` for synthetic injection, and `EnumDisplayMonitors` for screen geometry.

use crate::{BridgeError, BridgeEvent, InputBridge, ScreenGeometry};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use windows::Win32::Foundation::{BOOL, HINSTANCE, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    SetCursorPos, SetWindowsHookExW, ShowCursor, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

/// Thread-local sender for hook callbacks.
thread_local! {
    static HOOK_SENDER: std::cell::RefCell<Option<Sender<BridgeEvent>>> = const { std::cell::RefCell::new(None) };
    static LAST_MOUSE_POS: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((0, 0)) };
}

/// Windows implementation of [`InputBridge`].
pub struct WindowsBridge {
    capturing: Arc<AtomicBool>,
    suppressed: Arc<AtomicBool>,
}

impl WindowsBridge {
    pub fn new() -> Self {
        Self {
            capturing: Arc::new(AtomicBool::new(false)),
            suppressed: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Low-level mouse hook callback.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let (lx, ly) = LAST_MOUSE_POS.get();

        let event = match wparam.0 as u32 {
            WM_MOUSEMOVE => {
                let dx = (info.pt.x - lx) as f64;
                let dy = (info.pt.y - ly) as f64;
                LAST_MOUSE_POS.set((info.pt.x, info.pt.y));
                Some(BridgeEvent::MouseMove { dx, dy })
            }
            WM_LBUTTONDOWN => Some(BridgeEvent::MouseButton {
                button: 0,
                pressed: true,
            }),
            WM_LBUTTONUP => Some(BridgeEvent::MouseButton {
                button: 0,
                pressed: false,
            }),
            WM_RBUTTONDOWN => Some(BridgeEvent::MouseButton {
                button: 1,
                pressed: true,
            }),
            WM_RBUTTONUP => Some(BridgeEvent::MouseButton {
                button: 1,
                pressed: false,
            }),
            WM_MOUSEWHEEL => {
                let delta = ((info.mouseData >> 16) as i16) as f64 / 120.0;
                Some(BridgeEvent::Scroll { dx: 0.0, dy: delta })
            }
            _ => None,
        };

        if let Some(ev) = event {
            HOOK_SENDER.with(|s| {
                if let Some(ref sender) = *s.borrow() {
                    let _ = sender.send(ev);
                }
            });
        }
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

/// Low-level keyboard hook callback.
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let keycode = info.vkCode;
        let modifiers = get_current_modifiers();

        let event = match wparam.0 as u32 {
            WM_KEYDOWN => Some(BridgeEvent::KeyDown { keycode, modifiers }),
            WM_KEYUP => Some(BridgeEvent::KeyUp { keycode, modifiers }),
            _ => None,
        };

        if let Some(ev) = event {
            HOOK_SENDER.with(|s| {
                if let Some(ref sender) = *s.borrow() {
                    let _ = sender.send(ev);
                }
            });
        }
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

/// Read current modifier key state from `GetAsyncKeyState`.
fn get_current_modifiers() -> u8 {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let mut m = 0u8;
    unsafe {
        if GetAsyncKeyState(0x10) < 0 {
            m |= crate::modifiers::SHIFT;
        } // VK_SHIFT
        if GetAsyncKeyState(0x11) < 0 {
            m |= crate::modifiers::CTRL;
        } // VK_CONTROL
        if GetAsyncKeyState(0x12) < 0 {
            m |= crate::modifiers::ALT;
        } // VK_MENU
        if GetAsyncKeyState(0x5B) < 0 || GetAsyncKeyState(0x5C) < 0 {
            m |= crate::modifiers::META;
        } // VK_LWIN/RWIN
    }
    m
}

/// Callback for `EnumDisplayMonitors`.
unsafe extern "system" fn monitor_enum_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = unsafe { &mut *(data.0 as *mut Vec<ScreenGeometry>) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let r = info.rcMonitor;
        monitors.push(ScreenGeometry {
            id: monitors.len() as u32,
            x: r.left,
            y: r.top,
            width: (r.right - r.left) as u32,
            height: (r.bottom - r.top) as u32,
            scale_factor: 1.0, // DPI-aware scaling handled separately
        });
    }
    TRUE
}

impl InputBridge for WindowsBridge {
    fn start_capture(&self) -> Result<Receiver<BridgeEvent>, BridgeError> {
        if self.capturing.swap(true, Ordering::SeqCst) {
            return Err(BridgeError::AlreadyCapturing);
        }

        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || unsafe {
            HOOK_SENDER.with(|s| *s.borrow_mut() = Some(tx));

            // Initialize last mouse position
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            LAST_MOUSE_POS.set((pt.x, pt.y));

            let mouse_hook =
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), HINSTANCE::default(), 0);
            let kb_hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                HINSTANCE::default(),
                0,
            );

            if mouse_hook.is_err() || kb_hook.is_err() {
                log::error!("SetWindowsHookExW failed");
                return;
            }

            // Message pump — required for low-level hooks to fire
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        });

        Ok(rx)
    }

    fn inject(&self, event: &BridgeEvent) -> Result<(), BridgeError> {
        match event {
            BridgeEvent::MouseMove { dx, dy } => {
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: *dx as i32,
                            dy: *dy as i32,
                            dwFlags: MOUSEEVENTF_MOVE,
                            ..Default::default()
                        },
                    },
                };
                send_input(&[input])?;
            }
            BridgeEvent::MouseWarp { x, y } => unsafe {
                if SetCursorPos(*x as i32, *y as i32).is_err() {
                    return Err(BridgeError::OsError("SetCursorPos failed".into()));
                }
            },
            BridgeEvent::MouseButton { button, pressed } => {
                let flags = match (button, pressed) {
                    (0, true) => MOUSEEVENTF_LEFTDOWN,
                    (0, false) => MOUSEEVENTF_LEFTUP,
                    (1, true) => MOUSEEVENTF_RIGHTDOWN,
                    (1, false) => MOUSEEVENTF_RIGHTUP,
                    _ => return Ok(()),
                };
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dwFlags: flags,
                            ..Default::default()
                        },
                    },
                };
                send_input(&[input])?;
            }
            BridgeEvent::Scroll { dx, dy } => {
                let mut inputs = Vec::new();
                if *dy != 0.0 {
                    inputs.push(INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                mouseData: (*dy * 120.0) as u32,
                                dwFlags: MOUSEEVENTF_WHEEL,
                                ..Default::default()
                            },
                        },
                    });
                }
                if *dx != 0.0 {
                    inputs.push(INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                mouseData: (*dx * 120.0) as u32,
                                dwFlags: MOUSEEVENTF_HWHEEL,
                                ..Default::default()
                            },
                        },
                    });
                }
                if !inputs.is_empty() {
                    send_input(&inputs)?;
                }
            }
            BridgeEvent::KeyDown { keycode, .. } => {
                let input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(*keycode as u16),
                            ..Default::default()
                        },
                    },
                };
                send_input(&[input])?;
            }
            BridgeEvent::KeyUp { keycode, .. } => {
                let input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(*keycode as u16),
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                };
                send_input(&[input])?;
            }
        }
        Ok(())
    }

    fn screens(&self) -> Result<Vec<ScreenGeometry>, BridgeError> {
        let mut monitors: Vec<ScreenGeometry> = Vec::new();
        let ptr = &mut monitors as *mut Vec<ScreenGeometry>;
        unsafe {
            if !EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(monitor_enum_proc),
                LPARAM(ptr as _),
            )
            .as_bool()
            {
                return Err(BridgeError::OsError("EnumDisplayMonitors failed".into()));
            }
        }
        Ok(monitors)
    }

    fn warp_cursor(&self, x: f64, y: f64) -> Result<(), BridgeError> {
        unsafe {
            if SetCursorPos(x as i32, y as i32).is_err() {
                return Err(BridgeError::OsError("SetCursorPos failed".into()));
            }
        }
        Ok(())
    }

    fn suppress_local(&self) -> Result<(), BridgeError> {
        self.suppressed.store(true, Ordering::SeqCst);
        unsafe {
            ShowCursor(false);
        }
        Ok(())
    }

    fn resume_local(&self) -> Result<(), BridgeError> {
        self.suppressed.store(false, Ordering::SeqCst);
        unsafe {
            ShowCursor(true);
        }
        Ok(())
    }
}

/// Send input events via `SendInput`. Returns error on failure.
fn send_input(inputs: &[INPUT]) -> Result<(), BridgeError> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(BridgeError::OsError(format!(
            "SendInput: sent {sent}/{} events",
            inputs.len()
        )));
    }
    Ok(())
}
