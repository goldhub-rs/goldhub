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
        Self(if seed == 0 {
            0x5851_f42d_4c95_7f2d
        } else {
            seed
        })
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
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
    pub trigger_only: bool,
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
#[allow(clippy::struct_excessive_bools)]
pub struct DoorEntity {
    pub model: String,
    pub targetname: String,
    pub target: String,
    pub spawnflags: u32,
    pub speed: f32,
    pub wait: f32,
    pub dmg: f32,
    pub wait_timer: f32,
    pub blocked_cooldown: f32,
    pub touch_cooldown: f32,
    pub state: DoorState,
    pub is_rotating: bool,
    pub is_toggle: bool,
    pub is_oneway: bool,
    pub use_only: bool,
    pub start_open: bool,
    pub passable: bool,
    pub rot_axis: Vec3,
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
    pub base_gs_mins: [f32; 3],
    pub base_gs_maxs: [f32; 3],
    pub base_gs_origin: [f32; 3],
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
    pub bounce_count: u8,
    pub play_bounce_sound: bool,
    pub is_metal: bool,
    pub on_ground: bool,
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

/// Bomb plant trigger volume (`func_bomb_target`).
#[derive(Component, Debug, Clone)]
pub struct BombTargetZoneEntity {
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Legacy point bombsite entity (`info_bomb_target`, 256u radius fallback).
#[derive(Component, Debug, Clone)]
pub struct BombTargetPointEntity {
    pub origin: Vec3,
    pub radius: f32,
}

/// Reusable trigger volume (`trigger_multiple`, e.g. `de_nuke` `GlassDoor1..4`).
#[derive(Component, Debug, Clone)]
pub struct TriggerMultipleEntity {
    pub target: String,
    pub wait: f32,
    pub next_trigger_time: f32,
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// One-shot trigger volume (`trigger_once`).
#[derive(Component, Debug, Clone)]
pub struct TriggerOnceEntity {
    pub target: String,
    pub triggered: bool,
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Assassination VIP safety zone (`func_vip_safetyzone`).
#[derive(Component, Debug, Clone)]
pub struct VipSafetyZoneEntity {
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Prison escape zone (`func_escapezone`).
#[derive(Component, Debug, Clone)]
pub struct EscapeZoneEntity {
    pub bmin: Vec3,
    pub bmax: Vec3,
}

/// Map-placed or dropped defuse kit pickup (`item_thighpack`).
#[derive(Component, Debug, Clone)]
pub struct DefuseKitPickup {
    pub origin: Vec3,
    pub collected: bool,
}

/// Map-wide scenario conditions & spawn points extracted from BSP entities (`CheckMapConditions`).
#[derive(Resource, Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ScenarioMapData {
    pub ct_spawns: Vec<([f32; 3], f32)>,
    pub t_spawns: Vec<([f32; 3], f32)>,
    pub vip_spawn: Option<([f32; 3], f32)>,
    pub has_bomb_target: bool,
    pub has_bomb_zone: bool,
    pub has_rescue_zone: bool,
    pub has_vip_safety_zone: bool,
    pub has_escape_zone: bool,
    pub has_buy_zone: bool,
    pub hostage_count: u32,
    pub hostages_rescued: u32,
    pub bomb_defused: bool,
    pub bomb_exploded: bool,
}

/// Map-wide bomb parameters parsed from `info_map_parameters`.
#[derive(Resource, Debug, Clone)]
pub struct BombMapParams {
    pub bomb_radius: f32,
    pub ct_cant_buy: bool,
    pub t_cant_buy: bool,
}

impl Default for BombMapParams {
    fn default() -> Self {
        Self {
            bomb_radius: 500.0,
            ct_cant_buy: false,
            t_cant_buy: false,
        }
    }
}

/// Event requesting a full ReGameDLL `CleanUpMap` / `RestartRound` reset.
#[derive(Event, Debug, Clone, Copy, Default)]
pub struct RoundResetEvent;

pub struct EntityRuntimePlugin;

impl Plugin for EntityRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BombMapParams>();
        app.init_resource::<ScenarioMapData>();
        app.add_event::<FireTargetEvent>();
        app.add_event::<BreakEntityEvent>();
        app.add_event::<RoundResetEvent>();
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
                update_defuse_kit_pickups_system,
                handle_round_reset_entities_system,
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
    mut scenario_data: ResMut<ScenarioMapData>,
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
    *scenario_data = ScenarioMapData::default();

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
        let mut lip_str = "";
        let mut angles_str = "";
        let mut dmg_str = "";
        let mut buying_str = "";

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
                        "lip" => lip_str = parts[3],
                        "dmg" => dmg_str = parts[3],
                        "buying" => buying_str = parts[3],
                        "angle" | "angles" if angles_str.is_empty() => {
                            angles_str = parts[3];
                        }
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
            "info_player_start" | "info_player_deathmatch" | "info_vip_start" => {
                let yaw = angles_str
                    .split_whitespace()
                    .nth(1)
                    .or_else(|| angles_str.split_whitespace().next())
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(0.0);
                let pt = ([gx, gy, gz + 36.0], yaw);
                if classname == "info_player_start" {
                    scenario_data.ct_spawns.push(pt);
                } else if classname == "info_player_deathmatch" {
                    scenario_data.t_spawns.push(pt);
                } else {
                    scenario_data.vip_spawn = Some(pt);
                }
            }
            "hostage_entity" | "monster_scientist" => {
                if classname == "hostage_entity" {
                    scenario_data.hostage_count += 1;
                }
            }
            "item_thighpack" => {
                commands.spawn(DefuseKitPickup {
                    origin: bevy_origin,
                    collected: false,
                });
            }
            "func_breakable" => {
                let health = health_str.parse::<f32>().unwrap_or(1.0).max(1.0);
                let mat_id = material_str.parse::<u32>().unwrap_or(0);
                let material = BreakableMaterial::from_u32(mat_id);
                let explodemagnitude = explodemagnitude_str.parse::<u32>().unwrap_or(0);
                let spawnflags = spawnflags_str.parse::<u32>().unwrap_or(0);
                let trigger_only = (spawnflags & 1) != 0;

                // Submodel bounds derived directly from BSP CollisionModel brush_entities
                let (bmin, bmax) = if model_str.starts_with('*') {
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref())
                } else {
                    (
                        bevy_origin - Vec3::splat(16.0),
                        bevy_origin + Vec3::splat(16.0),
                    )
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
                        trigger_only,
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
                let dmg = dmg_str.parse::<f32>().unwrap_or(2.0);
                let movesnd = movesnd_str.parse::<u32>().unwrap_or(0);
                let stopsnd = stopsnd_str.parse::<u32>().unwrap_or(0);
                let move_sound = if movesnd > 0 {
                    Some(format!("doors/doormove{}.wav", movesnd.clamp(1, 10)))
                } else {
                    None
                };
                let stop_sound = if stopsnd > 0 {
                    Some(format!("doors/doorstop{}.wav", stopsnd.clamp(1, 8)))
                } else {
                    None
                };
                let distance = distance_str.parse::<f32>().unwrap_or(90.0);
                let lip = lip_str.parse::<f32>().unwrap_or(0.0);

                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                let (base_gs_mins, base_gs_maxs, base_gs_origin) =
                    get_submodel_raw_gs(model_str, collision_res.as_ref());
                let center = (bmin + bmax) * 0.5;
                let spawnflags = spawnflags_str.parse::<u32>().unwrap_or(0);
                let start_open = (spawnflags & 1) != 0;
                let reverse = (spawnflags & 2) != 0;
                let passable = (spawnflags & 8) != 0;
                let is_oneway = (spawnflags & 16) != 0;
                let is_toggle = (spawnflags & 32) != 0 || wait < 0.0;
                let use_only = (spawnflags & 256) != 0;
                // GoldSrc CBaseToggle::AxisDir: Z-rot=default(Y Bevy), BIT(6)=GS Y(NEG_Z Bevy), BIT(7)=GS X(X Bevy)
                let rot_axis = if (spawnflags & 128) != 0 {
                    Vec3::X
                } else if (spawnflags & 64) != 0 {
                    Vec3::NEG_Z
                } else {
                    Vec3::Y
                };
                let hinge_origin = if origin_str.is_empty() {
                    Vec3::new(bmin.x, center.y, bmin.z)
                } else {
                    bevy_origin
                };

                let (start_pos, target_pos, start_rot, target_rot) = if is_rotating {
                    let angle_deg = if reverse { -distance } else { distance };
                    let angle_rad = angle_deg.to_radians();
                    (
                        center,
                        center,
                        Quat::IDENTITY,
                        Quat::from_axis_angle(rot_axis, angle_rad),
                    )
                } else {
                    let move_dir = if angles_str == "-1" || angles_str == "-1 0 0" {
                        Vec3::Y
                    } else if angles_str == "-2" || angles_str == "-2 0 0" {
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
                    // GoldSrc CBaseDoor::Spawn: dist = (|movedir * (size - 2)|) - lip
                    let size_along_dir = (bmax - bmin).dot(move_dir.abs());
                    let dist = ((size_along_dir - 2.0) - lip).max(0.0);
                    (
                        center,
                        center + move_dir * dist,
                        Quat::IDENTITY,
                        Quat::IDENTITY,
                    )
                };

                let (pos1, pos2, rot1, rot2) = if start_open {
                    (target_pos, start_pos, target_rot, start_rot)
                } else {
                    (start_pos, target_pos, start_rot, target_rot)
                };

                commands.spawn((
                    DoorEntity {
                        model: model_str.to_string(),
                        targetname: targetname.to_string(),
                        target: target.to_string(),
                        spawnflags,
                        speed,
                        wait,
                        dmg,
                        wait_timer: 0.0,
                        blocked_cooldown: 0.0,
                        touch_cooldown: 0.0,
                        state: DoorState::Closed,
                        is_rotating,
                        is_toggle,
                        is_oneway,
                        use_only,
                        start_open,
                        passable,
                        rot_axis,
                        hinge_origin,
                        start_pos: pos1,
                        target_pos: pos2,
                        start_rot: rot1,
                        target_rot: rot2,
                        distance,
                        current_progress: 0.0,
                        move_sound,
                        stop_sound,
                        bmin,
                        bmax,
                        base_gs_mins,
                        base_gs_maxs,
                        base_gs_origin,
                    },
                    Transform::from_translation(pos1),
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
                    (
                        bevy_origin - Vec3::splat(256.0),
                        bevy_origin + Vec3::splat(256.0),
                    )
                };
                if classname == "func_hostage_rescue" {
                    scenario_data.has_rescue_zone = true;
                }
                commands.spawn((
                    HostageRescueZoneEntity { bmin, bmax },
                    Transform::from_translation((bmin + bmax) * 0.5),
                    Visibility::default(),
                ));
                tracing::info!(classname, "Spawned Hostage Rescue Zone");
            }
            "trigger_multiple" => {
                let wait = wait_str.parse::<f32>().unwrap_or(0.2);
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                commands.spawn(TriggerMultipleEntity {
                    target: target.to_string(),
                    wait,
                    next_trigger_time: 0.0,
                    bmin,
                    bmax,
                });
            }
            "trigger_once" => {
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                commands.spawn(TriggerOnceEntity {
                    target: target.to_string(),
                    triggered: false,
                    bmin,
                    bmax,
                });
            }
            "func_vip_safetyzone" => {
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                scenario_data.has_vip_safety_zone = true;
                commands.spawn(VipSafetyZoneEntity { bmin, bmax });
            }
            "func_escapezone" => {
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                scenario_data.has_escape_zone = true;
                commands.spawn(EscapeZoneEntity { bmin, bmax });
            }
            "trigger_push" => {
                let speed = speed_str.parse::<f32>().unwrap_or(100.0).max(10.0);
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                let yaw = angles_str
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(0.0);
                let rad = yaw.to_radians();
                let dir = Vec3::new(rad.cos(), 0.0, -rad.sin());
                commands.spawn(TriggerPushEntity {
                    speed,
                    dir,
                    bmin,
                    bmax,
                });
                tracing::info!(speed = speed, "Spawned trigger_push entity");
            }
            "trigger_hurt" => {
                let dmg = health_str
                    .parse::<f32>()
                    .or_else(|_| dmg_str.parse::<f32>())
                    .unwrap_or(10.0)
                    .max(1.0);
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                commands.spawn(TriggerHurtEntity {
                    dmg,
                    bmin,
                    bmax,
                    next_dmg_time: 0.0,
                });
                tracing::info!(dmg = dmg, "Spawned trigger_hurt entity");
            }
            "func_buyzone" => {
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                scenario_data.has_buy_zone = true;
                commands.spawn(BuyZoneEntity { bmin, bmax });
                tracing::info!("Spawned func_buyzone entity");
            }
            "func_bomb_target" => {
                let (bmin, bmax) =
                    get_submodel_bounds(model_str, bevy_origin, collision_res.as_ref());
                scenario_data.has_bomb_target = true;
                scenario_data.has_bomb_zone = true;
                commands.spawn(BombTargetZoneEntity { bmin, bmax });
                tracing::info!(?bmin, ?bmax, "Spawned func_bomb_target bombsite volume");
            }
            "info_bomb_target" => {
                scenario_data.has_bomb_target = true;
                commands.spawn(BombTargetPointEntity {
                    origin: bevy_origin,
                    radius: 256.0,
                });
                tracing::info!(?bevy_origin, "Spawned info_bomb_target bombsite point");
            }
            "info_map_parameters" => {
                let mut radius = 500.0f32;
                let buying = buying_str.parse::<u32>().unwrap_or(0);
                for line in block.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("\"bombradius\"") {
                        let val_str = rest.trim().trim_matches('"');
                        if let Ok(r) = val_str.parse::<f32>() {
                            radius = r.clamp(80.0, 2048.0);
                        }
                    }
                }
                commands.insert_resource(BombMapParams {
                    bomb_radius: radius,
                    ct_cant_buy: buying == 2 || buying == 3,
                    t_cant_buy: buying == 1 || buying == 3,
                });
                tracing::info!(bomb_radius = radius, buying, "Loaded info_map_parameters");
            }
            _ => {}
        }
    }
}

/// Extracts raw GoldSrc `(mins, maxs, origin)` for a BSP submodel brush entity.
fn get_submodel_raw_gs(
    model_str: &str,
    collision_res: Option<&Res<crate::player::LevelCollisionResource>>,
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    if let Some(sub_num_str) = model_str.strip_prefix('*')
        && let Ok(sub_num) = sub_num_str.parse::<usize>()
        && let Some(col_res) = collision_res
        && let Some(ref col_model) = col_res.model
        && let Some(ent) = col_model
            .brush_entities
            .iter()
            .find(|e| e.model_index == sub_num)
    {
        return (ent.mins, ent.maxs, ent.origin);
    }
    ([0.0; 3], [0.0; 3], [0.0; 3])
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
        && let Some(ent) = col_model
            .brush_entities
            .iter()
            .find(|e| e.model_index == sub_num)
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
    mut studio_query: Query<(
        &WorldStudioEntity,
        &mut Visibility,
        Option<&mut crate::studio::world_entity::EntityCollisionBounds>,
    )>,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut door_query: Query<&mut DoorEntity>,
) {
    for ev in events.read() {
        if ev.target.is_empty() {
            continue;
        }

        tracing::info!(target = ev.target, "Handling FireTargetEvent");

        // 1. EnvRender: Hide or modify target entities
        for render in &mut render_query {
            if render.targetname == ev.target {
                for (studio_ent, mut vis, bounds_opt) in &mut studio_query {
                    if studio_ent.targetname == render.target {
                        *vis = Visibility::Hidden;
                        if let Some(mut bounds) = bounds_opt {
                            bounds.mins = Vec3::ZERO;
                            bounds.maxs = Vec3::ZERO;
                        }
                        tracing::info!(
                            target = render.target,
                            "Hidden WorldStudioEntity and cleared collision bounds via env_render"
                        );
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
                    tracing::info!(
                        sound = ambient.message,
                        "Stopped ambient_generic looping playback"
                    );
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

        // 4. Trigger doors with matching targetname (trigger_multiple / trigger_once / buttons)
        for mut door in &mut door_query {
            if door.targetname == ev.target {
                match door.state {
                    DoorState::Closed => {
                        door.state = DoorState::Opening;
                        let center = (door.bmin + door.bmax) * 0.5;
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
                    DoorState::Open if door.is_toggle => {
                        door.state = DoorState::Closing;
                    }
                    DoorState::Open | DoorState::Opening | DoorState::Closing => {
                        // GoldSrc doors.cpp:523: Trigger activations on moving or open doors are strictly ignored!
                        // Doors must fully finish closing to TS_AT_BOTTOM (Closed) before trigger can re-open them.
                    }
                }
            }
        }
    }
}

/// Handles material-based gib bursts, explosions, and brush despawning when a breakable entity is shattered.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity
)]
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
    mut hostage_query: Query<
        (
            &mut crate::studio::world_entity::HostageEntity,
            &mut crate::studio::animation::StudioAnimatedModel,
            &mut crate::studio::world_entity::EntityCollisionBounds,
            &GlobalTransform,
        ),
        (
            Without<crate::studio::viewmodel::ViewModelWeapon>,
            Without<crate::player::PlayerController>,
        ),
    >,
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
                        tracing::info!(
                            model = ev.model,
                            "Disabled physical collision for broken BSP entity"
                        );
                    }
                }
            }
        }

        let gib_model = match ev.material {
            BreakableMaterial::Computer => "models/computergibs.mdl",
            BreakableMaterial::Glass | BreakableMaterial::UnbreakableGlass => {
                "models/glassgibs.mdl"
            }
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

            // Explosion flash point light and genuine zerogxplode.spr sprite provide authentic GoldSrc visual
            let _ = &mut meshes;

            // 5. Blast camera kick on player (no physical knockback on breakables matching CS 1.6)
            for (p_trans, _p_ctrl, recoil_opt) in &mut player_query {
                let p_pos = p_trans.translation;
                let dist = ev.origin.distance(p_pos);
                let blast_radius = 240.0_f32;
                if dist < blast_radius {
                    let frac = (1.0 - (dist / blast_radius)).clamp(0.0, 1.0);
                    if let Some(mut recoil) = recoil_opt {
                        recoil.punch_pitch -= 3.0 * frac;
                        let yaw_kick = if ((dist * 10.0) as usize).is_multiple_of(2) {
                            1.5
                        } else {
                            -1.5
                        };
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
                gibs: 12,
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
            // Authentic GoldSrc breakable shatter sound (debris/bust*)
            let break_sound = match ev.material {
                BreakableMaterial::Glass | BreakableMaterial::UnbreakableGlass => {
                    let snd_idx = ((((ev.origin.x + ev.origin.y) * 10.0).abs() as usize) % 3) + 1;
                    format!("debris/bustglass{snd_idx}.wav")
                }
                BreakableMaterial::Wood => {
                    let snd_idx = ((((ev.origin.x + ev.origin.y) * 10.0).abs() as usize) % 3) + 1;
                    format!("debris/bustcrate{snd_idx}.wav")
                }
                BreakableMaterial::Metal => {
                    let snd_idx = ((((ev.origin.x + ev.origin.y) * 10.0).abs() as usize) % 2) + 1;
                    format!("debris/bustmetal{snd_idx}.wav")
                }
                BreakableMaterial::Flesh => {
                    let snd_idx = ((((ev.origin.x + ev.origin.y) * 10.0).abs() as usize) % 2) + 1;
                    format!("debris/bustflesh{snd_idx}.wav")
                }
                BreakableMaterial::CinderBlock | BreakableMaterial::Rocks => {
                    let snd_idx = ((((ev.origin.x + ev.origin.y) * 10.0).abs() as usize) % 2) + 1;
                    format!("debris/bustconcrete{snd_idx}.wav")
                }
                BreakableMaterial::CeilingTile => "debris/bustceiling.wav".to_string(),
                BreakableMaterial::Computer => "weapons/explode3.wav".to_string(),
            };

            if let Some(snd) =
                audio_bank.get_or_load_sound(&break_sound, &search_paths.dirs, &mut audio_sources)
            {
                commands.spawn((
                    AudioPlayer::new(snd),
                    PlaybackSettings::DESPAWN
                        .with_spatial(true)
                        .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 64.0))
                        .with_volume(bevy::audio::Volume::new(1.0)),
                    Transform::from_translation(ev.origin),
                ));
            }

            let shooter = EnvShooterEntity {
                targetname: String::new(),
                shootmodel: gib_model.to_string(),
                gibs: 5,
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
    let Some(model_data) =
        model_store.get_or_load(&shooter.shootmodel, search_paths, images, materials)
    else {
        tracing::warn!(model = shooter.shootmodel, "Could not load gib shootmodel");
        return;
    };

    let mut rng = SimpleRng::new(u64::from(shooter.origin.x.to_bits()) ^ u64::from(shooter.gibs));

    let key = shooter.shootmodel.replace('\\', "/").to_lowercase();
    let mesh_handles: Vec<Handle<Mesh>> = model_store
        .static_meshes
        .entry(key)
        .or_insert_with(|| {
            model_data
                .submeshes
                .iter()
                .map(|t| meshes.add(create_initial_bevy_mesh(t, false)))
                .collect()
        })
        .clone();

    let is_metal = shooter.shootmodel.to_ascii_lowercase().contains("metal");
    for i in 0..shooter.gibs {
        // Compute randomized initial velocity with variance
        let vx = rng.range_f32(-shooter.variance, shooter.variance) * 15.0;
        let vy = shooter.velocity + rng.range_f32(-shooter.variance, shooter.variance) * 20.0;
        let vz = rng.range_f32(-shooter.variance, shooter.variance) * 15.0;

        let spawn_pos = shooter.origin
            + Vec3::new(
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
                    bounce_count: 0,
                    play_bounce_sound: i % 4 == 0,
                    is_metal,
                    on_ground: false,
                },
                Transform::from_translation(spawn_pos)
                    .with_scale(Vec3::splat(rng.range_f32(0.08, 0.14))),
                Visibility::default(),
            ))
            .id();

        // Authentic GoldSrc / Xash3D cl_tent.c: R_BreakModel selects ONE submodel (bodypart)
        // rather than spawning all submodels simultaneously on top of each other!
        if !model_data.submeshes.is_empty() && !mesh_handles.is_empty() {
            let submesh_idx = (rng.next_u32() as usize) % model_data.submeshes.len();
            let template = &model_data.submeshes[submesh_idx];
            let mesh_handle = mesh_handles[submesh_idx].clone();
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

/// Advances position, ballistic gravity, BSP floor bounces, and debris clatter audio for active gib particles.
#[allow(clippy::too_many_arguments)]
pub fn update_gib_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    search_paths: Option<Res<GameSearchPaths>>,
    mut audio_bank: Option<ResMut<GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
    mut query: Query<(Entity, &mut GibParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut gib, mut transform) in &mut query {
        gib.lifetime += dt;
        if gib.lifetime >= gib.max_lifetime {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        if gib.on_ground {
            // Once settled on ground, gib rests still (FL_ONGROUND parity)
            continue;
        }

        // Apply GoldSrc downward gravity (400 u/s^2) and air drag
        gib.velocity.y -= 400.0 * dt;
        gib.velocity.x *= (1.0 - 0.5 * dt).max(0.0);
        gib.velocity.z *= (1.0 - 0.5 * dt).max(0.0);

        let old_pos = transform.translation;
        let new_pos = old_pos + gib.velocity * dt;

        let mut hit = false;
        if let Some(col_res) = collision_res.as_ref()
            && let Some(ref col_model) = col_res.model
            && gib.velocity.length_squared() > 16.0
        {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col_model);
            let tr = bsp.trace_player_hull(
                [old_pos.x, -old_pos.z, old_pos.y],
                [new_pos.x, -new_pos.z, new_pos.y],
                0,
            );
            if tr.fraction < 1.0 {
                hit = true;
                let hit_n = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                    .normalize_or_zero();

                let normal_vel = gib.velocity.dot(hit_n);
                transform.translation = old_pos + (new_pos - old_pos) * tr.fraction + hit_n * 0.05;
                // GoldSrc entity.cpp:760-766: traceNormal[2] > 0.9 (hit floor)
                // When floor is hit, damp velocity: damp = bounceFactor * 0.5;
                // If baseline.origin[2] <= 0 && baseline.origin[2] >= gravity * 3 => damp = 0 (Stop)
                if hit_n.y > 0.7 {
                    // Settle immediately if falling slowly or after first bounce to eliminate jitter
                    if normal_vel.abs() < 120.0
                        || gib.velocity.length_squared() < 4900.0
                        || gib.bounce_count >= 1
                    {
                        gib.on_ground = true;
                        gib.velocity = Vec3::ZERO;
                        gib.angular_velocity = Vec3::ZERO;
                    } else if normal_vel < 0.0 {
                        // GoldSrc damp = 0.5 * 0.4 = 0.2
                        gib.velocity = (gib.velocity - hit_n * (1.2 * normal_vel)) * 0.25;
                        gib.angular_velocity *= 0.3;
                    }
                } else {
                    if normal_vel < 0.0 {
                        gib.velocity = (gib.velocity - hit_n * (1.2 * normal_vel)) * 0.25;
                    }
                    gib.angular_velocity *= 0.3;
                }
                gib.bounce_count = gib.bounce_count.saturating_add(1);

                if gib.play_bounce_sound
                    && gib.is_metal
                    && gib.bounce_count <= 2
                    && let (Some(paths), Some(ref mut bank), Some(ref mut sources)) = (
                        search_paths.as_ref(),
                        audio_bank.as_mut(),
                        audio_sources.as_mut(),
                    )
                {
                    let idx = ((old_pos.x.abs() * 10.0) as usize % 3) + 1;
                    let snd_path = format!("debris/metal{idx}.wav");
                    if let Some(h) = bank.get_or_load_sound(&snd_path, &paths.dirs, sources) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 64.0))
                                .with_volume(bevy::audio::Volume::new(0.45)),
                            Transform::from_translation(transform.translation),
                        ));
                    }
                }
            }
        }

        if !hit {
            transform.translation = new_pos;
        }
        transform.rotate_local_x(gib.angular_velocity.x * dt);
        transform.rotate_local_y(gib.angular_velocity.y * dt);
        transform.rotate_local_z(gib.angular_velocity.z * dt);
    }
}

/// Computes visual submodel translation and rotation for a door at `progress`.
fn compute_door_pose(door: &DoorEntity, progress: f32) -> (Vec3, Quat) {
    if door.is_rotating {
        let rot = door.start_rot.slerp(door.target_rot, progress);
        let p = door.hinge_origin;
        (p - rot.mul_vec3(p), rot)
    } else {
        let delta = (door.target_pos - door.start_pos) * progress;
        (delta, Quat::IDENTITY)
    }
}

/// Computes `target_rot` for a rotating door so its non-oneway swing opens away from `actor_pos`
/// following canonical ReGameDLL `CRotDoor::DoorGoUp` (dlls/doors.cpp:544-624).
fn orient_rotating_door_away(door: &mut DoorEntity, actor_pos: Vec3) {
    if !door.is_rotating || door.is_oneway {
        return;
    }
    // Convert actor position to canonical GoldSrc 2D coords (GS X = bevy.x, GS Y = -bevy.z)
    let to_activator_x = actor_pos.x;
    let to_activator_y = -actor_pos.z;

    let origin_x = door.base_gs_origin[0];
    let origin_y = door.base_gs_origin[1];
    // Note: `base_gs_mins` and `base_gs_maxs` in `BrushEntityCollision` are already in absolute world space.
    let lo_x = door.base_gs_mins[0];
    let lo_y = door.base_gs_mins[1];
    let hi_x = door.base_gs_maxs[0];
    let hi_y = door.base_gs_maxs[1];

    let moment_arm_x = to_activator_x - origin_x;
    let moment_arm_y = to_activator_y - origin_y;

    let sign: f32 = if lo_x > to_activator_x {
        if to_activator_y < lo_y {
            if moment_arm_y.abs() > moment_arm_x.abs() {
                if moment_arm_y < 0.0 { 1.0 } else { -1.0 }
            } else {
                if moment_arm_x > 0.0 { 1.0 } else { -1.0 }
            }
        } else if to_activator_y > hi_y {
            if moment_arm_y.abs() > moment_arm_x.abs() {
                if moment_arm_y < 0.0 { 1.0 } else { -1.0 }
            } else {
                if moment_arm_x < 0.0 { 1.0 } else { -1.0 }
            }
        } else {
            if moment_arm_y < 0.0 { 1.0 } else { -1.0 }
        }
    } else {
        if to_activator_x <= hi_x {
            if to_activator_y < lo_y {
                if moment_arm_x > 0.0 { 1.0 } else { -1.0 }
            } else if to_activator_y > hi_y {
                if moment_arm_x < 0.0 { 1.0 } else { -1.0 }
            } else {
                1.0
            }
        } else if to_activator_y < lo_y {
            if moment_arm_y.abs() > moment_arm_x.abs() {
                if moment_arm_y > 0.0 { 1.0 } else { -1.0 }
            } else {
                if moment_arm_x > 0.0 { 1.0 } else { -1.0 }
            }
        } else if to_activator_y > hi_y {
            if moment_arm_y.abs() > moment_arm_x.abs() {
                if moment_arm_y > 0.0 { 1.0 } else { -1.0 }
            } else {
                if moment_arm_x < 0.0 { 1.0 } else { -1.0 }
            }
        } else {
            if moment_arm_y > 0.0 { 1.0 } else { -1.0 }
        }
    };

    let reverse_sign = if (door.spawnflags & 2) != 0 {
        -1.0
    } else {
        1.0
    };

    // In GoldSrc: AngularMove(m_vecAngle2 * sign, pev->speed);
    // Opening away from player requires positive sign in Bevy coordinate system.
    let final_angle_deg = sign * reverse_sign * door.distance;
    door.target_rot = Quat::from_axis_angle(door.rot_axis, final_angle_deg.to_radians());
}

/// Synchronizes visual mesh transform and `SOLID_BSP` collision bounds for a door leaf.
fn sync_door_mesh_and_collision(
    door: &DoorEntity,
    submodel_query: &mut Query<
        (&BspSubmodelMesh, &mut Transform),
        (
            Without<DoorEntity>,
            Without<crate::player::PlayerController>,
        ),
    >,
    collision_res: &mut Option<ResMut<crate::player::LevelCollisionResource>>,
) {
    let (vis_pos, vis_rot) = compute_door_pose(door, door.current_progress);
    for (submesh, mut mesh_t) in submodel_query.iter_mut() {
        if submesh.model == door.model {
            mesh_t.translation = vis_pos;
            mesh_t.rotation = vis_rot;
        }
    }

    let Some(col_res) = collision_res.as_mut() else {
        return;
    };
    let Some(col_model) = col_res.model.as_mut() else {
        return;
    };
    let Some(sub_num_str) = door.model.strip_prefix('*') else {
        return;
    };
    let Ok(sub_num) = sub_num_str.parse::<usize>() else {
        return;
    };

    for ent in &mut col_model.brush_entities {
        if ent.model_index == sub_num {
            ent.solid = if door.passable {
                goldhub_formats::ir::SOLID_NOT
            } else {
                goldhub_formats::ir::SOLID_BSP
            };
            if door.is_rotating {
                let (yaw, pitch, roll) = vis_rot.to_euler(EulerRot::YXZ);
                ent.angles = [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()];
                let radius = (door.bmax - door.bmin).length() * 0.5 + 8.0;
                let c = ent.origin;
                ent.mins = [c[0] - radius, c[1] - radius, door.base_gs_mins[2]];
                ent.maxs = [c[0] + radius, c[1] + radius, door.base_gs_maxs[2]];
            } else {
                let gs_delta = [vis_pos.x, -vis_pos.z, vis_pos.y];
                ent.origin = [
                    door.base_gs_origin[0] + gs_delta[0],
                    door.base_gs_origin[1] + gs_delta[1],
                    door.base_gs_origin[2] + gs_delta[2],
                ];
                // Keep broadphase AABB updated at live shifted position!
                ent.mins = [
                    door.base_gs_mins[0] + gs_delta[0],
                    door.base_gs_mins[1] + gs_delta[1],
                    door.base_gs_mins[2] + gs_delta[2],
                ];
                ent.maxs = [
                    door.base_gs_maxs[0] + gs_delta[0],
                    door.base_gs_maxs[1] + gs_delta[1],
                    door.base_gs_maxs[2] + gs_delta[2],
                ];
            }
        }
    }
}

/// Checks if player hull `[p_pos ± (16, half_h, 16)]` overlaps `door` at its current progress;
/// if overlapping, returns an outward push delta in Bevy world coordinates (`SV_PushMove`/`SV_PushRotate`).
fn compute_door_push_on_player(
    door: &DoorEntity,
    prev_progress: f32,
    p_bevy: Vec3,
    half_h: f32,
) -> Option<Vec3> {
    if door.passable {
        return None;
    }

    if door.is_rotating {
        let cur_rot = door.start_rot.slerp(door.target_rot, door.current_progress);
        let prev_rot = door.start_rot.slerp(door.target_rot, prev_progress);
        let pivot = door.hinge_origin;
        let inv_rot = cur_rot.inverse();
        let local_p = inv_rot * (p_bevy - pivot) + pivot;

        let pmin = local_p - Vec3::new(16.0, half_h, 16.0);
        let pmax = local_p + Vec3::new(16.0, half_h, 16.0);
        if pmax.x <= door.bmin.x
            || pmin.x >= door.bmax.x
            || pmax.y <= door.bmin.y
            || pmin.y >= door.bmax.y
            || pmax.z <= door.bmin.z
            || pmin.z >= door.bmax.z
        {
            return None;
        }

        // Player overlaps the swinging door blade: compute tangential sweep + minimum exit normal
        let sweep_pt =
            (cur_rot * inv_rot) * (p_bevy - pivot) - (prev_rot * (inv_rot * (p_bevy - pivot)));
        let pen_x_lo = pmax.x - door.bmin.x;
        let pen_x_hi = door.bmax.x - pmin.x;
        let pen_z_lo = pmax.z - door.bmin.z;
        let pen_z_hi = door.bmax.z - pmin.z;
        let min_pen = pen_x_lo.min(pen_x_hi).min(pen_z_lo).min(pen_z_hi);
        let local_eject = if (min_pen - pen_x_lo).abs() < 1e-4 {
            Vec3::new(-(pen_x_lo + 0.5), 0.0, 0.0)
        } else if (min_pen - pen_x_hi).abs() < 1e-4 {
            Vec3::new(pen_x_hi + 0.5, 0.0, 0.0)
        } else if (min_pen - pen_z_lo).abs() < 1e-4 {
            Vec3::new(0.0, 0.0, -(pen_z_lo + 0.5))
        } else {
            Vec3::new(0.0, 0.0, pen_z_hi + 0.5)
        };
        let world_eject = cur_rot * local_eject;
        let push = if world_eject.length_squared() > sweep_pt.length_squared() {
            world_eject
        } else {
            sweep_pt + world_eject * 0.5
        };
        Some(Vec3::new(push.x, 0.0, push.z))
    } else {
        let cur_delta = (door.target_pos - door.start_pos) * door.current_progress;
        let prev_delta = (door.target_pos - door.start_pos) * prev_progress;
        let step_move = cur_delta - prev_delta;
        let cur_min = door.bmin + cur_delta;
        let cur_max = door.bmax + cur_delta;

        let pmin = p_bevy - Vec3::new(16.0, half_h, 16.0);
        let pmax = p_bevy + Vec3::new(16.0, half_h, 16.0);
        if pmax.x <= cur_min.x
            || pmin.x >= cur_max.x
            || pmax.y <= cur_min.y
            || pmin.y >= cur_max.y
            || pmax.z <= cur_min.z
            || pmin.z >= cur_max.z
        {
            return None;
        }

        let dir = step_move.normalize_or_zero();
        if dir.length_squared() < 1e-4 {
            return None;
        }

        // GoldSrc SV_PushMove parity: sliding doors only push players who intersect the leading face
        // along the movement direction. Brushing against the lateral side face does not drag or block.
        let is_frontal = if dir.x > 0.5 {
            pmax.x >= cur_max.x - 2.0
        } else if dir.x < -0.5 {
            pmin.x <= cur_min.x + 2.0
        } else if dir.z > 0.5 {
            pmax.z >= cur_max.z - 2.0
        } else if dir.z < -0.5 {
            pmin.z <= cur_min.z + 2.0
        } else if dir.y > 0.5 {
            pmax.y >= cur_max.y - 2.0
        } else if dir.y < -0.5 {
            pmin.y <= cur_min.y + 2.0
        } else {
            true
        };

        if !is_frontal {
            return None;
        }

        // Push player along sliding door travel by step movement
        Some(step_move)
    }
}

/// Attempts to push player by `push_world` excluding `self_model` from solids; returns `false` if blocked.
fn try_push_player_excluding_self(
    self_model: &str,
    push_world: Vec3,
    player: &mut crate::player::PlayerController,
    col_res: &mut Option<ResMut<crate::player::LevelCollisionResource>>,
) -> bool {
    let gs_push = [push_world.x, -push_world.z, push_world.y];
    let Some(col_res_mut) = col_res.as_mut() else {
        player.sim.current.origin[0] += gs_push[0];
        player.sim.current.origin[1] += gs_push[1];
        player.sim.current.origin[2] += gs_push[2];
        return true;
    };
    let Some(col_model) = col_res_mut.model.as_mut() else {
        player.sim.current.origin[0] += gs_push[0];
        player.sim.current.origin[1] += gs_push[1];
        player.sim.current.origin[2] += gs_push[2];
        return true;
    };

    let self_idx = self_model
        .strip_prefix('*')
        .and_then(|s| s.parse::<usize>().ok());
    let mut saved_solid = None;
    if let Some(idx) = self_idx {
        for ent in &mut col_model.brush_entities {
            if ent.model_index == idx {
                saved_solid = Some((idx, ent.solid));
                ent.solid = goldhub_formats::ir::SOLID_NOT;
                break;
            }
        }
    }

    let start = player.sim.current.origin;
    let end = [
        start[0] + gs_push[0],
        start[1] + gs_push[1],
        start[2] + gs_push[2],
    ];
    let hull = player.sim.current.active_hull();
    let tr = {
        use goldhub_physics::WorldTrace;
        let bsp = goldhub_physics::BspWorldCollision::new(col_model);
        bsp.trace_player_hull(start, end, hull)
    };

    if let Some((idx, s)) = saved_solid {
        for ent in &mut col_model.brush_entities {
            if ent.model_index == idx {
                ent.solid = s;
                break;
            }
        }
    }

    if tr.allsolid || tr.fraction < 0.85 {
        false
    } else {
        player.sim.current.origin = tr.endpos;
        true
    }
}

/// Advances active door state, interpolates movement, executes `SV_PushMove`/`DoorBlocked`, and manages audio.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub fn update_doors_system(
    mut commands: Commands,
    time: Res<Time>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut collision_res: Option<ResMut<crate::player::LevelCollisionResource>>,
    mut door_query: Query<(Entity, &mut DoorEntity)>,
    mut submodel_query: Query<
        (&BspSubmodelMesh, &mut Transform),
        (
            Without<DoorEntity>,
            Without<crate::player::PlayerController>,
        ),
    >,
    mut player_query: Query<&mut crate::player::PlayerController>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok(mut player) = player_query.get_single_mut() else {
        return;
    };
    let gs_ori = player.sim.current.origin;
    let player_hull_center = Vec3::new(gs_ori[0], gs_ori[2], -gs_ori[1]);
    let half_h = if player.sim.current.is_ducking {
        18.0
    } else {
        36.0
    };

    // 1. Direct DoorTouch (unnamed non-use_only doors; named doors wait for trigger_multiple/Use!)
    for (_door_ent, mut door) in &mut door_query {
        door.blocked_cooldown = (door.blocked_cooldown - dt).max(0.0);
        door.touch_cooldown = (door.touch_cooldown - dt).max(0.0);

        if !door.targetname.is_empty() || door.use_only || door.touch_cooldown > 0.0 {
            continue;
        }

        let (vis_pos, vis_rot) = compute_door_pose(&door, door.current_progress);
        let touches_door = if door.is_rotating {
            let pivot = door.hinge_origin;
            let local_p = vis_rot.inverse() * (player_hull_center - pivot) + pivot;
            local_p.x >= door.bmin.x - 20.0
                && local_p.x <= door.bmax.x + 20.0
                && local_p.y >= door.bmin.y - half_h
                && local_p.y <= door.bmax.y + half_h
                && local_p.z >= door.bmin.z - 20.0
                && local_p.z <= door.bmax.z + 20.0
        } else {
            let cmin = door.bmin + vis_pos;
            let cmax = door.bmax + vis_pos;
            player_hull_center.x >= cmin.x - 20.0
                && player_hull_center.x <= cmax.x + 20.0
                && player_hull_center.y >= cmin.y - half_h
                && player_hull_center.y <= cmax.y + half_h
                && player_hull_center.z >= cmin.z - 20.0
                && player_hull_center.z <= cmax.z + 20.0
        };

        if !touches_door {
            continue;
        }

        if door.state == DoorState::Closed {
            orient_rotating_door_away(&mut door, player_hull_center);
            door.state = DoorState::Opening;
            door.touch_cooldown = 0.6;
        } else if door.is_toggle && door.state == DoorState::Open {
            // `de_nuke` Squeaky (`SF_DOOR_NO_AUTO_RETURN`): touching from open side closes it!
            door.state = DoorState::Closing;
            door.touch_cooldown = 0.6;
        } else {
            continue;
        }

        if let Some(ref move_snd_name) = door.move_sound
            && let Some(h) =
                audio_bank.get_or_load_sound(move_snd_name, &search_paths.dirs, &mut audio_sources)
        {
            commands.spawn((
                AudioPlayer::new(h),
                PlaybackSettings::DESPAWN
                    .with_spatial(true)
                    .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 128.0))
                    .with_volume(bevy::audio::Volume::new(1.5)),
                Transform::from_translation((door.bmin + door.bmax) * 0.5),
            ));
        }
    }

    // 2. Integrate active doors with SV_PushMove / SV_PushRotate and collect blocked group targets
    let mut blocked_groups: Vec<(String, DoorState, f32)> = Vec::new();
    for (_door_ent, mut door) in &mut door_query {
        let travel_time = if door.is_rotating {
            door.distance.to_radians() / door.speed.to_radians().max(0.1)
        } else {
            door.start_pos.distance(door.target_pos) / door.speed.max(1.0)
        };
        let step = if travel_time > 0.0 {
            dt / travel_time
        } else {
            1.0
        };

        match door.state {
            DoorState::Closed => {
                // Keep START_OPEN / initial pose synced
                sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);
            }
            DoorState::Opening => {
                let prev_progress = door.current_progress;
                door.current_progress = (door.current_progress + step).min(1.0);
                sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);

                let cur_ori = player.sim.current.origin;
                let cur_p = Vec3::new(cur_ori[0], cur_ori[2], -cur_ori[1]);
                if let Some(push) = compute_door_push_on_player(&door, prev_progress, cur_p, half_h)
                    && !try_push_player_excluding_self(
                        &door.model,
                        push,
                        &mut player,
                        &mut collision_res,
                    )
                {
                    door.current_progress = prev_progress;
                    sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);
                    if door.blocked_cooldown <= 0.0 {
                        door.blocked_cooldown = 0.25;
                        if door.dmg > 0.0 {
                            player.health = (player.health - door.dmg).max(0.0);
                        }
                        if door.wait >= 0.0 {
                            door.state = DoorState::Closing;
                            if !door.targetname.is_empty() {
                                blocked_groups.push((
                                    door.targetname.clone(),
                                    DoorState::Closing,
                                    prev_progress,
                                ));
                            }
                        }
                    }
                    continue;
                }

                if door.current_progress >= 1.0 {
                    door.state = DoorState::Open;
                    door.wait_timer = door.wait;
                    door.touch_cooldown = 0.45;
                    if let Some(ref stop_snd_name) = door.stop_sound
                        && let Some(h) = audio_bank.get_or_load_sound(
                            stop_snd_name,
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
            DoorState::Open => {
                if !door.is_toggle && door.wait >= 0.0 {
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
                let prev_progress = door.current_progress;
                door.current_progress = (door.current_progress - step).max(0.0);
                sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);

                let cur_ori = player.sim.current.origin;
                let cur_p = Vec3::new(cur_ori[0], cur_ori[2], -cur_ori[1]);
                if let Some(push) = compute_door_push_on_player(&door, prev_progress, cur_p, half_h)
                    && !try_push_player_excluding_self(
                        &door.model,
                        push,
                        &mut player,
                        &mut collision_res,
                    )
                {
                    door.current_progress = prev_progress;
                    sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);
                    if door.blocked_cooldown <= 0.0 {
                        door.blocked_cooldown = 0.25;
                        if door.dmg > 0.0 {
                            player.health = (player.health - door.dmg).max(0.0);
                        }
                        if door.wait >= 0.0 {
                            door.state = DoorState::Opening;
                            if !door.targetname.is_empty() {
                                blocked_groups.push((
                                    door.targetname.clone(),
                                    DoorState::Opening,
                                    prev_progress,
                                ));
                            }
                        }
                    }
                    continue;
                }

                if door.current_progress <= 0.0 {
                    door.state = DoorState::Closed;
                    door.touch_cooldown = 0.45;
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
                }
            }
        }
    }

    // 3. Reverse and lockstep-synchronize companion leaves in team doors (`de_nuke` GlassDoor1..4)
    for (group_name, reversed_state, sync_prog) in blocked_groups {
        for (_ent, mut door) in &mut door_query {
            if door.targetname == group_name && door.wait >= 0.0 {
                door.state = reversed_state;
                door.current_progress = sync_prog;
                door.blocked_cooldown = 0.25;
                sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);
            }
        }
    }

    // 4. Synchronize state and progress across all door leaves sharing targetname (GoldSrc m_pLink parity)
    let mut team_states: std::collections::HashMap<String, (DoorState, f32, f32)> =
        std::collections::HashMap::new();
    for (_ent, door) in &door_query {
        if !door.targetname.is_empty() && door.state != DoorState::Closed {
            team_states
                .entry(door.targetname.clone())
                .and_modify(|(st, prog, tm)| {
                    if *st == DoorState::Closed && door.state != DoorState::Closed {
                        *st = door.state;
                        *prog = door.current_progress;
                        *tm = door.wait_timer;
                    }
                })
                .or_insert((door.state, door.current_progress, door.wait_timer));
        }
    }
    for (group, (st, prog, tm)) in team_states {
        for (_ent, mut door) in &mut door_query {
            if door.targetname == group {
                if door.state != st {
                    door.state = st;
                    door.wait_timer = tm;
                }
                if (door.current_progress - prog).abs() > 1e-3 {
                    door.current_progress = prog;
                    sync_door_mesh_and_collision(&door, &mut submodel_query, &mut collision_res);
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

    let mut matched_targetname: Option<String> = None;
    let mut used_any = false;
    for mut door in &mut door_query {
        // GoldSrc parity: Only doors with SF_DOOR_USE_ONLY (spawnflags & 256) accept +use (E).
        // Sliding doors with triggers (de_nuke) or touch-only doors (de_inferno) ignore +use!
        if !door.use_only {
            continue;
        }

        let (vis_pos, _) = compute_door_pose(&door, door.current_progress);
        let center = (door.bmin + door.bmax) * 0.5 + vis_pos;
        if cam_pos.distance(center) > USE_REACH + 64.0 {
            continue;
        }

        if ray_intersects_aabb(
            cam_pos,
            cam_forward,
            USE_REACH,
            door.bmin + vis_pos - Vec3::splat(16.0),
            door.bmax + vis_pos + Vec3::splat(16.0),
        )
        .is_some()
        {
            if !door.targetname.is_empty() {
                matched_targetname = Some(door.targetname.clone());
            }
            match door.state {
                DoorState::Closed => {
                    orient_rotating_door_away(&mut door, cam_pos);
                    door.state = DoorState::Opening;
                    used_any = true;
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
                    // ReGameDLL doors.cpp:462: Only SF_DOOR_NO_AUTO_RETURN (is_toggle) doors can be manually closed via USE.
                    // Auto-return doors (e.g. de_inferno wait=4) ignore USE while open and return automatically.
                    if door.is_toggle {
                        door.state = DoorState::Closing;
                        used_any = true;
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
                }
                DoorState::Closing | DoorState::Opening => {
                    // GoldSrc doors.cpp:523: USE is ignored while door is moving.
                }
            }
            break;
        }
    }

    if let Some(group) = matched_targetname {
        for mut door in &mut door_query {
            if door.targetname == group && door.state == DoorState::Closed {
                door.state = DoorState::Opening;
            }
        }
    }

    // GoldSrc PlayerUse audio parity: wpn_select on success, wpn_denyselect on empty/fail
    let use_snd = if used_any {
        "common/wpn_select.wav"
    } else {
        "common/wpn_denyselect.wav"
    };
    if let Some(h) = audio_bank.get_or_load_sound(use_snd, &search_paths.dirs, &mut audio_sources) {
        commands.spawn((
            AudioPlayer::new(h),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::new(0.55)),
        ));
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

/// Updates player interaction with trigger_push, trigger_hurt, trigger_multiple, and trigger_once zones.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_triggers_system(
    mut commands: Commands,
    time: Res<Time>,
    mut player_query: Query<&mut crate::player::PlayerController>,
    push_query: Query<&TriggerPushEntity>,
    mut hurt_query: Query<&mut TriggerHurtEntity>,
    mut multi_query: Query<&mut TriggerMultipleEntity>,
    mut once_query: Query<&mut TriggerOnceEntity>,
    mut fire_events: EventWriter<FireTargetEvent>,
    search_paths: Option<Res<GameSearchPaths>>,
    mut audio_bank: Option<ResMut<GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok(mut p_ctrl) = player_query.get_single_mut() else {
        return;
    };
    let gs_ori = p_ctrl.sim.current.origin;
    let p_pos = Vec3::new(gs_ori[0], gs_ori[2], -gs_ori[1]);
    let half_h = if p_ctrl.sim.current.is_ducking {
        18.0
    } else {
        36.0
    };
    let hull_overlaps = |bmin: Vec3, bmax: Vec3| -> bool {
        p_pos.x + 16.0 >= bmin.x
            && p_pos.x - 16.0 <= bmax.x
            && p_pos.y + half_h >= bmin.y
            && p_pos.y - half_h <= bmax.y
            && p_pos.z + 16.0 >= bmin.z
            && p_pos.z - 16.0 <= bmax.z
    };

    // 1. Trigger Push
    for push in &push_query {
        if hull_overlaps(push.bmin, push.bmax) {
            let gs_dir = [push.dir.x, -push.dir.z, push.dir.y];
            p_ctrl.sim.current.velocity[0] += gs_dir[0] * push.speed * dt * 5.0;
            p_ctrl.sim.current.velocity[1] += gs_dir[1] * push.speed * dt * 5.0;
            p_ctrl.sim.current.velocity[2] += gs_dir[2] * push.speed * dt * 5.0;
        }
    }

    // 2. Trigger Hurt (inflicts real damage & pain feedback)
    for mut hurt in &mut hurt_query {
        if hull_overlaps(hurt.bmin, hurt.bmax) && hurt.next_dmg_time <= time.elapsed_secs() {
            hurt.next_dmg_time = time.elapsed_secs() + 0.5;
            p_ctrl.health = (p_ctrl.health - hurt.dmg).max(0.0);
            if let (Some(paths), Some(bank), Some(sources)) = (
                search_paths.as_ref(),
                audio_bank.as_deref_mut(),
                audio_sources.as_deref_mut(),
            ) && let Some(h) =
                bank.get_or_load_sound("player/pl_pain2.wav", &paths.dirs, sources)
            {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
        }
    }

    // 3. Trigger Multiple (e.g. `de_nuke` GlassDoor1..4)
    let now = time.elapsed_secs();
    for mut multi in &mut multi_query {
        if hull_overlaps(multi.bmin, multi.bmax) && now >= multi.next_trigger_time {
            multi.next_trigger_time = now + multi.wait.max(0.1);
            if !multi.target.is_empty() {
                fire_events.send(FireTargetEvent {
                    target: multi.target.clone(),
                });
            }
        }
    }

    // 4. Trigger Once
    for mut once in &mut once_query {
        if !once.triggered && hull_overlaps(once.bmin, once.bmax) {
            once.triggered = true;
            if !once.target.is_empty() {
                fire_events.send(FireTargetEvent {
                    target: once.target.clone(),
                });
            }
        }
    }
}

/// Handles picking up map-placed `item_thighpack` defusal kits on CTs (`#Got_defuser`).
pub fn update_defuse_kit_pickups_system(
    mut commands: Commands,
    mut player_q: Query<&mut crate::player::PlayerController>,
    mut pickups: Query<&mut DefuseKitPickup>,
    search_paths: Option<Res<GameSearchPaths>>,
    mut audio_bank: Option<ResMut<GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
    mut pickup_events: EventWriter<crate::hud::ItemPickupEvent>,
) {
    let Ok(mut player) = player_q.get_single_mut() else {
        return;
    };
    if player.team != goldhub_game_cstrike::CsTeam::CounterTerrorist || player.has_defuser {
        return;
    }

    let ori = player.sim.current.origin;
    let p_center = Vec3::new(ori[0], ori[2], -ori[1]);
    for mut kit in &mut pickups {
        if !kit.collected && p_center.distance(kit.origin) <= 44.0 {
            kit.collected = true;
            player.has_defuser = true;
            player.set_center_message("#Got_defuser", 3.0);
            pickup_events.send(crate::hud::ItemPickupEvent {
                weapon_id: None,
                ammo_caliber: None,
                is_defuser: true,
            });
            if let (Some(paths), Some(bank), Some(sources)) = (
                search_paths.as_ref(),
                audio_bank.as_deref_mut(),
                audio_sources.as_deref_mut(),
            ) && let Some(h) = bank.get_or_load_sound("items/kevlar.wav", &paths.dirs, sources)
            {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
        }
    }
}

/// Executes ReGameDLL `CleanUpMap` for breakables, doors, decals, gibs, smoke, and triggers on `RoundResetEvent`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn handle_round_reset_entities_system(
    mut commands: Commands,
    mut events: EventReader<RoundResetEvent>,
    mut breakables: Query<&mut BreakableEntity>,
    mut submodels: Query<
        (&BspSubmodelMesh, &mut Visibility, &mut Transform),
        (
            Without<DoorEntity>,
            Without<crate::player::PlayerController>,
        ),
    >,
    mut doors: Query<&mut DoorEntity>,
    mut triggers_once: Query<&mut TriggerOnceEntity>,
    mut pickups: Query<&mut DefuseKitPickup>,
    mut scenario_map: Option<ResMut<ScenarioMapData>>,
    mut meshes: ResMut<Assets<Mesh>>,
    decals_q: Query<(Entity, &crate::sprite::BulletHoleDecal)>,
    gibs_q: Query<Entity, With<GibParticle>>,
    smoke_q: Query<Entity, With<crate::sprite::SmokeBillboardParticle>>,
    mut collision_res: Option<ResMut<crate::player::LevelCollisionResource>>,
) {
    let mut triggered = false;
    for _ in events.read() {
        triggered = true;
    }
    if !triggered {
        return;
    }
    if let Some(ref mut map_data) = scenario_map {
        map_data.hostages_rescued = 0;
        map_data.bomb_defused = false;
        map_data.bomb_exploded = false;
    }

    // 1. Restore all breakables (`func_breakable`: health, visibility, SOLID_BSP collision)
    for mut b in &mut breakables {
        b.health = b.max_health;
        b.is_broken = false;
        if !b.model.is_empty() {
            for (sub, mut vis, _) in &mut submodels {
                if sub.model == b.model {
                    *vis = Visibility::Inherited;
                }
            }
            if let Some(ref mut col_res) = collision_res
                && let Some(ref mut col_model) = col_res.model
                && let Some(sub_idx_str) = b.model.strip_prefix('*')
                && let Ok(sub_idx) = sub_idx_str.parse::<usize>()
            {
                for ent in &mut col_model.brush_entities {
                    if ent.model_index == sub_idx {
                        ent.solid = goldhub_formats::ir::SOLID_BSP;
                    }
                }
            }
        }
    }

    // 2. Reset doors to closed/initial pose
    for mut door in &mut doors {
        door.state = DoorState::Closed;
        door.current_progress = 0.0;
        door.wait_timer = 0.0;
        door.blocked_cooldown = 0.0;
        door.touch_cooldown = 0.0;
        let (vis_pos, vis_rot) = compute_door_pose(&door, 0.0);
        for (sub, _, mut tf) in &mut submodels {
            if sub.model == door.model {
                tf.translation = vis_pos;
                tf.rotation = vis_rot;
            }
        }
        if let Some(ref mut col_res) = collision_res
            && let Some(ref mut col_model) = col_res.model
            && let Some(sub_idx_str) = door.model.strip_prefix('*')
            && let Ok(sub_idx) = sub_idx_str.parse::<usize>()
        {
            for ent in &mut col_model.brush_entities {
                if ent.model_index == sub_idx {
                    ent.solid = if door.passable {
                        goldhub_formats::ir::SOLID_NOT
                    } else {
                        goldhub_formats::ir::SOLID_BSP
                    };
                    ent.angles = [0.0; 3];
                    ent.origin = door.base_gs_origin;
                    ent.mins = door.base_gs_mins;
                    ent.maxs = door.base_gs_maxs;
                }
            }
        }
    }

    // 3. Reset one-shot triggers & item pickups, despawn transient round FX (decals, gibs, smoke)
    for mut once in &mut triggers_once {
        once.triggered = false;
    }
    for mut p in &mut pickups {
        p.collected = false;
    }
    for (e, decal) in &decals_q {
        crate::sprite::despawn_decal_entity(&mut commands, Some(&mut meshes), e, decal);
    }
    for e in &gibs_q {
        commands.entity(e).despawn_recursive();
    }
    for e in &smoke_q {
        commands.entity(e).despawn_recursive();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_door_base(is_rotating: bool) -> DoorEntity {
        DoorEntity {
            model: "*1".into(),
            targetname: String::new(),
            target: String::new(),
            spawnflags: 0,
            speed: 100.0,
            wait: 4.0,
            dmg: 0.0,
            wait_timer: 0.0,
            blocked_cooldown: 0.0,
            touch_cooldown: 0.0,
            state: DoorState::Opening,
            is_rotating,
            is_toggle: false,
            is_oneway: false,
            use_only: false,
            start_open: false,
            passable: false,
            rot_axis: Vec3::Y,
            hinge_origin: Vec3::ZERO,
            start_pos: Vec3::ZERO,
            target_pos: Vec3::new(0.0, 0.0, 64.0),
            start_rot: Quat::IDENTITY,
            target_rot: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            distance: 64.0,
            current_progress: 0.5,
            move_sound: None,
            stop_sound: None,
            bmin: Vec3::new(-4.0, 0.0, -32.0),
            bmax: Vec3::new(4.0, 96.0, 32.0),
            base_gs_mins: [-4.0, -32.0, 0.0],
            base_gs_maxs: [4.0, 32.0, 96.0],
            base_gs_origin: [0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn test_orient_rotating_door_away_quadrants() {
        let mut door = test_door_base(true);
        // Test with real de_inferno non-zero world origin and bounds:
        door.base_gs_origin = [302.0, -314.0, 172.0];
        door.base_gs_mins = [302.0, -316.0, 128.0];
        door.base_gs_maxs = [366.0, -312.0, 236.0];
        door.distance = 90.0;

        // Actor on South side of door blade (GS x = 330, GS y = -340 -> Bevy x = 330, z = 340)
        orient_rotating_door_away(&mut door, Vec3::new(330.0, 172.0, 340.0));
        let (axis, rot_angle) = door.target_rot.to_axis_angle();
        assert!(rot_angle > 0.1, "Door must open away from South actor");

        // Actor on North side of door blade (GS x = 330, GS y = -280 -> Bevy x = 330, z = 280)
        orient_rotating_door_away(&mut door, Vec3::new(330.0, 172.0, 280.0));
        let (axis_rev, rot_angle_rev) = door.target_rot.to_axis_angle();
        assert!(rot_angle_rev > 0.1, "Door must open away from North actor");
        // Rotations from front vs back must have opposite directions around Y
        assert!(
            (axis.y - (-axis_rev.y)).abs() < 1e-4,
            "Rotations from front vs back must be opposite"
        );
    }

    #[test]
    fn test_door_push_sliding_and_passable() {
        let mut door = test_door_base(false);
        // Sliding door traveling +Z pushes intersecting player hull along +Z
        let push = compute_door_push_on_player(&door, 0.25, Vec3::new(0.0, 36.0, 48.0), 36.0);
        let push_v = push.expect("Sliding door must push intersecting player");
        assert!(push_v.z > 0.0, "Expected +Z push, got {push_v:?}");

        door.passable = true;
        assert!(
            compute_door_push_on_player(&door, 0.25, Vec3::new(0.0, 36.0, 48.0), 36.0).is_none()
        );
    }

    #[test]
    fn test_door_push_rotating_leaf_ejects_player() {
        let door = test_door_base(true);
        // Progress = 0.5 -> blade rotated 45 deg around Y; point near local blade origin intersects
        let push = compute_door_push_on_player(&door, 0.25, Vec3::new(4.0, 36.0, 4.0), 36.0);
        let push_v = push.expect("Rotating door leaf must push overlapping player");
        assert!(push_v.length() > 0.5);
        assert_eq!(push_v.y, 0.0);
    }
}
