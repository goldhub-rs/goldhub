//! Level loading, asset pipeline integration, and scene illumination.

use std::fs;
use std::path::Path;

use bevy::prelude::*;
use goldhub_formats::pipeline::AssetPipeline;

use crate::camera::SpectatorCamera;
use crate::mesh_bridge::to_bevy_mesh;

const SAMPLE_BSP_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/regamedll_test_map_v8.bsp";

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_level);
    }
}

fn setup_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 1. Illumination: Sun + Ambient Light
    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(500.0, 1000.0, 500.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.9, 0.92, 1.0),
        brightness: 350.0,
    });

    // 2. Geometry Loading: Load BSP Level or Fallback Arena
    let bsp_path = Path::new(SAMPLE_BSP_PATH);
    let mut spawned_bsp = false;

    if bsp_path.exists()
        && let Ok(raw_bytes) = fs::read(bsp_path)
    {
        let pipeline = AssetPipeline::new(".cache/goldhub/assets");
        if let Ok(mesh_buffer) = pipeline.load_bsp_mesh(&raw_bytes) {
            let bevy_mesh = to_bevy_mesh(&mesh_buffer);
            let mesh_handle = meshes.add(bevy_mesh);
            let material_handle = materials.add(StandardMaterial {
                base_color: Color::srgb(0.72, 0.74, 0.78),
                perceptual_roughness: 0.85,
                reflectance: 0.15,
                cull_mode: None, // Dual-sided to view map geometry without backface culling issues
                ..default()
            });

            commands.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::from_xyz(0.0, 0.0, 0.0),
            ));

            tracing::info!(
                vertex_count = mesh_buffer.vertex_count(),
                index_count = mesh_buffer.index_count(),
                "Spawned BSP30 Level into Viewport"
            );
            spawned_bsp = true;
        }
    }

    // Fallback arena if BSP file was not found
    if !spawned_bsp {
        tracing::warn!("Sample BSP map not found, spawning fallback grid arena");
        let plane_mesh = meshes.add(Plane3d::default().mesh().size(1000.0, 1000.0));
        let plane_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.35, 0.45),
            perceptual_roughness: 0.7,
            ..default()
        });

        commands.spawn((
            Mesh3d(plane_mesh),
            MeshMaterial3d(plane_mat),
            Transform::IDENTITY,
        ));
    }

    // 3. Spawn Spectator Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 150.0, 400.0).looking_at(Vec3::new(0.0, 50.0, 0.0), Vec3::Y),
        SpectatorCamera::default(),
    ));
}
