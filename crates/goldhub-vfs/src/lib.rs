//! High-performance sandboxed Zero-Copy Virtual File System (VFS) for GoldHub.

pub mod error;
pub mod resolver;

pub use error::VfsError;
pub use resolver::{MMAP_THRESHOLD_BYTES, VfsFileHandle, VfsMountLayer, VfsResolver};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_vfs_layering_and_priority_override() {
        let base_dir = tempdir().unwrap();
        let mod_dir = tempdir().unwrap();

        // Base game file: valve/title.txt -> "Half-Life"
        let base_file = base_dir.path().join("title.txt");
        fs::write(&base_file, b"Half-Life").unwrap();

        // Mod override: cstrike/title.txt -> "Counter-Strike"
        let mod_file = mod_dir.path().join("title.txt");
        fs::write(&mod_file, b"Counter-Strike").unwrap();

        // Base unique file: valve/halflife.wad -> "WAD3"
        let base_wad = base_dir.path().join("halflife.wad");
        fs::write(&base_wad, b"WAD3_BASE").unwrap();

        let mut vfs = VfsResolver::new();
        // Mount valve with priority 100
        vfs.mount("valve.hl", base_dir.path(), 100).unwrap();
        // Mount cstrike with priority 200 (overrides valve)
        vfs.mount("valve.cstrike", mod_dir.path(), 200).unwrap();

        // Check overridden asset
        let handle = vfs.open_asset("title.txt").unwrap();
        assert_eq!(handle.as_bytes(), b"Counter-Strike");

        // Check fallback to base layer
        let handle_wad = vfs.open_asset("halflife.wad").unwrap();
        assert_eq!(handle_wad.as_bytes(), b"WAD3_BASE");

        // Check non-existent file
        let err = vfs.open_asset("missing.mdl").unwrap_err();
        assert!(matches!(err, VfsError::FileNotFound(_)));
    }

    #[test]
    fn test_vfs_blocks_sandbox_escape() {
        let vfs = VfsResolver::new();

        let err = vfs.open_asset("../../../etc/shadow").unwrap_err();
        assert!(matches!(err, VfsError::PathTraversal { .. }));

        let err_abs = vfs.open_asset("/windows/system32/cmd.exe").unwrap_err();
        assert!(matches!(err_abs, VfsError::PathTraversal { .. }));
    }

    #[test]
    fn test_vfs_mmap_large_asset() {
        let temp = tempdir().unwrap();
        let large_file_path = temp.path().join("de_dust2.bsp");

        // Create file larger than 64 KiB
        let payload = vec![0x42u8; (MMAP_THRESHOLD_BYTES + 1024) as usize];
        fs::write(&large_file_path, &payload).unwrap();

        let mut vfs = VfsResolver::new();
        vfs.mount("valve.cstrike", temp.path(), 100).unwrap();

        let handle = vfs.open_asset("de_dust2.bsp").unwrap();
        assert_eq!(handle.len(), payload.len());
        assert_eq!(handle[0], 0x42);
        assert_eq!(handle.blake3_hash(), blake3::hash(&payload));
    }
}
