//! GoldSrc audio subsystem integration for Bevy.
//!
//! Connects `goldhub-audio` (1-to-1 PM_PlayStepSound, PM_CheckFalling, materials.txt)
//! with Bevy Audio 3D spatial playback and real CS 1.6 game sound files.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use goldhub_audio::{MaterialTable, StepSoundContext, WaterPosition};

use crate::player::{CameraControlMode, PlayerController};

/// Search paths for resolving game assets (WAD, BSP, WAV sounds).
#[derive(Resource, Default, Debug, Clone)]
pub struct GameSearchPaths {
    pub dirs: Vec<PathBuf>,
}

/// Central audio asset bank and sound resolver for GoldSrc WAV sounds.
#[derive(Resource)]
pub struct GoldSrcAudioBank {
    pub materials: MaterialTable,
    pub sound_cache: HashMap<String, Handle<AudioSource>>,
}

impl Default for GoldSrcAudioBank {
    fn default() -> Self {
        Self {
            materials: MaterialTable::new(),
            sound_cache: HashMap::new(),
        }
    }
}

impl GoldSrcAudioBank {
    /// Resolves and loads a WAV sound by relative path (e.g. `"player/pl_step1.wav"`),
    /// caching the resulting Bevy `Handle<AudioSource>`.
    pub fn get_or_load_sound(
        &mut self,
        rel_path: &str,
        search_dirs: &[PathBuf],
        audio_assets: &mut Assets<AudioSource>,
    ) -> Option<Handle<AudioSource>> {
        let clean_path = rel_path.replace('\\', "/");
        if let Some(handle) = self.sound_cache.get(&clean_path) {
            return Some(handle.clone());
        }

        // Search candidates across search directories
        let mut candidates = Vec::new();
        for dir in search_dirs {
            candidates.push(dir.join("sound").join(&clean_path));
            candidates.push(dir.join(&clean_path));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("valve").join("sound").join(&clean_path));
                candidates.push(parent.join("cstrike").join("sound").join(&clean_path));
            }
        }
        // Fallback relative paths
        candidates.push(PathBuf::from("sound").join(&clean_path));
        candidates.push(PathBuf::from("assets").join("sound").join(&clean_path));

        for candidate in candidates {
            if candidate.exists()
                && let Ok(bytes) = fs::read(&candidate)
            {
                tracing::info!("Loaded GoldSrc audio asset: {:?}", candidate);
                let audio_source = AudioSource {
                    bytes: Arc::from(bytes.into_boxed_slice()),
                };
                let handle = audio_assets.add(audio_source);
                self.sound_cache.insert(clean_path, handle.clone());
                return Some(handle);
            }
        }

        tracing::debug!("GoldSrc sound not found on disk: {}", clean_path);
        None
    }
}

/// Plugin integrating GoldSrc player footsteps, falling impacts, and material sound effects.
pub struct GoldSrcAudioPlugin;

impl Plugin for GoldSrcAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameSearchPaths>();
        app.init_resource::<GoldSrcAudioBank>();
        app.add_systems(Startup, setup_goldsrc_audio_bank);
        app.add_systems(
            Update,
            footstep_sound_system.after(crate::player::player_movement_system),
        );
    }
}

fn setup_goldsrc_audio_bank(
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
) {
    // Attempt to load materials.txt from search paths or standard CS 1.6 directories
    let mut material_candidates = Vec::new();
    for dir in &search_paths.dirs {
        material_candidates.push(dir.join("sound").join("materials.txt"));
        material_candidates.push(dir.join("materials.txt"));
        if let Some(parent) = dir.parent() {
            material_candidates.push(parent.join("cstrike").join("sound").join("materials.txt"));
            material_candidates.push(parent.join("valve").join("sound").join("materials.txt"));
        }
    }
    // Hardcoded common locations check
    material_candidates.push(PathBuf::from(
        "C:/Games/CS 1.6 GoldClient/cstrike/sound/materials.txt",
    ));
    material_candidates.push(PathBuf::from(
        "C:/Games/CS 1.6 GoldClient/valve/sound/materials.txt",
    ));

    for candidate in material_candidates {
        if candidate.exists()
            && let Ok(content) = fs::read_to_string(&candidate)
            && let Ok(table) = MaterialTable::parse_materials_txt(&content)
        {
            tracing::info!(
                "Loaded GoldSrc materials.txt from {:?} ({} entries)",
                candidate,
                table.len()
            );
            audio_bank.materials = table;
            return;
        }
    }

    tracing::warn!("GoldSrc materials.txt not found, using default concrete mapping");
}

/// Evaluates player movement and falls each frame, spawning 3D spatialized sound instances.
pub fn footstep_sound_system(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_assets: ResMut<Assets<AudioSource>>,
    mut query: Query<(&Transform, &mut PlayerController)>,
) {
    if *mode != CameraControlMode::Player {
        return;
    }

    let dt_ms = time.delta_secs() * 1000.0;
    if dt_ms <= 0.0 {
        return;
    }

    for (transform, mut player) in &mut query {
        if !player.is_active {
            continue;
        }

        // Advance step timer cooldown
        player.step_controller.advance_time(dt_ms);

        let on_ground = player.sim.current.on_ground;
        let was_on_ground = player.was_on_ground;
        let on_ladder = player.sim.current.on_ladder;
        let seed_step = player.seed_step & 1;
        let seed_vol = player.seed_vol % 5;

        let ctx = StepSoundContext {
            velocity: player.sim.current.velocity,
            on_ground,
            on_ladder,
            is_ducking: player.sim.current.is_ducking,
            duck_time_ms: 0.0,
            is_frozen: false,
            fall_velocity: player.sim.current.fall_velocity,
            is_dead: false,
            water_pos: WaterPosition::NotInWater,
            ground_texture: None, // Maps to Concrete or Ladder by specification
        };

        // 1. Landing / Falling impact check (PM_CheckFalling)
        if on_ground
            && !was_on_ground
            && let Some(fall_event) = player.step_controller.check_falling(
                &ctx,
                &audio_bank.materials,
                seed_step,
                seed_vol,
            )
            && let Some(handle) = audio_bank.get_or_load_sound(
                fall_event.sound_path,
                &search_paths.dirs,
                &mut audio_assets,
            )
        {
            let foot_pos = transform.translation - Vec3::new(0.0, 36.0, 0.0);
            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings::DESPAWN
                    .with_spatial(true)
                    .with_volume(Volume::new(fall_event.volume)),
                Transform::from_translation(foot_pos),
            ));
        }

        // 2. Regular footstep sound check (PM_PlayStepSound & PM_UpdateStepSound)
        player.seed_step = player.seed_step.wrapping_add(1);
        player.seed_vol = player.seed_vol.wrapping_add(1);

        let next_seed_step = player.seed_step & 1;
        let next_seed_vol = player.seed_vol % 5;

        if let Some(step_event) = player.step_controller.update_step_sound(
            &ctx,
            &audio_bank.materials,
            next_seed_step,
            next_seed_vol,
        ) && let Some(handle) = audio_bank.get_or_load_sound(
            step_event.sound_path,
            &search_paths.dirs,
            &mut audio_assets,
        ) {
            let foot_pos = transform.translation - Vec3::new(0.0, 36.0, 0.0);
            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings::DESPAWN
                    .with_spatial(true)
                    .with_volume(Volume::new(step_event.volume)),
                Transform::from_translation(foot_pos),
            ));
        }

        player.was_on_ground = on_ground;
    }
}
