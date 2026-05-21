//! KNP game server: accept connections, receive inputs, broadcast deltas.

use std::io;
use std::net::SocketAddr;

use crate::channel::ChannelManager;
use crate::packet::{Channel, Packet};
use crate::session::{self, ClientId, SessionManager};
use crate::socket::KnpSocket;

/// Messages the server produces for the game loop to consume.
pub enum ServerEvent {
    ClientConnected {
        client_id: ClientId,
        addr: SocketAddr,
    },
    ClientData {
        client_id: ClientId,
        channel: Channel,
        payload: Vec<u8>,
    },
}

/// KNP game server.
pub struct Server {
    socket: Box<dyn KnpSocket>,
    sessions: SessionManager,
    channels: ChannelManager,
    recv_buf: Vec<u8>,
}

impl Server {
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let socket = crate::socket::default_socket(addr)?;
        Ok(Self {
            socket,
            sessions: SessionManager::new(),
            channels: ChannelManager::new(),
            recv_buf: vec![0u8; 2048],
        })
    }

    /// Poll for incoming packets. Non-blocking. Returns events.
    pub fn poll(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();

        loop {
            match self.socket.recv_from(&mut self.recv_buf) {
                Ok((len, addr)) => {
                    let data = &self.recv_buf[..len];

                    if session::is_hello(data) {
                        // New client handshake
                        let (session_id, client_id) = self.sessions.accept_hello(addr);
                        let welcome = session::make_welcome(session_id, client_id);
                        let _ = self.socket.send_to(&welcome, addr);
                        events.push(ServerEvent::ClientConnected { client_id, addr });
                    } else if let Some((channel, payload)) = self.channels.receive(data) {
                        // Update last recv tick
                        if let Some(session) = self.sessions.get(&addr) {
                            let client_id = session.id;
                            events.push(ServerEvent::ClientData {
                                client_id,
                                channel,
                                payload,
                            });
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        events
    }

    /// Broadcast data to all connected clients on a channel.
    pub fn broadcast(&mut self, channel: Channel, payload: Vec<u8>) {
        let wire = self.channels.send(channel, payload);
        let addrs = self.sessions.broadcast_addrs();
        for addr in addrs {
            let _ = self.socket.send_to(&wire, addr);
        }
    }

    /// Send data to all clients except the sender.
    pub fn relay(&mut self, except: &SocketAddr, channel: Channel, payload: Vec<u8>) {
        let wire = self.channels.send(channel, payload);
        let addrs = self.sessions.broadcast_addrs_except(except);
        for addr in addrs {
            let _ = self.socket.send_to(&wire, addr);
        }
    }

    /// Send data to a specific client.
    pub fn send_to_addr(&mut self, addr: SocketAddr, channel: Channel, payload: Vec<u8>) {
        let wire = self.channels.send(channel, payload);
        let _ = self.socket.send_to(&wire, addr);
    }

    /// Assign an entity index to a client.
    pub fn assign_entity(&mut self, addr: &SocketAddr, entity_index: u32) {
        self.sessions.assign_entity(addr, entity_index);
    }

    /// Get session manager (read-only).
    pub fn sessions(&self) -> &SessionManager {
        &self.sessions
    }

    /// Number of connected clients.
    pub fn client_count(&self) -> usize {
        self.sessions.count()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}
