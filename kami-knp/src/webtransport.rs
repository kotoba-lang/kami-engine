//! KNP over WebTransport (browser multiplayer).
//!
//! WebTransport datagrams carry KNP packets identically to raw UDP.
//! Server accepts both UDP (native) and WebTransport (browser).
//! KNP header format is unchanged — only transport layer differs.
//!
//! Build: wasm-pack build --target web (wasm32 only)

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::channel::ChannelManager;
#[cfg(target_arch = "wasm32")]
use crate::packet::Channel;

/// WebTransport KNP client for browser.
#[cfg(target_arch = "wasm32")]
pub struct WebTransportClient {
    channels: ChannelManager,
    connected: bool,
    recv_queue: Vec<(Channel, Vec<u8>)>,
    send_queue: Vec<Vec<u8>>,
}

#[cfg(target_arch = "wasm32")]
impl WebTransportClient {
    pub fn new() -> Self {
        Self {
            channels: ChannelManager::new(),
            connected: false,
            recv_queue: Vec::new(),
            send_queue: Vec::new(),
        }
    }

    /// Queue a KNP packet for sending via WebTransport datagram.
    pub fn send(&mut self, channel: Channel, payload: Vec<u8>) {
        let wire = self.channels.send(channel, payload);
        self.send_queue.push(wire);
    }

    /// Process received datagram bytes (called from JS callback).
    pub fn on_datagram(&mut self, data: &[u8]) {
        if let Some((channel, payload)) = self.channels.receive(data) {
            self.recv_queue.push((channel, payload));
        }
    }

    /// Drain received events.
    pub fn drain_recv(&mut self) -> Vec<(Channel, Vec<u8>)> {
        std::mem::take(&mut self.recv_queue)
    }

    /// Drain packets to send.
    pub fn drain_send(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.send_queue)
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

/// JS interop: WebTransport setup and datagram pump.
/// Called from kami-web's render loop.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct KnpWebTransport {
    inner: WebTransportClient,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl KnpWebTransport {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: WebTransportClient::new(),
        }
    }

    /// Feed a received datagram from JS.
    #[wasm_bindgen(js_name = "onDatagram")]
    pub fn on_datagram(&mut self, data: &[u8]) {
        self.inner.on_datagram(data);
    }

    /// Send position data on unreliable channel. Returns wire bytes for JS to send.
    #[wasm_bindgen(js_name = "sendPosition")]
    pub fn send_position(&mut self, x: f32, y: f32, z: f32) -> Vec<u8> {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&x.to_le_bytes());
        payload.extend_from_slice(&y.to_le_bytes());
        payload.extend_from_slice(&z.to_le_bytes());
        self.inner.send(Channel::Unreliable, payload);
        self.inner.drain_send().into_iter().flatten().collect()
    }

    /// Send chat message on reliable channel. Returns wire bytes.
    #[wasm_bindgen(js_name = "sendChat")]
    pub fn send_chat(&mut self, msg: &str) -> Vec<u8> {
        self.inner
            .send(Channel::ReliableOrdered, msg.as_bytes().to_vec());
        self.inner.drain_send().into_iter().flatten().collect()
    }

    /// Get received position updates (returns flattened f32 array: [x,y,z, x,y,z, ...]).
    #[wasm_bindgen(js_name = "drainPositions")]
    pub fn drain_positions(&mut self) -> Vec<f32> {
        let events = self.inner.drain_recv();
        let mut positions = Vec::new();
        for (channel, data) in events {
            if channel == Channel::Unreliable && data.len() >= 12 {
                positions.push(f32::from_le_bytes(data[0..4].try_into().unwrap()));
                positions.push(f32::from_le_bytes(data[4..8].try_into().unwrap()));
                positions.push(f32::from_le_bytes(data[8..12].try_into().unwrap()));
            }
        }
        positions
    }

    #[wasm_bindgen(js_name = "setConnected")]
    pub fn set_connected(&mut self, connected: bool) {
        self.inner.set_connected(connected);
    }
}
