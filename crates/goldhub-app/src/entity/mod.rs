//! GoldSrc Entity and Trigger Runtime.
//!
//! Manages interactive BSP entities:
//! - `func_breakable`: takes damage, plays material hit/break sounds, spawns gibs, fires `target`.
//! - `env_render`: modifies rendering visibility of target entities.
//! - `ambient_generic`: plays looping or triggered audio sounds, responds to `USE_TOGGLE`.
//! - `env_shooter`: shoots particle/gib models (e.g. feathers, debris) with directional spread.

use bevy::prelude::*;

/// Minimalist zero-cost LCG PRNG for ballistic gib particle scatter.
struct SimpleRng(u64);

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x5851_f42d_4c95_7f2d } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::studio::store::{StudioModelStore, create_initial_bevy_mesh};
use crate::studio::world_entity::{BspEntitiesLump, WorldStudioEntity};

/// Material types for func_breakable and impact effects (matching GoldSrc `Materials` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum BreakableMaterial {
    #[default]
    Glass = 0,
    Wood = 1,
    Metal = 2,
    Flesh = 3,
    CinderBlock = 4,
    CeilingTile = 5,
    Computer = 6,
    UnbreakableGlass = 7,
    Rocks = 8,
}

impl BreakableMaterial {
    #[must_use]
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::Wood,
            2 => Self::Metal,
            3 => Self::Flesh,
            4 => Self::CinderBlock,
            5 => Self::CeilingTile,
            6 => Self::Computer,
            7 => Self::UnbreakableGlass,
            8 => Self::Rocks,
            _ => Self::Glass,
        }
    }
}

/// Breakable brush entity component (`func_breakable`).
#[derive(Component, Debug, Clone)]
pub struct BreakableEntity {
    pub health: f32,
    pub max_health: f32,
    pub material: BreakableMaterial,
    pub explodemagnitude: u32,
    pub model: String,
    pub target: String,
    pub targetname: String,
    pub bmin: Vec3,
    pub bmax: Vec3,
    pub is_broken: bool,
}

/// Tag component marking BSP submodel brush meshes (e.g. "*65" for radio).
#[derive(Component, Debug, Clone)]
pub struct BspSubmodelMesh {
    pub model: String,
}

/// Door state machine according to GoldSrc toggle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoorState {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

/// Interactive door entity (`func_door` or `func_door_rotating`).
#[derive(Component, Debug, Clone)]
pub struct DoorEntity {
    pub model: String,
    pub targetname: String,
    pub target: String,
    pub speed: f32,
    pub wait: f32,
    pub wait_timer: f32,
    pub state: DoorState,
    pub is_rotating: bool,
    pub use_only: bool,
    pub hinge_origin: Vec3,
    pub start_pos: Vec3,
    pub target_pos: Vec3,
    pub start_rot: Quat,
    pub target_rot: Quat,
    pub distance: f32,
    pub current_progress: f32,
    pub move_sound: Option<String>,
    pub stop_sound: Option<String>,
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Short-lived point light representing an explosion flash.
#[derive(Component, Debug)]
pub struct ExplosionFlash {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub initial_intensity: f32,
}

/// Expanding fiery explosion fireball sphere.
#[derive(Component, Debug)]
pub struct ExplosionFireball {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub start_scale: f32,
    pub end_scale: f32,
}

/// Expanding and rising dark smoke particle from an explosion.
#[derive(Component, Debug)]
pub struct ExplosionSmokeParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub start_scale: f32,
    pub end_scale: f32,
}

/// Entity modifying rendering properties of targets (`env_render`).
#[derive(Component, Debug, Clone)]
pub struct EnvRenderEntity {
    pub targetname: String,
    pub target: String,
    pub rendermode: u32,
    pub renderamt: u32,
}

/// Spatial or ambient audio entity (`ambient_generic`).
#[derive(Component, Debug, Clone)]
pub struct AmbientAudioEntity {
    pub targetname: String,
    pub message: String,
    pub is_looping: bool,
    pub is_playing: bool,
    pub player_entity: Option<Entity>,
    pub position: Vec3,
}

/// Gib and particle shooter entity (`env_shooter`).
#[derive(Component, Debug, Clone)]
pub struct EnvShooterEntity {
    pub targetname: String,
    pub shootmodel: String,
    pub gibs: u32,
    pub variance: f32,
    pub velocity: f32,
    pub origin: Vec3,
}

/// Active flying gib particle (e.g. chicken feather, shard).
#[derive(Component, Debug, Clone)]
pub struct GibParticle {
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Event fired when a target is triggered (e.g. on breakable death or button press).
#[derive(Event, Debug, Clone)]
pub struct FireTargetEvent {
    pub target: String,
}

/// Event fired when an entity is destroyed, triggering material gib particles.
#[derive(Event, Debug, Clone)]
pub struct BreakEntityEvent {
    pub origin: Vec3,
    pub material: BreakableMaterial,
    pub model: String,
    pub explodemagnitude: u32,
}

/// Rescue zone entity (`func_hostage_rescue` or `info_hostage_rescue`).
#[derive(Component, Debug, Clone)]
pub struct HostageRescueZoneEntity {
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Push trigger entity (`trigger_push`).
#[derive(Component, Debug, Clone)]
pub struct TriggerPushEntity {
    pub speed: f32,
    pub dir: Vec3,
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Hurt trigger entity (`trigger_hurt`).
#[derive(Component, Debug, Clone)]
pub struct TriggerHurtEntity {
    pub dmg: f32,
    pub bmin: Vec3,
    pub bmax: Vec3,
    pub next_dmg_time: f32,
}

/// Buy zone entity (`func_buyzone`).
#[derive(Component, Debug, Clone)]
pub struct BuyZoneEntity {
    pub bmin: Vec3,
    pub bmax: Vec3,
}

pub struct EntityRuntimePlugin;

impl Plugin for EntityRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<FireTargetEvent>();
        app.add_event::<BreakEntityEvent>();
        app.add_systems(
            Update,
            (
                handle_fire_target_events,
                handle_break_entity_events,
                update_gib_particles_system,
                update_doors_system,
                interact_door_system,
                update_explosion_flashes_system,
                update_explosion_fireballs_system,
                update_explosion_smokes_system,
                update_triggers_system,
            ),
        );
    }
}

/// Spawns interactive entities from the active BSP entity lump.
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn spawn_bsp_interactive_entities(
    mut commands: Commands,
    entities_res: Option<Res<BspEntitiesLump>>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    let Some(ref entities_lump) = entities_res else {
        return;
    };
    let Ok(entities_str) = std::str::from_utf8(&entities_lump.0) else {
        return;
    };

    for block in entities_str.split('}') {
        let mut classname = "";
        let mut target = "";
        let mut targetname = "";
        let mut model_str = "";
        let mut origin_str = "";
        let mut message_str = "";
        let mut health_str = "";
        let mut material_str = "";
        let mut rendermode_str = "";
        let mut renderamt_str = "";
        let mut shootmodel_str = "";
        let mut gibs_str = "";
        let mut variance_str = "";
        let mut velocity_str = "";
        let mut spawnflags_str = "";
        let mut explodemagnitude_str = "";
        let mut speed_str = "";
        let mut wait_str = "";
        let mut movesnd_str = "";
        let mut stopsnd_str = "";
        let mut distance_str = "";
        let mut angles_str = "";

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('"') {
                let parts: Vec<&str> = trimmed.split('"').collect();
                if parts.len() >= 4 {
                    match parts[1] {
                        "classname" => classname = parts[3],
                        "target" => target = parts[3],
                        "targetname" => targetname = parts[3],
                        "model" => model_str = parts[3],
                        "origin" => origin_str = parts[3],
                        "message" => message_str = parts[3],
                        "health" => health_str = parts[3],
                        "material" => material_str = parts[3],
                        "rendermode" => rendermode_str = parts[3],
                        "renderamt" => renderamt_str = parts[3],
                        "shootmodel" => shootmodel_str = parts[3],
                        "m_iGibs" => gibs_str = parts[3],
                        "m_flVariance" => variance_str = parts[3],
                        "m_flVelocity" => velocity_str = parts[3],
                        "spawnflags" => spawnflags_str = parts[3],
                        "explodemagnitude" => explodemagnitude_str = parts[3],
                        "speed" => speed_str = parts[3],
                        "wait" => wait_str = parts[3],
                        "movesnd" => movesnd_str = parts[3],
                        "stopsnd" => stopsnd_str = parts[3],
                        "distance" => distance_str = parts[3],
                        "angles" => angles_str = parts[3],
                        _ => {}
                    }
                }
            }
        }

        if classname.is_empty() {
            continue;
        }

        // Parse optional origin
        let mut gx = 0.0f32;
        let mut gy = 0.0f32;
        let mut gz = 0.0f32;
        if !origin_str.is_empty() {
            let mut coords = origin_str.split_whitespace();
            if let (Some(x), Some(y), Some(z)) = (coords.next(), coords.next(), coords.next()) {
                gx = x.parse::<f32>().unwrap_or(0.0);
                gy = y.parse::<f32>().unwrap_or(0.0);
                gz = z.parse::<f32>().unwrap_or(0.0);
            }
        }
        let bevy_origin = Vec3::new(gx, gz, -gy);

        match classname {
            "func_breakable" => {
                let health = health_str.parse::<f32>().unwrap_or(1.0).max(1.0);
                let mat_id = material_str.parse::<u32>().unwrap_or(0);
                let material = BreakableMaterial::from_u32(mat_id);
                let explodemagnitude = explodemagnitude_str.parse::<u32>().unwrap_or(0);

                // Submodel bounds derived directly from BSP CollisionModel brush_entities
                let (bmin, bmax) = if model_str.starts_with('*') {
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref())
                } else {
                    (bevy_origin - Vec3::splat(16.0), bevy_origin + Vec3::splat(16.0))
                };

                commands.spawn((
                    BreakableEntity {
                        health,
                        max_health: health,
                        material,
                        explodemagnitude,
                        model: model_str.to_string(),
                        target: target.to_string(),
                        targetname: targetname.to_string(),
                        bmin,
                        bmax,
                        is_broken: false,
                    },
                    Transform::from_translation((bmin + bmax) * 0.5),
                    Visibility::default(),
                ));

                tracing::info!(
                    model = model_str,
                    target = target,
                    health = health,
                    explodemagnitude = explodemagnitude,
                    "Spawned func_breakable entity"
                );
            }
            "func_door" | "func_door_rotating" => {
                let is_rotating = classname == "func_door_rotating";
                let speed = speed_str.parse::<f32>().unwrap_or(100.0).max(10.0);
                let wait = wait_str.parse::<f32>().unwrap_or(4.0);
                let movesnd = movesnd_str.parse::<u32>().unwrap_or(1);
                let stopsnd = stopsnd_str.parse::<u32>().unwrap_or(1);
                let move_sound = Some(format!("doors/doormove{}.wav", movesnd.clamp(1, 10)));
                let stop_sound = Some(format!("doors/doorstop{}.wav", stopsnd.clamp(1, 8)));
                let distance = distance_str.parse::<f32>().unwrap_or(90.0);

                let (bmin, bmax) = get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                let center = (bmin + bmax) * 0.5;
                let spawnflags = spawnflags_str.parse::<u32>().unwrap_or(0);
                let use_only = (spawnflags & 256) != 0;
                let reverse = (spawnflags & 2) != 0;
                let hinge_origin = if origin_str.is_empty() {
                    Vec3::new(bmin.x, center.y, bmin.z)
                } else {
                    bevy_origin
                };

                let (start_pos, target_pos, start_rot, target_rot) = if is_rotating {
                    let angle_deg = if reverse { -distance } else { distance };
                    let angle_rad = angle_deg.to_radians();
                    (center, center, Quat::IDENTITY, Quat::from_rotation_y(angle_rad))
                } else {
                    let move_dir = if angles_str == "-1 0 0" {
                        Vec3::Y
                    } else if angles_str == "-2 0 0" {
                        Vec3::NEG_Y
                    } else {
                        let yaw = angles_str
                            .split_whitespace()
                            .next()
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        let rad = yaw.to_radians();
                        Vec3::new(rad.cos(), 0.0, -rad.sin())
                    };
                    let dist = (bmax - bmin).dot(move_dir.abs()).max(32.0);
                    (center, center + move_dir * dist, Quat::IDENTITY, Quat::IDENTITY)
                };

                commands.spawn((
                    DoorEntity {
                        model: model_str.to_string(),
                        targetname: targetname.to_string(),
                        target: target.to_string(),
                        speed,
                        wait,
                        wait_timer: 0.0,
                        state: DoorState::Closed,
                        is_rotating,
                        use_only,
                        hinge_origin,
                        start_pos,
                        target_pos,
                        start_rot,
                        target_rot,
                        distance,
                        current_progress: 0.0,
                        move_sound,
                        stop_sound,
                        bmin,
                        bmax,
                    },
                    Transform::from_translation(center),
                    Visibility::default(),
                ));

                tracing::info!(
                    model = model_str,
                    classname = classname,
                    speed = speed,
                    "Spawned interactive door entity"
                );
            }
            "env_render" => {
                let rendermode = rendermode_str.parse::<u32>().unwrap_or(0);
                let renderamt = renderamt_str.parse::<u32>().unwrap_or(0);
                commands.spawn(EnvRenderEntity {
                    targetname: targetname.to_string(),
                    target: target.to_string(),
                    rendermode,
                    renderamt,
                });
            }
            "ambient_generic" => {
                if !message_str.is_empty() {
                    let spawnflags = spawnflags_str.parse::<u32>().unwrap_or(0);
                    let is_everywhere = (spawnflags & 1) != 0;
                    let is_small_radius = (spawnflags & 2) != 0;
                    let is_large_radius = (spawnflags & 8) != 0;
                    let is_start_off = (spawnflags & 16) != 0;
                    let is_not_looping = (spawnflags & 32) != 0;
                    let is_looping = !is_not_looping;

                    // Initial volume (GoldSrc health 1..10 where 10 = 100%)
                    let vol = (health_str.parse::<f32>().unwrap_or(10.0) / 10.0).clamp(0.05, 1.0);

                    // If entity is marked not-looping or start-off, it must only play when triggered
                    let starts_active = !is_start_off && !is_not_looping;

                    let mut player_entity = None;
                    let is_playing = starts_active;

                    if is_playing
                        && let Some(handle) = audio_bank.get_or_load_sound(
                            message_str,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        let mut settings = if is_looping {
                            PlaybackSettings::LOOP
                        } else {
                            PlaybackSettings::DESPAWN
                        };
                        if is_everywhere {
                            settings = settings.with_volume(bevy::audio::Volume::new(vol));
                        } else {
                            let scale = if is_small_radius {
                                1.0 / 4.0 // Drops off rapidly within ~150u (e.g. sheep.wav)
                            } else if is_large_radius {
                                1.0 / 64.0 // Audible throughout building (e.g. Opera.wav)
                            } else {
                                1.0 / 16.0 // Medium / static default
                            };
                            settings = settings
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(scale))
                                .with_volume(bevy::audio::Volume::new(vol));
                        }

                        let p = commands
                            .spawn((
                                AudioPlayer::new(handle),
                                settings,
                                Transform::from_translation(bevy_origin),
                            ))
                            .id();
                        player_entity = Some(p);
                        tracing::info!(
                            message = message_str,
                            pos = ?bevy_origin,
                            is_looping,
                            "Spawned active ambient_generic loop"
                        );
                    }

                    commands.spawn(AmbientAudioEntity {
                        targetname: targetname.to_string(),
                        message: message_str.to_string(),
                        is_looping,
                        is_playing,
                        player_entity,
                        position: bevy_origin,
                    });
                }
            }
            "env_shooter" => {
                let gibs = gibs_str.parse::<u32>().unwrap_or(10).max(1);
                let variance = variance_str.parse::<f32>().unwrap_or(5.0);
                let velocity = velocity_str.parse::<f32>().unwrap_or(100.0);
                commands.spawn(EnvShooterEntity {
                    targetname: targetname.to_string(),
                    shootmodel: shootmodel_str.to_string(),
                    gibs,
                    variance,
                    velocity,
                    origin: bevy_origin,
                });
            }
            "func_hostage_rescue" | "info_hostage_rescue" => {
                let (bmin, bmax) = if model_str.starts_with('*') {
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref())
                } else {
                    (bevy_origin - Vec3::splat(256.0), bevy_origin + Vec3::splat(256.0))
                };
                commands.spawn((
                    HostageRescueZoneEntity { bmin, bmax },
                    Transform::from_translation((bmin + bmax) * 0.5),
                    Visibility::default(),
                ));
                tracing::info!(classname, "Spawned Hostage Rescue Zone");
            }
            "trigger_push" => {
                let speed = speed_str.parse::<f32>().unwrap_or(100.0).max(10.0);
                let (bmin, bmax) = get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                let yaw = angles_str
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(0.0);
                let rad = yaw.to_radians();
                let dir = Vec3::new(rad.cos(), 0.0, -rad.sin());
                commands.spawn(TriggerPushEntity { speed, dir, bmin, bmax });
                tracing::info!(speed = speed, "Spawned trigger_push entity");
            }
            "trigger_hurt" => {
                let dmg = health_str.parse::<f32>().unwrap_or(10.0).max(1.0);
                let (bmin, bmax) = get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                commands.spawn(TriggerHurtEntity { dmg, bmin, bmax, next_dmg_time: 0.0 });
                tracing::info!(dmg = dmg, "Spawned trigger_hurt entity");
            }
            "func_buyzone" => {
                let (bmin, bmax) = get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                commands.spawn(BuyZoneEntity { bmin, bmax });
                tracing::info!("Spawned func_buyzone entity");
            }
            _ => {}
        }
    }
}

/// Helper extracting exact world bounds for any BSP submodel brush entity from `CollisionModel`.
fn get_submodel_bounds(
    model_str: &str,
    fallback_origin: Vec3,
    collision_res: Option<&Res<crate::player::LevelCollisionResource>>,
) -> (Vec3, Vec3) {
    if let Some(sub_num_str) = model_str.strip_prefix('*')
        && let Ok(sub_num) = sub_num_str.parse::<usize>()
        && let Some(col_res) = collision_res
        && let Some(ref col_model) = col_res.model
        && let Some(ent) = col_model.brush_entities.iter().find(|e| e.model_index == sub_num)
    {
        // Exact GoldSrc bounds from BSP submodel:
        // Bevy X = GS X, Bevy Y = GS Z, Bevy Z = -GS Y
        let bmin = Vec3::new(ent.mins[0], ent.mins[2], -ent.maxs[1]);
        let bmax = Vec3::new(ent.maxs[0], ent.maxs[2], -ent.mins[1]);
        return (bmin, bmax);
    }

    get_submodel_bounds_or_fallback(model_str, fallback_origin)
}

/// Helper mapping known cs_italy breakable brush submodels to exact Bevy world bounds.
fn get_submodel_bounds_or_fallback(model_str: &str, fallback_origin: Vec3) -> (Vec3, Vec3) {
    // Exact GoldSrc bounds from cs_italy.bsp:
    // Bevy X = GS X, Bevy Y = GS Z, Bevy Z = -GS Y
    match model_str {
        "*39" => (
            // Bottle 2: GS X: [-578, -558], Y: [617, 637], Z: [46, 62]
            Vec3::new(-578.0, 46.0, -637.0),
            Vec3::new(-558.0, 62.0, -617.0),
        ),
        "*40" => (
            // Bottle 1: GS X: [-570, -550], Y: [587, 607], Z: [46, 62]
            Vec3::new(-570.0, 46.0, -607.0),
            Vec3::new(-550.0, 62.0, -587.0),
        ),
        "*63" => (
            // Chicken 2: GS X: [1016, 1032], Y: [-72, -56], Z: [-152, -128]
            Vec3::new(1016.0, -152.0, 56.0),
            Vec3::new(1032.0, -128.0, 72.0),
        ),
        "*65" => (
            // Radio: GS X: [967, 1023], Y: [2306, 2362], Z: [128, 192]
            Vec3::new(967.0, 128.0, -2362.0),
            Vec3::new(1023.0, 192.0, -2306.0),
        ),
        "*66" => (
            // Chicken 1: GS X: [176, 192], Y: [-544, -528], Z: [-152, -128]
            Vec3::new(176.0, -152.0, 528.0),
            Vec3::new(192.0, -128.0, 544.0),
        ),
        _ => (
            fallback_origin - Vec3::splat(16.0),
            fallback_origin + Vec3::splat(16.0),
        ),
    }
}

/// Handles trigger dispatch when a target name is fired.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn handle_fire_target_events(
    mut commands: Commands,
    mut events: EventReader<FireTargetEvent>,
    mut render_query: Query<&EnvRenderEntity>,
    mut audio_query: Query<&mut AmbientAudioEntity>,
    shooter_query: Query<&EnvShooterEntity>,
    mut studio_query: Query<(&WorldStudioEntity, &mut Visibility)>,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for ev in events.read() {
        if ev.target.is_empty() {
            continue;
        }

        tracing::info!(target = ev.target, "Handling FireTargetEvent");

        // 1. EnvRender: Hide or modify target entities
        for render in &mut render_query {
            if render.targetname == ev.target {
                for (studio_ent, mut vis) in &mut studio_query {
                    if studio_ent.targetname == render.target {
                        *vis = Visibility::Hidden;
                        tracing::info!(target = render.target, "Hidden WorldStudioEntity via env_render");
                    }
                }
            }
        }

        // 2. AmbientAudio: Stop looping audio or play triggered sound
        for mut ambient in &mut audio_query {
            if ambient.targetname == ev.target {
                if ambient.is_playing {
                    // Turn off looping sound (e.g. Radio opera music)
                    if let Some(player_ent) = ambient.player_entity.take() {
                        commands.entity(player_ent).despawn();
                    }
                    ambient.is_playing = false;
                    tracing::info!(sound = ambient.message, "Stopped ambient_generic looping playback");
                } else {
                    // One-shot triggered playback (e.g. chicken death sound)
                    if let Some(handle) = audio_bank.get_or_load_sound(
                        &ambient.message,
                        &search_paths.dirs,
                        &mut audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(handle),
                            PlaybackSettings::DESPAWN
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 64.0)),
                            Transform::from_translation(ambient.position),
                        ));
                        tracing::info!(sound = ambient.message, "Triggered one-shot ambient sound");
                    }
                }
            }
        }

        // 3. EnvShooter: Shoot gib particles (e.g. feathers, debris)
        for shooter in &shooter_query {
            if shooter.targetname == ev.target {
                spawn_gibs(
                    &mut commands,
                    shooter,
                    &mut model_store,
                    &search_paths,
                    &mut images,
                    &mut materials,
                    &mut meshes,
                );
            }
        }
    }
}

/// Handles material-based gib bursts, explosions, and brush despawning when a breakable entity is shattered.
#[allow(clippy::too_many_arguments, clippy::too_many_lines, clippy::type_complexity)]
pub fn handle_break_entity_events(
    mut commands: Commands,
    mut events: EventReader<BreakEntityEvent>,
    mut submodel_query: Query<(&BspSubmodelMesh, &mut Visibility)>,
    mut collision_res: Option<ResMut<crate::player::LevelCollisionResource>>,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut player_query: Query<(
        &Transform,
        &mut crate::player::PlayerController,
        Option<&mut crate::combat::PlayerRecoil>,
    )>,
    mut hostage_query: Query<(
        &mut crate::studio::world_entity::HostageEntity,
        &mut crate::studio::animation::StudioAnimatedModel,
        &mut crate::studio::world_entity::EntityCollisionBounds,
        &GlobalTransform,
    ), (
        Without<crate::studio::viewmodel::ViewModelWeapon>,
        Without<crate::player::PlayerController>,
    )>,
) {
    for ev in events.read() {
        if !ev.model.is_empty() {
            for (submodel, mut vis) in &mut submodel_query {
                if submodel.model == ev.model {
                    *vis = Visibility::Hidden;
                    tracing::info!(model = ev.model, "Hidden BSP submodel on break");
                }
            }

            // Authentic GoldSrc pev->solid = SOLID_NOT: disable physical collision for broken brush entity
            if let Some(ref mut col_res) = collision_res
                && let Some(ref mut col_model) = col_res.model
                && let Some(sub_num_str) = ev.model.strip_prefix('*')
                && let Ok(sub_num) = sub_num_str.parse::<usize>()
            {
                for ent in &mut col_model.brush_entities {
                    if ent.model_index == sub_num {
                        ent.solid = goldhub_formats::ir::SOLID_NOT;
                        tracing::info!(model = ev.model, "Disabled physical collision for broken BSP entity");
                    }
                }
            }
        }

        let gib_model = match ev.material {
            BreakableMaterial::Computer => "models/computergibs.mdl",
            BreakableMaterial::Glass | BreakableMaterial::UnbreakableGlass => "models/glassgibs.mdl",
            BreakableMaterial::Wood => "models/woodgibs.mdl",
            BreakableMaterial::Metal => "models/metalplategibs.mdl",
            BreakableMaterial::Flesh => "models/fleshgibs.mdl",
            BreakableMaterial::CinderBlock => "models/cindergibs.mdl",
            BreakableMaterial::CeilingTile => "models/ceilinggibs.mdl",
            BreakableMaterial::Rocks => "models/rockgibs.mdl",
        };

        if ev.explodemagnitude > 0 {
            // Authentic GoldSrc Explosion (ExplosionCreate):
            // 1. Loud explosion sound
            let exp_snd = if let Some(h) = audio_bank.get_or_load_sound(
                "weapons/explode3.wav",
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                Some(h)
            } else if let Some(h) = audio_bank.get_or_load_sound(
                "weapons/explode4.wav",
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                Some(h)
            } else {
                audio_bank.get_or_load_sound(
                    "weapons/c4_explode1.wav",
                    &search_paths.dirs,
                    &mut audio_sources,
                )
            };
            if let Some(snd) = exp_snd {
                commands.spawn((
                    AudioPlayer::new(snd),
                    PlaybackSettings::DESPAWN
                        .with_spatial(true)
                        .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 64.0))
                        .with_volume(bevy::audio::Volume::new(1.0)),
                    Transform::from_translation(ev.origin),
                ));
            }

            // 2. High-energy explosion flash PointLight
            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.65, 0.2),
                    intensity: 15_000.0,
                    range: 350.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(ev.origin),
                ExplosionFlash {
                    lifetime: 0.0,
                    max_lifetime: 0.25,
                    initial_intensity: 15_000.0,
                },
            ));

            // 3. Expanding fiery fireball visual mesh
            let sphere_mesh = meshes.add(Sphere::new(1.0).mesh().uv(16, 16));
            let fireball_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.45, 0.1, 0.95),
                emissive: LinearRgba::new(6.0, 2.5, 0.3, 1.0),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            commands.spawn((
                Mesh3d(sphere_mesh.clone()),
                MeshMaterial3d(fireball_mat),
                Transform::from_translation(ev.origin).with_scale(Vec3::splat(4.0)),
                ExplosionFireball {
                    lifetime: 0.0,
                    max_lifetime: 0.35,
                    start_scale: 4.0,
                    end_scale: 48.0,
                },
            ));

            // 4. Rising smoke particles
            for i in 0..12 {
                let seed = u64::from(ev.origin.x.to_bits()) ^ ((i as u64) * 47);
                let mut rng = SimpleRng::new(seed);
                let vx = rng.range_f32(-35.0, 35.0);
                let vy = rng.range_f32(50.0, 100.0);
                let vz = rng.range_f32(-35.0, 35.0);
                let smoke_mat = materials.add(StandardMaterial {
                    base_color: Color::srgba(0.2, 0.18, 0.17, 0.6),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                });
                commands.spawn((
                    Mesh3d(sphere_mesh.clone()),
                    MeshMaterial3d(smoke_mat),
                    Transform::from_translation(ev.origin + Vec3::new(vx * 0.1, 6.0, vz * 0.1))
                        .with_scale(Vec3::splat(rng.range_f32(4.0, 7.0))),
                    ExplosionSmokeParticle {
                        velocity: Vec3::new(vx, vy, vz),
                        lifetime: 0.0,
                        max_lifetime: rng.range_f32(0.9, 1.3),
                        start_scale: rng.range_f32(4.0, 7.0),
                        end_scale: rng.range_f32(22.0, 32.0),
                    },
                ));
            }

            // 5. Blast impulse and recoil camera kick on player
            for (p_trans, mut p_ctrl, recoil_opt) in &mut player_query {
                let p_pos = p_trans.translation;
                let dist = ev.origin.distance(p_pos);
                let blast_radius = 320.0_f32;
                if dist < blast_radius {
                    let frac = (1.0 - (dist / blast_radius)).clamp(0.0, 1.0);
                    let blast_dir = (p_pos - ev.origin).normalize_or_zero();
                    p_ctrl.sim.current.velocity[0] += blast_dir.x * 450.0 * frac;
                    p_ctrl.sim.current.velocity[1] += -blast_dir.z * 450.0 * frac;
                    p_ctrl.sim.current.velocity[2] += (blast_dir.y * 300.0 + 200.0) * frac;

                    if let Some(mut recoil) = recoil_opt {
                        recoil.punch_pitch -= 16.0 * frac;
                        let yaw_kick = if ((dist * 10.0) as usize).is_multiple_of(2) { 9.0 } else { -9.0 };
                        recoil.punch_yaw += yaw_kick * frac;
                    }
                }
            }

            // 6. Blast damage on hostages
            for (mut hostage, mut h_anim, mut bounds, h_trans) in &mut hostage_query {
                let h_pos = h_trans.translation();
                let dist = ev.origin.distance(h_pos);
                let blast_radius = 320.0_f32;
                if dist < blast_radius && !hostage.is_dead {
                    let frac = (1.0 - (dist / blast_radius)).clamp(0.0, 1.0);
                    hostage.health -= 120.0 * frac;
                    if hostage.health > 0.0 {
                        h_anim.set_sequence(7, 30.0, false);
                    } else {
                        hostage.is_dead = true;
                        h_anim.set_sequence(31, 24.0, false);
                        bounds.mins = Vec3::ZERO;
                        bounds.maxs = Vec3::ZERO;
                        if let Some(h) = audio_bank.get_or_load_sound(
                            "radio/hosdown.wav",
                            &search_paths.dirs,
                            &mut audio_sources,
                        ) {
                            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                        }
                    }
                }
            }

            // 7. High-velocity bursting gibs (blast impulse 320 u/s)
            let shooter = EnvShooterEntity {
                targetname: String::new(),
                shootmodel: gib_model.to_string(),
                gibs: 24,
                variance: 14.0,
                velocity: 320.0,
                origin: ev.origin,
            };

            spawn_gibs(
                &mut commands,
                &shooter,
                &mut model_store,
                &search_paths,
                &mut images,
                &mut materials,
                &mut meshes,
            );
        } else {
            let shooter = EnvShooterEntity {
                targetname: String::new(),
                shootmodel: gib_model.to_string(),
                gibs: 12,
                variance: 6.0,
                velocity: 120.0,
                origin: ev.origin,
            };

            spawn_gibs(
                &mut commands,
                &shooter,
                &mut model_store,
                &search_paths,
                &mut images,
                &mut materials,
                &mut meshes,
            );
        }
    }
}

/// Spawns 3D gibs (feathers or shards) according to `EnvShooterEntity` properties.
#[allow(clippy::too_many_arguments)]
fn spawn_gibs(
    commands: &mut Commands,
    shooter: &EnvShooterEntity,
    model_store: &mut StudioModelStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let Some(model_data) = model_store.get_or_load(&shooter.shootmodel, search_paths, images, materials)
    else {
        tracing::warn!(model = shooter.shootmodel, "Could not load gib shootmodel");
        return;
    };

    let mut rng = SimpleRng::new(u64::from(shooter.origin.x.to_bits()) ^ u64::from(shooter.gibs));

    for _ in 0..shooter.gibs {
        // Compute randomized initial velocity with variance
        let vx = rng.range_f32(-shooter.variance, shooter.variance) * 15.0;
        let vy = shooter.velocity + rng.range_f32(-shooter.variance, shooter.variance) * 20.0;
        let vz = rng.range_f32(-shooter.variance, shooter.variance) * 15.0;

        let spawn_pos = shooter.origin + Vec3::new(
            rng.range_f32(-6.0, 6.0),
            rng.range_f32(0.0, 8.0),
            rng.range_f32(-6.0, 6.0),
        );

        let gib_parent = commands
            .spawn((
                GibParticle {
                    velocity: Vec3::new(vx, vy, vz),
                    angular_velocity: Vec3::new(
                        rng.range_f32(-5.0, 5.0),
                        rng.range_f32(-5.0, 5.0),
                        rng.range_f32(-5.0, 5.0),
                    ),
                    lifetime: 0.0,
                    max_lifetime: rng.range_f32(3.0, 5.0),
                },
                Transform::from_translation(spawn_pos).with_scale(Vec3::splat(rng.range_f32(0.8, 1.3))),
                Visibility::default(),
            ))
            .id();

        for template in &model_data.submeshes {
            let bevy_mesh = create_initial_bevy_mesh(template, false);
            let mesh_handle = meshes.add(bevy_mesh);
            let child = commands
                .spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(template.material.clone()),
                    Transform::IDENTITY,
                ))
                .id();
            commands.entity(gib_parent).add_child(child);
        }
    }

    tracing::info!(
        gibs = shooter.gibs,
        model = shooter.shootmodel,
        "Spawned gib particles burst"
    );
}

/// Advances position, ballistic gravity, and rotation for active gib particles.
pub fn update_gib_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut GibParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut gib, mut transform) in &mut query {
        gib.lifetime += dt;
        if gib.lifetime >= gib.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Apply GoldSrc downward gravity (400 u/s^2) and air drag
        gib.velocity.y -= 400.0 * dt;
        gib.velocity.x *= 1.0 - 0.5 * dt;
        gib.velocity.z *= 1.0 - 0.5 * dt;

        transform.translation += gib.velocity * dt;
        transform.rotate_local_x(gib.angular_velocity.x * dt);
        transform.rotate_local_y(gib.angular_velocity.y * dt);
        transform.rotate_local_z(gib.angular_velocity.z * dt);
    }
}

/// Advances active door state, interpolates movement, and manages door audio and collision.
#[allow(clippy::too_many_arguments, clippy::type_complexity, clippy::too_many_lines)]
pub fn update_doors_system(
    mut commands: Commands,
    time: Res<Time>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut collision_res: Option<ResMut<crate::player::LevelCollisionResource>>,
    mut door_query: Query<(Entity, &mut DoorEntity)>,
    mut submodel_query: Query<(&BspSubmodelMesh, &mut Transform), (Without<DoorEntity>, Without<crate::player::PlayerController>)>,
    player_query: Query<&Transform, With<crate::player::PlayerController>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // Touch to open: if player approaches within touch proximity of door bounding box, open unless use_only
    if let Ok(player_t) = player_query.get_single() {
        let player_pos = player_t.translation;
        for (_door_ent, mut door) in &mut door_query {
            if door.state == DoorState::Closed && !door.use_only {
                let center = (door.bmin + door.bmax) * 0.5;
                if player_pos.distance(center) < 72.0
                    && (player_pos.x >= door.bmin.x - 32.0 && player_pos.x <= door.bmax.x + 32.0)
                    && (player_pos.y >= door.bmin.y - 36.0 && player_pos.y <= door.bmax.y + 72.0)
                    && (player_pos.z >= door.bmin.z - 32.0 && player_pos.z <= door.bmax.z + 32.0)
                {
                    door.state = DoorState::Opening;
                    if let Some(ref move_snd) = door.move_sound
                        && let Some(h) = audio_bank.get_or_load_sound(
                            move_snd,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation(center),
                        ));
                    }
                }
            }
        }
    }

    for (_door_ent, mut door) in &mut door_query {
        match door.state {
            DoorState::Closed => {}
            DoorState::Opening => {
                let travel_time = if door.is_rotating {
                    door.distance.to_radians() / door.speed.to_radians().max(0.1)
                } else {
                    door.start_pos.distance(door.target_pos) / door.speed.max(1.0)
                };
                let step = if travel_time > 0.0 { dt / travel_time } else { 1.0 };
                door.current_progress = (door.current_progress + step).min(1.0);

                // Update submodel mesh transform
                for (submesh, mut mesh_t) in &mut submodel_query {
                    if submesh.model == door.model {
                        if door.is_rotating {
                            let rot = door.start_rot.slerp(door.target_rot, door.current_progress);
                            let p = door.hinge_origin;
                            mesh_t.translation = p - rot.mul_vec3(p);
                            mesh_t.rotation = rot;
                        } else {
                            mesh_t.translation = (door.target_pos - door.start_pos) * door.current_progress;
                            mesh_t.rotation = Quat::IDENTITY;
                        }
                    }
                }

                // If door reaches open position
                if door.current_progress >= 1.0 {
                    door.state = DoorState::Open;
                    door.wait_timer = door.wait;
                    if let Some(ref stop_snd) = door.stop_sound
                        && let Some(h) = audio_bank.get_or_load_sound(
                            stop_snd,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation((door.bmin + door.bmax) * 0.5),
                        ));
                    }
                    // Disable physical collision while open
                    if let Some(ref mut col_res) = collision_res
                        && let Some(ref mut col_model) = col_res.model
                        && let Some(sub_num_str) = door.model.strip_prefix('*')
                        && let Ok(sub_num) = sub_num_str.parse::<usize>()
                    {
                        for ent in &mut col_model.brush_entities {
                            if ent.model_index == sub_num {
                                ent.solid = goldhub_formats::ir::SOLID_NOT;
                            }
                        }
                    }
                }
            }
            DoorState::Open => {
                if door.wait >= 0.0 {
                    door.wait_timer -= dt;
                    if door.wait_timer <= 0.0 {
                        door.state = DoorState::Closing;
                        if let Some(ref move_snd) = door.move_sound
                            && let Some(h) = audio_bank.get_or_load_sound(
                                move_snd,
                                &search_paths.dirs,
                                &mut audio_sources,
                            )
                        {
                            commands.spawn((
                                AudioPlayer::new(h),
                                PlaybackSettings::DESPAWN.with_spatial(true),
                                Transform::from_translation((door.bmin + door.bmax) * 0.5),
                            ));
                        }
                    }
                }
            }
            DoorState::Closing => {
                let travel_time = if door.is_rotating {
                    door.distance.to_radians() / door.speed.to_radians().max(0.1)
                } else {
                    door.start_pos.distance(door.target_pos) / door.speed.max(1.0)
                };
                let step = if travel_time > 0.0 { dt / travel_time } else { 1.0 };
                door.current_progress = (door.current_progress - step).max(0.0);

                // Update submodel mesh transform
                for (submesh, mut mesh_t) in &mut submodel_query {
                    if submesh.model == door.model {
                        if door.is_rotating {
                            let rot = door.start_rot.slerp(door.target_rot, door.current_progress);
                            let p = door.hinge_origin;
                            mesh_t.translation = p - rot.mul_vec3(p);
                            mesh_t.rotation = rot;
                        } else {
                            mesh_t.translation = (door.target_pos - door.start_pos) * door.current_progress;
                            mesh_t.rotation = Quat::IDENTITY;
                        }
                    }
                }

                // If door reaches closed position
                if door.current_progress <= 0.0 {
                    door.state = DoorState::Closed;
                    if let Some(ref stop_snd) = door.stop_sound
                        && let Some(h) = audio_bank.get_or_load_sound(
                            stop_snd,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation((door.bmin + door.bmax) * 0.5),
                        ));
                    }
                    // Re-enable solid collision
                    if let Some(ref mut col_res) = collision_res
                        && let Some(ref mut col_model) = col_res.model
                        && let Some(sub_num_str) = door.model.strip_prefix('*')
                        && let Ok(sub_num) = sub_num_str.parse::<usize>()
                    {
                        for ent in &mut col_model.brush_entities {
                            if ent.model_index == sub_num {
                                ent.solid = goldhub_formats::ir::SOLID_BSP;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Allows the player to open/close doors using the +use (E) key.
pub fn interact_door_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<crate::player::PlayerController>)>,
    mut door_query: Query<&mut DoorEntity>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    const USE_REACH: f32 = 80.0;

    if !keyboard.just_pressed(KeyCode::KeyE) {
        return;
    }

    let Ok(cam_t) = camera_query.get_single() else {
        return;
    };
    let cam_pos = cam_t.translation();
    let cam_forward = cam_t.forward().as_vec3();

    for mut door in &mut door_query {
        let center = (door.bmin + door.bmax) * 0.5;
        if cam_pos.distance(center) > USE_REACH + 64.0 {
            continue;
        }

        if ray_intersects_aabb(
            cam_pos,
            cam_forward,
            USE_REACH,
            door.bmin - Vec3::splat(16.0),
            door.bmax + Vec3::splat(16.0),
        )
        .is_some()
        {
            match door.state {
                DoorState::Closed => {
                    door.state = DoorState::Opening;
                    if let Some(ref snd) = door.move_sound
                        && let Some(h) = audio_bank.get_or_load_sound(
                            snd,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation(center),
                        ));
                    }
                }
                DoorState::Open => {
                    door.state = DoorState::Closing;
                    if let Some(ref snd) = door.move_sound
                        && let Some(h) = audio_bank.get_or_load_sound(
                            snd,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation(center),
                        ));
                    }
                }
                DoorState::Closing => {
                    door.state = DoorState::Opening;
                }
                DoorState::Opening => {}
            }
            break;
        }
    }
}

/// Advances explosion flash light decay and despawns when expired.
pub fn update_explosion_flashes_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut ExplosionFlash, &mut PointLight)>,
) {
    let dt = time.delta_secs();
    for (entity, mut flash, mut light) in &mut query {
        flash.lifetime += dt;
        if flash.lifetime >= flash.max_lifetime {
            commands.entity(entity).despawn();
        } else {
            let frac = 1.0 - (flash.lifetime / flash.max_lifetime);
            light.intensity = flash.initial_intensity * frac;
        }
    }
}

/// Advances explosion fireball expansion and emissive color fade.
pub fn update_explosion_fireballs_system(
    mut commands: Commands,
    time: Res<Time>,
    mut fireballs: Query<(
        Entity,
        &mut ExplosionFireball,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut fb, mut transform, mat_handle) in &mut fireballs {
        fb.lifetime += dt;
        if fb.lifetime >= fb.max_lifetime {
            commands.entity(entity).despawn();
        } else {
            let t = fb.lifetime / fb.max_lifetime;
            let current_scale = fb.start_scale + (fb.end_scale - fb.start_scale) * t.sqrt();
            transform.scale = Vec3::splat(current_scale);
            if let Some(mat) = materials.get_mut(mat_handle.0.id()) {
                let intensity = ((1.0 - t) * 6.0).max(0.0);
                mat.emissive = LinearRgba::new(intensity, intensity * 0.4, intensity * 0.05, 1.0);
                mat.base_color = Color::srgba(1.0, 0.45, 0.1, (1.0 - t).max(0.0));
            }
        }
    }
}

/// Advances explosion smoke particle drift, expansion, and alpha fade.
pub fn update_explosion_smokes_system(
    mut commands: Commands,
    time: Res<Time>,
    mut smokes: Query<(
        Entity,
        &mut ExplosionSmokeParticle,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut smoke, mut transform, mat_handle) in &mut smokes {
        smoke.lifetime += dt;
        if smoke.lifetime >= smoke.max_lifetime {
            commands.entity(entity).despawn();
        } else {
            let t = smoke.lifetime / smoke.max_lifetime;
            transform.translation += smoke.velocity * dt;
            smoke.velocity.y += 18.0 * dt;
            let current_scale = smoke.start_scale + (smoke.end_scale - smoke.start_scale) * t;
            transform.scale = Vec3::splat(current_scale);
            if let Some(mat) = materials.get_mut(mat_handle.0.id()) {
                let alpha = ((1.0 - t) * 0.6).max(0.0);
                mat.base_color = Color::srgba(0.2, 0.18, 0.17, alpha);
            }
        }
    }
}

/// Ray vs AABB intersection test.
#[must_use]
pub fn ray_intersects_aabb(
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    bmin: Vec3,
    bmax: Vec3,
) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = max_dist;

    for axis in 0..3 {
        let (o, d, min_v, max_v) = match axis {
            0 => (origin.x, dir.x, bmin.x, bmax.x),
            1 => (origin.y, dir.y, bmin.y, bmax.y),
            _ => (origin.z, dir.z, bmin.z, bmax.z),
        };

        if d.abs() < 1e-6 {
            if o < min_v || o > max_v {
                return None;
            }
        } else {
            let inv_d = 1.0 / d;
            let mut t1 = (min_v - o) * inv_d;
            let mut t2 = (max_v - o) * inv_d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }

    if tmin <= max_dist && tmax >= 0.0 {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

/// Updates player interaction with trigger_push and trigger_hurt zones.
pub fn update_triggers_system(
    time: Res<Time>,
    mut player_query: Query<(&Transform, &mut crate::player::PlayerController)>,
    push_query: Query<&TriggerPushEntity>,
    mut hurt_query: Query<&mut TriggerHurtEntity>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok((p_t, mut p_ctrl)) = player_query.get_single_mut() else {
        return;
    };
    let p_pos = p_t.translation;

    // 1. Trigger Push
    for push in &push_query {
        if p_pos.x >= push.bmin.x
            && p_pos.x <= push.bmax.x
            && p_pos.y >= push.bmin.y
            && p_pos.y <= push.bmax.y
            && p_pos.z >= push.bmin.z
            && p_pos.z <= push.bmax.z
        {
            let gs_dir = [push.dir.x, -push.dir.z, push.dir.y];
            p_ctrl.sim.current.velocity[0] += gs_dir[0] * push.speed * dt * 5.0;
            p_ctrl.sim.current.velocity[1] += gs_dir[1] * push.speed * dt * 5.0;
            p_ctrl.sim.current.velocity[2] += gs_dir[2] * push.speed * dt * 5.0;
        }
    }

    // 2. Trigger Hurt
    for mut hurt in &mut hurt_query {
        if p_pos.x >= hurt.bmin.x
            && p_pos.x <= hurt.bmax.x
            && p_pos.y >= hurt.bmin.y
            && p_pos.y <= hurt.bmax.y
            && p_pos.z >= hurt.bmin.z
            && p_pos.z <= hurt.bmax.z
            && hurt.next_dmg_time <= time.elapsed_secs()
        {
            hurt.next_dmg_time = time.elapsed_secs() + 0.5;
            tracing::info!(dmg = hurt.dmg, "Player touched trigger_hurt");
        }
    }
}
