//! Automatic resolution and unpacking of external WAD3 textures for GoldSrc BSP levels.

use std::fs;
use std::path::{Path, PathBuf};

use crate::codecs::AssetDecoder;
use crate::codecs::legacy::bsp::BspLevel;
use crate::codecs::legacy::wad::{Wad3Decoder, WadArchive};
use crate::ir::TextureDescriptor;

const DEFAULT_WAD_DIRS: &[&str] = &[
    "C:/Games/CS 1.6 GoldClient/cstrike",
    "C:/Games/CS 1.6 GoldClient/valve",
    "C:/Games/CS 1.6 GoldClient/cstrike_downloads",
    "D:/Repo/CS16/cstrike",
    "D:/Repo/CS16/valve",
    "cstrike",
    "valve",
];

/// Finds a WAD file by filename across candidate directories and environment variables.
#[must_use]
pub fn find_wad_file(wad_name: &str, extra_dirs: &[&Path]) -> Option<PathBuf> {
    let lower_name = wad_name.to_ascii_lowercase();

    // 1. Extra directories (e.g. BSP file directory and its parent)
    for dir in extra_dirs {
        let candidate = dir.join(&lower_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 2. Environment variable overrides
    if let Ok(env_dir) = std::env::var("GOLDHUB_WAD_DIR") {
        let candidate = Path::new(&env_dir).join(&lower_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Ok(env_dir) = std::env::var("GOLDS_GAME_DIR") {
        let candidate = Path::new(&env_dir).join(&lower_name);
        if candidate.exists() {
            return Some(candidate);
        }
        let candidate_cstrike = Path::new(&env_dir).join("cstrike").join(&lower_name);
        if candidate_cstrike.exists() {
            return Some(candidate_cstrike);
        }
    }

    // 3. Default known installation directories
    for default_dir in DEFAULT_WAD_DIRS {
        let candidate = Path::new(default_dir).join(&lower_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Resolves stub / external WAD textures for a BSP level in-place.
pub fn resolve_bsp_wad_textures(
    bsp: &BspLevel<'_>,
    textures: &mut [TextureDescriptor],
    extra_dirs: &[&Path],
) {
    let has_stubs = textures.iter().any(|t| t.width == 0 || t.height == 0);
    if !has_stubs {
        return;
    }

    // Collect all WAD names referenced in the BSP entities + common fallbacks
    let mut wad_names = bsp.extract_wad_names();
    for fallback in ["cstrike.wad", "halflife.wad", "cs_dust.wad", "decals.wad"] {
        if !wad_names.iter().any(|w| w.eq_ignore_ascii_case(fallback)) {
            wad_names.push(fallback.to_string());
        }
    }

    // Load each unique WAD archive into memory
    let mut loaded_wads: Vec<(Vec<u8>, PathBuf)> = Vec::new();
    for wad_name in &wad_names {
        if let Some(wad_path) = find_wad_file(wad_name, extra_dirs)
            && !loaded_wads.iter().any(|(_, p)| p == &wad_path)
            && let Ok(raw_bytes) = fs::read(&wad_path)
        {
            tracing::info!(path = %wad_path.display(), "Loaded WAD3 archive for texture resolution");
            loaded_wads.push((raw_bytes, wad_path));
        }
    }

    if !loaded_wads.is_empty() {
        // Parse WAD archives
        let decoder = Wad3Decoder;
        let archives: Vec<WadArchive<'_>> = loaded_wads
            .iter()
            .filter_map(|(bytes, _)| decoder.decode(bytes).ok())
            .collect();

        // Resolve stub textures
        let mut resolved_count = 0usize;
        for tex in textures.iter_mut() {
            if tex.width == 0 || tex.height == 0 {
                for archive in &archives {
                    if let Some(entry) = archive.find_entry(&tex.name)
                        && let Ok(unpacked) = archive.unpack_miptex(entry)
                    {
                        *tex = unpacked;
                        resolved_count += 1;
                        break;
                    }
                }
            }
        }

        tracing::info!(resolved = resolved_count, "Resolved external WAD textures");
    }

    // Assign fallback dummy for any still-unresolved textures (e.g. tool textures aaatrigger, clip, sky)
    for tex in textures.iter_mut() {
        if tex.width == 0 || tex.height == 0 {
            tex.width = 16;
            tex.height = 16;
            tex.mips = vec![crate::ir::MipLevel {
                width: 16,
                height: 16,
                data: vec![128; 16 * 16 * 4],
            }];
        }
    }
}
