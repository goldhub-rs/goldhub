//! First-person tournament player controller integrated with Bevy.
//!
//! Bridges 128 Hz fixed sub-tick physics (`goldhub-physics`) to Bevy's rendering pipeline
//! with smooth interpolation, camera mouse look, and seamless switching between
//! player movement and free-fly spectator mode via 'V'.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use goldhub_formats::ir::CollisionModel;
use goldhub_physics::{
    BspWorldCollision, EntityCompositeTrace, FixedSubtickSimulator, IN_BACK, IN_DUCK, IN_FORWARD,
    IN_JUMP, IN_MOVELEFT, IN_MOVERIGHT, IN_SPEED, PlayerCmd, WorldTrace,
};

use crate::camera::SpectatorCamera;

/// Active control mode for the engine viewport camera.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraControlMode {
    #[default]
    Player,
    Spectator,
}

/// Camera perspective mode (FirstPerson vs ThirdPerson).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraPerspective {
    #[default]
    FirstPerson,
    ThirdPerson,
}

/// Stores the BSP collision model for physical hull tracing.
#[derive(Resource, Default)]
pub struct LevelCollisionResource {
    pub model: Option<CollisionModel>,
}

/// Player controller component attached to the player camera entity.
#[derive(Component, Debug)]
pub struct PlayerController {
    pub sim: FixedSubtickSimulator,
    pub pitch: f32,
    pub yaw: f32,
    pub mouse_sensitivity: f32,
    pub is_active: bool,
    pub step_controller: goldhub_audio::StepSoundController,
    pub was_on_ground: bool,
    pub seed_step: u8,
    pub seed_vol: u8,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self {
            sim: FixedSubtickSimulator::default(),
            pitch: 0.0,
            yaw: 0.0,
            mouse_sensitivity: 0.002,
            is_active: true,
            step_controller: goldhub_audio::StepSoundController::default(),
            was_on_ground: false,
            seed_step: 0,
            seed_vol: 0,
        }
    }
}

/// Marker component for the in-game speedometer HUD text.
#[derive(Component)]
pub struct SpeedometerText;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraControlMode>();
        app.init_resource::<CameraPerspective>();
        app.init_resource::<LevelCollisionResource>();
        app.add_systems(Startup, setup_speedometer_hud);
        app.add_systems(
            Update,
            (
                player_movement_system,
                controller_toggle_system,
                update_speedometer_system,
                spawn_player_character_model_system,
                update_player_character_model_system.before(crate::studio::studio_animation_system),
            ),
        );
    }
}

fn setup_speedometer_hud(mut commands: Commands) {
    commands.spawn((
        SpeedometerText,
        Text::new("spd: 0.00 u/s"),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::srgb(0.0, 1.0, 0.4)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(24.0),
            left: Val::Px(24.0),
            ..default()
        },
    ));
}

fn update_speedometer_system(
    player_query: Query<&PlayerController>,
    mut text_query: Query<&mut Text, With<SpeedometerText>>,
) {
    if let Ok(player) = player_query.get_single() {
        let spd = player.sim.current.horizontal_speed();
        for mut text in &mut text_query {
            **text = format!("spd: {spd:.2} u/s");
        }
    }
}

/// Fallback world collision when no BSP map is loaded (infinite floor at Z = 0).
struct FallbackFlatWorld;

impl WorldTrace for FallbackFlatWorld {
    fn trace_player_hull(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        _hull_idx: u32,
    ) -> goldhub_physics::TraceResult {
        const DIST_EPSILON: f32 = 0.03125;
        if end[2] <= 0.0 {
            let frac = if start[2] > 0.0 {
                (start[2] / (start[2] - end[2])).clamp(0.0, 1.0)
            } else {
                0.0
            };
            goldhub_physics::TraceResult {
                startsolid: start[2] < 0.0,
                allsolid: false,
                fraction: frac,
                endpos: [
                    start[0] + (end[0] - start[0]) * frac,
                    start[1] + (end[1] - start[1]) * frac,
                    DIST_EPSILON,
                ],
                plane_normal: [0.0, 0.0, 1.0],
                plane_dist: 0.0,
                hit_entity: None,
            }
        } else {
            goldhub_physics::TraceResult {
                startsolid: false,
                allsolid: false,
                fraction: 1.0,
                endpos: end,
                plane_normal: [0.0, 0.0, 1.0],
                plane_dist: 0.0,
                hit_entity: None,
            }
        }
    }

    fn test_player_position(&self, pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
        if pos[2] < 0.0 { Some(0) } else { None }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn player_movement_system(
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    collision_res: Res<LevelCollisionResource>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut query: Query<(&mut Transform, &mut PlayerController)>,
    solid_query: Query<
        (
            &Transform,
            &crate::studio::EntityCollisionBounds,
            &crate::studio::WorldStudioEntity,
        ),
        Without<PlayerController>,
    >,
    mut commands: Commands,
    mut audio_bank: ResMut<crate::audio::GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    search_paths: Res<crate::audio::GameSearchPaths>,
) {
    if *mode != CameraControlMode::Player {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // Query active solid studio entities (filtering by is_solid so props/chickens don't block)
    let mut solid_entities = Vec::with_capacity(8);
    for (ent_trans, bounds, studio_ent) in &solid_query {
        if !studio_ent.is_solid {
            continue;
        }
        let p = ent_trans.translation;
        // Bevy to GoldSrc: gx = p.x, gy = -p.z, gz = p.y
        let gx = p.x;
        let gy = -p.z;
        let gz = p.y;
        let gs_mins = [gx + bounds.mins.x, gy - bounds.maxs.z, gz + bounds.mins.y];
        let gs_maxs = [gx + bounds.maxs.x, gy - bounds.mins.z, gz + bounds.maxs.y];
        solid_entities.push((gs_mins, gs_maxs));
    }

    for (mut transform, mut player) in &mut query {
        if !player.is_active {
            continue;
        }

        // 1. Mouse Look: Pitch and Yaw
        let mouse_delta = mouse_motion.delta;
        if mouse_delta.length_squared() > 0.0 {
            player.yaw -= mouse_delta.x * player.mouse_sensitivity;
            player.pitch = (player.pitch - mouse_delta.y * player.mouse_sensitivity)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        }

        // GoldSrc angles: pitch in degrees [-89, 89], yaw in degrees [0, 360]
        // In GoldSrc convention (and angle_vectors): negative pitch is looking UP (-sp > 0 -> +Z).
        // In Bevy mouse look, looking up gives positive player.pitch.
        // Therefore, pitch_deg must be inverted to match GoldSrc coordinate convention.
        let pitch_deg = -player.pitch.to_degrees();
        let yaw_deg = (player.yaw.to_degrees() + 90.0).rem_euclid(360.0);

        // 2. Keyboard Input mapping to PlayerCmd
        let mut forward_move = 0.0f32;
        let mut side_move = 0.0f32;
        let mut buttons = 0u32;
        let move_scale = player.sim.vars.maxspeed;

        if keyboard.pressed(KeyCode::KeyW) {
            forward_move += move_scale;
            buttons |= IN_FORWARD;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            forward_move -= move_scale;
            buttons |= IN_BACK;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            side_move -= move_scale;
            buttons |= IN_MOVELEFT;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            side_move += move_scale;
            buttons |= IN_MOVERIGHT;
        }
        if keyboard.pressed(KeyCode::Space) {
            buttons |= IN_JUMP;
        }
        if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::KeyC) {
            buttons |= IN_DUCK;
        }
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            buttons |= IN_SPEED;
        }
        if keyboard.just_pressed(KeyCode::KeyE) {
            // GoldSrc +use interact sound (wpn_denyselect.wav)
            if let Some(handle) = audio_bank.get_or_load_sound(
                "common/wpn_denyselect.wav",
                &search_paths.dirs,
                &mut audio_sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        }

        let cmd = PlayerCmd {
            forward_move,
            side_move,
            up_move: 0.0,
            buttons,
            view_angles: [pitch_deg, yaw_deg, 0.0],
        };

        // 3. Advance fixed 128 Hz simulation against BSP collision or fallback world (with solid entities)
        if let Some(ref col_model) = collision_res.model {
            let bsp_world = BspWorldCollision::new(col_model);
            let composite = EntityCompositeTrace::new(&bsp_world, &solid_entities);
            player.sim.advance(dt, &cmd, &composite);
        } else {
            let flat_world = FallbackFlatWorld;
            let composite = EntityCompositeTrace::new(&flat_world, &solid_entities);
            player.sim.advance(dt, &cmd, &composite);
        }

        // 4. Interpolate origin & view height for smooth sub-frame rendering
        let (gs_pos, view_h) = player.sim.interpolate_for_render();

        // 5. Coordinate system transformation from GoldSrc (Z-up) to Bevy (Y-up)
        let eye_pos = Vec3::new(gs_pos[0], gs_pos[2] + view_h, -gs_pos[1]);
        let cam_rot =
            Quat::from_rotation_y(player.yaw) * Quat::from_rotation_x(player.pitch);

        // Support GoldSrc Third-Person View (cam_idealdist = 64) with wall-clip protection
        let final_pos = if *perspective == CameraPerspective::ThirdPerson {
            const THIRD_PERSON_DIST: f32 = 64.0;
            let back_dir = cam_rot * Vec3::Z;
            let target_pos = eye_pos + back_dir * THIRD_PERSON_DIST;

            collision_res.model.as_ref().map_or(target_pos, |col_model| {
                use goldhub_physics::WorldTrace;
                let bsp_world = BspWorldCollision::new(col_model);
                let gs_eye = [eye_pos.x, -eye_pos.z, eye_pos.y];
                let gs_target = [target_pos.x, -target_pos.z, target_pos.y];
                let tr = bsp_world.trace_player_hull(gs_eye, gs_target, 0);
                if tr.fraction < 1.0 {
                    let clipped_dist = (tr.fraction * THIRD_PERSON_DIST - 4.0).max(8.0);
                    eye_pos + back_dir * clipped_dist
                } else {
                    target_pos
                }
            })
        } else {
            eye_pos
        };

        transform.translation = final_pos;
        transform.rotation = cam_rot;
    }
}

/// Allows toggling between authentic Player movement and Spectator noclip camera with 'M' / 'F3',
/// and toggling First-Person vs Third-Person with 'F5'.
fn controller_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<CameraControlMode>,
    mut perspective: ResMut<CameraPerspective>,
    mut query: Query<(&Transform, &mut PlayerController, &mut SpectatorCamera)>,
) {
    if keyboard.just_pressed(KeyCode::F5) {
        *perspective = match *perspective {
            CameraPerspective::FirstPerson => {
                tracing::info!("Camera perspective: ThirdPerson (GoldSrc cam_command 1)");
                CameraPerspective::ThirdPerson
            }
            CameraPerspective::ThirdPerson => {
                tracing::info!("Camera perspective: FirstPerson");
                CameraPerspective::FirstPerson
            }
        };
    }

    if keyboard.just_pressed(KeyCode::KeyM) || keyboard.just_pressed(KeyCode::F3) {
        let Ok((transform, mut player, mut spec)) = query.get_single_mut() else {
            return;
        };

        *mode = match *mode {
            CameraControlMode::Player => {
                tracing::info!("Switched to Spectator Noclip Camera (Fly mode)");
                spec.yaw = player.yaw;
                spec.pitch = player.pitch;
                spec.is_active = true;
                player.is_active = false;
                CameraControlMode::Spectator
            }
            CameraControlMode::Spectator => {
                tracing::info!(
                    "Switched to First-Person Player Controller (128 Hz Tournament CS 1.6)"
                );
                // Convert back from Bevy coordinates to GoldSrc coordinates
                let gs_x = transform.translation.x;
                let gs_y = -transform.translation.z;
                let gs_z = transform.translation.y - 17.0;
                player.sim.current.origin = [gs_x, gs_y, gs_z];
                player.sim.previous.origin = [gs_x, gs_y, gs_z];
                // Seamlessly transfer spectator flight inertia into player simulation velocity
                // Bevy X = GS X, Bevy Y = GS Z, Bevy Z = -GS Y
                let gs_vx = spec.velocity.x;
                let gs_vy = -spec.velocity.z;
                let gs_vz = spec.velocity.y;
                player.sim.current.velocity = [gs_vx, gs_vy, gs_vz];
                player.sim.previous.velocity = [gs_vx, gs_vy, gs_vz];
                player.sim.current.on_ground = false;
                player.yaw = spec.yaw;
                player.pitch = spec.pitch;
                player.is_active = true;
                spec.is_active = false;
                CameraControlMode::Player
            }
        };
    }
}

/// Parses player spawn coordinates (`info_player_start` / `info_player_deathmatch`) from BSP entities lump.
#[must_use]
pub fn parse_first_player_spawn(entities_raw: &[u8]) -> Option<([f32; 3], f32)> {
    let entities_str = std::str::from_utf8(entities_raw).ok()?;

    for block in entities_str.split('}') {
        let mut is_spawn = false;
        let mut origin = None;
        let mut angle = 0.0f32;

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"classname\"")
                && let Some(c_str) = trimmed.split('"').nth(3)
                && (c_str == "info_player_start" || c_str == "info_player_deathmatch")
            {
                is_spawn = true;
            } else if trimmed.starts_with("\"origin\"")
                && let Some(o_str) = trimmed.split('"').nth(3)
            {
                let mut parts = o_str.split_whitespace();
                if let (Some(x), Some(y), Some(z)) = (parts.next(), parts.next(), parts.next())
                    && let (Ok(fx), Ok(fy), Ok(fz)) =
                        (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>())
                {
                    origin = Some([fx, fy, fz]);
                }
            } else if trimmed.starts_with("\"angle\"")
                && let Some(a_str) = trimmed.split('"').nth(3)
                && let Ok(fa) = a_str.parse::<f32>()
            {
                angle = fa;
            }
        }

        if is_spawn && let Some(orig) = origin {
            return Some((orig, angle));
        }
    }

    None
}

/// Marker component for the 3D player character model (rendered in 3rd person).
#[derive(Component, Debug)]
pub struct PlayerCharacterModel;

#[allow(clippy::too_many_arguments)]
fn spawn_player_character_model_system(
    mut commands: Commands,
    player_model_query: Query<Entity, With<PlayerCharacterModel>>,
    player_query: Query<(&PlayerController, &Transform)>,
    mut model_store: ResMut<crate::studio::StudioModelStore>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !player_model_query.is_empty() {
        return;
    }
    let Some(ref paths) = search_paths else {
        return;
    };
    let Ok((player, player_transform)) = player_query.get_single() else {
        return;
    };

    // Authentic CS 1.6 CT player model (urban.mdl)
    let model_path = "models/player/urban/urban.mdl";
    let Some(model_data) = model_store.get_or_load(model_path, paths, &mut images, &mut materials)
    else {
        return;
    };

    let mut anim_component = crate::studio::StudioAnimatedModel::new(model_path, false);
    anim_component.set_sequence(1, 15.0, true); // initial idle1

    let parent = commands
        .spawn((
            PlayerCharacterModel,
            Transform::from_translation(player_transform.translation)
                .with_rotation(Quat::from_rotation_y(player.yaw + std::f32::consts::FRAC_PI_2)),
            Visibility::Hidden, // Hidden in 1st person by default
        ))
        .id();

    let mut mesh_children = Vec::with_capacity(model_data.submeshes.len());
    for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
        let bevy_mesh = crate::studio::store::create_initial_bevy_mesh(template, false);
        let mesh_handle = meshes.add(bevy_mesh);

        let child = commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(template.material.clone()),
                Transform::IDENTITY,
            ))
            .id();
        commands.entity(parent).add_child(child);
        mesh_children.push((child, mesh_handle, sub_idx));
    }

    anim_component.mesh_entities = mesh_children;
    commands.entity(parent).insert(anim_component);
    tracing::info!("Spawned PlayerCharacterModel for third-person view");
}

fn update_player_character_model_system(
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    player_query: Query<&PlayerController>,
    mut model_query: Query<
        (
            &mut Transform,
            &mut crate::studio::StudioAnimatedModel,
            &mut Visibility,
        ),
        With<PlayerCharacterModel>,
    >,
) {
    let Ok(player) = player_query.get_single() else {
        return;
    };
    let Ok((mut transform, mut anim, mut visibility)) = model_query.get_single_mut() else {
        return;
    };

    if *mode == CameraControlMode::Spectator {
        *visibility = Visibility::Hidden;
        return;
    }

    let gs_pos = player.sim.current.origin;
    let base_origin = Vec3::new(gs_pos[0], gs_pos[2], -gs_pos[1]);

    // Align StudioModel rest pose (+X) to Bevy camera forward (-Z)
    transform.rotation = Quat::from_rotation_y(player.yaw + std::f32::consts::FRAC_PI_2);

    // Animation state machine based on movement
    let speed = player.sim.current.horizontal_speed();
    let on_ground = player.sim.current.on_ground;
    let is_crouching = player.sim.current.is_ducking;

    let (seq, fps) = if !on_ground {
        (6, 30.0) // jump
    } else if is_crouching {
        if speed > 10.0 {
            (5, 30.0) // crouchrun
        } else {
            (2, 20.0) // crouch_idle
        }
    } else if speed > 10.0 {
        if speed > 150.0 {
            (4, 30.0) // run
        } else {
            (3, 30.0) // walk
        }
    } else {
        (1, 15.0) // idle1
    };

    anim.set_sequence(seq, fps, true);

    if *perspective == CameraPerspective::ThirdPerson {
        transform.translation = base_origin;
        *visibility = Visibility::Inherited;
    } else {
        // First Person: CS2-style first-person legs
        // Only visible when tilting camera down towards feet (pitch < -35 degrees).
        // Offset backward by 18 units along horizontal camera forward so shoulders/torso stay behind the camera plane
        // and only thighs, knees, and boots are visible.
        if player.pitch < -35.0_f32.to_radians() {
            let forward_h = Vec3::new(-player.yaw.sin(), 0.0, -player.yaw.cos());
            transform.translation = base_origin - forward_h * 18.0;
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}
