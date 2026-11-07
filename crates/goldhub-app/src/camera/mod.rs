//! Noclip spectator camera controller with smooth mouse look and 6DoF movement.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};

use crate::player::CameraControlMode;

pub struct SpectatorCameraPlugin;

impl Plugin for SpectatorCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spectator_camera_movement, cursor_grab_system));
    }
}

/// Spectator camera controller component.
#[derive(Component, Debug, Clone)]
pub struct SpectatorCamera {
    pub move_speed: f32,
    pub sprint_multiplier: f32,
    pub look_sensitivity: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub is_active: bool,
}

impl Default for SpectatorCamera {
    fn default() -> Self {
        Self {
            move_speed: 600.0,
            sprint_multiplier: 2.5,
            look_sensitivity: 0.002,
            pitch: 0.0,
            yaw: 0.0,
            is_active: true,
        }
    }
}

fn spectator_camera_movement(
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<(&mut Transform, &mut SpectatorCamera)>,
) {
    if *mode != CameraControlMode::Spectator {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (mut transform, mut cam) in &mut camera_query {
        if !cam.is_active {
            continue;
        }

        // 1. Mouse Look
        let mouse_delta = mouse_motion.delta;
        if mouse_delta.length_squared() > 0.0 {
            cam.yaw -= mouse_delta.x * cam.look_sensitivity;
            cam.pitch = (cam.pitch - mouse_delta.y * cam.look_sensitivity)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());

            transform.rotation = Quat::from_rotation_y(cam.yaw) * Quat::from_rotation_x(cam.pitch);
        }

        // 2. Linear Movement
        let mut speed = cam.move_speed;
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            speed *= cam.sprint_multiplier;
        }

        let forward = transform.forward();
        let right = transform.right();
        let up = Dir3::Y;

        let mut wish_dir = Vec3::ZERO;

        if keyboard.pressed(KeyCode::KeyW) {
            wish_dir += *forward;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            wish_dir -= *forward;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            wish_dir += *right;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            wish_dir -= *right;
        }
        if keyboard.pressed(KeyCode::Space) {
            wish_dir += *up;
        }
        if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::KeyC) {
            wish_dir -= *up;
        }

        if wish_dir.length_squared() > 0.0 {
            transform.translation += wish_dir.normalize() * speed * dt;
        }
    }
}

fn cursor_grab_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mode: Res<CameraControlMode>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
    mut camera_query: Query<&mut SpectatorCamera>,
) {
    let Ok(mut window) = window_query.get_single_mut() else {
        return;
    };

    if mouse_button.just_pressed(MouseButton::Left) {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
        if *mode == CameraControlMode::Spectator {
            for mut cam in &mut camera_query {
                cam.is_active = true;
            }
        }
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
        for mut cam in &mut camera_query {
            cam.is_active = false;
        }
    }
}
