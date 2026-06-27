//! Cross-platform clipboard access.
//!
//! macOS: `NSPasteboard` via `objc` runtime calls.
//! Windows: `OpenClipboard` / `GetClipboardData` / `SetClipboardData`.

/// Read clipboard text content.
pub fn clipboard_get() -> Result<String, crate::BridgeError> {
    platform::get()
}

/// Write text content to clipboard.
pub fn clipboard_set(content: &str) -> Result<(), crate::BridgeError> {
    platform::set(content)
}

#[cfg(target_os = "macos")]
mod platform {
    use crate::BridgeError;
    use std::process::Command;

    /// Read clipboard via `pbpaste` (avoids direct objc dependency).
    pub fn get() -> Result<String, BridgeError> {
        let output = Command::new("pbpaste")
            .output()
            .map_err(|e| BridgeError::OsError(format!("pbpaste failed: {e}")))?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Write clipboard via `pbcopy`.
    pub fn set(content: &str) -> Result<(), BridgeError> {
        use std::io::Write;
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BridgeError::OsError(format!("pbcopy spawn failed: {e}")))?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(content.as_bytes())
            .map_err(|e| BridgeError::OsError(format!("pbcopy write failed: {e}")))?;
        child
            .wait()
            .map_err(|e| BridgeError::OsError(format!("pbcopy wait failed: {e}")))?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use crate::BridgeError;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    pub fn get() -> Result<String, BridgeError> {
        unsafe {
            if OpenClipboard(None).is_err() {
                return Err(BridgeError::OsError("OpenClipboard failed".into()));
            }
            let handle = GetClipboardData(CF_UNICODETEXT);
            let result = if let Ok(h) = handle {
                let hmem = windows::Win32::Foundation::HGLOBAL(h.0 as _);
                let ptr = GlobalLock(hmem) as *const u16;
                if ptr.is_null() {
                    Err(BridgeError::OsError("GlobalLock returned null".into()))
                } else {
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(ptr, len);
                    let text = String::from_utf16_lossy(slice);
                    let _ = GlobalUnlock(hmem);
                    Ok(text)
                }
            } else {
                Ok(String::new())
            };
            let _ = CloseClipboard();
            result
        }
    }

    pub fn set(content: &str) -> Result<(), BridgeError> {
        let wide: Vec<u16> = content.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;
        unsafe {
            let hmem = GlobalAlloc(GMEM_MOVEABLE, size)
                .map_err(|_| BridgeError::OsError("GlobalAlloc failed".into()))?;
            let ptr = GlobalLock(hmem) as *mut u16;
            if ptr.is_null() {
                return Err(BridgeError::OsError("GlobalLock failed".into()));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            GlobalUnlock(hmem);

            if OpenClipboard(None).is_err() {
                return Err(BridgeError::OsError("OpenClipboard failed".into()));
            }
            let _ = SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0));
            let _ = CloseClipboard();
        }
        Ok(())
    }
}
