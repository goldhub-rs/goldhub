use std::fs;
use std::path::Path;

use goldhub_audio::MaterialTable;
use goldhub_audio::wav::decode_wav;

#[test]
fn test_decode_real_cs16_wav_assets() {
    let base_sound_dir = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/sound/player");
    if !base_sound_dir.exists() {
        return;
    }

    let files_to_test = [
        "pl_step1.wav",
        "pl_step2.wav",
        "pl_ladder1.wav",
        "pl_metal1.wav",
        "pl_fallpain1.wav",
    ];

    for file_name in &files_to_test {
        let p = base_sound_dir.join(file_name);
        if p.exists() {
            let bytes = fs::read(&p).expect("Failed to read test WAV");
            let sound = decode_wav(&bytes).expect("decode_wav failed on real CS 1.6 WAV");
            println!(
                "FILE {}: total_bytes={}, rate={}, channels={:?}, format={:?}, samples={}, duration={:.3}s, loop_start={:?}",
                file_name,
                bytes.len(),
                sound.sample_rate,
                sound.channels,
                sound.format,
                sound.sample_count,
                sound.duration_seconds(),
                sound.loop_start
            );
            println!("First 50 bytes: {:?}", &bytes[..50.min(bytes.len())]);
            if let Some(list_pos) = bytes.windows(4).position(|w| w == b"LIST") {
                println!(
                    "LIST chunk: {:?}",
                    String::from_utf8_lossy(&bytes[list_pos..bytes.len()])
                );
            }
            // Print chunks
            let mut pos = 12;
            while pos + 8 <= bytes.len() {
                let id = &bytes[pos..pos + 4];
                let size = u32::from_le_bytes([
                    bytes[pos + 4],
                    bytes[pos + 5],
                    bytes[pos + 6],
                    bytes[pos + 7],
                ]) as usize;
                let id_str = String::from_utf8_lossy(id);
                println!("   Chunk '{}' size={} offset={}", id_str, size, pos);
                pos += 8 + size;
                if size % 2 != 0 {
                    pos += 1;
                }
            }
            let transcoded = goldhub_audio::transcode_to_clean_16bit_44khz(&sound);
            let back = decode_wav(&transcoded).expect("transcoded WAV decode succeeds");
            assert_eq!(back.sample_rate, 44100);
            assert_eq!(back.format, goldhub_audio::SampleFormat::I16);
            assert!(back.sample_count > sound.sample_count); // 11025 -> 44100 is ~4x samples
        }
    }
}

#[test]
fn test_parse_real_cs16_materials_txt() {
    let p = Path::new("C:/Games/CS 1.6 GoldClient/cstrike/sound/materials.txt");
    if !p.exists() {
        return;
    }
    let content = fs::read_to_string(p).expect("Failed to read materials.txt");
    let table =
        MaterialTable::parse_materials_txt(&content).expect("Failed to parse materials.txt");
    assert!(table.len() > 100);

    // Canonical fallback to Concrete:
    // Unknown or unlisted textures must strictly return Concrete ('C')
    assert_eq!(
        table.find_texture_type("camo_wall16"),
        goldhub_audio::MaterialType::Concrete
    );
    assert_eq!(
        table.find_texture_type("out_roof4"),
        goldhub_audio::MaterialType::Concrete
    );
    assert_eq!(
        table.find_texture_type(""),
        goldhub_audio::MaterialType::Concrete
    );
    assert_eq!(
        table.find_texture_type("-0"),
        goldhub_audio::MaterialType::Concrete
    );
    // Reverse prefix matching must never happen (e.g. query "cstrike_" must not match "cstrike_CE2XCng")
    assert_eq!(
        table.find_texture_type("cstrike_"),
        goldhub_audio::MaterialType::Concrete
    );

    // Verified known textures from CS 1.6 materials.txt:
    assert_eq!(
        table.find_texture_type("generic015"),
        goldhub_audio::MaterialType::Grate
    );
    assert_eq!(
        table.find_texture_type("lab1_stair2b"),
        goldhub_audio::MaterialType::Metal
    );
}
