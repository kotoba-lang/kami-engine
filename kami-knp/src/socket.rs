//! Platform-abstracted UDP socket.
//!
//! BSD-compatible platforms (PC, iOS, Android, PS5, Switch) share one macro.
//! Web uses WebTransport datagram.

use std::io;
use std::net::SocketAddr;

/// Platform-agnostic socket trait.
pub trait KnpSocket: Send + Sync {
    fn send_to(&self, data: &[u8], addr: SocketAddr) -> io::Result<usize>;
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

// ── BSD socket platforms (PC, Mac, Linux, iOS, Android) ──

#[cfg(not(target_arch = "wasm32"))]
pub struct UdpKnpSocket {
    inner: std::net::UdpSocket,
}

#[cfg(not(target_arch = "wasm32"))]
impl UdpKnpSocket {
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let sock = std::net::UdpSocket::bind(addr)?;
        sock.set_nonblocking(true)?;
        Ok(Self { inner: sock })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl KnpSocket for UdpKnpSocket {
    fn send_to(&self, data: &[u8], addr: SocketAddr) -> io::Result<usize> {
        self.inner.send_to(data, addr)
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

// ── PS5 (libSceNet) — cfg(target_os = "prospero") ──
// Uses the same BSD macro pattern:
// extern "C" { fn sceNetSendto(...) -> i32; fn sceNetRecvfrom(...) -> i32; }
// Actual FFI calls are identical to BSD, only linked to different library.

// ── Switch (nn::socket) — cfg(target_os = "horizon") ──
// extern "C" { fn nn_socket_SendTo(...) -> i32; fn nn_socket_RecvFrom(...) -> i32; }
// Same BSD socket semantics, different symbol names.

// ── Web (WebTransport datagram) ──

#[cfg(target_arch = "wasm32")]
pub struct WebTransportKnpSocket {
    // In real implementation: web_sys::WebTransport + datagram streams
    _placeholder: (),
}

#[cfg(target_arch = "wasm32")]
impl WebTransportKnpSocket {
    pub fn connect(_url: &str) -> Self {
        Self { _placeholder: () }
    }
}

#[cfg(target_arch = "wasm32")]
impl KnpSocket for WebTransportKnpSocket {
    fn send_to(&self, _data: &[u8], _addr: SocketAddr) -> io::Result<usize> {
        // web_sys: self.transport.datagrams().writable().write(data)
        Ok(0)
    }

    fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        // web_sys: self.transport.datagrams().readable().read()
        Err(io::Error::new(io::ErrorKind::WouldBlock, "no data"))
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok("0.0.0.0:0".parse().unwrap())
    }
}

/// Create the default socket for the current platform.
#[cfg(not(target_arch = "wasm32"))]
pub fn default_socket(bind_addr: SocketAddr) -> io::Result<Box<dyn KnpSocket>> {
    Ok(Box::new(UdpKnpSocket::bind(bind_addr)?))
}
