//! Modern and cache codecs.

pub mod raw_blob;

pub use raw_blob::{BLOB_MAGIC, BlobHeader, RawBlobDecoder, encode_raw_blob};
