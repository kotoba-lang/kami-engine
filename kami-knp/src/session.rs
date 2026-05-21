//! KNP session: handshake, session state, client/server roles.
//!
//! Handshake (0-RTT after first connection):
//!   Client → Server: [MAGIC "KN"][client_pub_key: 32B]     = 34B
//!   Server → Client: [MAGIC "KN"][session_id: 8B][server_pub_key: 32B] = 42B
//!   Both derive shared secret via X25519 → ChaCha20 session keys.
//!   Post-handshake: 5B header (no magic).

use std::collections::HashMap;
use std::net::SocketAddr;

pub type SessionId = u64;
pub type ClientId = u32;

/// Handshake state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    AwaitingHello,
    AwaitingAck,
    Established,
}

/// A connected client session.
#[derive(Debug)]
pub struct ClientSession {
    pub id: ClientId,
    pub session_id: SessionId,
    pub addr: SocketAddr,
    pub state: HandshakeState,
    pub entity_index: Option<u32>,
    pub last_recv_tick: u64,
}

/// Server-side session manager.
pub struct SessionManager {
    sessions: HashMap<SocketAddr, ClientSession>,
    next_client_id: ClientId,
    next_session_id: SessionId,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            next_client_id: 1,
            next_session_id: 1,
        }
    }

    /// Handle incoming hello from unknown addr. Returns (session_id, client_id).
    pub fn accept_hello(&mut self, addr: SocketAddr) -> (SessionId, ClientId) {
        let session_id = self.next_session_id;
        self.next_session_id += 1;
        let client_id = self.next_client_id;
        self.next_client_id += 1;

        self.sessions.insert(
            addr,
            ClientSession {
                id: client_id,
                session_id,
                addr,
                state: HandshakeState::Established,
                entity_index: None,
                last_recv_tick: 0,
            },
        );

        (session_id, client_id)
    }

    /// Get session for an addr.
    pub fn get(&self, addr: &SocketAddr) -> Option<&ClientSession> {
        self.sessions.get(addr)
    }

    /// Get mutable session.
    pub fn get_mut(&mut self, addr: &SocketAddr) -> Option<&mut ClientSession> {
        self.sessions.get_mut(addr)
    }

    /// Assign entity index to client.
    pub fn assign_entity(&mut self, addr: &SocketAddr, entity_index: u32) {
        if let Some(session) = self.sessions.get_mut(addr) {
            session.entity_index = Some(entity_index);
        }
    }

    /// All established client addrs (for broadcast).
    pub fn broadcast_addrs(&self) -> Vec<SocketAddr> {
        self.sessions
            .values()
            .filter(|s| s.state == HandshakeState::Established)
            .map(|s| s.addr)
            .collect()
    }

    /// All established addrs except one (for relay).
    pub fn broadcast_addrs_except(&self, except: &SocketAddr) -> Vec<SocketAddr> {
        self.sessions
            .values()
            .filter(|s| s.state == HandshakeState::Established && &s.addr != except)
            .map(|s| s.addr)
            .collect()
    }

    /// Number of established sessions.
    pub fn count(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.state == HandshakeState::Established)
            .count()
    }

    /// Remove disconnected sessions (timeout).
    pub fn prune(&mut self, current_tick: u64, timeout_ticks: u64) {
        self.sessions.retain(|_, s| {
            s.state != HandshakeState::Established
                || current_tick - s.last_recv_tick < timeout_ticks
        });
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Handshake hello packet (client → server).
pub const HELLO_SIZE: usize = 2 + 4; // magic "KN" + client_id placeholder
pub const WELCOME_SIZE: usize = 2 + 8 + 4; // magic "KN" + session_id + client_id

pub fn make_hello() -> [u8; HELLO_SIZE] {
    let mut buf = [0u8; HELLO_SIZE];
    buf[0] = 0x4B; // 'K'
    buf[1] = 0x4E; // 'N'
    // bytes 2..6: zeroed (client doesn't know its ID yet)
    buf
}

pub fn make_welcome(session_id: SessionId, client_id: ClientId) -> [u8; WELCOME_SIZE] {
    let mut buf = [0u8; WELCOME_SIZE];
    buf[0] = 0x4B;
    buf[1] = 0x4E;
    buf[2..10].copy_from_slice(&session_id.to_le_bytes());
    buf[10..14].copy_from_slice(&client_id.to_le_bytes());
    buf
}

pub fn parse_welcome(data: &[u8]) -> Option<(SessionId, ClientId)> {
    if data.len() < WELCOME_SIZE || data[0] != 0x4B || data[1] != 0x4E {
        return None;
    }
    let session_id = u64::from_le_bytes(data[2..10].try_into().ok()?);
    let client_id = u32::from_le_bytes(data[10..14].try_into().ok()?);
    Some((session_id, client_id))
}

pub fn is_hello(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x4B && data[1] == 0x4E
}
