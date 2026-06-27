//! kami-bridge: OS-level input capture/injection bridge.
//!
//! Provides [`InputBridge`] trait for global mouse/keyboard capture,
//! injection, screen geometry discovery, and clipboard sync.
//! Platform implementations: macOS (CGEvent), Windows (Win32).

pub mod clipboard;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use std::sync::mpsc::Receiver;

/// Screen geometry descriptor.
#[derive(Debug, Clone)]
pub struct ScreenGeometry {
    /// OS display identifier.
    pub id: u32,
    /// Top-left X in global coordinate space.
    pub x: i32,
    /// Top-left Y in global coordinate space.
    pub y: i32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// HiDPI scale factor (e.g. 2.0 on Retina).
    pub scale_factor: f64,
}

/// OS-level input event for cross-machine transport.
///
/// Serialized over KNP between peers. Deliberately minimal
/// to keep wire overhead low on `Channel::Unreliable`.
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    /// Relative mouse movement delta.
    MouseMove { dx: f64, dy: f64 },
    /// Absolute mouse position (used for initial warp on edge transition).
    MouseWarp { x: f64, y: f64 },
    /// Mouse button press/release.
    MouseButton { button: u8, pressed: bool },
    /// Scroll wheel delta.
    Scroll { dx: f64, dy: f64 },
    /// Key press.
    KeyDown { keycode: u32, modifiers: u8 },
    /// Key release.
    KeyUp { keycode: u32, modifiers: u8 },
}

/// Modifier key bitmask constants.
pub mod modifiers {
    pub const SHIFT: u8 = 0b0001;
    pub const CTRL: u8 = 0b0010;
    pub const ALT: u8 = 0b0100;
    pub const META: u8 = 0b1000;
}

/// Compact wire encoding for [`BridgeEvent`].
///
/// Format: 1-byte tag + payload. Total 9–17 bytes per event.
/// Designed for `Channel::Unreliable` (mouse) and `Channel::ReliableOrdered` (keys).
impl BridgeEvent {
    /// Serialize to wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(17);
        match self {
            Self::MouseMove { dx, dy } => {
                buf.push(0x01);
                buf.extend_from_slice(&dx.to_le_bytes());
                buf.extend_from_slice(&dy.to_le_bytes());
            }
            Self::MouseWarp { x, y } => {
                buf.push(0x02);
                buf.extend_from_slice(&x.to_le_bytes());
                buf.extend_from_slice(&y.to_le_bytes());
            }
            Self::MouseButton { button, pressed } => {
                buf.push(0x03);
                buf.push(*button);
                buf.push(u8::from(*pressed));
            }
            Self::Scroll { dx, dy } => {
                buf.push(0x04);
                buf.extend_from_slice(&dx.to_le_bytes());
                buf.extend_from_slice(&dy.to_le_bytes());
            }
            Self::KeyDown { keycode, modifiers } => {
                buf.push(0x05);
                buf.extend_from_slice(&keycode.to_le_bytes());
                buf.push(*modifiers);
            }
            Self::KeyUp { keycode, modifiers } => {
                buf.push(0x06);
                buf.extend_from_slice(&keycode.to_le_bytes());
                buf.push(*modifiers);
            }
        }
        buf
    }

    /// Deserialize from wire bytes. Returns `None` on malformed input.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let (&tag, rest) = data.split_first()?;
        match tag {
            0x01 if rest.len() >= 16 => {
                let dx = f64::from_le_bytes(rest[0..8].try_into().ok()?);
                let dy = f64::from_le_bytes(rest[8..16].try_into().ok()?);
                Some(Self::MouseMove { dx, dy })
            }
            0x02 if rest.len() >= 16 => {
                let x = f64::from_le_bytes(rest[0..8].try_into().ok()?);
                let y = f64::from_le_bytes(rest[8..16].try_into().ok()?);
                Some(Self::MouseWarp { x, y })
            }
            0x03 if rest.len() >= 2 => Some(Self::MouseButton {
                button: rest[0],
                pressed: rest[1] != 0,
            }),
            0x04 if rest.len() >= 16 => {
                let dx = f64::from_le_bytes(rest[0..8].try_into().ok()?);
                let dy = f64::from_le_bytes(rest[8..16].try_into().ok()?);
                Some(Self::Scroll { dx, dy })
            }
            0x05 if rest.len() >= 5 => {
                let keycode = u32::from_le_bytes(rest[0..4].try_into().ok()?);
                Some(Self::KeyDown {
                    keycode,
                    modifiers: rest[4],
                })
            }
            0x06 if rest.len() >= 5 => {
                let keycode = u32::from_le_bytes(rest[0..4].try_into().ok()?);
                Some(Self::KeyUp {
                    keycode,
                    modifiers: rest[4],
                })
            }
            _ => None,
        }
    }
}

/// Edge of screen where cursor exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// OS-level input bridge trait.
///
/// Each platform implements capture (global hook), injection (synthetic events),
/// screen geometry discovery, cursor warp, and clipboard sync.
pub trait InputBridge: Send + 'static {
    /// Start global input capture. Returns a receiver of bridge events.
    ///
    /// The capture runs on a background thread; events are sent via channel.
    /// Call this once at startup.
    fn start_capture(&self) -> Result<Receiver<BridgeEvent>, BridgeError>;

    /// Inject an input event into the OS input stream.
    fn inject(&self, event: &BridgeEvent) -> Result<(), BridgeError>;

    /// Query all connected screen geometries.
    fn screens(&self) -> Result<Vec<ScreenGeometry>, BridgeError>;

    /// Warp the cursor to an absolute position in global coordinates.
    fn warp_cursor(&self, x: f64, y: f64) -> Result<(), BridgeError>;

    /// Suppress local input processing (hide cursor, block events from reaching apps).
    fn suppress_local(&self) -> Result<(), BridgeError>;

    /// Resume local input processing.
    fn resume_local(&self) -> Result<(), BridgeError>;
}

/// Bridge error type.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("OS API call failed: {0}")]
    OsError(String),
    #[error("Accessibility permission denied — grant Input Monitoring in System Settings")]
    PermissionDenied,
    #[error("Capture already started")]
    AlreadyCapturing,
    #[error("Channel disconnected")]
    ChannelClosed,
}

/// Create the platform-appropriate [`InputBridge`] implementation.
pub fn create_bridge() -> Box<dyn InputBridge> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSBridge::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsBridge::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        compile_error!("kami-bridge only supports macOS and Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_event_roundtrip() {
        let events = vec![
            BridgeEvent::MouseMove { dx: 1.5, dy: -2.3 },
            BridgeEvent::MouseWarp { x: 100.0, y: 200.0 },
            BridgeEvent::MouseButton {
                button: 0,
                pressed: true,
            },
            BridgeEvent::Scroll { dx: 0.0, dy: -3.0 },
            BridgeEvent::KeyDown {
                keycode: 42,
                modifiers: modifiers::SHIFT | modifiers::CTRL,
            },
            BridgeEvent::KeyUp {
                keycode: 42,
                modifiers: 0,
            },
        ];
        for ev in &events {
            let bytes = ev.to_bytes();
            let decoded = BridgeEvent::from_bytes(&bytes).expect("decode failed");
            assert_eq!(format!("{decoded:?}"), format!("{ev:?}"));
        }
    }

    #[test]
    fn bridge_event_malformed() {
        assert!(BridgeEvent::from_bytes(&[]).is_none());
        assert!(BridgeEvent::from_bytes(&[0xFF]).is_none());
        assert!(BridgeEvent::from_bytes(&[0x01, 0x00]).is_none()); // too short
    }
}
