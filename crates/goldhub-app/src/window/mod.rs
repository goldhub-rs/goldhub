//! Dynamic window mode and display management.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

/// Plugin managing full-screen, windowed transitions, and cursor management.
pub struct WindowControlPlugin;

impl Plugin for WindowControlPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle_fullscreen_mode);
    }
}

fn toggle_fullscreen_mode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    let f11 = keyboard.just_pressed(KeyCode::F11);
    let alt_enter = (keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight))
        && keyboard.just_pressed(KeyCode::Enter);

    if (f11 || alt_enter)
        && let Ok(mut window) = window_query.get_single_mut()
    {
        window.mode = match window.mode {
            WindowMode::BorderlessFullscreen(_) | WindowMode::Fullscreen(_) => WindowMode::Windowed,
            _ => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        };
    }
}
