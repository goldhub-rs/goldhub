//! Level loading, asset pipeline integration, and scene illumination.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use goldhub_formats::pipeline::AssetPipeline;

use crate::camera::SpectatorCamera;
use crate::mesh_bridge::to_bevy_submeshes;

const SAMPLE_BSP_PATH: &str = "D:/Repo/CS16/hlmsl/test_maps/de_inferno.bsp";
const FALLBACK_BSP_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/regamedll_test_map_v8.bsp";

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_level);
    }
}

#[allow(clippy::too_many_lines)]
fn setup_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // 1. Illumination: Sun + Ambient Light
    commands.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(500.0, 1500.0, 500.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.9, 0.92, 1.0),
        brightness: 450.0,
    });

    // 2. Geometry Loading: Load BSP Level from CLI args, de_inferno.bsp or fallback
    let cli_map = std::env::args().nth(1);
    let bsp_path = if let Some(ref arg) = cli_map
        && Path::new(arg).exists()
    {
        Path::new(arg)
    } else if Path::new(SAMPLE_BSP_PATH).exists() {
        Path::new(SAMPLE_BSP_PATH)
    } else {
        Path::new(FALLBACK_BSP_PATH)
    };
    let mut spawned_bsp = false;

    if bsp_path.exists()
        && let Ok(raw_bytes) = fs::read(bsp_path)
    {
        let pipeline = AssetPipeline::new(".cache/goldhub/assets");
        if let Ok((mesh_buffer, textures)) = pipeline.load_bsp_level_raw(&raw_bytes) {
            // 1. Create native GPU Images for each texture with Repeating wrap sampler
            let mut texture_materials: HashMap<u32, Handle<StandardMaterial>> = HashMap::new();

            for (idx, tex_desc) in textures.iter().enumerate() {
                let data = tex_desc
                    .mips
                    .first()
                    .map(|m| m.data.clone())
                    .unwrap_or_default();
                let mut img = Image::new(
                    bevy::render::render_resource::Extent3d {
                        width: tex_desc.width,
                        height: tex_desc.height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    data,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::render::render_asset::RenderAssetUsages::default(),
                );

                // Hardware repeating sampler enables infinite tiling on roofs and walls
                img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    address_mode_w: ImageAddressMode::Repeat,
                    ..default()
                });

                let img_handle = images.add(img);
                let is_cutout = tex_desc.name.starts_with('{')
                    || tex_desc
                        .mips
                        .first()
                        .is_some_and(|m| m.data.as_chunks::<4>().0.iter().any(|p| p[3] == 0));

                let mat_handle = materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(img_handle),
                    perceptual_roughness: 0.85,
                    reflectance: 0.15,
                    cull_mode: None,
                    alpha_mode: if is_cutout {
                        AlphaMode::Mask(0.5)
                    } else {
                        AlphaMode::Opaque
                    },
                    ..default()
                });

                texture_materials.insert(idx as u32, mat_handle);
            }

            // Fallback material for textures without diffuse image
            let fallback_mat = materials.add(StandardMaterial {
                base_color: Color::srgb(0.6, 0.6, 0.6),
                perceptual_roughness: 0.85,
                cull_mode: None,
                ..default()
            });

            // 2. Spawn partitioned submeshes by material_id
            let partitioned = to_bevy_submeshes(&mesh_buffer);
            for part in partitioned {
                let mesh_handle = meshes.add(part.mesh);
                let mat_handle = texture_materials
                    .get(&part.material_id)
                    .cloned()
                    .unwrap_or_else(|| fallback_mat.clone());

                commands.spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(mat_handle),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));
            }

            tracing::info!(
                vertex_count = mesh_buffer.vertex_count(),
                index_count = mesh_buffer.index_count(),
                texture_count = textures.len(),
                "Spawned BSP30 Level with Repeat Tiling into Viewport"
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
        Camera {
            clear_color: bevy::render::camera::ClearColorConfig::Custom(Color::srgb(
                0.12, 0.15, 0.2,
            )),
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        Transform::from_xyz(0.0, 300.0, 500.0).looking_at(Vec3::new(0.0, 100.0, 0.0), Vec3::Y),
        SpectatorCamera::default(),
    ));
}
