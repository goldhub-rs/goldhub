//! Transport layer primitives.

pub mod endpoint;
pub mod socket;

pub use endpoint::NetAddress;
pub use socket::UdpTransport;
