//! Connection lifecycle and flow management.

pub mod channel;
pub mod flow;
pub mod negotiation;
pub mod state;

pub use channel::{IncomingPacket, NetConnection};
pub use flow::{FlowStats, RateLimiter};
pub use negotiation::{
    ActiveChannelProfile, CapabilityNegotiator, ClientCapabilities, PROTOCOL_GOLDHUB_NEXTGEN_1,
    PROTOCOL_GOLDSRC_48, ServerMode,
};
pub use state::ConnectionState;
