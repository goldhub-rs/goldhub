//! Pluggable asset codecs and decoders.
//!
//! Codecs serve as ingestion adapters that decode diverse on-disk formats
//! into the clean Canonical Data IR types.

pub mod modern;

#[cfg(feature = "goldsrc")]
pub mod legacy;

use crate::error::FormatError;

/// Core SPI trait for pluggable asset decoders.
///
/// Any file format adapter implements this trait to decode raw bytes
/// into one of the canonical Data IR representations.
pub trait AssetDecoder<'a, TTarget> {
    /// Decodes raw file bytes into the target Canonical IR type.
    fn decode(&self, bytes: &'a [u8]) -> Result<TTarget, FormatError>;
}
