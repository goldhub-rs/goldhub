//! Authoritative CS 1.6 CBasePlayerWeapon state machine and weapon definitions.

use super::recoil::PlayerRecoil;
use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::entity::{BreakEntityEvent, BreakableEntity, FireTargetEvent, ray_intersects_aabb};
use crate::player::{LevelCollisionResource, PlayerController};
use crate::sprite::{
    SpriteStore, spawn_blood_sprite, spawn_bullet_hole_decal, spawn_explosion_sprite,
    spawn_muzzleflash_sprite,
};
use crate::studio::animation::StudioAnimatedModel;
use crate::studio::store::StudioModelStore;
use crate::studio::viewmodel::ViewModelWeapon;
use crate::studio::world_entity::{EntityCollisionBounds, HostageEntity, WorldStudioEntity};
use bevy::prelude::*;

pub use goldhub_game_cstrike::{WeaponDef, WeaponId, WeaponSlot, get_weapon_def};

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

/// Persistent weapon state for an inventory slot.
#[derive(Debug, Clone, Copy)]
pub struct WeaponSlotData {
    pub id: Option<WeaponId>,
    pub clip: i32,
    pub reserve: i32,
    pub silenced: bool,
    pub next_primary_attack: f32,
}

/// Player weapon slots inventory.
#[derive(Component, Debug, Clone)]
pub struct PlayerWeaponInventory {
    pub slot1: WeaponSlotData, // Primary
    pub slot2: WeaponSlotData, // Secondary
    pub slot3: WeaponSlotData, // Knife
    pub active_slot: u8,
}

impl Default for PlayerWeaponInventory {
    fn default() -> Self {
        let ak = get_weapon_def(WeaponId::AK47);
        let usp = get_weapon_def(WeaponId::USP45);
        let knife = get_weapon_def(WeaponId::Knife);
        Self {
            slot1: WeaponSlotData {
                id: Some(WeaponId::AK47),
                clip: ak.clip_size,
                reserve: ak.max_reserve,
                silenced: false,
                next_primary_attack: 0.0,
            },
            slot2: WeaponSlotData {
                id: Some(WeaponId::USP45),
                clip: usp.clip_size,
                reserve: usp.max_reserve,
                silenced: false,
                next_primary_attack: 0.0,
            },
            slot3: WeaponSlotData {
                id: Some(WeaponId::Knife),
                clip: knife.clip_size,
                reserve: knife.max_reserve,
                silenced: false,
                next_primary_attack: 0.0,
            },
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
                update_bullet_particles_system,
                super::recoil::decay_punchangle_system,
            ),
        );
    }
}

/// Switches active weapon on Digit1 (Primary), Digit2 (Secondary), Digit3 (Knife).
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
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

        // Save active weapon state into outgoing slot
        let old_slot = inv.active_slot;
        match old_slot {
            1 => {
                inv.slot1.clip = state.clip;
                inv.slot1.reserve = state.reserve;
                inv.slot1.silenced = state.silenced;
                inv.slot1.next_primary_attack = state.next_primary_attack;
            }
            2 => {
                inv.slot2.clip = state.clip;
                inv.slot2.reserve = state.reserve;
                inv.slot2.silenced = state.silenced;
                inv.slot2.next_primary_attack = state.next_primary_attack;
            }
            _ => {
                inv.slot3.clip = state.clip;
                inv.slot3.reserve = state.reserve;
                inv.slot3.silenced = state.silenced;
                inv.slot3.next_primary_attack = state.next_primary_attack;
            }
        }

        inv.active_slot = slot;
        let incoming_slot = match slot {
            1 => inv.slot1,
            2 => inv.slot2,
            _ => inv.slot3,
        };

        let Some(weapon_id) = incoming_slot.id else {
            continue;
        };

        let def = get_weapon_def(weapon_id);
        state.id = weapon_id;
        state.clip = incoming_slot.clip;
        state.reserve = incoming_slot.reserve;
        state.silenced = incoming_slot.silenced;
        // Deploy delay of 0.5s or preserve remaining lockout (e.g. 3.0s silencer attach)
        state.next_primary_attack = incoming_slot.next_primary_attack.max(0.5);
        state.next_secondary_attack = incoming_slot.next_primary_attack.max(0.5);
        state.in_reload = false;
        state.shots_fired = 0;
        state.zoom_level = 0;

        // Deploy sound (only knife or weapons with dedicated deploy sound)
        let deploy_snd = match weapon_id {
            WeaponId::Knife => "weapons/knife_deploy1.wav",
            _ => "",
        };
        if !deploy_snd.is_empty()
            && let Some(h) =
                audio_bank.get_or_load_sound(deploy_snd, &search_paths.dirs, &mut audio_sources)
        {
            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
        }

        // Swap viewmodel weapon mesh
        for (vm_entity, _, mut anim, _) in &mut vm_query {
            // Despawn child submeshes
            for (child_ent, _, _) in anim.mesh_entities.drain(..) {
                commands.entity(child_ent).despawn();
            }

            if let Some(model_data) =
                model_store.get_or_load(def.v_model, &search_paths, &mut images, &mut materials)
            {
                anim.model_id = def.v_model.to_string();
                let deploy_anim_idx = match weapon_id {
                    WeaponId::USP45 => {
                        if state.silenced {
                            6
                        } else {
                            14
                        }
                    }
                    WeaponId::M4A1 => {
                        if state.silenced {
                            5
                        } else {
                            12
                        }
                    }
                    _ => def.deploy_anim,
                };
                let deploy_fps = model_data
                    .model
                    .rig
                    .sequences
                    .get(deploy_anim_idx)
                    .map_or(30.0, |s| s.fps);
                anim.set_sequence(deploy_anim_idx, deploy_fps, false);

                // Precompute bone transforms for frame 0 so mesh never renders in T-pose for 1 frame
                let bone_matrices_f0 = model_data
                    .model
                    .rig
                    .sequences
                    .get(deploy_anim_idx)
                    .and_then(|s| s.bone_matrices.first());

                let mut new_children = Vec::with_capacity(model_data.submeshes.len());
                for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
                    let mut bevy_mesh =
                        crate::studio::store::create_initial_bevy_mesh(template, true);
                    if let Some(m0_slice) = bone_matrices_f0
                        && let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(pos_vec)) =
                            bevy_mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
                    {
                        for (v, out_p) in template.rest_vertices.iter().zip(pos_vec.iter_mut()) {
                            let bone_idx = v.bone_index as usize;
                            if bone_idx < m0_slice.len() {
                                let bm = Mat4::from_cols_array(&m0_slice[bone_idx]);
                                let p = bm.transform_point3(Vec3::from(v.position));
                                *out_p = [-p.y, p.z, -p.x];
                            }
                        }
                    }
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
            }
        }

        tracing::info!(slot = slot, weapon = def.name, "Switched active weapon");
    }
}

#[derive(Component, Debug)]
pub struct WeaponAudioChannel;

#[derive(Component, Debug)]
pub struct BulletParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub gravity: f32,
}

pub fn update_bullet_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut BulletParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, mut p, mut transform) in &mut query {
        p.lifetime += dt;
        if p.lifetime >= p.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        p.velocity.y -= p.gravity * dt;
        transform.translation += p.velocity * dt;
        let frac = (1.0 - (p.lifetime / p.max_lifetime)).max(0.01);
        transform.scale = Vec3::splat(frac);
    }
}

pub fn spawn_blood_splatter(
    commands: &mut Commands,
    origin: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh_handle = meshes.add(Sphere::new(1.8).mesh().ico(1).unwrap());
    let mat_handle = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.02, 0.02),
        unlit: true,
        ..default()
    });

    for i in 0..12 {
        let angle = (i as f32) * std::f32::consts::TAU / 12.0;
        let speed = 40.0 + ((i * 17) % 70) as f32;
        let vel = Vec3::new(
            angle.cos() * speed,
            20.0 + ((i * 13) % 40) as f32,
            angle.sin() * speed,
        );
        commands.spawn((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(mat_handle.clone()),
            Transform::from_translation(origin),
            BulletParticle {
                velocity: vel,
                lifetime: 0.0,
                max_lifetime: 0.35,
                gravity: 500.0,
            },
        ));
    }
}

pub fn spawn_bullet_dust_puff(
    commands: &mut Commands,
    origin: Vec3,
    normal: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh_handle = meshes.add(Sphere::new(1.4).mesh().ico(1).unwrap());
    let mat_handle = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.68, 0.64),
        unlit: true,
        ..default()
    });

    for i in 0..8 {
        let angle = (i as f32) * std::f32::consts::TAU / 8.0;
        let speed = 30.0 + ((i * 19) % 50) as f32;
        let spread_dir =
            (normal + Vec3::new(angle.cos() * 0.5, angle.sin() * 0.5, 0.0)).normalize();
        let vel = spread_dir * speed;
        commands.spawn((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(mat_handle.clone()),
            Transform::from_translation(origin),
            BulletParticle {
                velocity: vel,
                lifetime: 0.0,
                max_lifetime: 0.25,
                gravity: 60.0,
            },
        ));
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

/// SystemParam bundling mesh and material assets for combat VFX.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatVfxAssets<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
}

/// SystemParam bundling combat scene queries.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatQueries<'w, 's> {
    pub audio_channel: Query<'w, 's, Entity, With<WeaponAudioChannel>>,
    pub camera: Query<
        'w,
        's,
        &'static GlobalTransform,
        (With<Camera3d>, With<crate::player::PlayerController>),
    >,
    pub player: Query<
        'w,
        's,
        (
            &'static mut ActiveWeaponState,
            &'static mut PlayerRecoil,
            Option<&'static PlayerController>,
        ),
    >,
    pub breakable: Query<'w, 's, (Entity, &'static mut BreakableEntity)>,
    pub hostage: Query<
        'w,
        's,
        (
            Entity,
            &'static mut HostageEntity,
            &'static mut StudioAnimatedModel,
            &'static mut EntityCollisionBounds,
            &'static mut WorldStudioEntity,
            &'static GlobalTransform,
        ),
        (
            Without<ViewModelWeapon>,
            Without<crate::player::PlayerController>,
        ),
    >,
    pub props: Query<
        'w,
        's,
        (
            Entity,
            &'static GlobalTransform,
            &'static EntityCollisionBounds,
            &'static WorldStudioEntity,
        ),
        Without<HostageEntity>,
    >,
    pub vm: Query<
        'w,
        's,
        (
            &'static mut ViewModelWeapon,
            &'static mut StudioAnimatedModel,
        ),
    >,
}

/// Core weapon fire, reload, kickback recoil, and hitscan raytrace damage dispatch.
const BULLET_REACH: f32 = 8192.0;

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub fn weapon_fire_and_reload_system(
    mut commands: Commands,
    time: Res<Time>,
    mut audio: CombatAudio,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    collision_res: Res<LevelCollisionResource>,
    model_store: Res<StudioModelStore>,
    mut sprite_store: ResMut<SpriteStore>,
    mut vfx: CombatVfxAssets,
    mut queries: CombatQueries,
    mut events: CombatEvents,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok((mut state, mut recoil, player_ctrl)) = queries.player.get_single_mut() else {
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

    // 0. Secondary Attack (Right Click / ПКМ) - Silencer Toggle for USP and M4A1
    if mouse_button.just_pressed(MouseButton::Right)
        && state.next_secondary_attack <= 0.0
        && !state.in_reload
    {
        match state.id {
            WeaponId::USP45 => {
                state.silenced = !state.silenced;
                let (seq, fps) = if state.silenced {
                    (7, 37.0)
                } else {
                    (15, 37.0)
                };
                state.next_primary_attack = 3.0;
                state.next_secondary_attack = 3.0;
                for (_, mut anim) in &mut queries.vm {
                    anim.set_sequence(seq, fps, false);
                }
                return;
            }
            WeaponId::M4A1 => {
                state.silenced = !state.silenced;
                let (seq, fps) = if state.silenced {
                    (6, 30.0)
                } else {
                    (13, 30.0)
                };
                state.next_primary_attack = 3.0;
                state.next_secondary_attack = 3.0;
                for (_, mut anim) in &mut queries.vm {
                    anim.set_sequence(seq, fps, false);
                }
                return;
            }
            _ => {}
        }
    }

    // 1. Reload Lifecycle
    if state.in_reload {
        state.reload_timer -= dt;
        if state.reload_timer <= 0.0 {
            let needed = def.clip_size - state.clip;
            let transfer = needed.min(state.reserve);
            state.clip += transfer;
            state.reserve -= transfer;
            state.in_reload = false;
            tracing::info!(
                clip = state.clip,
                reserve = state.reserve,
                "Weapon reload finished"
            );
        }
        return;
    }

    // Manual reload on R key OR auto-reload when clip is empty and weapon can fire
    let wants_manual_reload =
        keyboard.just_pressed(KeyCode::KeyR) && state.clip < def.clip_size && state.reserve > 0;
    let wants_auto_reload =
        state.clip == 0 && state.reserve > 0 && state.next_primary_attack <= 0.0;
    if (wants_manual_reload || wants_auto_reload) && !state.in_reload {
        state.in_reload = true;
        state.reload_timer = def.reload_time;
        state.shots_fired = 0;

        if !def.reload_sound.is_empty()
            && let Some(h) = audio.audio_bank.get_or_load_sound(
                def.reload_sound,
                &audio.search_paths.dirs,
                &mut audio.audio_sources,
            )
        {
            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
        }

        let reload_anim_idx = match state.id {
            WeaponId::USP45 => {
                if state.silenced {
                    5
                } else {
                    13
                }
            }
            WeaponId::M4A1 => {
                if state.silenced {
                    4
                } else {
                    11
                }
            }
            _ => def.reload_anim,
        };

        for (_, mut anim) in &mut queries.vm {
            if let Some(model_data) = model_store.models.get(def.v_model) {
                let fps = model_data
                    .model
                    .rig
                    .sequences
                    .get(reload_anim_idx)
                    .map_or(30.0, |s| s.fps);
                anim.set_sequence(reload_anim_idx, fps, false);
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
            if let Some(h) = audio.audio_bank.get_or_load_sound(
                "weapons/dryfire_pistol.wav",
                &audio.search_paths.dirs,
                &mut audio.audio_sources,
            ) {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
            state.next_primary_attack = 0.2;
            return;
        }

        state.clip -= 1;
        state.next_primary_attack = def.cycle_time;
        state.shots_fired += 1;

        // Apply authentic kickback recoil with angular boundary clamping matching ReGameDLL
        let seed = (time.elapsed_secs() * 100.0).fract();
        recoil.apply_kickback(
            def.recoil_up,
            def.recoil_lateral,
            0.15,
            0.03,
            def.up_max,
            def.lateral_max,
            state.shots_fired,
            seed,
        );

        // Despawn previous shot sound on WeaponAudioChannel to prevent audio stacking / cacophony
        for old_audio in &queries.audio_channel {
            commands.entity(old_audio).despawn();
        }

        // Firing sound
        let shoot_snd = match state.id {
            WeaponId::USP45 => {
                if state.silenced {
                    "weapons/usp1.wav"
                } else {
                    "weapons/usp_unsil-1.wav"
                }
            }
            WeaponId::M4A1 => {
                if state.silenced {
                    "weapons/m4a1-1.wav"
                } else {
                    "weapons/m4a1_unsil-1.wav"
                }
            }
            _ => def.shoot_sound,
        };
        if let Some(h) = audio.audio_bank.get_or_load_sound(
            shoot_snd,
            &audio.search_paths.dirs,
            &mut audio.audio_sources,
        ) {
            commands.spawn((
                WeaponAudioChannel,
                AudioPlayer::new(h),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::new(1.0)),
            ));
        }

        // Viewmodel shoot animation
        let shoot_anim_idx = match state.id {
            WeaponId::USP45 => {
                if state.silenced {
                    1
                } else {
                    9
                }
            }
            WeaponId::M4A1 => {
                if state.silenced {
                    1
                } else {
                    8
                }
            }
            _ => def.shoot_anim,
        };
        for (_, mut anim) in &mut queries.vm {
            if let Some(model_data) = model_store.models.get(def.v_model) {
                let fps = model_data
                    .model
                    .rig
                    .sequences
                    .get(shoot_anim_idx)
                    .map_or(30.0, |s| s.fps);
                anim.set_sequence(shoot_anim_idx, fps, false);
            }
        }

        let Ok(cam_t) = queries.camera.get_single() else {
            return;
        };
        let cam_pos = cam_t.translation();
        let cam_forward = cam_t.forward().as_vec3();
        let cam_right = cam_t.right().as_vec3();
        let cam_up = cam_t.up().as_vec3();

        // Spawn authentic muzzle flash sprite at weapon barrel
        spawn_muzzleflash_sprite(
            &mut commands,
            cam_pos,
            cam_forward,
            cam_right,
            cam_up,
            true, // cl_righthand 1 (right-handed viewmodel)
            &mut sprite_store,
            &audio.search_paths,
            &mut vfx.images,
            &mut vfx.materials,
            &mut vfx.meshes,
        );

        // Brief dynamic muzzle flash light (0.05s)
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.85, 0.6),
                intensity: 6_000.0,
                range: 240.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(cam_pos + cam_forward * 24.0),
            crate::entity::ExplosionFlash {
                lifetime: 0.0,
                max_lifetime: 0.05,
                initial_intensity: 6_000.0,
            },
        ));

        // Authentic CS 1.6 bullet spread calculation based on movement speed, ground state, and shots fired
        let player_speed = player_ctrl.map_or(0.0, |p| {
            let v = p.sim.current.velocity;
            v[0].hypot(v[1])
        });
        let on_ground = player_ctrl.is_none_or(|p| p.sim.current.on_ground);
        let shots = state.shots_fired as f32;

        let spread = match state.id {
            WeaponId::AK47 => {
                let accuracy = ((shots * shots * shots) / 200.0 + 0.35).min(1.25);
                if !on_ground {
                    0.04 + 0.4 * accuracy
                } else if player_speed > 140.0 {
                    0.04 + 0.07 * accuracy
                } else {
                    0.0275 * accuracy
                }
            }
            WeaponId::USP45 => {
                let accuracy = (0.92 - (shots * 0.08)).clamp(0.6, 0.92);
                if state.silenced {
                    if !on_ground {
                        1.3 * (1.0 - accuracy)
                    } else if player_speed > 0.0 {
                        0.25 * (1.0 - accuracy)
                    } else {
                        0.15 * (1.0 - accuracy)
                    }
                } else {
                    if !on_ground {
                        1.2 * (1.0 - accuracy)
                    } else if player_speed > 0.0 {
                        0.225 * (1.0 - accuracy)
                    } else {
                        0.1 * (1.0 - accuracy)
                    }
                }
            }
            WeaponId::M4A1 => {
                let accuracy = ((shots * shots * shots) / 220.0 + 0.3).min(1.25);
                if state.silenced {
                    if !on_ground {
                        0.035 + 0.4 * accuracy
                    } else if player_speed > 140.0 {
                        0.035 + 0.07 * accuracy
                    } else {
                        0.025 * accuracy
                    }
                } else {
                    if !on_ground {
                        0.035 + 0.4 * accuracy
                    } else if player_speed > 140.0 {
                        0.035 + 0.07 * accuracy
                    } else {
                        0.0275 * accuracy
                    }
                }
            }
            WeaponId::Deagle => {
                if !on_ground {
                    1.5 * (1.0 - (0.9 - shots * 0.1).clamp(0.5, 0.9))
                } else if player_speed > 0.0 {
                    0.25 * (1.0 - (0.9 - shots * 0.1).clamp(0.5, 0.9))
                } else {
                    0.1 * (1.0 - (0.9 - shots * 0.1).clamp(0.5, 0.9))
                }
            }
            _ => 0.02,
        };

        // Circular Gaussian bullet spread and authentic punchangle trajectory
        let seed_x = (time.elapsed_secs() * 37.17).fract();
        let seed_y = (time.elapsed_secs() * 59.83).fract();
        let rx = (seed_x - 0.5) * 2.0;
        let ry = (seed_y - 0.5) * 2.0;
        let (punch_pitch, punch_yaw) = (
            recoil.punch_pitch.to_radians(),
            recoil.punch_yaw.to_radians(),
        );
        let shot_pitch = player_ctrl.map_or(0.0, |p| p.pitch) + punch_pitch;
        let shot_yaw = player_ctrl.map_or(0.0, |p| p.yaw) + punch_yaw;
        let shot_rot = Quat::from_rotation_y(shot_yaw) * Quat::from_rotation_x(shot_pitch);
        let shot_forward = shot_rot * (-Vec3::Z);
        let shot_right = shot_rot * Vec3::X;
        let shot_up = shot_rot * Vec3::Y;
        let bullet_dir =
            (shot_forward + shot_right * (rx * spread) + shot_up * (ry * spread)).normalize();

        // Authentic CS 1.6 ReGameDLL FireBullets3 penetration & wallbang loop
        let mut cur_src = cam_pos;
        let mut remaining_reach = BULLET_REACH;
        let mut cur_damage = def.damage;
        let mut penetrations_left = def.penetration;

        while penetrations_left > 0 || cur_src == cam_pos {
            // Trace world BSP
            let mut hit_wall_dist = remaining_reach;
            let mut hit_wall_normal = -bullet_dir;
            let mut hit_brush_entity = None;
            if let Some(ref col_model) = collision_res.model {
                use goldhub_physics::WorldTrace;
                let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
                let gs_start = [cur_src.x, -cur_src.z, cur_src.y];
                let ray_end = cur_src + bullet_dir * remaining_reach;
                let gs_end = [ray_end.x, -ray_end.z, ray_end.y];
                let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
                if tr.fraction < 1.0 {
                    hit_wall_dist = tr.fraction * remaining_reach;
                    hit_wall_normal =
                        Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                            .normalize_or_zero();
                    hit_brush_entity = tr.hit_entity;
                }
            }

            // Trace breakables (checking both direct brush model index and AABB intersection)
            let mut hit_breakable: Option<(Entity, f32, crate::entity::BreakableMaterial, u32)> =
                None;

            if let Some(model_idx) = hit_brush_entity {
                let model_str = format!("*{model_idx}");
                for (b_ent, b) in &queries.breakable {
                    if !b.is_broken && b.model == model_str {
                        hit_breakable =
                            Some((b_ent, hit_wall_dist, b.material, b.explodemagnitude));
                        break;
                    }
                }
            }

            if hit_breakable.is_none() {
                for (b_ent, b) in &queries.breakable {
                    if b.is_broken {
                        continue;
                    }
                    if let Some(t) = ray_intersects_aabb(
                        cur_src,
                        bullet_dir,
                        hit_wall_dist + 4.0,
                        b.bmin,
                        b.bmax,
                    ) && hit_breakable.as_ref().is_none_or(|(_, d, _, _)| t < *d)
                    {
                        hit_breakable = Some((b_ent, t, b.material, b.explodemagnitude));
                    }
                }
            }

            // Trace visual studio props (chickens, bottles) to companion breakables
            for (_ent, ent_t, bounds, studio_ent) in &queries.props {
                let ep = ent_t.translation();
                let bmin = ep + bounds.mins;
                let bmax = ep + bounds.maxs;
                if let Some(t) =
                    ray_intersects_aabb(cur_src, bullet_dir, remaining_reach, bmin, bmax)
                    && !studio_ent.targetname.is_empty()
                {
                    for (b_ent, b) in &queries.breakable {
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
            for (h_ent, _, _, bounds, _, ht) in &queries.hostage {
                let ep = ht.translation();
                let bmin = ep + bounds.mins;
                let bmax = ep + bounds.maxs;
                if let Some(t) =
                    ray_intersects_aabb(cur_src, bullet_dir, remaining_reach, bmin, bmax)
                    && hit_hostage.as_ref().is_none_or(|(_, d)| t < *d)
                {
                    hit_hostage = Some((h_ent, t));
                }
            }

            // Resolve hit in this penetration segment
            if let Some((b_ent, b_t, mat, exp_mag)) = hit_breakable
                && hit_hostage.as_ref().is_none_or(|(_, h_t)| b_t < *h_t)
            {
                if let Ok((_, mut breakable)) = queries.breakable.get_mut(b_ent) {
                    breakable.health -= cur_damage;
                    let was_broken = breakable.health <= 0.0 && !breakable.is_broken;
                    if was_broken {
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
                        if exp_mag > 0 {
                            spawn_explosion_sprite(
                                &mut commands,
                                center,
                                &mut sprite_store,
                                &audio.search_paths,
                                &mut vfx.images,
                                &mut vfx.materials,
                                &mut vfx.meshes,
                            );
                        }
                    }
                    if penetrations_left > 0 && remaining_reach > b_t + 16.0 {
                        penetrations_left -= 1;
                        cur_damage *= if was_broken { 0.85 } else { 0.5 };
                        cur_src += bullet_dir * (b_t + 16.0);
                        remaining_reach -= b_t + 16.0;
                        continue;
                    }
                }
                break;
            } else if let Some((h_ent, h_t)) = hit_hostage
                && h_t <= hit_wall_dist + 4.0
            {
                if let Ok((_, mut hostage, mut h_anim, mut bounds, mut studio_ent, ht)) =
                    queries.hostage.get_mut(h_ent)
                {
                    hostage.health -= cur_damage;
                    let hit_pos = cur_src + bullet_dir * h_t;
                    let flesh_snd = if ((time.elapsed_secs() * 100.0) as usize).is_multiple_of(2) {
                        "weapons/bullet_hit1.wav"
                    } else {
                        "weapons/bullet_hit2.wav"
                    };
                    if let Some(h) = audio.audio_bank.get_or_load_sound(
                        flesh_snd,
                        &audio.search_paths.dirs,
                        &mut audio.audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation(hit_pos),
                        ));
                    }
                    // Spawn authentic CS 1.6 blood sprite
                    spawn_blood_sprite(
                        &mut commands,
                        hit_pos,
                        &mut sprite_store,
                        &audio.search_paths,
                        &mut vfx.images,
                        &mut vfx.materials,
                        &mut vfx.meshes,
                    );

                    if hostage.health > 0.0 {
                        h_anim.set_sequence(7, 30.0, false);
                        let pain_idx = ((time.elapsed_secs() * 100.0) as usize % 6) + 1;
                        let snd = format!("hostage/hpain/hpain{pain_idx}.wav");
                        if let Some(h) = audio.audio_bank.get_or_load_sound(
                            &snd,
                            &audio.search_paths.dirs,
                            &mut audio.audio_sources,
                        ) {
                            commands.spawn((
                                AudioPlayer::new(h),
                                PlaybackSettings::DESPAWN.with_spatial(true),
                                Transform::from_translation(ht.translation()),
                            ));
                        }
                    } else if !hostage.is_dead {
                        hostage.is_dead = true;
                        h_anim.set_sequence(31, 24.0, false);
                        studio_ent.is_solid = false; // Dead hostages lose collision so player does not get stuck
                        bounds.mins = Vec3::ZERO;
                        bounds.maxs = Vec3::ZERO;
                        if let Some(h) = audio.audio_bank.get_or_load_sound(
                            "radio/hosdown.wav",
                            &audio.search_paths.dirs,
                            &mut audio.audio_sources,
                        ) {
                            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                        }
                    }

                    // Bullet passes through victim (flesh penetration)
                    if penetrations_left > 0 && remaining_reach > h_t + 24.0 {
                        penetrations_left -= 1;
                        cur_damage *= 0.65;
                        cur_src += bullet_dir * (h_t + 24.0);
                        remaining_reach -= h_t + 24.0;
                        continue;
                    }
                }
                break;
            } else if hit_wall_dist < remaining_reach {
                let hit_pos = cur_src + bullet_dir * hit_wall_dist;
                // Wall ricochet sound
                let ric_idx = ((time.elapsed_secs() * 100.0) as usize % 5) + 1;
                let ric_snd = format!("weapons/ric{ric_idx}.wav");
                if let Some(h) = audio.audio_bank.get_or_load_sound(
                    &ric_snd,
                    &audio.search_paths.dirs,
                    &mut audio.audio_sources,
                ) {
                    commands.spawn((
                        AudioPlayer::new(h),
                        PlaybackSettings::DESPAWN.with_spatial(true),
                        Transform::from_translation(hit_pos),
                    ));
                }
                // Spawn authentic CS 1.6 bullet hole decal on wall from decals.wad
                spawn_bullet_hole_decal(
                    &mut commands,
                    hit_pos,
                    hit_wall_normal,
                    &mut sprite_store,
                    &audio.search_paths,
                    &mut vfx.images,
                    &mut vfx.materials,
                    &mut vfx.meshes,
                );

                // Wallbang / Wall penetration: advance through wall thickness by weapon penetration_power
                if penetrations_left > 0
                    && def.penetration_power > 0.0
                    && remaining_reach > hit_wall_dist + def.penetration_power
                {
                    penetrations_left -= 1;
                    cur_damage *= 0.5; // ReGameDLL wall penetration damage factor
                    let pen_step = def.penetration_power;
                    cur_src = hit_pos + bullet_dir * pen_step;
                    remaining_reach -= hit_wall_dist + pen_step;
                    continue;
                }
            }
            break;
        }
    }
}
