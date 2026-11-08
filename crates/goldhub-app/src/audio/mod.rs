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

/// Default CS 1.6 game directories for resolving audio and materials.txt.
pub const DEFAULT_GAME_DIRS: &[&str] = &[
    "C:/Games/CS 1.6 GoldClient/cstrike",
    "C:/Games/CS 1.6 GoldClient/valve",
    "C:/Games/CS 1.6 GoldClient/cstrike_downloads",
    "D:/Repo/CS16/cstrike",
    "D:/Repo/CS16/valve",
    "cstrike",
    "valve",
];

/// Collects candidate game search directories in priority order:
/// 1. Extra directories provided from level setup (e.g. BSP directory and parent)
/// 2. GOLDHUB_GAME_DIR / GOLDS_GAME_DIR environment variable overrides
/// 3. DEFAULT_GAME_DIRS known installations
#[must_use]
pub fn collect_game_search_dirs(extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for d in extra_dirs {
        dirs.push(d.clone());
    }
    if let Ok(env_dir) = std::env::var("GOLDHUB_GAME_DIR") {
        dirs.push(PathBuf::from(&env_dir));
        dirs.push(PathBuf::from(&env_dir).join("cstrike"));
        dirs.push(PathBuf::from(&env_dir).join("valve"));
    }
    if let Ok(env_dir) = std::env::var("GOLDS_GAME_DIR") {
        dirs.push(PathBuf::from(&env_dir));
        dirs.push(PathBuf::from(&env_dir).join("cstrike"));
        dirs.push(PathBuf::from(&env_dir).join("valve"));
    }
    for def in DEFAULT_GAME_DIRS {
        dirs.push(PathBuf::from(def));
    }
    dirs
}

/// Search paths for resolving game assets (WAD, BSP, WAV sounds).
#[derive(Resource, Default, Debug, Clone)]
pub struct GameSearchPaths {
    pub dirs: Vec<PathBuf>,
}

/// Upward-facing walkable ground face in the level for authentic surface texture detection.
#[derive(Debug, Clone)]
pub struct GroundFace {
    pub aabb_min: [f32; 2],
    pub aabb_max: [f32; 2],
    pub plane_z: f32,
    pub plane_normal: [f32; 3],
    pub plane_dist: f32,
    pub vertices: Vec<[f32; 2]>,
    pub texture_name: String,
}

/// Level-wide spatial index of walkable floor surfaces mapping 2D positions to texture names.
#[derive(Resource, Default, Debug, Clone)]
pub struct LevelGroundMaterialIndex {
    pub faces: Vec<GroundFace>,
}

impl LevelGroundMaterialIndex {
    /// Traces straight down from player origin (down 64 units) to find the hit surface texture name.
    /// Matches canonical GoldSrc `PM_CatagorizeTextureType` & `TraceTexture`.
    #[must_use]
    pub fn trace_texture(&self, origin: [f32; 3]) -> Option<&str> {
        let px = origin[0];
        let py = origin[1];
        let pz = origin[2];
        let feet_z = pz - 36.0;

        let mut best_z = f32::NEG_INFINITY;
        let mut best_texture: Option<&str> = None;

        for face in &self.faces {
            // Quick 2D AABB test with 16-unit player margin
            if px < face.aabb_min[0] - 16.0
                || px > face.aabb_max[0] + 16.0
                || py < face.aabb_min[1] - 16.0
                || py > face.aabb_max[1] + 16.0
            {
                continue;
            }

            // Height test: ground must be near or below the player's feet
            let z_at_pos = if face.plane_normal[2].abs() > 0.001 {
                (face.plane_dist - face.plane_normal[0] * px - face.plane_normal[1] * py)
                    / face.plane_normal[2]
            } else {
                face.plane_z
            };

            if z_at_pos > feet_z + 16.0 || z_at_pos < feet_z - 48.0 {
                continue;
            }

            // 2D Point-in-polygon test
            if point_in_polygon_2d(px, py, &face.vertices) && z_at_pos > best_z {
                best_z = z_at_pos;
                best_texture = Some(&face.texture_name);
            }
        }

        best_texture
    }
}

/// Standard ray-casting point-in-polygon algorithm in 2D horizontal plane.
#[must_use]
pub fn point_in_polygon_2d(px: f32, py: f32, poly: &[[f32; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let [xi, yi] = poly[i];
        let [xj, yj] = poly[j];

        let intersect = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
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

        let all_search_dirs = collect_game_search_dirs(search_dirs);
        let mut candidates = Vec::new();
        for dir in &all_search_dirs {
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
                // Clean transcode to 16-bit 44.1 kHz PCM with anti-pop micro-fades:
                let final_bytes = goldhub_audio::decode_wav(&bytes).map_or(bytes, |sound| {
                    goldhub_audio::transcode_to_clean_16bit_44khz(&sound)
                });

                let audio_source = AudioSource {
                    bytes: Arc::from(final_bytes.into_boxed_slice()),
                };
                let handle = audio_assets.add(audio_source);
                self.sound_cache.insert(clean_path, handle.clone());
                return Some(handle);
            }
        }

        tracing::warn!("GoldSrc sound not found on disk: {}", clean_path);
        None
    }
}

/// Plugin integrating GoldSrc player footsteps, falling impacts, and material sound effects.
pub struct GoldSrcAudioPlugin;

impl Plugin for GoldSrcAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameSearchPaths>();
        app.init_resource::<LevelGroundMaterialIndex>();
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
    let all_dirs = collect_game_search_dirs(&search_paths.dirs);
    let mut material_candidates = Vec::new();
    for dir in &all_dirs {
        material_candidates.push(dir.join("sound").join("materials.txt"));
        material_candidates.push(dir.join("materials.txt"));
        if let Some(parent) = dir.parent() {
            material_candidates.push(parent.join("cstrike").join("sound").join("materials.txt"));
            material_candidates.push(parent.join("valve").join("sound").join("materials.txt"));
        }
    }

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
#[allow(clippy::too_many_arguments)]
pub fn footstep_sound_system(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    search_paths: Res<GameSearchPaths>,
    ground_materials: Res<LevelGroundMaterialIndex>,
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

    for (_transform, mut player) in &mut query {
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

        // Trace ground surface material texture name directly under player origin
        let ground_texture = if on_ladder {
            None
        } else {
            ground_materials.trace_texture(player.sim.current.origin)
        };

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
            ground_texture,
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
            tracing::info!(
                "Playing fall landing sound: {} (vol: {:.2})",
                fall_event.sound_path,
                fall_event.volume
            );
            // Local player's own impact is direct stereo on CHAN_BODY in GoldSrc
            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings::DESPAWN.with_volume(Volume::new(fall_event.volume)),
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
            tracing::info!(
                "Playing footstep/ladder sound: {} (vol: {:.2})",
                step_event.sound_path,
                step_event.volume
            );
            // Local player's own footsteps are direct stereo on CHAN_BODY in GoldSrc
            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings::DESPAWN.with_volume(Volume::new(step_event.volume)),
            ));
        }

        player.was_on_ground = on_ground;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_collect_game_search_dirs_contains_defaults() {
        let extra = vec![PathBuf::from("custom/path")];
        let dirs = collect_game_search_dirs(&extra);
        assert!(!dirs.is_empty());
        assert_eq!(dirs[0], PathBuf::from("custom/path"));
        assert!(dirs.iter().any(|d| d.to_string_lossy().contains("cstrike")));
    }

    #[test]
    fn test_audio_bank_loads_real_sound() {
        let mut bank = GoldSrcAudioBank::default();
        let mut audio_assets = Assets::<AudioSource>::default();
        let handle = bank.get_or_load_sound("player/pl_step1.wav", &[], &mut audio_assets);
        if Path::new("C:/Games/CS 1.6 GoldClient/cstrike/sound/player/pl_step1.wav").exists() {
            assert!(
                handle.is_some(),
                "Expected pl_step1.wav to be resolved and cached"
            );
            assert!(bank.sound_cache.contains_key("player/pl_step1.wav"));
        }
    }

    #[test]
    fn test_ground_material_spatial_trace() {
        let face = GroundFace {
            aabb_min: [-100.0, -100.0],
            aabb_max: [100.0, 100.0],
            plane_z: 0.0,
            plane_normal: [0.0, 0.0, 1.0],
            plane_dist: 0.0,
            vertices: vec![
                [-100.0, -100.0],
                [100.0, -100.0],
                [100.0, 100.0],
                [-100.0, 100.0],
            ],
            texture_name: "wood_floor".to_string(),
        };

        let index = LevelGroundMaterialIndex { faces: vec![face] };

        // Standing at center [0, 0, 36], feet at Z = 0
        let hit = index.trace_texture([0.0, 0.0, 36.0]);
        assert_eq!(hit, Some("wood_floor"));

        // Far outside AABB
        let miss = index.trace_texture([500.0, 500.0, 36.0]);
        assert_eq!(miss, None);
    }
}
