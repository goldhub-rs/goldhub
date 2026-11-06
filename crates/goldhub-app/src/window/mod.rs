//! Dynamic window mode, diagnostics, and display management.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};

/// Plugin managing full-screen, VSync toggling, and real-time title bar diagnostics.
pub struct WindowControlPlugin;

impl Plugin for WindowControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiagnosticsTracker>();
        app.add_systems(
            Update,
            (
                toggle_fullscreen_mode,
                toggle_vsync_mode,
                update_window_title_diagnostics,
            ),
        );
    }
}

#[derive(Resource)]
struct DiagnosticsTracker {
    frame_count: u32,
    timer: f32,
    current_fps: f32,
    frame_time_ms: f32,
}

impl Default for DiagnosticsTracker {
    fn default() -> Self {
        Self {
            frame_count: 0,
            timer: 0.0,
            current_fps: 0.0,
            frame_time_ms: 0.0,
        }
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

fn toggle_vsync_mode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    // Press V to toggle VSync on/off (AutoVsync vs AutoNoVsync)
    if keyboard.just_pressed(KeyCode::KeyV)
        && let Ok(mut window) = window_query.get_single_mut()
    {
        window.present_mode = match window.present_mode {
            PresentMode::AutoVsync => PresentMode::AutoNoVsync,
            _ => PresentMode::AutoVsync,
        };
    }
}

fn update_window_title_diagnostics(
    time: Res<Time>,
    mut tracker: ResMut<DiagnosticsTracker>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    tracker.frame_count += 1;
    tracker.timer += time.delta_secs();

    if tracker.timer >= 0.25 {
        tracker.current_fps = tracker.frame_count as f32 / tracker.timer;
        tracker.frame_time_ms = (tracker.timer / tracker.frame_count as f32) * 1000.0;
        tracker.frame_count = 0;
        tracker.timer = 0.0;

        if let Ok(mut window) = window_query.get_single_mut() {
            let vsync_str = match window.present_mode {
                PresentMode::AutoVsync => "ON (165Hz target / sync)",
                _ => "OFF (Uncapped / Full Core Load)",
            };

            window.title = format!(
                "GoldHub | FPS: {:.1} ({:.2} ms) | VSync [V]: {} | Toggle Fullscreen: [F11]",
                tracker.current_fps, tracker.frame_time_ms, vsync_str
            );
        }
    }
}
