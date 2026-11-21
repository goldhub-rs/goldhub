#![allow(unsafe_code)]
#![allow(clippy::cast_lossless, clippy::uninlined_format_args)]

//! Comparative Benchmark: GoldSrc Legacy Asset Loading vs FlatFormats Zero-Copy Mounting.
//!
//! Measures latency and memory throughput across real assets from Counter-Strike 1.6:
//! 1. Maps: Legacy BSP30 (de_dust2.bsp) parsing vs FlatWorld (de_dust2.flb) zero-copy mount.
//! 2. Models: StudioMDL (v_ak47.mdl) decoding vs FlatMesh (v_ak47.flb) mount.
//! 3. Audio: RIFF WAV (Birds1.wav) heap reading vs FlatAudio (Birds1.fls) zero-copy view.

use std::fs::File;
use std::path::Path;
use std::time::Instant;

use flatasset_audio::FlatAudioView;
use flatasset_core::bundle::FlatBundleView;
use flatasset_mesh::FlatMeshView;
use flatasset_world::FlatWorldView;
use memmap2::Mmap;

const LEGACY_BSP: &str = r"C:\Games\CS 1.6 GoldClient\cstrike\maps\de_dust2.bsp";
const FLAT_BSP_BUNDLE: &str = r"D:\Repo\cstrike_flat\maps\de_dust2_raw.flb";
const FLAT_BSP_RAW: &str = r"D:\Repo\flatworld-rs\converted_maps\de_dust2.flw";

const LEGACY_MDL: &str = r"C:\Games\CS 1.6 GoldClient\cstrike\models\v_ak47.mdl";
const FLAT_MDL_BUNDLE: &str = r"D:\Repo\cstrike_flat\models\v_ak47_raw.flb";
const FLAT_MDL_RAW: &str = r"D:\Repo\cstrike_flat\models\v_ak47.flm";

const LEGACY_WAV: &str = r"C:\Games\CS 1.6 GoldClient\cstrike\sound\ambience\Birds1.wav";
const FLAT_FLS: &str = r"D:\Repo\cstrike_flat\sound\ambience\Birds1.fls";

fn bench_maps() {
    println!("\n================================================================================");
    println!(" [1/3] MAP LOADING: Legacy BSP30 vs FlatWorld (.flb / .flw)");
    println!("================================================================================");

    if Path::new(LEGACY_BSP).exists() {
        let bsp_bytes = std::fs::read(LEGACY_BSP).expect("read legacy bsp");
        let start = Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            let bsp =
                goldhub_formats::codecs::legacy::bsp::BspLevel::from_bytes(&bsp_bytes).unwrap();
            let _collision = goldhub_formats::transpile::compile_bsp_collision(&bsp).unwrap();
        }
        let elapsed = start.elapsed();
        let per_op = elapsed.as_secs_f64() * 1e3 / (iterations as f64);
        println!(
            "Legacy BSP30 Parse + Collision (de_dust2.bsp, {} KB): {:.3} ms / op",
            bsp_bytes.len() / 1024,
            per_op
        );
    }

    if Path::new(FLAT_BSP_RAW).exists() {
        let file = File::open(FLAT_BSP_RAW).expect("open flat bsp raw");
        // SAFETY: File is mapped read-only for benchmarking zero-copy view mounting.
        let mmap = unsafe { Mmap::map(&file) }.expect("mmap flat bsp");
        let start = Instant::now();
        let iterations = 100_000;
        for _ in 0..iterations {
            let _world = FlatWorldView::new(&mmap).unwrap();
        }
        let elapsed = start.elapsed();
        let per_op_ns = elapsed.as_secs_f64() * 1e9 / (iterations as f64);
        let per_op_us = per_op_ns / 1e3;
        println!(
            "FlatWorld Direct mmap (de_dust2.flw, {} KB): {:.3} µs / op ({:.1} ns)",
            mmap.len() / 1024,
            per_op_us,
            per_op_ns
        );
    }

    if Path::new(FLAT_BSP_BUNDLE).exists() {
        let file = File::open(FLAT_BSP_BUNDLE).expect("open flat bsp bundle");
        // SAFETY: File is mapped read-only for benchmarking zero-copy view mounting.
        let mmap = unsafe { Mmap::map(&file) }.expect("mmap flat bsp");
        let start = Instant::now();
        let iterations = 50_000;
        for _ in 0..iterations {
            let bundle = FlatBundleView::new(&mmap).unwrap();
            let first_item = &bundle.items()[0];
            let item_bytes = bundle.item_bytes(first_item).unwrap();
            let _world = FlatWorldView::new(item_bytes).unwrap();
        }
        let elapsed = start.elapsed();
        let per_op_ns = elapsed.as_secs_f64() * 1e9 / (iterations as f64);
        let per_op_us = per_op_ns / 1e3;
        println!(
            "FlatBundle + World View (de_dust2_raw.flb): {:.3} µs / op ({:.1} ns)",
            per_op_us, per_op_ns
        );
    }
}

fn bench_models() {
    println!("\n================================================================================");
    println!(" [2/3] MODEL LOADING: StudioMDL vs FlatMesh (.flb / .flm)");
    println!("================================================================================");

    if Path::new(LEGACY_MDL).exists() {
        let mdl_bytes = std::fs::read(LEGACY_MDL).expect("read legacy mdl");
        let decoder = goldhub_formats::codecs::legacy::StudioMdlDecoder;
        let start = Instant::now();
        let iterations = 500;
        for _ in 0..iterations {
            let _model = decoder
                .decode_model_with_textures(&mdl_bytes, None)
                .unwrap();
        }
        let elapsed = start.elapsed();
        let per_op = elapsed.as_secs_f64() * 1e3 / (iterations as f64);
        println!(
            "Legacy StudioMDL Decode (v_ak47.mdl, {} KB): {:.3} ms / op",
            mdl_bytes.len() / 1024,
            per_op
        );
    }

    if Path::new(FLAT_MDL_RAW).exists() {
        let file = File::open(FLAT_MDL_RAW).expect("open flat mdl raw");
        // SAFETY: File is mapped read-only for benchmarking zero-copy view mounting.
        let mmap = unsafe { Mmap::map(&file) }.expect("mmap flat mdl");
        let start = Instant::now();
        let iterations = 200_000;
        for _ in 0..iterations {
            let _mesh = FlatMeshView::new(&mmap).unwrap();
        }
        let elapsed = start.elapsed();
        let per_op_ns = elapsed.as_secs_f64() * 1e9 / (iterations as f64);
        println!(
            "FlatMesh Direct mmap (v_ak47.flm, {} KB):    {:.1} ns / op",
            mmap.len() / 1024,
            per_op_ns
        );
    }

    if Path::new(FLAT_MDL_BUNDLE).exists() {
        let file = File::open(FLAT_MDL_BUNDLE).expect("open flat mdl bundle");
        // SAFETY: File is mapped read-only for benchmarking zero-copy view mounting.
        let mmap = unsafe { Mmap::map(&file) }.expect("mmap flat mdl");
        let start = Instant::now();
        let iterations = 100_000;
        for _ in 0..iterations {
            let bundle = FlatBundleView::new(&mmap).unwrap();
            let item = &bundle.items()[0];
            let item_bytes = bundle.item_bytes(item).unwrap();
            let _mesh = FlatMeshView::new(item_bytes).unwrap();
        }
        let elapsed = start.elapsed();
        let per_op_ns = elapsed.as_secs_f64() * 1e9 / (iterations as f64);
        println!(
            "FlatBundle + Mesh View (v_ak47_raw.flb):     {:.1} ns / op",
            per_op_ns
        );
    }
}

fn bench_audio() {
    println!("\n================================================================================");
    println!(" [3/3] AUDIO LOADING: RIFF WAV vs FlatAudio (.fls)");
    println!("================================================================================");

    if Path::new(LEGACY_WAV).exists() {
        let wav_bytes = std::fs::read(LEGACY_WAV).expect("read legacy wav");
        let start = Instant::now();
        let iterations = 10_000;
        for _ in 0..iterations {
            // Simulate typical WAV parse: checking headers and heap allocation of PCM
            let pcm = wav_bytes[44..].to_vec();
            std::hint::black_box(pcm);
        }
        let elapsed = start.elapsed();
        let per_op_us = elapsed.as_secs_f64() * 1e6 / (iterations as f64);
        println!(
            "Legacy RIFF WAV Reader (Birds1.wav, {} KB): {:.3} µs / op",
            wav_bytes.len() / 1024,
            per_op_us
        );
    }

    if Path::new(FLAT_FLS).exists() {
        let file = File::open(FLAT_FLS).expect("open flat fls");
        // SAFETY: File is mapped read-only for benchmarking zero-copy view mounting.
        let mmap = unsafe { Mmap::map(&file) }.expect("mmap flat fls");
        let start = Instant::now();
        let iterations = 100_000;
        for _ in 0..iterations {
            let view = FlatAudioView::new(&mmap).unwrap();
            if view.packet_count() > 0 {
                std::hint::black_box(view.packet_bytes(0).unwrap());
            }
        }
        let elapsed = start.elapsed();
        let per_op_ns = elapsed.as_secs_f64() * 1e9 / (iterations as f64);
        println!(
            "FlatAudio Zero-Copy Mount (Birds1.fls, {} KB): {:.2} ns / op",
            mmap.len() / 1024,
            per_op_ns
        );
    }
}

fn main() {
    println!("================================================================================");
    println!("          GOLDHUB ASSET LOADING SPEED BENCHMARK: LEGACY VS FLATFORMATS          ");
    println!("================================================================================");

    bench_maps();
    bench_models();
    bench_audio();

    println!("\n================================================================================");
    println!("                      BENCHMARK EXECUTION COMPLETE                              ");
    println!("================================================================================");
}
