//! Content-Addressable Storage (CAS) for pre-compiled asset blobs.

use std::fs::{File, create_dir_all};
use std::path::PathBuf;

use crate::codecs::AssetDecoder;
use crate::codecs::modern::{RawBlobDecoder, encode_raw_blob};
use crate::error::FormatError;
use crate::ir::MeshBuffer;

/// Local filesystem cache backed by BLAKE3 content addressing.
#[derive(Debug, Clone)]
pub struct AssetCache {
    cache_dir: PathBuf,
}

impl AssetCache {
    /// Initializes an asset cache in the given directory.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// Computes the content hash of arbitrary input bytes.
    #[inline(always)]
    pub fn hash(bytes: &[u8]) -> blake3::Hash {
        blake3::hash(bytes)
    }

    /// Looks up a cached mesh buffer by its source content hash.
    ///
    /// If present on disk, memory-maps the file via `memmap2` and decodes in $O(1)$ time.
    #[allow(unsafe_code)]
    pub fn lookup_mesh(&self, hash: &blake3::Hash) -> Option<MeshBuffer> {
        let path = self.mesh_path(hash);
        if !path.exists() {
            return None;
        }

        let file = File::open(&path).ok()?;
        // SAFETY: The cache file was written atomically by the pipeline, is immutable, and is not modified concurrently.
        let mmap = unsafe { memmap2::Mmap::map(&file).ok()? };

        let decoder = RawBlobDecoder;
        match decoder.decode(&mmap) {
            Ok(mesh) => Some(mesh),
            Err(err) => {
                tracing::warn!(
                    target: "goldhub::formats::cache",
                    ?err,
                    path = %path.display(),
                    "Corrupt cache blob detected, bypassing cache"
                );
                None
            }
        }
    }

    /// Stores a compiled mesh buffer into the cache atomically using a temporary file.
    pub fn store_mesh(&self, hash: &blake3::Hash, mesh: &MeshBuffer) -> Result<(), FormatError> {
        let path = self.mesh_path(hash);
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }

        let encoded = encode_raw_blob(mesh);
        let hex = hash.to_hex();
        let tmp_path = self
            .cache_dir
            .join(format!("{hex}.tmp.{}", std::process::id()));

        std::fs::write(&tmp_path, encoded)?;

        if let Err(err) = std::fs::rename(&tmp_path, &path) {
            // Clean up temporary file if rename failed
            let _ = std::fs::remove_file(&tmp_path);

            // If another thread or process won the race and created `path`, treat as success
            if !path.exists() {
                tracing::warn!(
                    target: "goldhub::formats::cache",
                    ?err,
                    path = %path.display(),
                    "Failed to persist asset cache blob"
                );
                return Err(FormatError::Io(err));
            }
        }

        Ok(())
    }

    fn mesh_path(&self, hash: &blake3::Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.cache_dir.join(format!("{hex}.ghbsp"))
    }
}
