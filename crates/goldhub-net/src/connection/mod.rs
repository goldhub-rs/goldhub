//! Connection lifecycle and flow management.

pub mod channel;
pub mod flow;
pub mod state;

pub use channel::{IncomingPacket, NetConnection};
pub use flow::{FlowStats, RateLimiter};
pub use state::ConnectionState;
