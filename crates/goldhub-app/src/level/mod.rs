//! Level loading, asset pipeline integration, and scene illumination.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use goldhub_formats::pipeline::AssetPipeline;

use crate::camera::SpectatorCamera;
use crate::mesh_bridge::to_bevy_submeshes;

pub mod skybox;

const SAMPLE_BSP_PATH: &str = "D:/Repo/CS16/hlmsl/test_maps/de_inferno.bsp";
const FALLBACK_BSP_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/regamedll_test_map_v8.bsp";

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_level);
        app.add_systems(Update, skybox::update_skybox_position);
    }
}

#[allow(clippy::too_many_lines)]
fn setup_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // 1. Illumination: Standard GoldSrc ambient illumination
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 750.0,
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
    let mut spawn_point: Option<([f32; 3], f32)> = None;

    if bsp_path.exists()
        && let Ok(raw_bytes) = fs::read(bsp_path)
    {
        if let Ok(bsp) = goldhub_formats::codecs::legacy::bsp::BspLevel::from_bytes(&raw_bytes) {
            if let Ok(collision) = goldhub_formats::transpile::compile_bsp_collision(&bsp) {
                commands.insert_resource(crate::player::LevelCollisionResource {
                    model: Some(collision),
                });
            }
            spawn_point = crate::player::parse_first_player_spawn(bsp.entities);
        }

        let mut search_dirs = Vec::new();
        if let Some(dir) = bsp_path.parent() {
            search_dirs.push(dir);
            if let Some(parent) = dir.parent() {
                search_dirs.push(parent);
            }
        }

        let pipeline = AssetPipeline::new(".cache/goldhub/assets");
        if let Ok((mesh_buffer, textures, lightmap_atlas, sky_name)) =
            pipeline.load_bsp_level_full_with_search_dirs(&raw_bytes, &search_dirs)
        {
            // 0. Spawn GoldSrc 6-sided Skybox cube
            let sky = sky_name.as_deref().unwrap_or("green");
            skybox::spawn_skybox(&mut commands, &mut meshes, &mut materials, &mut images, sky);

            // 1. Create native GPU Image for Lightmap Atlas
            let mut lm_img = Image::new(
                bevy::render::render_resource::Extent3d {
                    width: lightmap_atlas.width,
                    height: lightmap_atlas.height,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                lightmap_atlas.rgba_bytes,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::render::render_asset::RenderAssetUsages::default(),
            );
            lm_img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::ClampToEdge,
                address_mode_v: ImageAddressMode::ClampToEdge,
                address_mode_w: ImageAddressMode::ClampToEdge,
                mag_filter: bevy::image::ImageFilterMode::Linear,
                min_filter: bevy::image::ImageFilterMode::Linear,
                ..default()
            });
            let lm_handle = images.add(lm_img);

            // 2. Create native GPU Images for each diffuse texture with Repeating wrap sampler
            let mut texture_images: HashMap<u32, Handle<Image>> = HashMap::new();
            let mut texture_cutout: HashMap<u32, bool> = HashMap::new();

            for (idx, tex_desc) in textures.iter().enumerate() {
                let base_data = tex_desc.mips.first().map_or(&[][..], |m| m.data.as_slice());

                let (mip_data, mip_count) =
                    if !base_data.is_empty() && tex_desc.width > 0 && tex_desc.height > 0 {
                        goldhub_formats::ir::generate_rgba8_mipmaps(
                            tex_desc.width,
                            tex_desc.height,
                            base_data,
                        )
                    } else {
                        (base_data.to_vec(), 1)
                    };

                let img = Image {
                    data: mip_data,
                    texture_descriptor: bevy::render::render_resource::TextureDescriptor {
                        label: None,
                        size: bevy::render::render_resource::Extent3d {
                            width: tex_desc.width.max(1),
                            height: tex_desc.height.max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: mip_count,
                        sample_count: 1,
                        dimension: bevy::render::render_resource::TextureDimension::D2,
                        format: bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                        usage: bevy::render::render_resource::TextureUsages::TEXTURE_BINDING
                            | bevy::render::render_resource::TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        address_mode_w: ImageAddressMode::Repeat,
                        mag_filter: bevy::image::ImageFilterMode::Linear,
                        min_filter: bevy::image::ImageFilterMode::Linear,
                        mipmap_filter: bevy::image::ImageFilterMode::Linear,
                        anisotropy_clamp: 16,
                        ..default()
                    }),
                    texture_view_descriptor: None,
                    asset_usage: bevy::render::render_asset::RenderAssetUsages::default(),
                };

                let img_handle = images.add(img);
                let is_cutout = tex_desc.name.starts_with('{')
                    || tex_desc
                        .mips
                        .first()
                        .is_some_and(|m| m.data.as_chunks::<4>().0.iter().any(|p| p[3] == 0));

                texture_images.insert(idx as u32, img_handle);
                texture_cutout.insert(idx as u32, is_cutout);
            }

            // Material cache keyed by (material_id, render_mode, render_amt, render_color)
            let mut material_cache: HashMap<(u32, u32, u8, [u8; 3]), Handle<StandardMaterial>> =
                HashMap::new();

            // 3. Spawn partitioned submeshes by material & render attributes with baked Lightmap component
            let partitioned = to_bevy_submeshes(&mesh_buffer);
            for part in partitioned {
                let mesh_handle = meshes.add(part.mesh);
                let key = (
                    part.material_id,
                    part.render_mode,
                    part.render_amt,
                    part.render_color,
                );

                let mat_handle = material_cache.entry(key).or_insert_with(|| {
                    let img_handle = texture_images.get(&part.material_id).cloned();
                    let is_cutout = texture_cutout
                        .get(&part.material_id)
                        .copied()
                        .unwrap_or(false);

                    if part.render_mode == 2 {
                        // GoldSrc rendermode 2 (kRenderTransTexture) with authentic renderamt alpha & rendercolor tinting
                        let alpha = f32::from(part.render_amt) / 255.0;
                        let base_col = if part.render_color == [0, 0, 0] {
                            // If rendercolor is not specified (0, 0, 0), use neutral white modulation with glass alpha
                            Color::srgba(1.0, 1.0, 1.0, alpha)
                        } else {
                            Color::srgba(
                                f32::from(part.render_color[0]) / 255.0,
                                f32::from(part.render_color[1]) / 255.0,
                                f32::from(part.render_color[2]) / 255.0,
                                alpha,
                            )
                        };

                        materials.add(StandardMaterial {
                            base_color: base_col,
                            base_color_texture: img_handle,
                            perceptual_roughness: 0.15,
                            reflectance: 0.4,
                            cull_mode: None,
                            double_sided: true,
                            alpha_mode: AlphaMode::Blend,
                            lightmap_exposure: 2.2,
                            ..default()
                        })
                    } else if is_cutout || part.render_mode == 4 {
                        materials.add(StandardMaterial {
                            base_color: Color::WHITE,
                            base_color_texture: img_handle,
                            perceptual_roughness: 0.95,
                            reflectance: 0.02,
                            cull_mode: None,
                            double_sided: true,
                            alpha_mode: AlphaMode::Mask(0.5),
                            lightmap_exposure: 2.2,
                            ..default()
                        })
                    } else {
                        materials.add(StandardMaterial {
                            base_color: Color::WHITE,
                            base_color_texture: img_handle,
                            perceptual_roughness: 0.95,
                            reflectance: 0.02,
                            cull_mode: Some(bevy::render::render_resource::Face::Back),
                            alpha_mode: AlphaMode::Opaque,
                            lightmap_exposure: 2.2,
                            ..default()
                        })
                    }
                });

                commands.spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(mat_handle.clone()),
                    bevy::pbr::Lightmap {
                        image: lm_handle.clone(),
                        ..default()
                    },
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

    // 3. Spawn Tournament First-Person Player Camera (with Spectator mode support via 'M' / 'F3')
    let (spawn_pos, spawn_yaw) = spawn_point.unwrap_or(([0.0, 0.0, 36.0], 0.0));
    let bevy_spawn_pos = Vec3::new(
        spawn_pos[0],
        spawn_pos[2] + goldhub_physics::VEC_VIEW_STANDING,
        -spawn_pos[1],
    );
    let bevy_yaw = (spawn_yaw - 90.0).to_radians();

    let mut initial_grading = bevy::render::view::ColorGrading::default();
    crate::video_settings::VideoSettings::default().apply_to_color_grading(&mut initial_grading);

    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: bevy::render::camera::ClearColorConfig::Custom(Color::srgb(
                0.12, 0.15, 0.2,
            )),
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        initial_grading,
        Transform::from_translation(bevy_spawn_pos).with_rotation(Quat::from_rotation_y(bevy_yaw)),
        crate::player::PlayerController {
            sim: goldhub_physics::FixedSubtickSimulator::new(spawn_pos, spawn_yaw),
            pitch: 0.0,
            yaw: bevy_yaw,
            mouse_sensitivity: 0.002,
            is_active: true,
        },
        SpectatorCamera {
            move_speed: 600.0,
            sprint_multiplier: 2.5,
            look_sensitivity: 0.002,
            pitch: 0.0,
            yaw: bevy_yaw,
            is_active: false,
        },
    ));
}
