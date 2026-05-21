//! KNP game client: connect to server, send inputs, receive state.

use std::io;
use std::net::SocketAddr;

use crate::channel::ChannelManager;
use crate::packet::Channel;
use crate::session::{self, ClientId, SessionId};
use crate::socket::KnpSocket;

/// Client connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

/// Messages the client produces for the game loop.
pub enum ClientEvent {
    Connected {
        session_id: SessionId,
        client_id: ClientId,
    },
    Data {
        channel: Channel,
        payload: Vec<u8>,
    },
}

/// KNP game client.
pub struct Client {
    socket: Box<dyn KnpSocket>,
    server_addr: SocketAddr,
    channels: ChannelManager,
    state: ConnectionState,
    session_id: SessionId,
    client_id: ClientId,
    recv_buf: Vec<u8>,
}

impl Client {
    pub fn connect(server_addr: SocketAddr) -> io::Result<Self> {
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let socket = crate::socket::default_socket(bind_addr)?;

        // Send hello
        let hello = session::make_hello();
        socket.send_to(&hello, server_addr)?;

        Ok(Self {
            socket,
            server_addr,
            channels: ChannelManager::new(),
            state: ConnectionState::Connecting,
            session_id: 0,
            client_id: 0,
            recv_buf: vec![0u8; 2048],
        })
    }

    /// Poll for incoming data. Non-blocking.
    pub fn poll(&mut self) -> Vec<ClientEvent> {
        let mut events = Vec::new();

        loop {
            match self.socket.recv_from(&mut self.recv_buf) {
                Ok((len, _addr)) => {
                    let data = &self.recv_buf[..len];

                    if self.state == ConnectionState::Connecting {
                        // Expecting welcome
                        if let Some((session_id, client_id)) = session::parse_welcome(data) {
                            self.session_id = session_id;
                            self.client_id = client_id;
                            self.state = ConnectionState::Connected;
                            events.push(ClientEvent::Connected {
                                session_id,
                                client_id,
                            });
                        }
                    } else if let Some((channel, payload)) = self.channels.receive(data) {
                        events.push(ClientEvent::Data { channel, payload });
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        events
    }

    /// Send data to server on a channel.
    pub fn send(&mut self, channel: Channel, payload: Vec<u8>) {
        let wire = self.channels.send(channel, payload);
        let _ = self.socket.send_to(&wire, self.server_addr);
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn client_id(&self) -> ClientId {
        self.client_id
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
}
