//! Level loading, asset pipeline integration, and scene illumination.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use goldhub_formats::pipeline::AssetPipeline;

use crate::camera::SpectatorCamera;
use crate::mesh_bridge::to_bevy_submeshes;

pub mod bsp_material;
pub mod skybox;

const SAMPLE_BSP_PATH: &str = "D:/Repo/CS16/hlmsl/test_maps/de_inferno.bsp";
const FALLBACK_BSP_PATH: &str =
    "D:/Repo/GoldSrc.rs/references/regamedll/regamedll/extra/Test Maps/regamedll_test_map_v8.bsp";

/// Tag component for Viewmodel and Worldmodel key sunlight.
#[derive(Component)]
pub struct ViewModelSunLight;

/// Tag component for Viewmodel and Worldmodel fill ambient light.
#[derive(Component)]
pub struct ViewModelFillLight;

/// Active BSP level metadata and environment flags (e.g. Aztec weather/rain).
#[derive(Resource, Debug, Clone, Default)]
pub struct CurrentMapInfo {
    pub map_name: String,
    pub is_aztec: bool,
}

/// Cached handles to spawned BSP world materials partitioned by dynamic update needs.
#[derive(Resource, Debug, Default)]
pub struct BspDynamicMaterials {
    pub all: Vec<Handle<bsp_material::GoldSrcBspMaterial>>,
    pub water: Vec<Handle<bsp_material::GoldSrcBspMaterial>>,
}

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BspDynamicMaterials>();
        app.add_plugins(bsp_material::GoldSrcBspMaterialPlugin);
        app.add_systems(
            Startup,
            (
                setup_level,
                apply_deferred,
                crate::entity::spawn_bsp_interactive_entities,
                crate::studio::spawn_map_studio_entities,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            (
                skybox::update_skybox_position,
                update_viewmodel_dynamic_lighting_system,
                update_bsp_materials_dynamic_system,
            ),
        );
    }
}

#[allow(clippy::too_many_lines)]
pub fn setup_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bsp_materials: ResMut<Assets<bsp_material::GoldSrcBspMaterial>>,
    mut dyn_mats: ResMut<BspDynamicMaterials>,
    mut images: ResMut<Assets<Image>>,
) {
    dyn_mats.all.clear();
    dyn_mats.water.clear();
    // 1. Illumination: Soft sky ambient fill matching CS 1.6 radiosity bounce floor
    // Provides baseline ambient illumination for viewmodels and models so unlit sides
    // never drop to pitch black. (World map geometry uses GoldSrcBspMaterial and is unaffected).
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.92, 0.92, 0.96),
        brightness: 180.0,
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

    let map_stem = bsp_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_aztec = map_stem.contains("aztec");
    commands.insert_resource(CurrentMapInfo {
        map_name: map_stem,
        is_aztec,
    });

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
            commands.insert_resource(crate::studio::BspEntitiesLump(bsp.entities.to_vec()));
            // In authentic GoldSrc, map lights ('light', 'light_spot') are 100% baked into BSP lightmaps.
            // Spawning dynamic high-lumen PointLights blows out lightmaps and washes out shadows.
        }

        let mut search_dirs: Vec<std::path::PathBuf> = Vec::new();
        if let Some(dir) = bsp_path.parent() {
            search_dirs.push(dir.to_path_buf());
            if let Some(parent) = dir.parent() {
                search_dirs.push(parent.to_path_buf());
                if let Some(grandparent) = parent.parent() {
                    search_dirs.push(grandparent.to_path_buf());
                    search_dirs.push(grandparent.join("valve"));
                    search_dirs.push(grandparent.join("cstrike"));
                }
            }
        }

        let search_dirs = crate::audio::collect_game_search_dirs(&search_dirs);

        commands.insert_resource(crate::audio::GameSearchPaths {
            dirs: search_dirs.clone(),
        });

        let search_dirs_refs: Vec<&Path> = search_dirs
            .iter()
            .map(std::path::PathBuf::as_path)
            .collect();
        let pipeline = AssetPipeline::new(".cache/goldhub/assets");
        if let Ok((mesh_buffer, textures, lightmap_atlas, sky_name)) =
            pipeline.load_bsp_level_full_with_search_dirs(&raw_bytes, &search_dirs_refs)
        {
            // Build ground materials index for authentic GoldSrc footstep sounds
            if let Ok(bsp) = goldhub_formats::codecs::legacy::bsp::BspLevel::from_bytes(&raw_bytes)
            {
                let mut ground_faces = Vec::new();
                for face in bsp.faces {
                    let plane = &bsp.planes[face.plane_id as usize];
                    let sign = if face.side == 0 { 1.0 } else { -1.0 };
                    let norm = [
                        plane.normal[0] * sign,
                        plane.normal[1] * sign,
                        plane.normal[2] * sign,
                    ];
                    let dist = plane.distance * sign;

                    // Walkable floor surface threshold in GoldSrc: normal.z >= 0.7
                    if norm[2] >= 0.7 {
                        let first_edge = face.first_surfedge_id as usize;
                        let num_edges = face.surfedges_num as usize;
                        if first_edge + num_edges <= bsp.surfedges.len() {
                            let mut poly = Vec::with_capacity(num_edges);
                            let mut min_x = f32::INFINITY;
                            let mut max_x = f32::NEG_INFINITY;
                            let mut min_y = f32::INFINITY;
                            let mut max_y = f32::NEG_INFINITY;
                            let mut avg_z = 0.0;

                            for &surfedge in &bsp.surfedges[first_edge..first_edge + num_edges] {
                                let (edge_idx, reverse) = if surfedge >= 0 {
                                    (surfedge as usize, false)
                                } else {
                                    (surfedge.unsigned_abs() as usize, true)
                                };
                                if edge_idx < bsp.edges.len() {
                                    let edge = &bsp.edges[edge_idx];
                                    let v_idx = if reverse {
                                        edge.vertices[1] as usize
                                    } else {
                                        edge.vertices[0] as usize
                                    };
                                    if v_idx < bsp.vertices.len() {
                                        let v = bsp.vertices[v_idx];
                                        poly.push([v[0], v[1]]);
                                        min_x = min_x.min(v[0]);
                                        max_x = max_x.max(v[0]);
                                        min_y = min_y.min(v[1]);
                                        max_y = max_y.max(v[1]);
                                        avg_z += v[2];
                                    }
                                }
                            }

                            if poly.len() >= 3 {
                                avg_z /= poly.len() as f32;
                                let tex_info_idx = face.texture_info_id as usize;
                                if tex_info_idx < bsp.texture_infos.len() {
                                    let tex_id =
                                        bsp.texture_infos[tex_info_idx].texture_id as usize;
                                    let tex_name =
                                        textures.get(tex_id).map_or("concrete", |t| &t.name);
                                    ground_faces.push(crate::audio::GroundFace {
                                        aabb_min: [min_x, min_y],
                                        aabb_max: [max_x, max_y],
                                        plane_z: avg_z,
                                        plane_normal: norm,
                                        plane_dist: dist,
                                        vertices: poly,
                                        texture_name: tex_name.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
                tracing::info!(
                    "Indexed {} walkable ground surfaces for material footstep audio",
                    ground_faces.len()
                );
                commands.insert_resource(crate::audio::LevelGroundMaterialIndex {
                    faces: ground_faces,
                });

                let decal_faces = build_bsp_decal_faces(&bsp, &textures);
                tracing::info!(
                    "Indexed {} BSP surface polygons for clipped corner-wrapping decals",
                    decal_faces.len()
                );
                commands
                    .insert_resource(crate::sprite::LevelDecalSurfaceIndex { faces: decal_faces });
            }

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
                // GoldSrc raw luxels (0..255) must be sampled as linear UNORM without hardware sRGB decompression
                bevy::render::render_resource::TextureFormat::Rgba8Unorm,
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
            let mut texture_names: HashMap<u32, String> = HashMap::new();

            for (idx, tex_desc) in textures.iter().enumerate() {
                texture_names.insert(idx as u32, tex_desc.name.clone());
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
            let fallback_image_handle = images.add(Image::default());
            let mut material_cache: HashMap<
                (u32, u32, u8, [u8; 3]),
                Handle<bsp_material::GoldSrcBspMaterial>,
            > = HashMap::new();

            // 3. Spawn partitioned submeshes by material & render attributes with custom GoldSrc GL_MODULATE material
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
                    let img_handle = texture_images
                        .get(&part.material_id)
                        .cloned()
                        .unwrap_or_else(|| fallback_image_handle.clone());
                    let is_cutout = texture_cutout
                        .get(&part.material_id)
                        .copied()
                        .unwrap_or(false);

                    let tex_name = texture_names
                        .get(&part.material_id)
                        .map_or("", String::as_str)
                        .to_ascii_lowercase();

                    let is_glass = tex_name.contains("glass") || tex_name.contains("window");
                    let is_water = tex_name.starts_with('!')
                        || tex_name.contains("water")
                        || tex_name.contains("river");

                    let alpha_mode =
                        if (part.render_mode == 2 && is_glass) || part.render_mode == 3 || is_water
                        {
                            AlphaMode::Blend
                        } else if is_cutout || part.render_mode == 4 {
                            AlphaMode::Mask(0.5)
                        } else {
                            AlphaMode::Opaque
                        };

                    let render_mode = if is_water {
                        5
                    } else if is_cutout {
                        4
                    } else {
                        part.render_mode
                    };

                    let handle = bsp_materials.add(bsp_material::GoldSrcBspMaterial {
                        diffuse_texture: img_handle,
                        lightmap_texture: lm_handle.clone(),
                        uniform: bsp_material::GoldSrcBspMaterialUniform {
                            overbright: 2.0,
                            alpha_cutoff: 0.5,
                            render_mode,
                            render_amt: f32::from(part.render_amt),
                            flashlight_active: 0,
                            flashlight_radius: 92.0,
                            flashlight_intensity: 0.0,
                            flashlight_pos: Vec3::ZERO,
                            flashlight_dir: Vec3::Z,
                            water_alpha: 0.65,
                            time: 0.0,
                        },
                        cull_mode: if is_glass || is_water {
                            None
                        } else {
                            Some(bevy::render::render_resource::Face::Back)
                        },
                        alpha_mode,
                    });
                    dyn_mats.all.push(handle.clone());
                    if render_mode == 5 {
                        dyn_mats.water.push(handle.clone());
                    }
                    handle
                });

                let mut entity_cmds = commands.spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(mat_handle.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));

                if let Some(ref submodel) = part.submodel_id {
                    entity_cmds.insert(crate::entity::BspSubmodelMesh {
                        model: submodel.clone(),
                    });
                }
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

    commands
        .spawn((
            Camera3d::default(),
            Camera {
                clear_color: bevy::render::camera::ClearColorConfig::Custom(Color::srgb(
                    0.12, 0.15, 0.2,
                )),
                ..default()
            },
            Projection::Perspective(PerspectiveProjection {
                fov: 74.0f32.to_radians(),
                near: 1.0,
                far: 20000.0,
                ..default()
            }),
            bevy::audio::SpatialListener::new(2.0),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            initial_grading,
            bevy::render::view::RenderLayers::from_layers(&[0, 2]),
            Transform::from_translation(bevy_spawn_pos)
                .with_rotation(Quat::from_rotation_y(bevy_yaw)),
            crate::combat::PlayerWeaponInventory::default(),
            crate::combat::ActiveWeaponState::default(),
            crate::combat::PlayerRecoil::default(),
            crate::player::PlayerFlashlight::default(),
            crate::player::PlayerController {
                sim: goldhub_physics::FixedSubtickSimulator::new(spawn_pos, spawn_yaw),
                pitch: 0.0,
                yaw: bevy_yaw,
                mouse_sensitivity: 0.002,
                is_active: true,
                ..default()
            },
            SpectatorCamera {
                move_speed: 600.0,
                sprint_multiplier: 2.5,
                look_sensitivity: 0.002,
                pitch: 0.0,
                yaw: bevy_yaw,
                is_active: false,
                ..default()
            },
        ))
        .with_children(|parent| {
            // Viewmodel Key Light locked to Camera Space (upper-right)
            parent.spawn((
                ViewModelSunLight,
                DirectionalLight {
                    color: Color::srgb(1.0, 0.98, 0.92),
                    illuminance: 2800.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.4, -0.6, 0.0)),
                bevy::render::view::RenderLayers::from_layers(&[
                    crate::studio::viewmodel::VIEWMODEL_RENDER_LAYER,
                    2,
                ]),
            ));
            // Viewmodel Fill Light locked to Camera Space (lower-left, GoldSrc Half-Lambert parity)
            parent.spawn((
                ViewModelFillLight,
                DirectionalLight {
                    color: Color::srgb(0.92, 0.95, 0.98),
                    illuminance: 2000.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.5, 0.8, 0.0)),
                bevy::render::view::RenderLayers::from_layers(&[
                    crate::studio::viewmodel::VIEWMODEL_RENDER_LAYER,
                    2,
                ]),
            ));
        });
}

/// Parses entity string lumps and spawns authentic PointLight fixtures and environment lighting.
#[allow(clippy::too_many_lines, dead_code)]
fn spawn_bsp_lights(commands: &mut Commands, entities_bytes: &[u8]) {
    let Ok(entities_str) = std::str::from_utf8(entities_bytes) else {
        return;
    };

    let mut light_count = 0;
    for block in entities_str.split('}') {
        let mut classname = "";
        let mut origin_str = "";
        let mut light_str = "";
        let mut rendercolor_str = "";
        let mut renderamt = 100u32;

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"classname\"")
                && let Some(c) = trimmed.split('"').nth(3)
            {
                classname = c;
            } else if trimmed.starts_with("\"origin\"")
                && let Some(o) = trimmed.split('"').nth(3)
            {
                origin_str = o;
            } else if trimmed.starts_with("\"_light\"")
                && let Some(l) = trimmed.split('"').nth(3)
            {
                light_str = l;
            } else if trimmed.starts_with("\"rendercolor\"")
                && let Some(rc) = trimmed.split('"').nth(3)
            {
                rendercolor_str = rc;
            } else if trimmed.starts_with("\"renderamt\"")
                && let Some(ra) = trimmed.split('"').nth(3)
            {
                renderamt = ra.parse::<u32>().unwrap_or(100);
            }
        }

        if origin_str.is_empty() {
            continue;
        }

        let mut parts = origin_str.split_whitespace();
        let (Some(xs), Some(ys), Some(zs)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(fx), Ok(fy), Ok(fz)) = (xs.parse::<f32>(), ys.parse::<f32>(), zs.parse::<f32>())
        else {
            continue;
        };

        // GoldSrc (Z-up) -> Bevy (Y-up): X = X, Y = Z, Z = -Y
        let bevy_pos = Vec3::new(fx, fz, -fy);

        if classname == "light" || classname == "light_spot" || classname == "env_glow" {
            let mut r = 255.0f32;
            let mut g = 255.0f32;
            let mut b = 255.0f32;
            let mut intensity = 200.0f32;

            let color_src = if light_str.is_empty() {
                rendercolor_str
            } else {
                light_str
            };

            if !color_src.is_empty() {
                let mut cparts = color_src.split_whitespace();
                if let (Some(rs), Some(gs), Some(bs)) =
                    (cparts.next(), cparts.next(), cparts.next())
                {
                    r = rs.parse::<f32>().unwrap_or(255.0);
                    g = gs.parse::<f32>().unwrap_or(255.0);
                    b = bs.parse::<f32>().unwrap_or(255.0);
                }
                if let Some(ints) = cparts.next() {
                    intensity = ints.parse::<f32>().unwrap_or(200.0);
                } else if classname == "env_glow" {
                    intensity = (renderamt as f32) * 1.5;
                }
            }

            let color = Color::srgb(
                (r / 255.0).clamp(0.0, 1.0),
                (g / 255.0).clamp(0.0, 1.0),
                (b / 255.0).clamp(0.0, 1.0),
            );

            // Scale to Bevy lumens for unscaled GoldSrc world dimensions:
            // At GoldSrc room distance (d ~ 150-300 units), 250,000 lumens/intensity delivers ~300 lux locally
            let lumens = (intensity * 250_000.0).clamp(5_000_000.0, 60_000_000.0);
            let radius_range = (intensity * 4.0).clamp(300.0, 1000.0);

            commands.spawn((
                PointLight {
                    color,
                    intensity: lumens,
                    range: radius_range,
                    radius: 12.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(bevy_pos),
            ));
            light_count += 1;
        }
    }
    tracing::info!(
        spawned_lights = light_count,
        "Spawned GoldSrc BSP PointLights and Environment Illuminators"
    );
}

/// Dynamically adjusts Viewmodel illumination matching GoldSrc `R_LightPoint`.
///
/// Traces upward from the player origin to probe sky visibility and ceiling proximity, smoothly
/// modulating sun intensity and ambient fill between sunny courtyards and dark indoor tunnels.
#[allow(clippy::type_complexity)]
pub fn update_viewmodel_dynamic_lighting_system(
    time: Res<Time>,
    player_query: Query<&crate::player::PlayerController>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    mut sun_query: Query<
        &mut DirectionalLight,
        (With<ViewModelSunLight>, Without<ViewModelFillLight>),
    >,
    mut fill_query: Query<
        &mut DirectionalLight,
        (With<ViewModelFillLight>, Without<ViewModelSunLight>),
    >,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let Ok(player) = player_query.get_single() else {
        return;
    };
    let ori = player.sim.current.origin;
    let eye_z = ori[2] + player.sim.current.view_height;

    // Default outdoor lighting values
    let mut is_outdoor = true;
    let mut ceiling_dist = 2048.0;

    if let Some(col_res) = &collision_res
        && let Some(col) = &col_res.model
    {
        use goldhub_physics::WorldTrace;
        let bsp = goldhub_physics::BspWorldCollision::new(col);
        let start = [ori[0], ori[1], eye_z];
        let end = [ori[0], ori[1], eye_z + 2048.0];
        let tr = bsp.trace_player_hull(start, end, 0);
        if !tr.is_sky && tr.fraction < 0.99 {
            is_outdoor = false;
            ceiling_dist = (tr.endpos[2] - eye_z).max(32.0);
        }
    }

    let (target_sun, target_fill) = if is_outdoor {
        (2800.0, 2000.0)
    } else {
        // In indoor corridors / ducts (like de_nuke vents with low ceilings ~64 units),
        // scale lighting down authentically without dropping into silhouette darkness (GoldSrc ambient floor ~0.18).
        let factor = (ceiling_dist / 384.0).clamp(0.18, 0.90);
        (2200.0 * factor, 1600.0 * factor)
    };

    let blend_speed = (dt * 8.0).min(1.0);

    for mut sun in &mut sun_query {
        sun.illuminance += (target_sun - sun.illuminance) * blend_speed;
    }
    for mut fill in &mut fill_query {
        fill.illuminance += (target_fill - fill.illuminance) * blend_speed;
    }
}

fn build_bsp_decal_faces(
    bsp: &goldhub_formats::codecs::legacy::bsp::BspLevel<'_>,
    textures: &[goldhub_formats::ir::TextureDescriptor],
) -> Vec<crate::sprite::BspDecalFace> {
    let mut out = Vec::with_capacity(bsp.faces.len());
    for (model_index, model) in bsp.models.iter().enumerate() {
        let first_face = model.first_face as usize;
        let num_faces = model.num_faces as usize;
        if first_face + num_faces > bsp.faces.len() {
            continue;
        }
        for face in &bsp.faces[first_face..first_face + num_faces] {
            let tex_info_idx = face.texture_info_id as usize;
            let skip = bsp.texture_infos.get(tex_info_idx).is_some_and(|ti| {
                textures.get(ti.texture_id as usize).is_some_and(|t| {
                    let n = t.name.to_ascii_lowercase();
                    n == "sky"
                        || n == "aaatrigger"
                        || n == "clip"
                        || n == "origin"
                        || n == "{invisible"
                        || n.starts_with('*')
                        || n.starts_with('!')
                })
            });
            if skip {
                continue;
            }
            let Some(plane) = bsp.planes.get(face.plane_id as usize) else {
                continue;
            };
            let sign = if face.side == 0 { 1.0 } else { -1.0 };
            let normal = Vec3::new(
                plane.normal[0] * sign,
                plane.normal[2] * sign,
                -plane.normal[1] * sign,
            )
            .normalize_or_zero();
            if normal.length_squared() < 0.5 {
                continue;
            }

            let first_edge = face.first_surfedge_id as usize;
            let num_edges = face.surfedges_num as usize;
            if first_edge + num_edges > bsp.surfedges.len() {
                continue;
            }

            let mut vertices = Vec::with_capacity(num_edges);
            let mut aabb_min = Vec3::splat(f32::INFINITY);
            let mut aabb_max = Vec3::splat(f32::NEG_INFINITY);

            for &surfedge in &bsp.surfedges[first_edge..first_edge + num_edges] {
                let (edge_idx, reverse) = if surfedge >= 0 {
                    (surfedge as usize, false)
                } else {
                    (surfedge.unsigned_abs() as usize, true)
                };
                if let Some(edge) = bsp.edges.get(edge_idx) {
                    let v_idx = if reverse {
                        edge.vertices[1] as usize
                    } else {
                        edge.vertices[0] as usize
                    };
                    if let Some(v) = bsp.vertices.get(v_idx) {
                        let bv = Vec3::new(v[0], v[2], -v[1]);
                        aabb_min = aabb_min.min(bv);
                        aabb_max = aabb_max.max(bv);
                        vertices.push(bv);
                    }
                }
            }

            if vertices.len() >= 3 {
                let plane_dist = vertices[0].dot(normal);
                out.push(crate::sprite::BspDecalFace {
                    model_index,
                    aabb_min,
                    aabb_max,
                    normal,
                    plane_dist,
                    vertices,
                });
            }
        }
    }
    out
}

#[derive(Default)]
pub struct BspShaderSyncCache {
    initialized: bool,
    active: u32,
    pos: Vec3,
    dir: Vec3,
    intensity: f32,
    water_alpha: f32,
}

/// Updates dynamic shader parameters across active BSP world materials:
/// - Real-time GoldSrc flashlight dynamic lighting (`CL_UpdateFlashlight` trace + `R_AddDynamicLights`).
/// - Real-time animated water sine ripples (`r_wateralpha` parity).
#[allow(clippy::too_many_arguments)]
pub fn update_bsp_materials_dynamic_system(
    time: Res<Time>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    player_query: Query<
        (&GlobalTransform, &crate::player::PlayerFlashlight),
        With<crate::player::PlayerController>,
    >,
    video_settings: Option<Res<crate::video_settings::VideoSettings>>,
    dyn_mats: Res<BspDynamicMaterials>,
    mut cache: Local<BspShaderSyncCache>,
    mut bsp_materials: ResMut<Assets<bsp_material::GoldSrcBspMaterial>>,
) {
    const FLASHLIGHT_MAX_DIST: f32 = 2048.0;
    const FLASHLIGHT_DLIGHT_RADIUS: f32 = 92.0;

    let t = time.elapsed_secs();
    let (active, pos, dir, intensity) = if let Ok((gt, flash)) = player_query.get_single()
        && flash.is_on
    {
        let eye_pos = gt.translation();
        let forward = gt.forward().as_vec3();
        if let Some(col_res) = collision_res.as_ref()
            && let Some(ref col) = col_res.model
        {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col);
            let end = eye_pos + forward * FLASHLIGHT_MAX_DIST;
            let tr = bsp.trace_player_hull(
                [eye_pos.x, -eye_pos.z, eye_pos.y],
                [end.x, -end.z, end.y],
                0,
            );
            if tr.fraction < 1.0 && !tr.is_sky {
                let hit_pos = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);
                let norm = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                    .normalize_or_zero();
                let hit_dist = tr.fraction * FLASHLIGHT_MAX_DIST;
                let falloff = if hit_dist < 500.0 {
                    1.0
                } else {
                    let ratio = 500.0 / hit_dist;
                    ratio * ratio
                };
                (1u32, hit_pos + norm * 3.0, forward, 0.72 * falloff)
            } else {
                (0u32, Vec3::ZERO, Vec3::Z, 0.0)
            }
        } else {
            (0u32, Vec3::ZERO, Vec3::Z, 0.0)
        }
    } else {
        (0u32, Vec3::ZERO, Vec3::Z, 0.0)
    };

    let water_alpha = video_settings.map_or(0.65, |vs| vs.water_alpha);
    let global_changed = !cache.initialized
        || cache.active != active
        || (cache.water_alpha - water_alpha).abs() > 1e-4
        || (active == 1
            && (cache.pos.distance_squared(pos) > 0.01
                || cache.dir.distance_squared(dir) > 1e-5
                || (cache.intensity - intensity).abs() > 1e-3));

    if global_changed {
        cache.initialized = true;
        cache.active = active;
        cache.pos = pos;
        cache.dir = dir;
        cache.intensity = intensity;
        cache.water_alpha = water_alpha;

        for handle in &dyn_mats.all {
            if let Some(mat) = bsp_materials.get_mut(handle) {
                mat.uniform.time = t;
                mat.uniform.flashlight_active = active;
                mat.uniform.water_alpha = water_alpha;
                if active == 1 {
                    mat.uniform.flashlight_pos = pos;
                    mat.uniform.flashlight_dir = dir;
                    mat.uniform.flashlight_radius = FLASHLIGHT_DLIGHT_RADIUS;
                    mat.uniform.flashlight_intensity = intensity;
                }
            }
        }
    } else {
        // Only water materials need per-frame ripple `time` updates when flashlight & settings are idle.
        for handle in &dyn_mats.water {
            if let Some(mat) = bsp_materials.get_mut(handle) {
                mat.uniform.time = t;
            }
        }
    }
}
