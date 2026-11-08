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
            assert!(sound.sample_rate > 0);
            assert!(sound.sample_count > 0);
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
}
