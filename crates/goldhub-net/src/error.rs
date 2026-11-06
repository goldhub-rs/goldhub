//! Strongly typed error hierarchy for GoldHub networking.

use thiserror::Error;

/// Core networking and protocol errors.
#[derive(Debug, Error)]
pub enum NetError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Packet too small: expected at least {expected} bytes, received {received}")]
    PacketTooSmall { expected: usize, received: usize },

    #[error("Packet exceeds maximum transmission unit: max {max} bytes, received {received}")]
    PacketTooLarge { max: usize, received: usize },

    #[error("Malformed packet header: {0}")]
    MalformedHeader(&'static str),

    #[error("Malformed command payload for opcode {opcode:#04x}: {reason}")]
    MalformedPayload { opcode: u8, reason: String },

    #[error("Stream fragment overflow: stream {stream_id}, offset {offset}, total {total_len}")]
    FragmentOverflow {
        stream_id: u8,
        offset: usize,
        total_len: usize,
    },

    #[error("Ring buffer capacity exhausted: requested {requested} bytes")]
    RingCapacityExhausted { requested: usize },

    #[error("Connection lost or timed out: {0}")]
    ConnectionClosed(&'static str),

    #[error("Wire-Ring storage error: {0}")]
    RingStorage(#[from] wire_ring::MemoryError),

    #[error("Wire-Ring channel error: {0}")]
    RingError(#[from] wire_ring::RingError),
}
