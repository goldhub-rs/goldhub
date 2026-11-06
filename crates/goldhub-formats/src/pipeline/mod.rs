//! Composable asset loading pipelines.
//!
//! Provides clean separation of concern:
//! - Direct modern asset loading (0 legacy overhead)
//! - Hybrid legacy transpile & cache pipeline

use std::path::PathBuf;

use crate::cache::AssetCache;
use crate::codecs::AssetDecoder;
use crate::codecs::modern::RawBlobDecoder;
use crate::error::FormatError;
use crate::ir::MeshBuffer;

#[cfg(feature = "goldsrc")]
use crate::codecs::legacy::Bsp30Decoder;
#[cfg(feature = "goldsrc")]
use crate::transpile::compile_bsp_geometry;

/// Unified asset pipeline facade.
#[derive(Debug, Clone)]
pub struct AssetPipeline {
    cache: AssetCache,
}

impl AssetPipeline {
    /// Creates a new asset pipeline with the specified cache directory.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache: AssetCache::new(cache_dir),
        }
    }

    /// Accesses the underlying asset cache.
    pub fn cache(&self) -> &AssetCache {
        &self.cache
    }

    /// Accesses the underlying asset cache mutably.
    pub fn cache_mut(&mut self) -> &mut AssetCache {
        &mut self.cache
    }

    /// Loads a pre-compiled native blob mesh directly into canonical IR (Modern Path).
    pub fn load_blob_mesh(&self, bytes: &[u8]) -> Result<MeshBuffer, FormatError> {
        let decoder = RawBlobDecoder;
        decoder.decode(bytes)
    }

    /// Ingests a BSP30 level with automatic BLAKE3 cache acceleration (Legacy Bridge Path).
    ///
    /// On cache hit: returns the cached mesh instantaneously via `memmap2` (< 15 ms).
    /// On cache miss: parses, triangulates, commits to cache, and returns the compiled mesh.
    #[cfg(feature = "goldsrc")]
    pub fn load_bsp_mesh(&self, raw_bytes: &[u8]) -> Result<MeshBuffer, FormatError> {
        let hash = AssetCache::hash(raw_bytes);

        // 1. Check warm cache hit
        if let Some(cached) = self.cache.lookup_mesh(&hash) {
            return Ok(cached);
        }

        // 2. Cold cache miss: parse and transpile
        let decoder = Bsp30Decoder;
        let bsp = decoder.decode(raw_bytes)?;
        let mesh = compile_bsp_geometry(&bsp)?;

        // 3. Persist to cache for subsequent instant loads
        let _ = self.cache.store_mesh(&hash, &mesh);

        Ok(mesh)
    }
}
