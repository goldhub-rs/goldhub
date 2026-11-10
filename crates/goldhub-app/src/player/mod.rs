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
    pub health: f32,
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
            health: 100.0,
            seed_step: 0,
            seed_vol: 0,
        }
    }
}

/// Marker component for the in-game speedometer HUD text.
#[derive(Component)]
pub struct SpeedometerText;

/// Tracks the player's flashlight state and spotlight child entity.
#[derive(Component, Debug, Default)]
pub struct PlayerFlashlight {
    pub is_on: bool,
    pub light_entity: Option<Entity>,
}

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
                player_flashlight_system,
                spawn_player_character_model_system,
                update_player_character_model_system.before(crate::studio::studio_animation_system),
                update_third_person_weapon_system.after(crate::studio::studio_animation_system),
            ),
        );
    }
}

fn player_flashlight_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_query: Query<
        (Entity, &mut PlayerFlashlight),
        (With<Camera3d>, With<PlayerController>),
    >,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut audio_bank: Option<ResMut<crate::audio::GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyF) {
        return;
    }

    let Ok((camera_entity, mut flashlight)) = camera_query.get_single_mut() else {
        return;
    };

    flashlight.is_on = !flashlight.is_on;

    if flashlight.is_on {
        let light = commands
            .spawn((
                SpotLight {
                    color: Color::srgb(0.96, 0.98, 1.0),
                    intensity: 25_000_000.0,
                    range: 2000.0,
                    inner_angle: 18.0f32.to_radians(),
                    outer_angle: 32.0f32.to_radians(),
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, -0.2),
            ))
            .id();
        commands.entity(camera_entity).add_child(light);
        flashlight.light_entity = Some(light);
    } else if let Some(light) = flashlight.light_entity.take() {
        commands.entity(light).despawn();
    }

    // Play flashlight click: items/flashlight1.wav
    if let (Some(paths), Some(ref mut bank), Some(ref mut sources)) =
        (search_paths, audio_bank.as_mut(), audio_sources.as_mut())
        && let Some(handle) = bank.get_or_load_sound("items/flashlight1.wav", &paths.dirs, sources)
    {
        commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
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
    mut query: Query<(
        &mut Transform,
        &mut PlayerController,
        Option<&crate::combat::PlayerRecoil>,
    )>,
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

    for (mut transform, mut player, recoil) in &mut query {
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

        // 4. Fall damage and impact landing punch
        let just_landed = !player.was_on_ground && player.sim.current.on_ground;
        player.was_on_ground = player.sim.current.on_ground;
        if just_landed {
            let fvel = player.sim.current.fall_velocity;
            if fvel > 580.0 {
                player.pitch -= (fvel - 580.0) * 0.0004;
                if fvel > 700.0 {
                    let dmg = (((fvel - 700.0) / 324.0) * 100.0).clamp(5.0, 100.0);
                    player.health = (player.health - dmg).max(0.0);
                    let pain_snd = if fvel > 880.0 {
                        "player/pl_fallpain1.wav"
                    } else {
                        "player/pl_pain2.wav"
                    };
                    if let Some(h) = audio_bank.get_or_load_sound(
                        pain_snd,
                        &search_paths.dirs,
                        &mut audio_sources,
                    ) {
                        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                    }
                }
            }
        }

        // 5. Interpolate origin & view height for smooth sub-frame rendering
        let (gs_pos, view_h) = player.sim.interpolate_for_render();

        // 6. Coordinate system transformation from GoldSrc (Z-up) to Bevy (Y-up)
        let eye_pos = Vec3::new(gs_pos[0], gs_pos[2] + view_h, -gs_pos[1]);
        let (punch_pitch, punch_yaw) = recoil.map_or((0.0, 0.0), |r| {
            (r.punch_pitch.to_radians(), r.punch_yaw.to_radians())
        });
        let cam_rot = Quat::from_rotation_y(player.yaw + punch_yaw)
            * Quat::from_rotation_x(player.pitch + punch_pitch);

        // Support GoldSrc Third-Person View (cam_idealdist = 64) with wall-clip protection
        let final_pos = if *perspective == CameraPerspective::ThirdPerson {
            const THIRD_PERSON_DIST: f32 = 64.0;
            let back_dir = cam_rot * Vec3::Z;
            let target_pos = eye_pos + back_dir * THIRD_PERSON_DIST;

            collision_res
                .model
                .as_ref()
                .map_or(target_pos, |col_model| {
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
            Transform::from_translation(player_transform.translation).with_rotation(
                Quat::from_rotation_y(player.yaw + std::f32::consts::FRAC_PI_2),
            ),
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

/// Marker and state component for the weapon held by the player character in 3rd person.
#[derive(Component, Debug, Default)]
pub struct PlayerThirdPersonWeapon {
    pub current_weapon_id: Option<crate::combat::WeaponId>,
    pub model_id: String,
    pub mesh_entities: Vec<(Entity, Handle<Mesh>, usize)>,
}

fn update_player_character_model_system(
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    player_query: Query<(&PlayerController, Option<&crate::combat::ActiveWeaponState>)>,
    model_store: Res<crate::studio::StudioModelStore>,
    mut model_query: Query<
        (
            &mut Transform,
            &mut crate::studio::StudioAnimatedModel,
            &mut Visibility,
        ),
        With<PlayerCharacterModel>,
    >,
) {
    let Ok((player, active_weapon)) = player_query.get_single() else {
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

    let on_ground = player.sim.current.on_ground;
    let is_crouching = player.sim.current.is_ducking;
    let speed = player.sim.current.horizontal_speed();

    let anim_ext = active_weapon.map_or("knife", |w| {
        crate::combat::get_weapon_def(w.id).anim_extension
    });

    if !on_ground {
        anim.set_sequence(6, 30.0, true); // jump
    } else if speed > 10.0 {
        if is_crouching {
            anim.set_sequence(5, 30.0, true); // crouchrun
        } else if speed > 150.0 {
            anim.set_sequence(4, 30.0, true); // run
        } else {
            anim.set_sequence(3, 30.0, true); // walk
        }
    } else {
        // Authentic CS 1.6 standing/crouching weapon-specific aim poses (ReGameDLL GetAnimDesired)
        let normalized_id = anim.model_id.replace('\\', "/").to_lowercase();
        let target_seq_name = format!(
            "{}_{}",
            if is_crouching {
                "crouch_aim"
            } else {
                "ref_aim"
            },
            anim_ext
        );

        let seq_idx = model_store
            .models
            .get(&normalized_id)
            .and_then(|m| {
                m.model
                    .rig
                    .sequences
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case(&target_seq_name))
            })
            .unwrap_or(if is_crouching { 2 } else { 1 });

        let fps = model_store
            .models
            .get(&normalized_id)
            .and_then(|m| m.model.rig.sequences.get(seq_idx))
            .map_or(15.0, |s| s.fps);

        anim.set_sequence(seq_idx, fps, true);
    }

    if *perspective == CameraPerspective::ThirdPerson {
        transform.translation = base_origin;
        *visibility = Visibility::Inherited;
    } else {
        *visibility = Visibility::Hidden;
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
fn update_third_person_weapon_system(
    mut commands: Commands,
    perspective: Res<CameraPerspective>,
    player_query: Query<Option<&crate::combat::ActiveWeaponState>, With<PlayerController>>,
    model_parent_query: Query<
        (Entity, &crate::studio::StudioAnimatedModel),
        With<PlayerCharacterModel>,
    >,
    mut weapon_query: Query<(Entity, &mut PlayerThirdPersonWeapon)>,
    mut visibility_query: Query<&mut Visibility, Without<PlayerCharacterModel>>,
    mut model_store: ResMut<crate::studio::StudioModelStore>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(active_weapon) = player_query.get_single() else {
        return;
    };
    let Ok((parent_entity, player_anim)) = model_parent_query.get_single() else {
        return;
    };
    let Some(ref paths) = search_paths else {
        return;
    };

    let current_id = active_weapon.map_or(crate::combat::WeaponId::Knife, |w| w.id);
    let def = crate::combat::get_weapon_def(current_id);

    // 1. Ensure weapon tracker component exists
    let Ok((_, mut weapon_data)) = weapon_query.get_single_mut() else {
        let wid = commands
            .spawn((
                PlayerThirdPersonWeapon::default(),
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        commands.entity(parent_entity).add_child(wid);
        return;
    };

    // 2. Check if weapon model needs switching
    if weapon_data.current_weapon_id != Some(current_id) {
        for &(child, _, _) in &weapon_data.mesh_entities {
            commands.entity(child).despawn();
        }
        weapon_data.mesh_entities.clear();
        weapon_data.current_weapon_id = Some(current_id);
        weapon_data.model_id = def.p_model.to_string();

        if let Some(model_data) =
            model_store.get_or_load(def.p_model, paths, &mut images, &mut materials)
        {
            for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
                let bevy_mesh = crate::studio::store::create_initial_bevy_mesh(template, false);
                let mesh_handle = meshes.add(bevy_mesh);
                let child = commands
                    .spawn((
                        Mesh3d(mesh_handle.clone()),
                        MeshMaterial3d(template.material.clone()),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                    ))
                    .id();
                commands.entity(parent_entity).add_child(child);
                weapon_data
                    .mesh_entities
                    .push((child, mesh_handle, sub_idx));
            }
        }
    }

    // 3. Update visibility and bone deformation
    let is_tp = *perspective == CameraPerspective::ThirdPerson;
    for &(child, ref mesh_handle, submesh_idx) in &weapon_data.mesh_entities {
        if let Ok(mut vis) = visibility_query.get_mut(child) {
            let target_vis = if is_tp {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != target_vis {
                *vis = target_vis;
            }
        }

        if !is_tp {
            continue;
        }

        let normalized_id = def.p_model.replace('\\', "/").to_lowercase();
        let Some(p_model_data) = model_store.models.get(&normalized_id) else {
            continue;
        };

        let urban_bones = &player_anim.current_bone_transforms;
        if urban_bones.is_empty() {
            continue;
        }

        // Match bones between p_*.mdl and urban.mdl
        let p_bones = &p_model_data.model.rig.bones;
        let mut merged_matrices = Vec::with_capacity(p_bones.len());
        for b in p_bones {
            let lower_name = b.name.to_lowercase();
            let matched_mat = if lower_name == "bip01 r hand" && urban_bones.len() > 15 {
                Some(urban_bones[15])
            } else if lower_name == "bip01 r forearm" && urban_bones.len() > 14 {
                Some(urban_bones[14])
            } else if lower_name == "bip01 r upperarm" && urban_bones.len() > 13 {
                Some(urban_bones[13])
            } else if lower_name == "bip01 r clavicle" && urban_bones.len() > 12 {
                Some(urban_bones[12])
            } else if lower_name == "bip01" && !urban_bones.is_empty() {
                Some(urban_bones[0])
            } else {
                None
            };

            if let Some(m) = matched_mat {
                merged_matrices.push(m);
            } else if b.parent_index >= 0 && (b.parent_index as usize) < merged_matrices.len() {
                let parent_m = merged_matrices[b.parent_index as usize];
                let local_m = Mat4::from_cols_array(&b.local_matrix);
                merged_matrices.push(parent_m * local_m);
            } else {
                merged_matrices.push(Mat4::from_cols_array(&b.local_matrix));
            }
        }

        let Some(template) = p_model_data.submeshes.get(submesh_idx) else {
            continue;
        };
        let Some(mesh) = meshes.get_mut(mesh_handle) else {
            continue;
        };
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(pos_vec)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };

        for (v, out_p) in template.rest_vertices.iter().zip(pos_vec.iter_mut()) {
            let bone_idx = v.bone_index as usize;
            if bone_idx < merged_matrices.len() {
                let bm = &merged_matrices[bone_idx];
                let p = bm.transform_point3(Vec3::from(v.position));
                *out_p = [p.x, p.z, -p.y];
            }
        }
    }
}
