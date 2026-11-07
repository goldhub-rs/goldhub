//! GoldHub interactive engine shell, rendering viewport, and game host.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::type_complexity)]

pub mod camera;
pub mod level;
pub mod mesh_bridge;
pub mod player;
pub mod video_settings;
pub mod window;

pub use camera::{SpectatorCamera, SpectatorCameraPlugin};
pub use level::LevelPlugin;
pub use mesh_bridge::{PartitionedMesh, to_bevy_mesh, to_bevy_submeshes};
pub use player::{CameraControlMode, LevelCollisionResource, PlayerController, PlayerPlugin};
pub use video_settings::{VideoSettings, VideoSettingsPlugin};
pub use window::WindowControlPlugin;

use bevy::prelude::*;
use bevy::window::{WindowMode, WindowResolution};

/// Assembles the complete GoldHub Bevy application.
pub fn build_goldhub_app() -> App {
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "GoldHub Engine Viewport".into(),
                    resolution: WindowResolution::new(1280.0, 720.0),
                    mode: WindowMode::Windowed,
                    resizable: true,
                    present_mode: bevy::window::PresentMode::Fifo,
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: "assets".into(),
                ..default()
            })
            .disable::<bevy::log::LogPlugin>(),
    );

    app.add_plugins((
        WindowControlPlugin,
        SpectatorCameraPlugin,
        PlayerPlugin,
        LevelPlugin,
        VideoSettingsPlugin,
    ));

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
