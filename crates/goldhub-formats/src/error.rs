//! Unified error types for asset decoding, format parsing, and pipeline operations.

use thiserror::Error;

/// Errors returned by asset codecs, transpilers, and blob cache operations.
#[derive(Debug, Error)]
pub enum FormatError {
    #[error("invalid magic header: expected {expected:?}, got {got:?}")]
    InvalidMagic { expected: Vec<u8>, got: Vec<u8> },

    #[error("unsupported format version: expected {expected}, got {got}")]
    UnsupportedVersion { expected: u32, got: u32 },

    #[error("unexpected end of file: required at least {expected} bytes, but got {got}")]
    BufferTooSmall { expected: usize, got: usize },

    #[error("malformed asset data in {location}: {reason}")]
    MalformedData {
        location: &'static str,
        reason: String,
    },

    #[error("unsupported asset feature: {0}")]
    UnsupportedFeature(String),

    #[error("I/O error during asset operation: {0}")]
    Io(#[from] std::io::Error),
}
