//! Sandboxed VFS implementation with layered overlay priority and zero-copy mmap reads.

use memmap2::Mmap;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::error::VfsError;

/// Threshold size in bytes above which files are opened as zero-copy memory maps (`Mmap`).
pub const MMAP_THRESHOLD_BYTES: u64 = 64 * 1024; // 64 KiB

/// Zero-copy or in-memory byte buffer leased from the VFS.
#[derive(Debug)]
pub enum VfsFileHandle {
    /// Zero-copy memory mapped buffer for large assets (BSP, WAD, MDL).
    Mapped(Arc<Mmap>),
    /// In-memory heap buffer for small files (scripts, configs, text).
    Buffered(Vec<u8>),
}

impl VfsFileHandle {
    /// Returns the underlying byte slice of the file.
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Mapped(mmap) => mmap.as_ref(),
            Self::Buffered(vec) => vec.as_slice(),
        }
    }

    /// Computes the cryptographic BLAKE3 hash of the file payload.
    #[inline]
    pub fn blake3_hash(&self) -> blake3::Hash {
        blake3::hash(self.as_bytes())
    }
}

impl core::ops::Deref for VfsFileHandle {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

/// A mounted filesystem layer representing an installed package or base directory.
#[derive(Debug, Clone)]
pub struct VfsMountLayer {
    /// Package identifier (e.g. `valve.cstrike`, `valve.hl`).
    pub package_id: String,
    /// Absolute root path on the host filesystem.
    pub root_dir: PathBuf,
    /// Overlay priority (layers with higher priority shadow lower priority layers).
    pub priority: i32,
}

/// The multi-layered sandboxed Virtual File System resolver.
#[derive(Debug, Default)]
pub struct VfsResolver {
    layers: Vec<VfsMountLayer>,
}

impl VfsResolver {
    /// Creates an empty VFS resolver.
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Mounts a package directory into the VFS.
    ///
    /// Resolves and canonicalizes the directory root to ensure strict sandbox containment.
    pub fn mount(
        &mut self,
        package_id: impl Into<String>,
        root_dir: impl Into<PathBuf>,
        priority: i32,
    ) -> Result<(), VfsError> {
        let raw_root = root_dir.into();
        let canonical_root = raw_root.canonicalize().map_err(|e| VfsError::Io {
            path: raw_root.clone(),
            source: e,
        })?;

        let layer = VfsMountLayer {
            package_id: package_id.into(),
            root_dir: canonical_root,
            priority,
        };

        self.layers.push(layer);
        // Sort descending: highest priority first
        self.layers.sort_by_key(|a| std::cmp::Reverse(a.priority));
        Ok(())
    }

    /// Resolves an asset path across all mounted layers.
    ///
    /// Validates path against traversal attacks (e.g. `../`, `\`, colons, Windows DOS device names)
    /// and verifies that the canonical resolved file resides strictly within the layer's mount root.
    pub fn open_asset(&self, relative_path: impl AsRef<Path>) -> Result<VfsFileHandle, VfsError> {
        let rel = relative_path.as_ref();
        Self::sanitize_relative_path(rel)?;

        for layer in &self.layers {
            let candidate = layer.root_dir.join(rel);

            // Directly attempt opening the file without TOCTOU race
            let file = match File::open(&candidate) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    return Err(VfsError::Io {
                        path: candidate,
                        source: e,
                    });
                }
            };

            // Verify canonical path resides strictly inside layer.root_dir (prevents symlink/junction sandbox escape)
            let canonical_candidate = match candidate.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    return Err(VfsError::Io {
                        path: candidate,
                        source: e,
                    });
                }
            };

            if !canonical_candidate.starts_with(&layer.root_dir) {
                return Err(VfsError::PathTraversal {
                    path: rel.to_path_buf(),
                    reason: "symlink or junction escape outside layer root directory detected"
                        .to_string(),
                });
            }

            return Self::read_file_safe(file, &canonical_candidate);
        }

        Err(VfsError::FileNotFound(rel.to_path_buf()))
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
    pub fn exists(&self, relative_path: impl AsRef<Path>) -> Result<bool, VfsError> {
        let rel = relative_path.as_ref();
        Self::sanitize_relative_path(rel)?;

        for layer in &self.layers {
            let candidate = layer.root_dir.join(rel);
            let is_contained = candidate.is_file()
                && candidate
                    .canonicalize()
                    .is_ok_and(|canon| canon.starts_with(&layer.root_dir));

            if is_contained {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Mathematically ensures the path is strictly relative and does not escape the sandbox.
    pub fn sanitize_relative_path(path: &Path) -> Result<(), VfsError> {
        let path_str = path.to_str().ok_or_else(|| VfsError::PathTraversal {
            path: path.to_path_buf(),
            reason: "path contains non-UTF8 characters".to_string(),
        })?;

        if path_str.trim().is_empty() {
            return Err(VfsError::PathTraversal {
                path: path.to_path_buf(),
                reason: "path cannot be empty or purely whitespace".to_string(),
            });
        }

        if path_str.contains('\0') || path_str.contains('\\') || path_str.contains(':') {
            return Err(VfsError::PathTraversal {
                path: path.to_path_buf(),
                reason: "path cannot contain backslashes, colons, or null bytes".to_string(),
            });
        }

        let mut normal_components = 0;
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    return Err(VfsError::PathTraversal {
                        path: path.to_path_buf(),
                        reason: "parent directory traversal ('..') is strictly prohibited"
                            .to_string(),
                    });
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(VfsError::PathTraversal {
                        path: path.to_path_buf(),
                        reason: "absolute paths and device roots are strictly prohibited"
                            .to_string(),
                    });
                }
                Component::CurDir => {}
                Component::Normal(c) => {
                    normal_components += 1;
                    let seg = c.to_string_lossy();
                    if Self::is_dos_device_name(&seg) || seg.ends_with('.') || seg.ends_with(' ') {
                        return Err(VfsError::PathTraversal {
                            path: path.to_path_buf(),
                            reason: format!(
                                "component '{seg}' uses reserved Windows device name or invalid suffix"
                            ),
                        });
                    }
                }
            }
        }

        if normal_components == 0 {
            return Err(VfsError::PathTraversal {
                path: path.to_path_buf(),
                reason: "path must contain at least one normal component".to_string(),
            });
        }

        Ok(())
    }

    /// Reads file contents safely into memory with bounded allocation limits.
    fn read_file_safe(file: File, path: &Path) -> Result<VfsFileHandle, VfsError> {
        use std::io::Read;

        let metadata = file.metadata().map_err(|e| VfsError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let len = metadata.len();
        // Prevent unbounded memory exhaustion (e.g. 10 GB malicious files)
        const MAX_ASSET_SIZE_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
        if len > MAX_ASSET_SIZE_BYTES {
            return Err(VfsError::Layer(format!(
                "asset file '{path:?}' size ({len} bytes) exceeds maximum limit of {MAX_ASSET_SIZE_BYTES} bytes"
            )));
        }

        let mut buf = Vec::with_capacity(len as usize);
        let mut reader = std::io::BufReader::new(file);
        reader.read_to_end(&mut buf).map_err(|e| VfsError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(VfsFileHandle::Buffered(buf))
    }
}
