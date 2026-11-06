//! Dynamic window mode, diagnostics, and display management.

use std::thread;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};

/// Plugin managing full-screen, VSync toggling, CPU pacing, and real-time title bar diagnostics.
pub struct WindowControlPlugin;

impl Plugin for WindowControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiagnosticsTracker>();
        app.init_resource::<FramePacer>();
        app.add_systems(
            Update,
            (
                toggle_fullscreen_mode,
                toggle_vsync_mode,
                frame_pacing_system,
                update_window_title_diagnostics,
            ),
        );
    }
}

#[derive(Resource)]
struct FramePacer {
    last_frame_instant: Instant,
    target_frame_duration: Duration,
    pacing_enabled: bool,
}

impl Default for FramePacer {
    fn default() -> Self {
        Self {
            last_frame_instant: Instant::now(),
            target_frame_duration: Duration::from_nanos(6_060_606), // ~165.0 Hz (1 / 165s)
            pacing_enabled: true,
        }
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
    mut pacer: ResMut<FramePacer>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    // Press V to toggle VSync on/off (Fifo vs Immediate) + CPU Pacing
    if keyboard.just_pressed(KeyCode::KeyV) {
        pacer.pacing_enabled = !pacer.pacing_enabled;

        if let Ok(mut window) = window_query.get_single_mut() {
            window.present_mode = if pacer.pacing_enabled {
                PresentMode::Fifo
            } else {
                PresentMode::Immediate
            };
        }
    }
}

fn frame_pacing_system(mut pacer: ResMut<FramePacer>) {
    if !pacer.pacing_enabled {
        pacer.last_frame_instant = Instant::now();
        return;
    }

    let elapsed = pacer.last_frame_instant.elapsed();
    if let Some(sleep_duration) = pacer.target_frame_duration.checked_sub(elapsed) {
        // Yield CPU thread slices to bring core utilization down to 1-3%
        if let Some(safe_sleep) = sleep_duration.checked_sub(Duration::from_micros(250))
            && safe_sleep > Duration::from_micros(500)
        {
            thread::sleep(safe_sleep);
        }
        // Micro-spin wait for sub-millisecond precision
        while pacer.last_frame_instant.elapsed() < pacer.target_frame_duration {
            thread::yield_now();
        }
    }

    pacer.last_frame_instant = Instant::now();
}

fn update_window_title_diagnostics(
    time: Res<Time>,
    pacer: Res<FramePacer>,
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
            let vsync_str = if pacer.pacing_enabled {
                "ON (165 FPS Hard Lock / Low CPU Load)"
            } else {
                "OFF (Uncapped / Full Core Load)"
            };

            window.title = format!(
                "GoldHub | FPS: {:.1} ({:.2} ms) | VSync [V]: {} | Fullscreen: [F11]",
                tracker.current_fps, tracker.frame_time_ms, vsync_str
            );
        }
    }
}
