//! Error types for package manifest parsing and semantic validation.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during manifest parsing or semantic validation.
#[derive(Debug, Error, PartialEq)]
pub enum ManifestError {
    #[error("failed to deserialize TOML manifest: {0}")]
    TomlDeserialization(String),

    #[error("failed to parse KDL document: {0}")]
    KdlParse(String),

    #[error("unsupported schema version {found}, maximum supported is {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },

    #[error("package ID '{id}' is invalid: {reason}")]
    InvalidPackageId { id: String, reason: String },

    #[error("field '{field}' has invalid value: {reason}")]
    InvalidField { field: &'static str, reason: String },

    #[error("float field '{field}' ({value}) is not finite or out of range [{min}, {max}]")]
    FloatOutOfRange {
        field: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("path traversal attempt detected in path '{path}': {reason}")]
    PathTraversal { path: PathBuf, reason: String },

    #[error("cyclic dependency detected: package '{id}' depends on itself directly or indirectly")]
    CyclicDependency { id: String },

    #[error("duplicate identifier detected in {scope}: '{name}'")]
    DuplicateIdentifier { scope: &'static str, name: String },

    #[error("validation error: {0}")]
    Validation(String),
}
