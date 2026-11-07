#![cfg(feature = "goldsrc")]

use std::fs;
use std::path::Path;
use tempfile::tempdir;

use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{Bsp30Decoder, Wad3Decoder};
use goldhub_formats::pipeline::AssetPipeline;
use goldhub_formats::transpile::{compile_bsp_collision, compile_bsp_geometry};

const SAMPLE_BSP_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/regamedll_test_map_v8.bsp";
const SAMPLE_WAD_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/src/regamedll_zedit.wad";

#[test]
fn test_decode_real_wad3_archive() {
    let path = Path::new(SAMPLE_WAD_PATH);
    if !path.exists() {
        eprintln!("Skipping test: sample WAD3 file not found");
        return;
    }

    let bytes = fs::read(path).expect("read sample wad");
    let decoder = Wad3Decoder;
    let wad = decoder.decode(&bytes).expect("decode sample wad");

    assert!(!wad.entries.is_empty(), "WAD should contain entries");

    // Unpack first texture entry
    let first_tex_entry = wad.entries.iter().find(|e| e.entry_type == 0x43);
    assert!(
        first_tex_entry.is_some(),
        "WAD should contain at least one texture"
    );

    let tex_desc = wad
        .unpack_miptex(first_tex_entry.unwrap())
        .expect("unpack miptex into RGBA8 descriptor");

    assert!(tex_desc.width > 0);
    assert!(tex_desc.height > 0);
    assert_eq!(tex_desc.mips.len(), 1);
    assert_eq!(
        tex_desc.mips[0].data.len(),
        (tex_desc.width * tex_desc.height * 4) as usize
    );
}

#[test]
fn test_decode_and_transpile_real_bsp30_level() {
    let path = Path::new(SAMPLE_BSP_PATH);
    if !path.exists() {
        eprintln!("Skipping test: sample BSP30 file not found");
        return;
    }

    let bytes = fs::read(path).expect("read sample bsp");
    let decoder = Bsp30Decoder;
    let bsp = decoder.decode(&bytes).expect("decode sample bsp");

    assert!(!bsp.faces.is_empty());
    assert!(!bsp.vertices.is_empty());
    assert!(!bsp.planes.is_empty());
    assert!(!bsp.clip_nodes.is_empty());

    // 1. Transpile geometry into Canonical MeshBuffer
    let mesh = compile_bsp_geometry(&bsp).expect("compile bsp geometry");
    assert!(mesh.vertex_count() > 0);
    assert!(mesh.index_count() > 0);
    assert!(!mesh.submeshes.is_empty());

    // 2. Transpile collision hulls
    let collision = compile_bsp_collision(&bsp).expect("compile bsp collision");
    assert!(!collision.hulls.is_empty());
    assert!(!collision.planes.is_empty());

    // 3. Test Pipeline caching
    let cache_dir = tempdir().unwrap();
    let pipeline = AssetPipeline::new(cache_dir.path());

    // Cold load
    let cold_mesh = pipeline.load_bsp_mesh(&bytes).expect("cold load bsp mesh");
    assert_eq!(cold_mesh.vertex_count(), mesh.vertex_count());

    // Warm load (from CAS cache)
    let warm_mesh = pipeline
        .load_bsp_mesh(&bytes)
        .expect("warm load from cache");
    assert_eq!(warm_mesh.vertex_count(), mesh.vertex_count());
    assert_eq!(warm_mesh.index_count(), mesh.index_count());
}


#[test]
fn test_decode_real_skybox_tga() {
    let tga_path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/gfx/env/greenup.tga");
    if !tga_path.exists() {
        eprintln!("Skipping test: greenup.tga not found");
        return;
    }

    let bytes = fs::read(tga_path).expect("read sample tga");
    let tga = goldhub_formats::codecs::legacy::decode_tga(&bytes).expect("decode tga");

    assert_eq!(tga.width, 256);
    assert_eq!(tga.height, 256);
    assert_eq!(tga.rgba.len(), 256 * 256 * 4);
}
