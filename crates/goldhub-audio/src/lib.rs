//! Canonical GoldSrc / CS 1.6 audio engine, WAV PCM decoder, materials texture parser,
//! footstep physics sound system, and 3D spatial attenuation model.

pub mod attenuation;
pub mod error;
pub mod footsteps;
pub mod materials;
pub mod wav;

pub use attenuation::{
    ATTN_IDLE, ATTN_NONE, ATTN_NORM, ATTN_STATIC, SND_CLIP_DISTANCE, SpatialVolume, spatialize_3d,
    spatialize_channel,
};
pub use error::AudioError;
pub use footsteps::{
    CHAN_BODY, FootstepSoundEvent, PM_PLAYER_FALL_PUNCH_THRESHHOLD, PM_PLAYER_MAX_SAFE_FALL_SPEED,
    PM_PLAYER_MIN_BOUNCE_SPEED, StepSoundContext, StepSoundController, WaterPosition,
};
pub use materials::{
    MAX_TEXTURENAME_LENGTH, MAX_TEXTURES, MaterialEntry, MaterialTable, MaterialType, StepType,
    sanitize_texture_name,
};
pub use wav::{
    AudioChannels, PcmData, RiffChunkHeader, SampleFormat, WavSound, WaveFmtPcm, decode_wav,
    encode_wav_pcm, is_supported_sample_rate, transcode_to_clean_16bit_44khz,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wav_pcm_decode_mono_16bit() {
        let sample_rate = 22050;
        let pcm_raw: Vec<i16> = vec![0, 1000, -1000, 2000, -2000, 0];
        let mut bytes = Vec::new();
        for &s in &pcm_raw {
            bytes.extend_from_slice(&s.to_le_bytes());
        }

        let wav_bytes = encode_wav_pcm(AudioChannels::Mono, sample_rate, 16, &bytes);
        let decoded = decode_wav(&wav_bytes).expect("WAV decode succeeds");

        assert_eq!(decoded.channels, AudioChannels::Mono);
        assert_eq!(decoded.sample_rate, 22050);
        assert_eq!(decoded.format, SampleFormat::I16);
        assert_eq!(decoded.sample_count, pcm_raw.len());
        assert_eq!(decoded.data, PcmData::I16(pcm_raw));
    }

    #[test]
    fn test_wav_pcm_decode_stereo_8bit() {
        let sample_rate = 11025;
        let pcm_raw: Vec<u8> = vec![128, 128, 200, 50, 255, 0];
        let wav_bytes = encode_wav_pcm(AudioChannels::Stereo, sample_rate, 8, &pcm_raw);
        let decoded = decode_wav(&wav_bytes).expect("WAV decode succeeds");

        assert_eq!(decoded.channels, AudioChannels::Stereo);
        assert_eq!(decoded.sample_rate, 11025);
        assert_eq!(decoded.format, SampleFormat::U8);
        assert_eq!(decoded.sample_count, 3); // 6 bytes / 2 channels = 3 frames
        assert_eq!(decoded.data, PcmData::U8(pcm_raw));
    }

    #[test]
    fn test_wav_pcm_cue_loop_parsing() {
        let sample_rate = 44100;
        let pcm_raw: Vec<u8> = vec![128; 100];
        let mut wav_bytes = encode_wav_pcm(AudioChannels::Mono, sample_rate, 8, &pcm_raw);

        // Append a cue chunk with loopstart = 24
        let mut cue_payload = vec![0u8; 36];
        cue_payload[32..36].copy_from_slice(&24u32.to_le_bytes());

        wav_bytes.extend_from_slice(b"cue ");
        wav_bytes.extend_from_slice(&(cue_payload.len() as u32).to_le_bytes());
        wav_bytes.extend_from_slice(&cue_payload);

        let decoded = decode_wav(&wav_bytes).expect("WAV with cue succeeds");
        assert_eq!(decoded.loop_start, Some(24));
    }

    #[test]
    fn test_materials_parsing_and_lookup() {
        let sample_materials = r#"
// GoldSrc sound materials map
C OUT_CONC1
M VENT_METAL1
D DIRT_FLOOR
V DUCT_WALL
G GRATE_STEEL
T TILE_FLOOR
S WATER_PUDDLE
W WOOD_CRATE
"#;
        let table = MaterialTable::parse_materials_txt(sample_materials).expect("parse materials");
        assert_eq!(table.len(), 8);

        // Concrete lookup
        assert_eq!(table.find_texture_type("OUT_CONC1"), MaterialType::Concrete);
        // Animated/special texture prefix stripping
        assert_eq!(
            table.find_texture_type("+0VENT_METAL1"),
            MaterialType::Metal
        );
        assert_eq!(table.find_texture_type("{GRATE_STEEL"), MaterialType::Grate);
        assert_eq!(
            table.find_texture_type("!WATER_PUDDLE"),
            MaterialType::Slosh
        );
        assert_eq!(table.find_texture_type("WOOD_CRATE"), MaterialType::Wood);

        // Unregistered textures default to Concrete
        assert_eq!(
            table.find_texture_type("UNKNOWN_TEX"),
            MaterialType::Concrete
        );
    }

    #[test]
    fn test_step_sound_alternating_feet_and_random() {
        let mut controller = StepSoundController::new();

        // Right foot first, random_bit 0 -> pl_step1.wav
        let snd1 = controller
            .play_step_sound(StepType::Concrete, 1.0, 0, 1)
            .expect("sound emitted");
        assert!(controller.step_left);
        assert_eq!(snd1.sound_path, "player/pl_step2.wav"); // left foot toggle: 0 + 2 = 2 -> pl_step2

        // Next step: toggles back to right foot: 1 + 0 = 1 -> pl_step3
        let snd2 = controller
            .play_step_sound(StepType::Concrete, 1.0, 1, 1)
            .expect("sound emitted");
        assert!(!controller.step_left);
        assert_eq!(snd2.sound_path, "player/pl_step3.wav");
    }

    #[test]
    fn test_step_sound_timing_and_ducking_volume() {
        let mut controller = StepSoundController::new();
        let table = MaterialTable::new();

        let ctx_walk = StepSoundContext {
            velocity: [200.0, 0.0, 0.0],
            on_ground: true,
            on_ladder: false,
            is_ducking: false,
            duck_time_ms: 0.0,
            is_frozen: false,
            fall_velocity: 0.0,
            is_dead: false,
            water_pos: WaterPosition::NotInWater,
            ground_texture: None,
        };

        // Normal walk on concrete
        let snd = controller
            .update_step_sound(&ctx_walk, &table, 0, 1)
            .expect("walk step sound");
        assert_eq!(snd.volume, 0.5);
        assert_eq!(controller.time_step_sound_ms, 300.0);

        // Cooldown active: next query must yield None
        assert!(
            controller
                .update_step_sound(&ctx_walk, &table, 0, 1)
                .is_none()
        );

        // Advance cooldown
        controller.advance_time(300.0);
        assert_eq!(controller.time_step_sound_ms, 0.0);

        // Ducking step: +100ms cooldown, 35% volume scaling
        let ctx_duck = StepSoundContext {
            velocity: [180.0, 0.0, 0.0],
            on_ground: true,
            on_ladder: false,
            is_ducking: true,
            duck_time_ms: 500.0, // < 950.0
            is_frozen: false,
            fall_velocity: 0.0,
            is_dead: false,
            water_pos: WaterPosition::NotInWater,
            ground_texture: None,
        };

        let duck_snd = controller
            .update_step_sound(&ctx_duck, &table, 0, 1)
            .expect("duck step sound");
        assert!((duck_snd.volume - (0.5 * 0.35)).abs() < 1e-5);
        assert_eq!(controller.time_step_sound_ms, 400.0); // 300 + 100
    }

    #[test]
    fn test_ladder_sound_timing() {
        let mut controller = StepSoundController::new();
        let table = MaterialTable::new();

        let ctx_ladder = StepSoundContext {
            velocity: [0.0, 0.0, 160.0],
            on_ground: false,
            on_ladder: true,
            is_ducking: false,
            duck_time_ms: 0.0,
            is_frozen: false,
            fall_velocity: 0.0,
            is_dead: false,
            water_pos: WaterPosition::NotInWater,
            ground_texture: None,
        };

        let snd = controller
            .update_step_sound(&ctx_ladder, &table, 0, 1)
            .expect("ladder sound");
        assert_eq!(snd.step_type, StepType::Ladder);
        assert_eq!(snd.volume, 0.35);
        assert_eq!(controller.time_step_sound_ms, 450.0); // 350 + 100 (fLadder adds 100)
    }

    #[test]
    fn test_falling_sound_impact() {
        let mut controller = StepSoundController::new();
        let table = MaterialTable::new();

        let mut ctx = StepSoundContext {
            velocity: [0.0, 0.0, 0.0],
            on_ground: true,
            on_ladder: false,
            is_ducking: false,
            duck_time_ms: 0.0,
            is_frozen: false,
            fall_velocity: 200.0, // below 250 threshold
            is_dead: false,
            water_pos: WaterPosition::NotInWater,
            ground_texture: None,
        };

        // Below threshold: no sound
        assert!(controller.check_falling(&ctx, &table, 0, 1).is_none());

        // Above threshold, between 250 and 290: vol = 0.0 -> no sound
        ctx.fall_velocity = 270.0;
        assert!(controller.check_falling(&ctx, &table, 0, 1).is_none());

        // Between 290 and 580: vol = 0.85
        ctx.fall_velocity = 400.0;
        let snd_medium = controller
            .check_falling(&ctx, &table, 0, 1)
            .expect("fall impact sound");
        assert_eq!(snd_medium.volume, 0.85);

        // Greater than 580: vol = 1.0
        ctx.fall_velocity = 650.0;
        let snd_hard = controller
            .check_falling(&ctx, &table, 0, 1)
            .expect("hard fall sound");
        assert_eq!(snd_hard.volume, 1.0);
    }

    #[test]
    fn test_spatial_sound_attenuation_and_panning() {
        let listener_origin = [0.0, 0.0, 0.0];
        let listener_right = [1.0, 0.0, 0.0]; // Facing +Y, right is +X

        // Sound straight to the right at 500 units with ATTN_NORM (0.8)
        let sound_right = [500.0, 0.0, 0.0];
        let vol = spatialize_3d(listener_origin, listener_right, sound_right, 1.0, ATTN_NORM);

        // Sound on the right should have higher right volume than left volume
        assert!(vol.right_255 > vol.left_255);
        assert!(vol.left_255 == 0); // (1 - 0.4)*(1 - 1.0) = 0

        // Sound straight in front: dot = 0
        let sound_front = [0.0, 500.0, 0.0];
        let vol_front = spatialize_3d(listener_origin, listener_right, sound_front, 1.0, ATTN_NORM);
        assert_eq!(vol_front.left_255, vol_front.right_255);
        // dist_scaled = 500 * (0.8 / 1000) = 0.4. scale = (1 - 0.4) * 1.0 = 0.6. master_vol = 255.
        // 255 * 0.6 = 153.
        assert_eq!(vol_front.left_255, 153);
    }
}
