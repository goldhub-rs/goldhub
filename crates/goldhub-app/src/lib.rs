//! GoldHub interactive engine shell, rendering viewport, and game host.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::type_complexity)]

pub mod audio;
pub mod camera;
pub mod combat;
pub mod entity;
pub mod hud;
pub mod level;
pub mod mesh_bridge;
pub mod player;
pub mod sprite;
pub mod studio;
pub mod video_settings;
pub mod window;

pub use entity::{
    AmbientAudioEntity, BreakableEntity, BreakableMaterial, EntityRuntimePlugin, EnvRenderEntity,
    EnvShooterEntity, FireTargetEvent, GibParticle,
};

pub use audio::{
    GameSearchPaths, GoldSrcAudioBank, GoldSrcAudioPlugin, GroundFace, LevelGroundMaterialIndex,
};
pub use camera::{SpectatorCamera, SpectatorCameraPlugin};
pub use level::{CurrentMapInfo, LevelPlugin};
pub use mesh_bridge::{PartitionedMesh, to_bevy_mesh, to_bevy_submeshes};
pub use player::{CameraControlMode, LevelCollisionResource, PlayerController, PlayerPlugin};
pub use sprite::{SpritePlugin, SpriteStore};
pub use studio::{
    BspEntitiesLump, StudioAnimatedModel, StudioModelData, StudioModelPlugin, StudioModelStore,
    ViewModelWeapon, WorldStudioEntity,
};
pub use video_settings::{VideoSettings, VideoSettingsPlugin};
pub use window::WindowControlPlugin;

use bevy::prelude::*;
use bevy::window::{WindowMode, WindowResolution};
use bevy::winit::{UpdateMode, WinitSettings};

/// Assembles the complete GoldHub Bevy application.
pub fn build_goldhub_app() -> App {
    let mut app = App::new();

    // Power-saving and CPU throttling: continuous in focus, low-power reactive when unfocused
    app.insert_resource(WinitSettings {
        focused_mode: UpdateMode::Continuous,
        unfocused_mode: UpdateMode::reactive_low_power(std::time::Duration::from_millis(100)),
    });

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "GoldHub Engine Viewport".into(),
                    resolution: WindowResolution::new(1024.0, 768.0),
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
        GoldSrcAudioPlugin,
        StudioModelPlugin,
        EntityRuntimePlugin,
        SpritePlugin,
        combat::CombatPlugin,
        hud::HudPlugin,
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
    #[test]
    fn test_goldsrc_audio_bank_resource_and_plugin() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<GameSearchPaths>();
        app.init_resource::<GoldSrcAudioBank>();
        assert!(app.world().contains_resource::<GoldSrcAudioBank>());
        assert!(app.world().contains_resource::<GameSearchPaths>());
    }

    #[test]
    fn test_studio_model_plugin_and_resources_registration() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(StudioModelPlugin);
        assert!(app.world().contains_resource::<StudioModelStore>());
        assert!(app.world().contains_resource::<BspEntitiesLump>());
    }

    #[test]
    fn test_gameplay_plugins_schedule_without_query_conflicts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<AudioSource>>()
            .init_resource::<Assets<Font>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<bevy::input::mouse::AccumulatedMouseMotion>()
            .add_event::<bevy::input::mouse::MouseWheel>()
            .init_resource::<GameSearchPaths>()
            .init_resource::<GoldSrcAudioBank>()
            .init_resource::<player::CameraControlMode>()
            .init_resource::<player::CameraPerspective>()
            .init_resource::<LevelCollisionResource>()
            .add_plugins((
                PlayerPlugin,
                StudioModelPlugin,
                EntityRuntimePlugin,
                SpritePlugin,
                combat::CombatPlugin,
                hud::HudPlugin,
            ));

        // Execute Startup + 2 Update frames to validate zero Bevy Query aliasing or system-param panics
        app.update();
        app.update();
        assert!(
            app.world()
                .contains_resource::<combat::weapon::HudFastSwitch>()
        );
        assert!(
            app.world()
                .contains_resource::<combat::weapon::PendingWeaponSelection>()
        );
    }
}
