//! First-person ViewModel (weapons and hands) attached to the player camera.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use goldhub_physics::WorldTrace;

use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::player::{LevelCollisionResource, PlayerController};
use super::animation::StudioAnimatedModel;
use super::store::{StudioModelStore, create_initial_bevy_mesh};
use super::world_entity::{EntityCollisionBounds, WorldStudioEntity};

/// Component marking the first-person ViewModel weapon entity.
#[derive(Component, Debug, Clone)]
pub struct ViewModelWeapon {
    pub last_swing: u8,
    pub attack_cooldown: f32,
    pub bob_phase: f32,
    pub current_sway: Vec2,
}

impl Default for ViewModelWeapon {
    fn default() -> Self {
        Self {
            last_swing: 0,
            attack_cooldown: 0.0,
            bob_phase: 0.0,
            current_sway: Vec2::ZERO,
        }
    }
}

/// Marker component indicating a camera entity already has a ViewModel spawned.
#[derive(Component)]
pub struct HasViewModel;

/// Spawns the default first-person weapon (v_knife.mdl) attached as a child to the player camera.
pub fn spawn_viewmodel_system(
    mut commands: Commands,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    camera_query: Query<Entity, (With<Camera3d>, With<PlayerController>, Without<HasViewModel>)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for cam_entity in &camera_query {
        const KNIFE_PATH: &str = "models/v_knife.mdl";
        let Some(model_data) =
            model_store.get_or_load(KNIFE_PATH, &search_paths, &mut images, &mut materials)
        else {
            tracing::warn!("Failed to load ViewModel v_knife.mdl from search paths");
            continue;
        };

        // Find idle sequence (sequence 0 in CS 1.6 v_knife)
        let idle_fps = model_data.model.rig.sequences.first().map_or(16.0, |s| s.fps);
        let mut anim_component = StudioAnimatedModel::new(KNIFE_PATH, true);
        anim_component.set_sequence(0, idle_fps, true);

        // Spawn ViewModel root entity as a child of the camera
        let vm_root = commands
            .spawn((
                ViewModelWeapon::default(),
                Transform::IDENTITY,
                Visibility::default(),
            ))
            .id();

        // Spawn child entities for each submesh
        let mut mesh_children = Vec::with_capacity(model_data.submeshes.len());
        for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
            let bevy_mesh = create_initial_bevy_mesh(template, true);
            let mesh_handle = meshes.add(bevy_mesh);

            let child_entity = commands
                .spawn((
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(template.material.clone()),
                    Transform::IDENTITY,
                ))
                .id();

            commands.entity(vm_root).add_child(child_entity);
            mesh_children.push((child_entity, mesh_handle, sub_idx));
        }

        anim_component.mesh_entities = mesh_children;
        commands.entity(vm_root).insert(anim_component);
        commands.entity(cam_entity).add_child(vm_root);
        commands.entity(cam_entity).insert(HasViewModel);

        tracing::info!(
            submeshes = model_data.submeshes.len(),
            "Attached first-person Knife ViewModel to player camera"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KnifeHitType {
    Miss,
    Wall,
    Flesh,
}

/// Ray-AABB slab intersection test returning distance `t` along `dir` if hit.
fn ray_intersects_aabb(
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

    if tmin <= max_dist {
        Some(tmin)
    } else {
        None
    }
}

/// Maximum forward trace reach for knife slash and stab attacks (GoldSrc KnifeSwingDistance).
const KNIFE_REACH: f32 = 48.0;

/// Handles player attack inputs (LMB slash, RMB stab) and audio triggers with forward ray tracing.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn viewmodel_input_system(
    mut commands: Commands,
    time: Res<Time>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    model_store: Res<StudioModelStore>,
    collision_res: Res<LevelCollisionResource>,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<PlayerController>)>,
    entity_query: Query<
        (&GlobalTransform, &EntityCollisionBounds),
        With<WorldStudioEntity>,
    >,
    mut vm_query: Query<(&mut ViewModelWeapon, &mut StudioAnimatedModel)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Some(knife_data) = model_store.models.get("models/v_knife.mdl") else {
        return;
    };
    let sequences = &knife_data.model.rig.sequences;

    for (mut vm, mut anim) in &mut vm_query {
        if vm.attack_cooldown > 0.0 {
            vm.attack_cooldown -= dt;
        }

        // Check if current non-looping attack has completed
        if !anim.is_looping
            && let Some(seq) = sequences.get(anim.sequence_index)
        {
            let max_f = (seq.bone_matrices.len().saturating_sub(1)) as f32;
            if anim.frame_progress >= max_f {
                // Transition back to idle
                let idle_fps = sequences.first().map_or(16.0, |s| s.fps);
                anim.set_sequence(0, idle_fps, true);
            }
        }

        // Only allow new attack if not mid-animation or cooldown expired
        if vm.attack_cooldown > 0.0 {
            continue;
        }

        let lmb = mouse_button.just_pressed(MouseButton::Left);
        let rmb = mouse_button.just_pressed(MouseButton::Right);
        if !lmb && !rmb {
            continue;
        }

        // Raytrace 48 units from camera forward vector (CS 1.6 KnifeSwingDistance / KnifeStabDistance)
        let (cam_pos, cam_forward) = camera_query.get_single().map_or(
            (Vec3::ZERO, Vec3::NEG_Z),
            |cam_t| (cam_t.translation(), cam_t.forward().as_vec3()),
        );

        // 1. Trace against solid world entities (hostages, chickens, props)
        let mut closest_entity_dist: Option<f32> = None;
        for (ent_t, bounds) in &entity_query {
            let ep = ent_t.translation();
            let bmin = ep + bounds.mins;
            let bmax = ep + bounds.maxs;
            if let Some(t) = ray_intersects_aabb(cam_pos, cam_forward, KNIFE_REACH, bmin, bmax)
                && closest_entity_dist.is_none_or(|cd| t < cd)
            {
                closest_entity_dist = Some(t);
            }
        }

        // 2. Trace against world BSP collision geometry
        let mut wall_hit_dist: Option<f32> = None;
        if let Some(ref col_model) = collision_res.model {
            let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
            // Convert Bevy ray to GoldSrc coordinates: X_gs = X_bevy, Y_gs = -Z_bevy, Z_gs = Y_bevy
            let gs_start = [cam_pos.x, -cam_pos.z, cam_pos.y];
            let ray_end = cam_pos + cam_forward * KNIFE_REACH;
            let gs_end = [ray_end.x, -ray_end.z, ray_end.y];

            let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0); // 0 is HULL_POINT
            if tr.fraction < 1.0 {
                wall_hit_dist = Some(tr.fraction * KNIFE_REACH);
            }
        }

        let hit_type = match (closest_entity_dist, wall_hit_dist) {
            (Some(edist), Some(wdist)) => {
                if edist <= wdist {
                    KnifeHitType::Flesh
                } else {
                    KnifeHitType::Wall
                }
            }
            (Some(_), None) => KnifeHitType::Flesh,
            (None, Some(_)) => KnifeHitType::Wall,
            (None, None) => KnifeHitType::Miss,
        };

        if lmb {
            // Primary Attack: alternating midslash1 (seq 6) and midslash2 (seq 7), 55 FPS, 66 frames!
            vm.last_swing = 1 - vm.last_swing;
            let target_seq = if vm.last_swing == 0 { 6 } else { 7 };
            let slash_fps = sequences.get(target_seq).map_or(55.0, |s| s.fps);

            anim.set_sequence(target_seq, slash_fps, false);
            vm.attack_cooldown = 0.35;

            let sound_name = match hit_type {
                KnifeHitType::Flesh => {
                    const HITS: [&str; 4] = [
                        "weapons/knife_hit1.wav",
                        "weapons/knife_hit2.wav",
                        "weapons/knife_hit3.wav",
                        "weapons/knife_hit4.wav",
                    ];
                    let pick = ((time.elapsed_secs() * 1000.0) as usize) % HITS.len();
                    HITS[pick]
                }
                KnifeHitType::Wall => "weapons/knife_hitwall1.wav",
                KnifeHitType::Miss => {
                    if vm.last_swing == 0 {
                        "weapons/knife_slash1.wav"
                    } else {
                        "weapons/knife_slash2.wav"
                    }
                }
            };

            if let Some(handle) = audio_bank.get_or_load_sound(
                sound_name,
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        } else if rmb {
            // Secondary Attack: heavy stab (seq 4 if hit, seq 5 if miss)
            let (target_seq, stab_fps, sound_name) = match hit_type {
                KnifeHitType::Flesh => {
                    let fps = sequences.get(4).map_or(35.0, |s| s.fps);
                    (4, fps, "weapons/knife_stab.wav")
                }
                KnifeHitType::Wall => {
                    let fps = sequences.get(4).map_or(35.0, |s| s.fps);
                    (4, fps, "weapons/knife_hitwall1.wav")
                }
                KnifeHitType::Miss => {
                    let fps = sequences.get(5).map_or(40.0, |s| s.fps);
                    // Stabbing air in CS 1.6 plays slash whoosh sound!
                    (5, fps, "weapons/knife_slash1.wav")
                }
            };

            anim.set_sequence(target_seq, stab_fps, false);
            vm.attack_cooldown = 1.0;

            if let Some(handle) = audio_bank.get_or_load_sound(
                sound_name,
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        }
    }
}

/// Applies authentic weapon bobbing and mouse sway lag, and manages first/third person visibility.
pub fn viewmodel_bob_and_sway_system(
    time: Res<Time>,
    perspective: Option<Res<crate::player::CameraPerspective>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    player_query: Query<&PlayerController>,
    mut vm_query: Query<(&mut ViewModelWeapon, &mut Transform, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok(player) = player_query.get_single() else {
        return;
    };
    let speed = player.sim.current.horizontal_speed();

    for (mut vm, mut transform, mut visibility) in &mut vm_query {
        // Hide viewmodel in Third-Person mode
        if let Some(ref pers) = perspective {
            if **pers == crate::player::CameraPerspective::ThirdPerson {
                *visibility = Visibility::Hidden;
                continue;
            }
            *visibility = Visibility::Inherited;
        }

        // 1. Movement Bobbing
        if speed > 10.0 && player.sim.current.on_ground {
            let bob_freq = (speed / 120.0).clamp(2.0, 7.0);
            vm.bob_phase += dt * bob_freq * std::f32::consts::PI;

            let bob_x = (vm.bob_phase * 0.5).sin() * 0.35;
            let bob_y = (vm.bob_phase).sin().abs() * -0.25;
            transform.translation.x = bob_x;
            transform.translation.y = bob_y;
        } else {
            // Smoothly return to rest position
            transform.translation.x *= 1.0 - dt * 10.0;
            transform.translation.y *= 1.0 - dt * 10.0;
        }

        // 2. Mouse Sway Lag
        let target_sway_x = (-mouse_motion.delta.x * 0.0008).clamp(-0.05, 0.05);
        let target_sway_y = (mouse_motion.delta.y * 0.0008).clamp(-0.05, 0.05);

        vm.current_sway.x = vm.current_sway.x.lerp(target_sway_x, dt * 12.0);
        vm.current_sway.y = vm.current_sway.y.lerp(target_sway_y, dt * 12.0);

        transform.rotation = Quat::from_rotation_y(vm.current_sway.x)
            * Quat::from_rotation_x(vm.current_sway.y);
    }
}

/// True Hor+ Widescreen FOV system for the player camera.
pub fn widescreen_horplus_fov_system(
    mut query: Query<&mut Projection, With<Camera3d>>,
) {
    // Hor+ vertical FOV keeping 4:3 90-degree framing invariant:
    // tan(90 deg / 2) * 0.75 = 0.75 -> VFOV = 2 * atan(0.75) ≈ 1.2870022 rad (73.74 degrees)
    const HORPLUS_VFOV_RAD: f32 = 1.287_002_2;

    for mut projection in &mut query {
        if let Projection::Perspective(ref mut perspective) = *projection
            && (perspective.fov - HORPLUS_VFOV_RAD).abs() > 1e-4
        {
            perspective.fov = HORPLUS_VFOV_RAD;
        }
    }
}
