#![cfg(feature = "goldsrc")]

use std::fs;
use std::path::Path;
use tempfile::tempdir;

use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{Bsp30Decoder, StudioMdlDecoder, Wad3Decoder};
use goldhub_formats::ir::{SkeletalRig, StudioModel};
use goldhub_formats::pipeline::AssetPipeline;
use goldhub_formats::transpile::{compile_bsp_collision, compile_bsp_geometry};

const SAMPLE_BSP_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/regamedll_test_map_v8.bsp";
const SAMPLE_WAD_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/src/regamedll_zedit.wad";

#[test]
fn test_inspect_fence_entities() {
    let path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/maps/de_inferno.bsp");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(path).unwrap();
    let bsp = Bsp30Decoder.decode(&bytes).unwrap();
    let textures = bsp.extract_embedded_textures();
    let entities_str = std::str::from_utf8(bsp.entities).unwrap();

    // Find texture indices for {fence and {cufence
    let fence_tex_ids: Vec<usize> = textures
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.name.contains("fence") || t.name.contains("net") || t.name.contains("wire")
        })
        .map(|(i, _)| i)
        .collect();
    let td = tempdir().unwrap();
    let pipeline = AssetPipeline::new(td.path());
    let _bsp = Bsp30Decoder.decode(&bytes).unwrap();
    let (mesh, textures, _, _) = pipeline
        .load_bsp_level_full_with_search_dirs(&bytes, &[])
        .unwrap();
    let cufence_id = textures.iter().position(|t| t.name == "{cufence").unwrap() as u32;
    let count = mesh
        .submeshes
        .iter()
        .filter(|sm| sm.material_id == cufence_id)
        .count();
    println!("Compiled cufence submeshes count: {count}");
    let face = &bsp.faces[7233];
    let ti = &bsp.texture_infos[face.texture_info_id as usize];
    println!(
        "Face 7233: tex_id={}, s_vec={:?}, s_shift={}, t_vec={:?}, t_shift={}",
        ti.texture_id, ti.s, ti.s_shift, ti.t, ti.t_shift
    );
    let sm = mesh
        .submeshes
        .iter()
        .find(|s| s.name == "face_7233")
        .unwrap();
    println!(
        "Submesh face_7233: start={}, count={}",
        sm.index_start, sm.index_count
    );
    for idx in sm.index_start..sm.index_start + sm.index_count {
        let v_idx = mesh.indices[idx as usize];
        let v = &mesh.vertices[v_idx as usize];
        println!("  vert {v_idx}: pos={:?}, uv={:?}", v.position, v.uv0);
    }

    for (m_idx, model) in bsp.models.iter().enumerate() {
        let first = model.first_face as usize;
        let num = model.num_faces as usize;
        for f_idx in first..first + num {
            if let Some(f) = bsp.faces.get(f_idx)
                && let Some(ti) = bsp.texture_infos.get(f.texture_info_id as usize)
                && fence_tex_ids.contains(&(ti.texture_id as usize))
            {
                println!(
                    "Model *{m_idx} uses fence tex {} on face {f_idx}",
                    ti.texture_id
                );
                break;
            }
        }
    }

    for block in entities_str.split('}') {
        if block.contains("fence")
            || block.contains("*1")
            || block.contains("*2")
            || block.contains("*3")
            || block.contains("*4")
            || block.contains("*5")
        {
            // println!("Entity:\n{}", block.trim());
        }
    }
}

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

#[test]
fn test_inspect_nuke_glass_doors_compiled() {
    let path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/maps/de_nuke.bsp");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(path).unwrap();
    let temp_cache = tempdir().unwrap();
    let pipeline = AssetPipeline::new(temp_cache.path());
    let extra_dirs = [Path::new("C:/Games/CS 1.6 GoldClient/cstrike")];
    let _bsp = Bsp30Decoder.decode(&bytes).unwrap();
    let (mesh, textures, _, _) = pipeline
        .load_bsp_level_full_with_search_dirs(&bytes, &extra_dirs)
        .expect("load de_nuke with glass");

    // Verify glass_med is present and has rendered submeshes
    let glass_tex_id = textures.iter().position(|t| t.name == "glass_med");
    assert!(
        glass_tex_id.is_some(),
        "de_nuke should have glass_med texture"
    );
    let glass_id = glass_tex_id.unwrap() as u32;

    let glass_submeshes = mesh
        .submeshes
        .iter()
        .filter(|s| s.material_id == glass_id)
        .count();
    assert!(
        glass_submeshes > 0,
        "de_nuke should have compiled glass submeshes"
    );
    println!("de_nuke compiled glass_med submeshes: {glass_submeshes}");
}

#[test]
fn test_inspect_de_dust2() {
    let path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/maps/de_dust2.bsp");
    if !path.exists() {
        return;
    }

    let bytes = fs::read(path).unwrap();
    let temp_cache = tempdir().unwrap();
    let pipeline = AssetPipeline::new(temp_cache.path());

    let (mesh, textures, _atlas, sky) = pipeline
        .load_bsp_level_full_with_search_dirs(
            &bytes,
            &[Path::new("C:/Games/CS 1.6 GoldClient/cstrike")],
        )
        .expect("load de_dust2 level full with wad resolution");

    println!(
        "de_dust2: mesh vertex_count = {}, submeshes = {}",
        mesh.vertex_count(),
        mesh.submeshes.len()
    );
    println!("de_dust2: skyname = {sky:?}");
    println!("de_dust2: total textures = {}", textures.len());

    let is_tool = |name: &str| {
        let n = name.to_ascii_lowercase();
        n == "aaatrigger"
            || n == "clip"
            || n == "sky"
            || n == "origin"
            || n == "null"
            || n == "hint"
            || n == "skip"
    };

    let unresolved_visual_stubs = textures
        .iter()
        .filter(|t| t.width <= 16 && t.height <= 16 && !is_tool(&t.name))
        .count();
    println!("de_dust2: unresolved non-tool stubs = {unresolved_visual_stubs}");

    assert_eq!(
        unresolved_visual_stubs, 0,
        "All visible world textures on de_dust2 should be resolved!"
    );
}

#[test]
fn test_compile_bsp_collision_brush_entities() {
    let path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/maps/de_inferno.bsp");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(path).unwrap();
    let bsp = Bsp30Decoder.decode(&bytes).unwrap();
    let col = compile_bsp_collision(&bsp).expect("compile collision");

    assert!(
        !col.brush_entities.is_empty(),
        "de_inferno must have brush entities!"
    );
    let solid_count = col
        .brush_entities
        .iter()
        .filter(|e| e.solid == goldhub_formats::ir::SOLID_BSP)
        .count();
    let nonsolid_count = col
        .brush_entities
        .iter()
        .filter(|e| e.solid == goldhub_formats::ir::SOLID_NOT)
        .count();
    println!(
        "de_inferno collision: total physents = {}, solid = {}, nonsolid = {}",
        col.brush_entities.len(),
        solid_count,
        nonsolid_count
    );

    // Verify crates, doors, breakables are solid
    let breakables = col
        .brush_entities
        .iter()
        .filter(|e| e.classname == "func_breakable")
        .count();
    let walls = col
        .brush_entities
        .iter()
        .filter(|e| e.classname == "func_wall")
        .count();
    let doors = col
        .brush_entities
        .iter()
        .filter(|e| e.classname.starts_with("func_door"))
        .count();
    println!("  func_breakable: {breakables}, func_wall: {walls}, doors: {doors}");
    assert!(
        solid_count > 50,
        "Should have dozens of solid brush entities"
    );
}

#[test]
fn test_inspect_inferno_shutters() {
    let path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/maps/de_inferno.bsp");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(path).unwrap();
    let temp_cache = tempdir().unwrap();
    let pipeline = AssetPipeline::new(temp_cache.path());
    let extra_dirs = [Path::new("C:/Games/CS 1.6 GoldClient/cstrike")];
    let bsp = Bsp30Decoder.decode(&bytes).unwrap();
    let (_mesh, _textures, _, _) = pipeline
        .load_bsp_level_full_with_search_dirs(&bytes, &extra_dirs)
        .unwrap();

    let entities_str = std::str::from_utf8(bsp.entities).unwrap();
    for ent_block in entities_str.split('}') {
        if ent_block.contains("\"model\" \"*1\"") || ent_block.contains("\"model\" \"*16\"") {
            println!("FENCE ENTITY BLOCK:\n{}", ent_block.trim());
        }
    }
}

#[test]
fn test_inspect_de_nuke_and_cs_assault() {
    let temp_cache = tempdir().unwrap();
    let pipeline = AssetPipeline::new(temp_cache.path());
    let extra_dirs = [Path::new("C:/Games/CS 1.6 GoldClient/cstrike")];

    let is_tool = |name: &str| {
        let n = name.to_ascii_lowercase();
        n == "aaatrigger"
            || n == "clip"
            || n == "sky"
            || n == "origin"
            || n == "null"
            || n == "hint"
            || n == "skip"
    };

    for map_name in [
        "de_nuke.bsp",
        "cs_assault.bsp",
        "cs_italy.bsp",
        "de_inferno.bsp",
    ] {
        let path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/maps").join(map_name);
        if !path.exists() {
            continue;
        }

        let bytes = fs::read(&path).unwrap();
        let decoder = Bsp30Decoder;
        let _bsp = decoder.decode(&bytes).unwrap();
        let (mesh, textures, _atlas, sky) = pipeline
            .load_bsp_level_full_with_search_dirs(&bytes, &extra_dirs)
            .unwrap_or_else(|e| panic!("failed to load {map_name}: {e:?}"));

        let dummy_data = vec![128; 16 * 16 * 4];
        let mut unresolved_names = Vec::new();
        for t in &textures {
            if t.mips.first().is_some_and(|m| m.data == dummy_data) && !is_tool(&t.name) {
                unresolved_names.push(t.name.clone());
            }
        }
        let unresolved = unresolved_names.len();
        println!("{map_name}: unresolved non-tool textures = {unresolved_names:?}");
        for t in &textures {
            if t.name.starts_with('{') {
                let first_mip = &t.mips[0].data;
                let has_zero_alpha = first_mip.as_chunks::<4>().0.iter().any(|c| c[3] == 0);
                let blue_count = first_mip
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|c| c[0] == 0 && c[1] == 0 && c[2] == 255)
                    .count();
                println!(
                    "  Cutout texture: {} (w={}, h={}, zero_alpha={}, blue_pixels={})",
                    t.name, t.width, t.height, has_zero_alpha, blue_count
                );
            }
        }

        println!(
            "{map_name}: vertices = {}, submeshes = {}, textures = {}, sky = {sky:?}, unresolved = {unresolved}",
            mesh.vertex_count(),
            mesh.submeshes.len(),
            textures.len(),
        );

        assert_eq!(
            unresolved, 0,
            "All visible world textures for {map_name} should be resolved!"
        );
    }
}

#[test]
fn test_decode_real_studio_models() {
    let knife_path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/models/v_knife.mdl");
    if knife_path.exists() {
        let bytes = fs::read(knife_path).expect("read v_knife.mdl");
        let decoder = StudioMdlDecoder;

        // Decode SkeletalRig
        let rig: SkeletalRig = decoder.decode(&bytes).expect("decode knife rig");
        assert_eq!(rig.bones.len(), 39, "v_knife should have 39 bones");
        assert_eq!(rig.hitboxes.len(), 5, "v_knife should have 5 hitboxes");
        assert_eq!(rig.sequences.len(), 8, "v_knife should have 8 sequences");
        assert_eq!(rig.sequences[0].name, "idle");
        assert_eq!(rig.sequences[0].frame_count, 151);
        assert_eq!(rig.sequences[0].bone_matrices.len(), 151);
        assert_eq!(rig.sequences[0].bone_matrices[0].len(), 39);

        // Decode Full StudioModel
        let model: StudioModel = decoder.decode(&bytes).expect("decode knife model");
        assert_eq!(model.textures.len(), 4, "v_knife has 4 embedded textures");
        assert_eq!(model.textures[0].name, "knifeskin.bmp");
        assert_eq!(model.textures[0].width, 512);
        assert_eq!(model.textures[0].height, 512);
        assert_eq!(model.textures[0].rgba_pixels.len(), 512 * 512 * 4);

        assert_eq!(model.body_parts.len(), 1);
        let bp = &model.body_parts[0];
        assert_eq!(bp.name, "studio");
        assert_eq!(bp.models.len(), 1);
        let sub = &bp.models[0];
        assert_eq!(sub.name, "reference");
        assert_eq!(sub.meshes.len(), 4);

        for (m_idx, mesh) in sub.meshes.iter().enumerate() {
            assert!(!mesh.vertices.is_empty(), "mesh {m_idx} has vertices");
            assert!(!mesh.indices.is_empty(), "mesh {m_idx} has indices");
            assert_eq!(
                mesh.indices.len() % 3,
                0,
                "mesh {m_idx} indices form triangles"
            );
        }
    }

    let guerilla_path =
        Path::new("C:/Games/CS 1.6 GoldClient/cstrike/models/player/guerilla/guerilla.mdl");
    if guerilla_path.exists() {
        let bytes = fs::read(guerilla_path).expect("read guerilla.mdl");
        let decoder = StudioMdlDecoder;
        let model: StudioModel = decoder.decode(&bytes).expect("decode guerilla model");
        assert!(!model.rig.bones.is_empty());
        assert!(!model.rig.sequences.is_empty());
        assert_eq!(model.textures.len(), 13, "guerilla has 13 textures");
    }

    let arctic_path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/models/arcticorange.mdl");
    let arctic_t_path = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/models/arcticorangeT.mdl");
    if arctic_path.exists() && arctic_t_path.exists() {
        let model_bytes = fs::read(arctic_path).expect("read arcticorange.mdl");
        let tex_bytes = fs::read(arctic_t_path).expect("read arcticorangeT.mdl");
        let decoder = StudioMdlDecoder;
        let model = decoder
            .decode_model_with_textures(&model_bytes, Some(&tex_bytes))
            .expect("decode arctic with external T.mdl");
        assert_eq!(
            model.textures.len(),
            3,
            "arcticorange external textures loaded"
        );
    }
}
