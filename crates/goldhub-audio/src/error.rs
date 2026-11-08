//! Typed error definitions for the GoldSrc audio engine and asset decoders.

use thiserror::Error;

/// Domain errors occurring during audio decoding, sound processing, and material parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AudioError {
    /// Unexpected buffer length or truncated chunk in audio stream.
    #[error("buffer too small: required at least {expected} bytes, but got {got}")]
    BufferTooSmall {
        /// Required byte count.
        expected: usize,
        /// Actual available byte count.
        got: usize,
    },

    /// Invalid FourCC or RIFF container identifier.
    #[error("invalid magic identifier in {location}: expected {expected:?}, got {got:?}")]
    InvalidMagic {
        /// Chunk or header location.
        location: &'static str,
        /// Expected FourCC ASCII bytes.
        expected: [u8; 4],
        /// Actual FourCC bytes read.
        got: [u8; 4],
    },

    /// Missing mandatory RIFF chunk such as `fmt ` or `data`.
    #[error("missing required chunk: '{0}'")]
    MissingChunk(&'static str),

    /// Unsupported audio format tag (only PCM format 1 is supported).
    #[error("unsupported audio format code: {0} (only PCM format 1 is supported)")]
    UnsupportedFormat(u16),

    /// Unsupported channel configuration (only mono (1) and stereo (2) are supported).
    #[error("unsupported channel count: {0} (only mono and stereo supported)")]
    UnsupportedChannels(u16),

    /// Unsupported sample rate.
    #[error("unsupported sample rate: {0} Hz (supported: 11025, 22050, 44100 Hz)")]
    UnsupportedSampleRate(u32),

    /// Unsupported bit depth (only 8-bit unsigned and 16-bit signed PCM supported).
    #[error("unsupported bit depth: {0} bits (only 8-bit and 16-bit supported)")]
    UnsupportedBitsPerSample(u16),

    /// Corrupted or inconsistent chunk length metadata.
    #[error("malformed chunk '{chunk}': declared length {declared_length} exceeds buffer capacity")]
    MalformedChunk {
        /// FourCC chunk identifier.
        chunk: &'static str,
        /// Declared length in bytes.
        declared_length: usize,
    },

    /// Parse failure in texture materials definition.
    #[error("invalid materials line at {line_number}: {reason}")]
    InvalidMaterialEntry {
        /// 1-based line number.
        line_number: usize,
        /// Detailed reason.
        reason: String,
    },
}
