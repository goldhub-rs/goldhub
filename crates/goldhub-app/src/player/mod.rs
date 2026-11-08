//! First-person tournament player controller integrated with Bevy.
//!
//! Bridges 128 Hz fixed sub-tick physics (`goldhub-physics`) to Bevy's rendering pipeline
//! with smooth interpolation, camera mouse look, and seamless switching between
//! player movement and free-fly spectator mode via 'V'.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use goldhub_formats::ir::CollisionModel;
use goldhub_physics::{
    BspWorldCollision, FixedSubtickSimulator, IN_BACK, IN_DUCK, IN_FORWARD, IN_JUMP, IN_MOVELEFT,
    IN_MOVERIGHT, IN_SPEED, PlayerCmd, WorldTrace,
};

use crate::camera::SpectatorCamera;

/// Active control mode for the engine viewport camera.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraControlMode {
    #[default]
    Player,
    Spectator,
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
        app.init_resource::<LevelCollisionResource>();
        app.add_systems(Startup, setup_speedometer_hud);
        app.add_systems(
            Update,
            (
                player_movement_system,
                controller_toggle_system,
                update_speedometer_system,
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

#[allow(clippy::too_many_arguments)]
pub fn player_movement_system(
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    collision_res: Res<LevelCollisionResource>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut query: Query<(&mut Transform, &mut PlayerController)>,
) {
    if *mode != CameraControlMode::Player {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
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

        let cmd = PlayerCmd {
            forward_move,
            side_move,
            up_move: 0.0,
            buttons,
            view_angles: [pitch_deg, yaw_deg, 0.0],
        };

        // 3. Advance fixed 128 Hz simulation against BSP collision or fallback world
        if let Some(ref col_model) = collision_res.model {
            let bsp_world = BspWorldCollision::new(col_model);
            player.sim.advance(dt, &cmd, &bsp_world);
        } else {
            let flat_world = FallbackFlatWorld;
            player.sim.advance(dt, &cmd, &flat_world);
        }

        // 4. Interpolate origin & view height for smooth sub-frame rendering
        let (gs_pos, view_h) = player.sim.interpolate_for_render();

        // 5. Coordinate system transformation from GoldSrc (Z-up) to Bevy (Y-up)
        // Bevy X = GoldSrc X, Bevy Y = GoldSrc Z + view_height, Bevy Z = -GoldSrc Y
        let bevy_cam_pos = Vec3::new(gs_pos[0], gs_pos[2] + view_h, -gs_pos[1]);

        transform.translation = bevy_cam_pos;
        transform.rotation =
            Quat::from_rotation_y(player.yaw) * Quat::from_rotation_x(player.pitch);
    }
}

/// Allows toggling between authentic Player movement and Spectator noclip camera with 'M' or 'F3'.
fn controller_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<CameraControlMode>,
    mut query: Query<(&Transform, &mut PlayerController, &mut SpectatorCamera)>,
) {
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
                player.sim.current.velocity = [0.0; 3];
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
