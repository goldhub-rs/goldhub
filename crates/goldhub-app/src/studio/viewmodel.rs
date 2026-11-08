//! First-person ViewModel (weapons and hands) attached to the player camera.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::player::PlayerController;
use super::animation::StudioAnimatedModel;
use super::store::{StudioModelStore, create_initial_bevy_mesh};

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

/// Handles player attack inputs (LMB slash, RMB stab) and audio triggers.
#[allow(clippy::too_many_arguments)]
pub fn viewmodel_input_system(
    mut commands: Commands,
    time: Res<Time>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    model_store: Res<StudioModelStore>,
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

        if mouse_button.just_pressed(MouseButton::Left) {
            // Primary Attack: alternating slash1 (seq 1) and slash2 (seq 2)
            vm.last_swing = 1 - vm.last_swing;
            let target_seq = if vm.last_swing == 0 { 1 } else { 2 };
            let slash_fps = sequences.get(target_seq).map_or(30.0, |s| s.fps);

            anim.set_sequence(target_seq, slash_fps, false);
            vm.attack_cooldown = 0.4;

            // Play knife slash audio
            let sound_name = if vm.last_swing == 0 {
                "weapons/knife_slash1.wav"
            } else {
                "weapons/knife_slash2.wav"
            };
            if let Some(handle) = audio_bank.get_or_load_sound(
                sound_name,
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        } else if mouse_button.just_pressed(MouseButton::Right) {
            // Secondary Attack: heavy stab (seq 4)
            let target_seq = 4;
            let stab_fps = sequences.get(target_seq).map_or(30.0, |s| s.fps);

            anim.set_sequence(target_seq, stab_fps, false);
            vm.attack_cooldown = 1.0;

            // Play knife stab audio
            if let Some(handle) = audio_bank.get_or_load_sound(
                "weapons/knife_stab.wav",
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        }
    }
}

/// Applies authentic weapon bobbing and mouse sway lag.
pub fn viewmodel_bob_and_sway_system(
    time: Res<Time>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    player_query: Query<&PlayerController>,
    mut vm_query: Query<(&mut ViewModelWeapon, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok(player) = player_query.get_single() else {
        return;
    };
    let speed = player.sim.current.horizontal_speed();

    for (mut vm, mut transform) in &mut vm_query {
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
