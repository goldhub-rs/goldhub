//! Package manifest specification (`game.toml`).
//!
//! Enforces strict zero-unknown-fields deserialization and semantic validation
//! against path traversals, invalid identifiers, and un-sanitized values.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

use crate::error::ManifestError;

/// Current supported schema version for `game.toml`.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Root manifest representing a game package or modification in GoldHub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamePackageManifest {
    pub package: PackageMeta,
    pub environment: EnvironmentSpec,
    #[serde(default)]
    pub render: RenderSpec,
    #[serde(default)]
    pub network: NetworkSpec,
}

impl GamePackageManifest {
    /// Parses a `game.toml` string and validates all semantic constraints.
    pub fn parse_str(content: &str) -> Result<Self, ManifestError> {
        let manifest: Self = toml::from_str(content)
            .map_err(|e| ManifestError::TomlDeserialization(e.to_string()))?;

        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates all semantic properties of the manifest.
    pub fn validate(&self) -> Result<(), ManifestError> {
        // Schema version check
        if self.package.schema > CURRENT_SCHEMA_VERSION || self.package.schema == 0 {
            return Err(ManifestError::UnsupportedSchemaVersion {
                found: self.package.schema,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }

        // Validate package ID format: namespace.name (alphanumeric, lowercase, dots, underscores, dashes)
        validate_package_id(&self.package.id)?;

        // Validate string fields length and content
        validate_string_field("package.name", &self.package.name, 1, 128)?;
        validate_string_field("package.version", &self.package.version, 1, 32)?;
        if let Some(ref desc) = self.package.description {
            validate_string_field("package.description", desc, 1, 1024)?;
        }
        for author in &self.package.authors {
            validate_string_field("package.authors[]", author, 1, 128)?;
        }

        // Validate base dependency (cannot self-reference)
        if let Some(ref base) = self.environment.base {
            validate_package_id(base)?;
            if base == &self.package.id {
                return Err(ManifestError::CyclicDependency {
                    id: self.package.id.clone(),
                });
            }
        }

        // Validate mount point (must be a clean relative path without parent components)
        validate_relative_mount_path(&self.environment.mount_point)?;

        // Validate render parameters
        if !self.render.fov.is_finite() || !(30.0..=160.0).contains(&self.render.fov) {
            return Err(ManifestError::FloatOutOfRange {
                field: "render.fov",
                value: self.render.fov,
                min: 30.0,
                max: 160.0,
            });
        }

        if !self.render.gamma.is_finite() || !(0.5..=3.0).contains(&self.render.gamma) {
            return Err(ManifestError::FloatOutOfRange {
                field: "render.gamma",
                value: self.render.gamma,
                min: 0.5,
                max: 3.0,
            });
        }

        for &c in &self.render.clear_color {
            if !c.is_finite() || !(0.0..=1.0).contains(&c) {
                return Err(ManifestError::FloatOutOfRange {
                    field: "render.clear_color[i]",
                    value: c,
                    min: 0.0,
                    max: 1.0,
                });
            }
        }

        // Validate network spec
        if self.network.default_port == 0 {
            return Err(ManifestError::InvalidField {
                field: "network.default_port",
                reason: "port cannot be 0".to_string(),
            });
        }

        if self.network.protocols.is_empty() {
            return Err(ManifestError::InvalidField {
                field: "network.protocols",
                reason: "protocols list cannot be empty".to_string(),
            });
        }

        for proto in &self.network.protocols {
            validate_string_field("network.protocols[]", proto, 1, 32)?;
        }

        Ok(())
    }
}

/// Package identity and metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageMeta {
    /// Unique canonical identifier (e.g., `valve.cstrike`, `valve.hl`).
    pub id: String,
    /// Human-readable title of the game or mod.
    pub name: String,
    /// Semver string (e.g. `1.6.0`).
    pub version: String,
    /// Schema version for forward-compatibility validation.
    pub schema: u32,
    /// Optional package description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional authors or maintainers.
    #[serde(default)]
    pub authors: Vec<String>,
}

/// Environment and VFS mounting specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSpec {
    /// Parent base package ID required for assets (e.g. `valve.hl` for CS).
    #[serde(default)]
    pub base: Option<String>,
    /// Relative directory path for package assets (e.g. `cstrike`).
    pub mount_point: PathBuf,
    /// VFS overlay layer priority (higher values override lower ones).
    #[serde(default = "default_layer_priority")]
    pub priority: i32,
}

const fn default_layer_priority() -> i32 {
    100
}

/// Rendering and visual defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderSpec {
    /// Default camera Field of View in degrees.
    #[serde(default = "default_fov")]
    pub fov: f32,
    /// Default display gamma.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// Default background clear color `[R, G, B, A]`.
    #[serde(default = "default_clear_color")]
    pub clear_color: [f32; 4],
}

impl Default for RenderSpec {
    fn default() -> Self {
        Self {
            fov: default_fov(),
            gamma: default_gamma(),
            clear_color: default_clear_color(),
        }
    }
}

const fn default_fov() -> f32 {
    90.0
}

const fn default_gamma() -> f32 {
    1.8
}

const fn default_clear_color() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

/// Network specification for client protocol compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSpec {
    /// Default port for dedicated server or broadcast queries.
    #[serde(default = "default_port")]
    pub default_port: u16,
    /// Supported protocols (e.g. `[47, 48, "goldhub-v1"]`).
    #[serde(default = "default_protocols")]
    pub protocols: Vec<String>,
}

impl Default for NetworkSpec {
    fn default() -> Self {
        Self {
            default_port: default_port(),
            protocols: default_protocols(),
        }
    }
}

const fn default_port() -> u16 {
    27015
}

fn default_protocols() -> Vec<String> {
    vec!["48".to_string(), "goldhub-v1".to_string()]
}

/// Validates that package ID consists of alphanumeric characters, dots, dashes, and underscores,
/// with at least one dot separating namespace and name (e.g. `valve.cstrike`).
fn validate_package_id(id: &str) -> Result<(), ManifestError> {
    if id.is_empty() {
        return Err(ManifestError::InvalidPackageId {
            id: id.to_string(),
            reason: "package ID cannot be empty".to_string(),
        });
    }

    if id.len() > 128 {
        return Err(ManifestError::InvalidPackageId {
            id: id.to_string(),
            reason: "package ID exceeds 128 characters".to_string(),
        });
    }

    if !id.contains('.') {
        return Err(ManifestError::InvalidPackageId {
            id: id.to_string(),
            reason: "package ID must contain at least one namespace separator '.' (e.g. 'namespace.pkg')"
                .to_string(),
        });
    }

    for part in id.split('.') {
        if part.is_empty() {
            return Err(ManifestError::InvalidPackageId {
                id: id.to_string(),
                reason: "package ID cannot contain consecutive or leading/trailing dots"
                    .to_string(),
            });
        }

        let is_valid_char = part
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');

        if !is_valid_char {
            return Err(ManifestError::InvalidPackageId {
                id: id.to_string(),
                reason: format!(
                    "segment '{part}' contains invalid characters (only lowercase a-z, 0-9, '-', '_' allowed)"
                ),
            });
        }
    }

    Ok(())
}

/// Validates string field bounds, UTF-8 non-empty content, and absence of control characters.
fn validate_string_field(
    field: &'static str,
    val: &str,
    min_len: usize,
    max_len: usize,
) -> Result<(), ManifestError> {
    let trimmed = val.trim();
    if trimmed.len() < min_len {
        return Err(ManifestError::InvalidField {
            field,
            reason: format!("field cannot be empty or purely whitespace (minimum {min_len} chars)"),
        });
    }

    if val.len() > max_len {
        return Err(ManifestError::InvalidField {
            field,
            reason: format!("field exceeds maximum length of {max_len} bytes"),
        });
    }

    if val
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ManifestError::InvalidField {
            field,
            reason: "field contains forbidden control characters".to_string(),
        });
    }

    Ok(())
}

/// Checks if a component matches a reserved Windows DOS device name.
fn is_dos_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Validates that a path is relative and does not contain parent components (`..`), root prefixes,
/// colons (NTFS streams), Windows device names, or backslashes.
fn validate_relative_mount_path(path: &Path) -> Result<(), ManifestError> {
    let path_str = path.to_str().ok_or_else(|| ManifestError::PathTraversal {
        path: path.to_path_buf(),
        reason: "mount path contains non-UTF8 characters".to_string(),
    })?;

    if path_str.trim().is_empty() {
        return Err(ManifestError::PathTraversal {
            path: path.to_path_buf(),
            reason: "mount path cannot be empty or purely whitespace".to_string(),
        });
    }

    // Disallow cross-platform bypass chars
    if path_str.contains('\0') || path_str.contains('\\') || path_str.contains(':') {
        return Err(ManifestError::PathTraversal {
            path: path.to_path_buf(),
            reason: "mount path cannot contain backslashes, colons, or null bytes".to_string(),
        });
    }

    let mut normal_components = 0;
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(ManifestError::PathTraversal {
                    path: path.to_path_buf(),
                    reason: "parent directory references ('..') are strictly prohibited"
                        .to_string(),
                });
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ManifestError::PathTraversal {
                    path: path.to_path_buf(),
                    reason: "absolute paths and root prefixes are strictly prohibited".to_string(),
                });
            }
            Component::CurDir => {}
            Component::Normal(c) => {
                normal_components += 1;
                let seg = c.to_string_lossy();
                if is_dos_device_name(&seg) || seg.ends_with('.') || seg.ends_with(' ') {
                    return Err(ManifestError::PathTraversal {
                        path: path.to_path_buf(),
                        reason: format!(
                            "component '{seg}' uses a reserved DOS device name or invalid suffix"
                        ),
                    });
                }
            }
        }
    }

    if normal_components == 0 {
        return Err(ManifestError::PathTraversal {
            path: path.to_path_buf(),
            reason: "mount path must contain at least one normal directory component".to_string(),
        });
    }

    Ok(())
}
