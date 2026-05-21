//! KNP: KAMI Network Protocol — custom UDP for all platforms.

pub mod channel;
pub mod crypto;
pub mod packet;
pub mod session;
pub mod socket;

#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(target_arch = "wasm32")]
pub mod webtransport;

pub use channel::ChannelManager;
pub use packet::{Channel as PacketChannel, Header, Packet};
pub use session::{ClientId, SessionId};
