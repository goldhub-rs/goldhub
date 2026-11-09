//! Authoritative CS 1.6 CBasePlayerWeapon state machine and weapon definitions.

use bevy::prelude::*;
use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::entity::{BreakEntityEvent, BreakableEntity, FireTargetEvent, ray_intersects_aabb};
use crate::player::LevelCollisionResource;
use crate::studio::animation::StudioAnimatedModel;
use crate::studio::store::StudioModelStore;
use crate::studio::viewmodel::ViewModelWeapon;
use crate::studio::world_entity::{EntityCollisionBounds, HostageEntity, WorldStudioEntity};
use super::recoil::PlayerRecoil;

/// Canonical CS 1.6 Weapon Identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum WeaponId {
    #[default]
    Knife = 29,
    Glock18 = 17,
    USP45 = 16,
    Deagle = 26,
    AK47 = 28,
    M4A1 = 22,
    AWP = 18,
}

/// Static weapon balance and asset definition matching ReGameDLL_CS.
#[derive(Debug, Clone, Copy)]
pub struct WeaponDef {
    pub name: &'static str,
    pub damage: f32,
    pub cycle_time: f32,
    pub clip_size: i32,
    pub max_reserve: i32,
    pub reload_time: f32,
    pub v_model: &'static str,
    pub shoot_sound: &'static str,
    pub reload_sound: &'static str,
    pub deploy_anim: usize,
    pub shoot_anim: usize,
    pub reload_anim: usize,
    pub recoil_up: f32,
    pub recoil_lateral: f32,
    pub is_automatic: bool,
}

/// Returns the authoritative weapon definition matching ReGameDLL.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn get_weapon_def(id: WeaponId) -> WeaponDef {
    match id {
        WeaponId::Knife => WeaponDef {
            name: "Knife",
            damage: 25.0,
            cycle_time: 0.35,
            clip_size: 0,
            max_reserve: 0,
            reload_time: 0.0,
            v_model: "models/v_knife.mdl",
            shoot_sound: "weapons/knife_slash1.wav",
            reload_sound: "",
            deploy_anim: 3,
            shoot_anim: 6,
            reload_anim: 0,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            is_automatic: false,
        },
        WeaponId::Glock18 => WeaponDef {
            name: "Glock-18",
            damage: 20.0,
            cycle_time: 0.15,
            clip_size: 20,
            max_reserve: 120,
            reload_time: 2.2,
            v_model: "models/v_glock18.mdl",
            shoot_sound: "weapons/glock18-1.wav",
            reload_sound: "weapons/glock_clipout.wav",
            deploy_anim: 6,
            shoot_anim: 1,
            reload_anim: 5,
            recoil_up: 0.8,
            recoil_lateral: 0.3,
            is_automatic: false,
        },
        WeaponId::USP45 => WeaponDef {
            name: "USP-45",
            damage: 34.0,
            cycle_time: 0.16,
            clip_size: 12,
            max_reserve: 100,
            reload_time: 2.7,
            v_model: "models/v_usp.mdl",
            shoot_sound: "weapons/usp_unsil-1.wav",
            reload_sound: "weapons/usp_clipout.wav",
            deploy_anim: 6,
            shoot_anim: 1,
            reload_anim: 5,
            recoil_up: 0.9,
            recoil_lateral: 0.35,
            is_automatic: false,
        },
        WeaponId::Deagle => WeaponDef {
            name: "Desert Eagle",
            damage: 54.0,
            cycle_time: 0.22,
            clip_size: 7,
            max_reserve: 35,
            reload_time: 2.2,
            v_model: "models/v_deagle.mdl",
            shoot_sound: "weapons/deagle-1.wav",
            reload_sound: "weapons/de_clipout.wav",
            deploy_anim: 5,
            shoot_anim: 1,
            reload_anim: 4,
            recoil_up: 1.8,
            recoil_lateral: 0.6,
            is_automatic: false,
        },
        WeaponId::AK47 => WeaponDef {
            name: "AK-47",
            damage: 36.0,
            cycle_time: 0.0955,
            clip_size: 30,
            max_reserve: 90,
            reload_time: 2.45,
            v_model: "models/v_ak47.mdl",
            shoot_sound: "weapons/ak47-1.wav",
            reload_sound: "weapons/ak47_clipout.wav",
            deploy_anim: 2,
            shoot_anim: 3,
            reload_anim: 1,
            recoil_up: 1.0,
            recoil_lateral: 0.375,
            is_automatic: true,
        },
        WeaponId::M4A1 => WeaponDef {
            name: "M4A1",
            damage: 32.0,
            cycle_time: 0.0875,
            clip_size: 30,
            max_reserve: 90,
            reload_time: 3.05,
            v_model: "models/v_m4a1.mdl",
            shoot_sound: "weapons/m4a1_unsil-1.wav",
            reload_sound: "weapons/m4a1_clipout.wav",
            deploy_anim: 2,
            shoot_anim: 3,
            reload_anim: 1,
            recoil_up: 0.9,
            recoil_lateral: 0.35,
            is_automatic: true,
        },
        WeaponId::AWP => WeaponDef {
            name: "AWP",
            damage: 115.0,
            cycle_time: 1.45,
            clip_size: 10,
            max_reserve: 30,
            reload_time: 2.5,
            v_model: "models/v_awp.mdl",
            shoot_sound: "weapons/awp1.wav",
            reload_sound: "weapons/boltpull1.wav",
            deploy_anim: 3,
            shoot_anim: 1,
            reload_anim: 2,
            recoil_up: 3.5,
            recoil_lateral: 0.8,
            is_automatic: false,
        },
    }
}

/// Active weapon state component managing attack delays, clip, and reload.
#[derive(Component, Debug, Clone)]
pub struct ActiveWeaponState {
    pub id: WeaponId,
    pub clip: i32,
    pub reserve: i32,
    pub next_primary_attack: f32,
    pub next_secondary_attack: f32,
    pub in_reload: bool,
    pub reload_timer: f32,
    pub shots_fired: u32,
    pub silenced: bool,
    pub zoom_level: u8,
}

impl Default for ActiveWeaponState {
    fn default() -> Self {
        let def = get_weapon_def(WeaponId::Knife);
        Self {
            id: WeaponId::Knife,
            clip: def.clip_size,
            reserve: def.max_reserve,
            next_primary_attack: 0.0,
            next_secondary_attack: 0.0,
            in_reload: false,
            reload_timer: 0.0,
            shots_fired: 0,
            silenced: false,
            zoom_level: 0,
        }
    }
}

/// Player weapon slots inventory.
#[derive(Component, Debug, Clone)]
pub struct PlayerWeaponInventory {
    pub primary: Option<WeaponId>,
    pub secondary: Option<WeaponId>,
    pub knife: WeaponId,
    pub active_slot: u8,
}

impl Default for PlayerWeaponInventory {
    fn default() -> Self {
        Self {
            primary: Some(WeaponId::AK47),
            secondary: Some(WeaponId::USP45),
            knife: WeaponId::Knife,
            active_slot: 3,
        }
    }
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                weapon_switch_system,
                weapon_fire_and_reload_system,
                super::recoil::decay_punchangle_system,
            ),
        );
    }
}

/// Switches active weapon on Digit1 (Primary), Digit2 (Secondary), Digit3 (Knife).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn weapon_switch_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut model_store: ResMut<StudioModelStore>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut player_query: Query<(&mut PlayerWeaponInventory, &mut ActiveWeaponState)>,
    mut vm_query: Query<(
        Entity,
        &mut ViewModelWeapon,
        &mut StudioAnimatedModel,
        &mut Transform,
    )>,
) {
    let mut new_slot = None;
    if keyboard.just_pressed(KeyCode::Digit1) {
        new_slot = Some(1);
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        new_slot = Some(2);
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        new_slot = Some(3);
    }

    let Some(slot) = new_slot else {
        return;
    };

    for (mut inv, mut state) in &mut player_query {
        if inv.active_slot == slot {
            continue;
        }

        let target_weapon = match slot {
            1 => inv.primary,
            2 => inv.secondary,
            _ => Some(inv.knife),
        };

        let Some(weapon_id) = target_weapon else {
            continue;
        };

        inv.active_slot = slot;
        let def = get_weapon_def(weapon_id);
        state.id = weapon_id;
        state.clip = def.clip_size;
        state.reserve = def.max_reserve;
        state.next_primary_attack = 0.5; // Deploy delay
        state.in_reload = false;
        state.shots_fired = 0;
        state.zoom_level = 0;

        // Deploy sound
        if let Some(h) = audio_bank.get_or_load_sound("weapons/knife_deploy1.wav", &search_paths.dirs, &mut audio_sources) {
            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
        }

        // Swap viewmodel weapon mesh
        for (vm_entity, _, mut anim, _) in &mut vm_query {
            // Despawn child submeshes
            for (child_ent, _, _) in anim.mesh_entities.drain(..) {
                commands.entity(child_ent).despawn();
            }

            if let Some(model_data) = model_store.get_or_load(def.v_model, &search_paths, &mut images, &mut materials) {
                let mut new_children = Vec::with_capacity(model_data.submeshes.len());
                for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
                    let bevy_mesh = crate::studio::store::create_initial_bevy_mesh(template, false);
                    let mesh_handle = meshes.add(bevy_mesh);
                    let child = commands
                        .spawn((
                            Mesh3d(mesh_handle.clone()),
                            MeshMaterial3d(template.material.clone()),
                            Transform::IDENTITY,
                            bevy::render::view::NoFrustumCulling,
                        ))
                        .id();
                    commands.entity(vm_entity).add_child(child);
                    new_children.push((child, mesh_handle, sub_idx));
                }
                anim.mesh_entities = new_children;
                let deploy_fps = model_data.model.rig.sequences.get(def.deploy_anim).map_or(30.0, |s| s.fps);
                anim.set_sequence(def.deploy_anim, deploy_fps, false);
            }
        }

        tracing::info!(slot = slot, weapon = def.name, "Switched active weapon");
    }
}

/// SystemParam bundling audio resources for weapon sounds.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatAudio<'w> {
    pub search_paths: Res<'w, GameSearchPaths>,
    pub audio_bank: ResMut<'w, GoldSrcAudioBank>,
    pub audio_sources: ResMut<'w, Assets<AudioSource>>,
}

/// SystemParam bundling combat event writers.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatEvents<'w> {
    pub fire: EventWriter<'w, FireTargetEvent>,
    pub break_ent: EventWriter<'w, BreakEntityEvent>,
}

/// Core weapon fire, reload, kickback recoil, and hitscan raytrace damage dispatch.
const BULLET_REACH: f32 = 8192.0;

#[allow(clippy::too_many_arguments, clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn weapon_fire_and_reload_system(
    mut commands: Commands,
    time: Res<Time>,
    mut audio: CombatAudio,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    collision_res: Res<LevelCollisionResource>,
    model_store: Res<StudioModelStore>,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<crate::player::PlayerController>)>,
    mut player_query: Query<(&mut ActiveWeaponState, &mut PlayerRecoil)>,
    mut breakable_query: Query<(Entity, &mut BreakableEntity)>,
    mut hostage_query: Query<(
        Entity,
        &mut HostageEntity,
        &mut StudioAnimatedModel,
        &mut EntityCollisionBounds,
        &GlobalTransform,
    ), (Without<ViewModelWeapon>, Without<crate::player::PlayerController>)>,
    entity_query: Query<(Entity, &GlobalTransform, &EntityCollisionBounds, &WorldStudioEntity), Without<HostageEntity>>,
    mut events: CombatEvents,
    mut vm_query: Query<(&mut ViewModelWeapon, &mut StudioAnimatedModel)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok((mut state, mut recoil)) = player_query.get_single_mut() else {
        return;
    };

    if state.next_primary_attack > 0.0 {
        state.next_primary_attack = (state.next_primary_attack - dt).max(0.0);
    }
    if state.next_secondary_attack > 0.0 {
        state.next_secondary_attack = (state.next_secondary_attack - dt).max(0.0);
    }

    if state.id == WeaponId::Knife {
        return;
    }

    let def = get_weapon_def(state.id);

    // 1. Reload Lifecycle
    if state.in_reload {
        state.reload_timer -= dt;
        if state.reload_timer <= 0.0 {
            let needed = def.clip_size - state.clip;
            let transfer = needed.min(state.reserve);
            state.clip += transfer;
            state.reserve -= transfer;
            state.in_reload = false;
            tracing::info!(clip = state.clip, reserve = state.reserve, "Weapon reload finished");
        }
        return;
    }

    // Manual reload on R key
    if keyboard.just_pressed(KeyCode::KeyR) && state.clip < def.clip_size && state.reserve > 0 {
        state.in_reload = true;
        state.reload_timer = def.reload_time;
        state.shots_fired = 0;

        if !def.reload_sound.is_empty()
            && let Some(h) = audio.audio_bank.get_or_load_sound(def.reload_sound, &audio.search_paths.dirs, &mut audio.audio_sources)
        {
            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
        }

        for (_, mut anim) in &mut vm_query {
            if let Some(model_data) = model_store.models.get(def.v_model) {
                let fps = model_data.model.rig.sequences.get(def.reload_anim).map_or(30.0, |s| s.fps);
                anim.set_sequence(def.reload_anim, fps, false);
            }
        }
        return;
    }

    // 2. Primary Attack (LMB)
    let fire_pressed = if def.is_automatic {
        mouse_button.pressed(MouseButton::Left)
    } else {
        mouse_button.just_pressed(MouseButton::Left)
    };

    if fire_pressed && state.next_primary_attack <= 0.0 {
        if state.clip <= 0 {
            // Dry fire click
            if let Some(h) = audio.audio_bank.get_or_load_sound("weapons/dryfire_pistol.wav", &audio.search_paths.dirs, &mut audio.audio_sources) {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
            state.next_primary_attack = 0.2;
            return;
        }

        state.clip -= 1;
        state.next_primary_attack = def.cycle_time;
        state.shots_fired += 1;

        // Apply kickback recoil
        let seed = (time.elapsed_secs() * 100.0).fract();
        recoil.apply_kickback(def.recoil_up, def.recoil_lateral, 0.15, 0.03, state.shots_fired, seed);

        // Firing sound
        if let Some(h) = audio.audio_bank.get_or_load_sound(def.shoot_sound, &audio.search_paths.dirs, &mut audio.audio_sources) {
            commands.spawn((
                AudioPlayer::new(h),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::new(1.0)),
            ));
        }

        // Viewmodel shoot animation
        for (_, mut anim) in &mut vm_query {
            if let Some(model_data) = model_store.models.get(def.v_model) {
                let fps = model_data.model.rig.sequences.get(def.shoot_anim).map_or(30.0, |s| s.fps);
                anim.set_sequence(def.shoot_anim, fps, false);
            }
        }

        // Raytrace bullet hitscan forward (8192 units max range)
        let Ok(cam_t) = camera_query.get_single() else {
            return;
        };
        let cam_pos = cam_t.translation();
        let cam_forward = cam_t.forward().as_vec3();

        // Trace breakables
        let mut hit_breakable: Option<(Entity, f32, crate::entity::BreakableMaterial, u32)> = None;
        for (b_ent, b) in &breakable_query {
            if b.is_broken {
                continue;
            }
            if let Some(t) = ray_intersects_aabb(cam_pos, cam_forward, BULLET_REACH, b.bmin, b.bmax)
                && hit_breakable.as_ref().is_none_or(|(_, d, _, _)| t < *d)
            {
                hit_breakable = Some((b_ent, t, b.material, b.explodemagnitude));
            }
        }

        // Trace visual studio props (chickens, bottles) to companion breakables
        for (_ent, ent_t, bounds, studio_ent) in &entity_query {
            let ep = ent_t.translation();
            let bmin = ep + bounds.mins;
            let bmax = ep + bounds.maxs;
            if let Some(t) = ray_intersects_aabb(cam_pos, cam_forward, BULLET_REACH, bmin, bmax)
                && !studio_ent.targetname.is_empty()
            {
                for (b_ent, b) in &breakable_query {
                    if !b.is_broken
                        && ((b.bmin + b.bmax) * 0.5).distance(ep) < 64.0
                        && hit_breakable.as_ref().is_none_or(|(_, d, _, _)| t < *d)
                    {
                        hit_breakable = Some((b_ent, t, b.material, b.explodemagnitude));
                        break;
                    }
                }
            }
        }

        // Trace hostages
        let mut hit_hostage: Option<(Entity, f32)> = None;
        for (h_ent, _, _, bounds, ht) in &hostage_query {
            let ep = ht.translation();
            let bmin = ep + bounds.mins;
            let bmax = ep + bounds.maxs;
            if let Some(t) = ray_intersects_aabb(cam_pos, cam_forward, BULLET_REACH, bmin, bmax)
                && hit_hostage.as_ref().is_none_or(|(_, d)| t < *d)
            {
                hit_hostage = Some((h_ent, t));
            }
        }

        // Trace world BSP
        let mut hit_wall_dist = BULLET_REACH;
        if let Some(ref col_model) = collision_res.model {
            use goldhub_physics::WorldTrace;
            let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
            let gs_start = [cam_pos.x, -cam_pos.z, cam_pos.y];
            let ray_end = cam_pos + cam_forward * BULLET_REACH;
            let gs_end = [ray_end.x, -ray_end.z, ray_end.y];
            let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
            if tr.fraction < 1.0 {
                hit_wall_dist = tr.fraction * BULLET_REACH;
            }
        }

        // Resolve hit
        if let Some((b_ent, b_t, mat, exp_mag)) = hit_breakable
            && b_t < hit_wall_dist
            && hit_hostage.as_ref().is_none_or(|(_, h_t)| b_t < *h_t)
        {
            if let Ok((_, mut breakable)) = breakable_query.get_mut(b_ent) {
                breakable.health -= def.damage;
                if breakable.health <= 0.0 && !breakable.is_broken {
                    breakable.is_broken = true;
                    let center = (breakable.bmin + breakable.bmax) * 0.5;
                    events.break_ent.send(BreakEntityEvent {
                        origin: center,
                        material: mat,
                        model: breakable.model.clone(),
                        explodemagnitude: exp_mag,
                    });
                    events.fire.send(FireTargetEvent {
                        target: breakable.target.clone(),
                    });
                }
            }
        } else if let Some((h_ent, h_t)) = hit_hostage
            && h_t < hit_wall_dist
        {
            if let Ok((_, mut hostage, mut h_anim, mut bounds, ht)) = hostage_query.get_mut(h_ent) {
                hostage.health -= def.damage;
                if hostage.health > 0.0 {
                    h_anim.set_sequence(7, 30.0, false);
                    let pain_idx = ((time.elapsed_secs() * 100.0) as usize % 6) + 1;
                    let snd = format!("hostage/hpain/hpain{pain_idx}.wav");
                    if let Some(h) = audio.audio_bank.get_or_load_sound(&snd, &audio.search_paths.dirs, &mut audio.audio_sources) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation(ht.translation()),
                        ));
                    }
                } else if !hostage.is_dead {
                    hostage.is_dead = true;
                    h_anim.set_sequence(31, 24.0, false);
                    bounds.mins = Vec3::ZERO;
                    bounds.maxs = Vec3::ZERO;
                    if let Some(h) = audio.audio_bank.get_or_load_sound("radio/hosdown.wav", &audio.search_paths.dirs, &mut audio.audio_sources) {
                        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                    }
                }
            }
        } else if hit_wall_dist < BULLET_REACH {
            // Wall ricochet sound
            let ric_idx = ((time.elapsed_secs() * 100.0) as usize % 5) + 1;
            let ric_snd = format!("weapons/ric{ric_idx}.wav");
            if let Some(h) = audio.audio_bank.get_or_load_sound(&ric_snd, &audio.search_paths.dirs, &mut audio.audio_sources) {
                let hit_pos = cam_pos + cam_forward * hit_wall_dist;
                commands.spawn((
                    AudioPlayer::new(h),
                    PlaybackSettings::DESPAWN.with_spatial(true),
                    Transform::from_translation(hit_pos),
                ));
            }
        }
    }
}
