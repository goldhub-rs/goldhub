//! Virtual File System error types.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during VFS layer operations and asset resolution.
#[derive(Debug, Error)]
pub enum VfsError {
    #[error("file not found in any mounted VFS layer: '{0}'")]
    FileNotFound(PathBuf),

    #[error("path traversal attempt detected for relative path '{path}': {reason}")]
    PathTraversal { path: PathBuf, reason: String },

    #[error("I/O error accessing path '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("BLAKE3 integrity mismatch for '{path}': expected {expected}, calculated {actual}")]
    IntegrityMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("layer error: {0}")]
    Layer(String),
}
