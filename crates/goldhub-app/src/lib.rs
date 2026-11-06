//! GoldHub interactive engine shell, rendering viewport, and game host.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::type_complexity)]

pub mod camera;
pub mod level;
pub mod mesh_bridge;
pub mod window;

pub use camera::{SpectatorCamera, SpectatorCameraPlugin};
pub use level::LevelPlugin;
pub use mesh_bridge::to_bevy_mesh;
pub use window::WindowControlPlugin;

use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

/// Assembles the complete GoldHub Bevy application.
pub fn build_goldhub_app() -> App {
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "GoldHub".into(),
                    resolution: WindowResolution::new(1920.0, 1080.0),
                    mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                    present_mode: bevy::window::PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: "assets".into(),
                ..default()
            }),
    );

    app.add_plugins((WindowControlPlugin, SpectatorCameraPlugin, LevelPlugin));

    app
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goldhub_plugins_register_cleanly() {
        // Build headless app for testing systems without GPU window
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins((WindowControlPlugin, SpectatorCameraPlugin));
        assert!(app.is_plugin_added::<WindowControlPlugin>());
        assert!(app.is_plugin_added::<SpectatorCameraPlugin>());
    }
}
