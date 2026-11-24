//! GoldSrc Sprite (`.spr`) and Decal runtime for GoldHub.
//!
//! Provides authentic CS 1.6:
//! - Muzzle flashes (`sprites/muzzleflash1.spr`)
//! - Blood impacts (`sprites/blood.spr`)
//! - Explosions (`sprites/zerogxplode.spr`)
//! - Bullet hole decals on walls (`{shot1`..`{shot5` from `decals.wad`)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::RenderLayers;
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{SpriteData, SpriteDecoder, Wad3Decoder};

use crate::audio::GameSearchPaths;
use crate::studio::viewmodel::VIEWMODEL_RENDER_LAYER;

/// Cached GPU-ready sprite data.
#[derive(Clone)]
pub struct SpriteAsset {
    pub materials: Vec<Handle<StandardMaterial>>,
    pub width: u32,
    pub height: u32,
    pub tex_format: u32,
}

/// Resource managing decoded sprites and materials.
#[derive(Resource, Default)]
pub struct SpriteStore {
    pub sprites: HashMap<String, SpriteAsset>,
    pub decals: HashMap<String, Handle<StandardMaterial>>,
    pub decals_wad_cache: Option<Vec<u8>>,
    pub unit_quad_mesh: Option<Handle<Mesh>>,
    pub spark_streak_mesh: Option<Handle<Mesh>>,
    pub spark_mat_bright: Option<Handle<StandardMaterial>>,
    pub spark_mat_amber: Option<Handle<StandardMaterial>>,
    pub smoke_cloud_mesh: Option<Handle<Mesh>>,
    pub spark_dot_mat: Option<Handle<StandardMaterial>>,
    pub brass_mesh: Option<Handle<Mesh>>,
    pub brass_mat: Option<Handle<StandardMaterial>>,
    pub unit_sphere_mesh: Option<Handle<Mesh>>,
    pub blood_mat: Option<Handle<StandardMaterial>>,
    pub dust_puff_mat: Option<Handle<StandardMaterial>>,
    pub bullet_tracer_mat: Option<Handle<StandardMaterial>>,
    pub grenade_fallback_mat: Option<Handle<StandardMaterial>>,
    pub flashlight_mat: Option<Handle<StandardMaterial>>,
}

impl SpriteStore {
    pub fn get_or_create_unit_quad(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        if let Some(ref handle) = self.unit_quad_mesh {
            handle.clone()
        } else {
            let handle = meshes.add(Rectangle::new(1.0, 1.0).mesh());
            self.unit_quad_mesh = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_spark_streak_mesh(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        if let Some(ref handle) = self.spark_streak_mesh {
            handle.clone()
        } else {
            let handle = meshes.add(Rectangle::new(1.4, 5.5).mesh());
            self.spark_streak_mesh = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_smoke_cloud_mesh(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        if let Some(ref handle) = self.smoke_cloud_mesh {
            handle.clone()
        } else {
            let handle = meshes.add(Rectangle::new(210.0, 210.0).mesh());
            self.smoke_cloud_mesh = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_brass_mesh_and_mat(
        &mut self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> (Handle<Mesh>, Handle<StandardMaterial>) {
        let mesh = self
            .brass_mesh
            .get_or_insert_with(|| meshes.add(Cylinder::new(0.6, 2.8).mesh()))
            .clone();
        let mat = self
            .brass_mat
            .get_or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb(0.92, 0.76, 0.22),
                    metallic: 0.90,
                    perceptual_roughness: 0.25,
                    ..default()
                })
            })
            .clone();
        (mesh, mat)
    }

    pub fn get_or_create_spark_dot_mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        dot_tex: Option<Handle<Image>>,
        base_color: Color,
        emissive: LinearRgba,
    ) -> Handle<StandardMaterial> {
        self.spark_dot_mat
            .get_or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color,
                    base_color_texture: dot_tex,
                    emissive,
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone()
    }

    pub fn get_or_create_unit_sphere(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        if let Some(ref handle) = self.unit_sphere_mesh {
            handle.clone()
        } else {
            let handle = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
            self.unit_sphere_mesh = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_blood_mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if let Some(ref handle) = self.blood_mat {
            handle.clone()
        } else {
            let mat = StandardMaterial {
                base_color: Color::srgb(0.55, 0.02, 0.02),
                unlit: true,
                ..default()
            };
            let handle = materials.add(mat);
            self.blood_mat = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_dust_puff_mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if let Some(ref handle) = self.dust_puff_mat {
            handle.clone()
        } else {
            let mat = StandardMaterial {
                base_color: Color::srgba(0.85, 0.82, 0.78, 0.35),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            };
            let handle = materials.add(mat);
            self.dust_puff_mat = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_bullet_tracer_mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if let Some(ref handle) = self.bullet_tracer_mat {
            handle.clone()
        } else {
            let mat = StandardMaterial {
                base_color: Color::srgba(1.0, 0.9, 0.4, 0.8),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            };
            let handle = materials.add(mat);
            self.bullet_tracer_mat = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_grenade_mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if let Some(ref handle) = self.grenade_fallback_mat {
            handle.clone()
        } else {
            let mat = StandardMaterial {
                base_color: Color::srgb(0.2, 0.35, 0.15),
                perceptual_roughness: 0.8,
                ..default()
            };
            let handle = materials.add(mat);
            self.grenade_fallback_mat = Some(handle.clone());
            handle
        }
    }

    pub fn get_or_create_flashlight_mat(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        const DIM: usize = 64;
        if let Some(ref handle) = self.flashlight_mat {
            return handle.clone();
        }
        let mut data = Vec::with_capacity(DIM * DIM * 4);
        let center = (DIM as f32 - 1.0) * 0.5;
        let radius = center;
        for y in 0..DIM {
            for x in 0..DIM {
                let dx = (x as f32 - center) / radius;
                let dy = (y as f32 - center) / radius;
                let dist = dx.hypot(dy);
                if dist >= 1.0 {
                    data.extend_from_slice(&[0, 0, 0, 0]);
                } else {
                    let t = (1.0 - dist).clamp(0.0, 1.0);
                    let smooth = t * t * (3.0 - 2.0 * t);
                    let a = (smooth * 240.0) as u8;
                    data.extend_from_slice(&[255, 250, 235, a]);
                }
            }
        }
        let image = Image::new(
            bevy::render::render_resource::Extent3d {
                width: DIM as u32,
                height: DIM as u32,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::render::render_asset::RenderAssetUsages::default(),
        );
        let img_handle = images.add(image);
        let mat = StandardMaterial {
            base_color_texture: Some(img_handle),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            depth_bias: 200.0,
            ..default()
        };
        let mat_handle = materials.add(mat);
        self.flashlight_mat = Some(mat_handle.clone());
        mat_handle
    }
    /// Loads or retrieves a cached GoldSrc sprite by relative path (e.g. "sprites/muzzleflash1.spr").
    pub fn get_or_load_sprite(
        &mut self,
        sprite_path: &str,
        search_paths: &GameSearchPaths,
        images: &mut ResMut<Assets<Image>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> Option<SpriteAsset> {
        let key = sprite_path.replace('\\', "/").to_lowercase();
        if let Some(cached) = self.sprites.get(&key) {
            return Some(cached.clone());
        }

        let file_path = resolve_file_path(sprite_path, search_paths)?;
        let raw_bytes = fs::read(&file_path).ok()?;
        let decoder = SpriteDecoder;
        let sprite: SpriteData = decoder.decode(&raw_bytes).ok()?;

        let is_blood = key.contains("blood");
        let mut mat_handles = Vec::with_capacity(sprite.frames.len());
        for frame in &sprite.frames {
            let mut rgba_pixels = frame.rgba_pixels.clone();
            if is_blood {
                for pixel in rgba_pixels.as_chunks_mut::<4>().0 {
                    if pixel[3] > 0 {
                        // Tint grayscale blood sprite to iconic CS 1.6 deep arterial red
                        let lum =
                            (u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])) / 3;
                        pixel[0] = ((lum * 200) / 255) as u8;
                        pixel[1] = ((lum * 20) / 255) as u8;
                        pixel[2] = ((lum * 20) / 255) as u8;
                    }
                }
            }

            let image = Image::new(
                Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                rgba_pixels,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            let img_handle = images.add(image);

            let is_wall_puff = key.contains("wall_puff");
            let alpha_mode = if is_wall_puff {
                AlphaMode::Blend
            } else {
                match sprite.tex_format {
                    1 => AlphaMode::Add,       // SPR_ADDITIVE
                    3 => AlphaMode::Mask(0.5), // SPR_ALPHTEST
                    0 => AlphaMode::Opaque,    // SPR_NORMAL
                    _ => AlphaMode::Blend,     // SPR_INDEXALPHA
                }
            };

            let mat = StandardMaterial {
                base_color: if is_wall_puff {
                    Color::srgba(0.85, 0.85, 0.85, 0.40)
                } else {
                    Color::WHITE
                },
                base_color_texture: Some(img_handle),
                cull_mode: None,
                unlit: true,
                alpha_mode,
                ..default()
            };
            mat_handles.push(materials.add(mat));
        }

        let asset = SpriteAsset {
            materials: mat_handles,
            width: sprite.max_width,
            height: sprite.max_height,
            tex_format: sprite.tex_format,
        };

        self.sprites.insert(key, asset.clone());
        Some(asset)
    }

    /// Loads or retrieves a bullet decal texture from `decals.wad`.
    pub fn get_or_load_decal(
        &mut self,
        decal_name: &str,
        search_paths: &GameSearchPaths,
        images: &mut ResMut<Assets<Image>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> Option<Handle<StandardMaterial>> {
        let key = decal_name.to_lowercase();
        if let Some(cached) = self.decals.get(&key) {
            return Some(cached.clone());
        }

        if self.decals_wad_cache.is_none()
            && let Some(wad_path) = resolve_file_path("decals.wad", search_paths)
            && let Ok(bytes) = fs::read(&wad_path)
        {
            self.decals_wad_cache = Some(bytes);
        }

        let wad_bytes = self.decals_wad_cache.as_ref()?;
        let wad = Wad3Decoder.decode(wad_bytes).ok()?;
        let entry = wad.find_entry(decal_name)?;
        let tex_desc = wad.unpack_miptex(entry).ok()?;

        let image = Image::new(
            Extent3d {
                width: tex_desc.width,
                height: tex_desc.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            tex_desc.mips.into_iter().next()?.data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let img_handle = images.add(image);

        let mat = StandardMaterial {
            base_color_texture: Some(img_handle),
            cull_mode: None,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 1000.0,
            ..default()
        };
        let mat_handle = materials.add(mat);
        self.decals.insert(key, mat_handle.clone());
        Some(mat_handle)
    }
}

/// Component attached to animated billboard sprites.
#[derive(Component, Debug, Clone)]
pub struct AnimatedSprite {
    pub materials: Vec<Handle<StandardMaterial>>,
    pub current_frame: usize,
    pub timer: f32,
    pub frame_duration: f32,
    pub is_looping: bool,
}

/// Marker component for viewmodel-attached sprites whose local orientation already faces the overlay camera.
#[derive(Component, Debug)]
pub struct StickyViewModelSprite;

/// Surface decal category matching GoldSrc `DECAL_*` tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactDecalKind {
    BulletShot,
    BreakableGlass,
    UnbreakableGlass,
}

/// Fast flying golden-orange metallic impact spark particle (`R_BulletImpactParticles` / `R_SparkStreaks` parity).
#[derive(Component, Debug, Clone)]
pub struct SparkStreakParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Drifting, expanding, fading smoke cloud particle (`EV_CreateSmoke` & nozzle wisps parity).
#[derive(Component, Debug, Clone)]
pub struct SmokeBillboardParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub hold_fade_at: f32,
    pub start_scale: f32,
    pub end_scale: f32,
    pub grow_duration: f32,
    pub base_rgb: [f32; 3],
    pub base_alpha: f32,
}

/// Component tagging world decals with position, surface normal, fade-out timer, and optional clipped mesh handle.
#[derive(Component, Debug, Clone)]
pub struct BulletHoleDecal {
    pub position: Vec3,
    pub normal: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub custom_mesh: Option<Handle<Mesh>>,
}

/// Convex BSP face polygon used for GoldSrc `R_DecalPoly` (Sutherland-Hodgman) decal clipping.
#[derive(Debug, Clone)]
pub struct BspDecalFace {
    pub model_index: usize,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub normal: Vec3,
    pub plane_dist: f32,
    pub vertices: Vec<Vec3>,
}

/// Level-wide spatial index of BSP surfaces for edge-clipped and corner-wrapping decals.
#[derive(Resource, Default, Debug, Clone)]
pub struct LevelDecalSurfaceIndex {
    pub faces: Vec<BspDecalFace>,
}

/// Despawns a decal entity and releases any custom edge-clipped `Mesh` handle from GPU assets.
pub fn despawn_decal_entity(
    commands: &mut Commands,
    meshes: Option<&mut Assets<Mesh>>,
    entity: Entity,
    decal: &BulletHoleDecal,
) {
    if let (Some(mesh_assets), Some(mesh_handle)) = (meshes, decal.custom_mesh.as_ref()) {
        mesh_assets.remove(mesh_handle);
    }
    commands.entity(entity).despawn();
}

/// Marker component for camera-facing billboard quad sprites (e.g. embers, static puffs, sparks).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct BillboardSprite;

/// Upward floating motion for smoke puff sprites matching GoldSrc `R_Sprite_Smoke` (+30 u/s).
#[derive(Component)]
pub struct RisingSmokeSprite {
    pub speed: f32,
}

/// High-velocity yellow/gold sparks for HE explosions matching GoldSrc `R_FlickerParticles` (`pt_blob2`).
#[derive(Component)]
pub struct FlickerParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Gunshot wall smoke puff particle with GoldSrc `EV_WallPuff_Wind` gas physics.
#[derive(Component)]
pub struct WallPuffParticle {
    pub velocity: Vec3,
    pub x_wind_direction: bool,
    pub y_wind_direction: bool,
    pub x_wind_magnitude: f32,
    pub y_wind_magnitude: f32,
    pub frame_counter: f32,
}

/// Falling rain streak particle (`env_rain` / `cl_weather` Aztec weather parity).
#[derive(Component, Debug, Clone)]
pub struct RainStreakParticle {
    pub velocity: Vec3,
    pub ground_y: f32,
}

/// Expanding floor/water ripple splash particle from rain collision.
#[derive(Component, Debug, Clone)]
pub struct RainSplashParticle {
    pub timer: f32,
    pub max_lifetime: f32,
    pub start_scale: f32,
    pub end_scale: f32,
}

/// Weather configuration and pre-allocated asset handles for zero-allocation rain simulation.
#[derive(Resource, Default)]
pub struct WeatherRainStore {
    pub streak_mesh: Option<Handle<Mesh>>,
    pub streak_mat: Option<Handle<StandardMaterial>>,
    pub splash_mesh: Option<Handle<Mesh>>,
    pub splash_mat: Option<Handle<StandardMaterial>>,
    pub rng_seed: u32,
}

/// Spawns animated CS 1.6 blood spray and impact sprites facing the camera.
pub fn spawn_blood_sprite(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    // 1. Initial high-priority blood spray burst (sprites/bloodspray.spr)
    if let Some(spray) =
        sprite_store.get_or_load_sprite("sprites/bloodspray.spr", search_paths, images, materials)
        && let Some(first_mat) = spray.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin).with_scale(Vec3::splat(26.0)),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: spray.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 1.0 / 28.0, // 28 FPS
                is_looping: false,
            },
        ));
    }

    // 2. Lingering splattered blood drop (sprites/blood.spr)
    if let Some(blood) =
        sprite_store.get_or_load_sprite("sprites/blood.spr", search_paths, images, materials)
        && let Some(first_mat) = blood.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin + Vec3::new(0.0, 1.5, 0.0))
                .with_scale(Vec3::splat(16.0)),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: blood.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 1.0 / 22.0, // 22 FPS
                is_looping: false,
            },
        ));
    }
}

/// Spawns a CS 1.6 muzzle flash billboard sprite at an explicit world-space barrel coordinate.
#[allow(clippy::too_many_arguments)]
pub fn spawn_muzzleflash_at(
    commands: &mut Commands,
    barrel_pos: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let Some(sprite) = sprite_store.get_or_load_sprite(
        "sprites/muzzleflash1.spr",
        search_paths,
        images,
        materials,
    ) else {
        return;
    };

    let total_frames = sprite.materials.len().max(1);
    let frame_idx = ((barrel_pos.x.abs() * 100.0) as usize) % total_frames;
    let Some(mat) = sprite
        .materials
        .get(frame_idx)
        .cloned()
        .or_else(|| sprite.materials.first().cloned())
    else {
        return;
    };
    let quad = sprite_store.get_or_create_unit_quad(meshes);

    commands.spawn((
        Mesh3d(quad),
        MeshMaterial3d(mat.clone()),
        Transform::from_translation(barrel_pos).with_scale(Vec3::splat(14.0)),
        bevy::render::view::NoFrustumCulling,
        AnimatedSprite {
            materials: vec![mat],
            current_frame: 0,
            timer: 0.0,
            frame_duration: 0.05, // 0.05s flash
            is_looping: false,
        },
    ));
}

/// Spawns a CS 1.6 muzzle flash billboard attached directly as a child of the ViewModel root
/// on `VIEWMODEL_RENDER_LAYER`, sticking cleanly to barrel motion across sprinting/jumping.
#[allow(clippy::too_many_arguments)]
pub fn spawn_muzzleflash_attached(
    commands: &mut Commands,
    vm_root: Entity,
    right_handed: bool,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let Some(sprite) = sprite_store.get_or_load_sprite(
        "sprites/muzzleflash1.spr",
        search_paths,
        images,
        materials,
    ) else {
        return;
    };

    let Some(mat) = sprite.materials.first().cloned() else {
        return;
    };
    let quad = sprite_store.get_or_create_unit_quad(meshes);

    // Remember `vm_root` carries `scale = Vec3::new(-1.0, 1.0, 1.0)` when right_handed!
    // In `vm_root` local space, `x = -6.0` * scale.x(-1.0) places muzzle +6.0 right in camera space.
    let local_x = if right_handed { -6.0 } else { 6.0 };
    let flash_child = commands
        .spawn((
            Mesh3d(quad),
            MeshMaterial3d(mat),
            Transform::from_translation(Vec3::new(local_x, -4.5, -28.0))
                .with_scale(Vec3::splat(12.0)),
            RenderLayers::layer(VIEWMODEL_RENDER_LAYER),
            StickyViewModelSprite,
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: sprite.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 0.025, // crisp 50ms 2-frame flash
                is_looping: false,
            },
        ))
        .id();
    commands.entity(vm_root).add_child(flash_child);
}

/// Spawns authentic CS 1.6 twin HE fireball sprites (`fexplo.spr` + `eexplo.spr`, `ggrenade.cpp:268-287`).
pub fn spawn_explosion_sprite(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    // Primary fireball (scale ~2.5-3.0 -> 280x280)
    if let Some(sprite) = sprite_store
        .get_or_load_sprite("sprites/fexplo.spr", search_paths, images, materials)
        .or_else(|| {
            sprite_store.get_or_load_sprite(
                "sprites/zerogxplode.spr",
                search_paths,
                images,
                materials,
            )
        })
        && let Some(first_mat) = sprite.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin + Vec3::Y * 45.0).with_scale(Vec3::splat(280.0)),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: sprite.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 1.0 / 28.0,
                is_looping: false,
            },
        ));
    }

    // Secondary offset plume (`eexplo.spr` at +320 scale)
    if let Some(sprite2) =
        sprite_store.get_or_load_sprite("sprites/eexplo.spr", search_paths, images, materials)
        && let Some(first_mat2) = sprite2.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(first_mat2),
            Transform::from_translation(origin + Vec3::new(14.0, 75.0, -10.0))
                .with_scale(Vec3::splat(320.0)),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: sprite2.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 1.0 / 26.0,
                is_looping: false,
            },
        ));
    }
}

/// Spawns 20 billowing CS 1.6 smoke cloud billboards when a smoke grenade deploys (`sprites/gas_puff_01.spr`, 22s).
pub fn spawn_smoke_cloud(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let spr_opt =
        sprite_store.get_or_load_sprite("sprites/gas_puff_01.spr", search_paths, images, materials);
    let template_mats: Vec<Handle<StandardMaterial>> = spr_opt
        .as_ref()
        .map_or_else(Vec::new, |s| s.materials.clone());

    let tex_handle = template_mats
        .first()
        .and_then(|h| materials.get(h).and_then(|m| m.base_color_texture.clone()));

    let mesh = sprite_store.get_or_create_smoke_cloud_mesh(meshes);
    let mut seed = origin.x.to_bits()
        ^ origin.y.to_bits().rotate_left(13)
        ^ origin.z.to_bits().rotate_left(25);
    let mut rand_unit = || -> f32 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (((seed & 0xFFFF) as f32) / 65536.0) * 2.0 - 1.0
    };

    // CS 1.6 EV_CreateSmoke 1:1 parity:
    // 20 billowing animated clouds clustered in [-100, 100] at origin.z + 30,
    // gentle drift ([-5, 5] u/s), 15s hold at full opacity, then 14s smooth fade out (29s total).
    for i in 0..20 {
        let offset = if i == 0 {
            Vec3::new(0.0, 30.0, 0.0)
        } else {
            Vec3::new(
                rand_unit() * 100.0,
                30.0 + (i % 5) as f32 * 8.0 + rand_unit() * 6.0,
                rand_unit() * 100.0,
            )
        };
        let drift_vel = Vec3::new(
            rand_unit() * 5.0,
            1.2 + rand_unit() * 1.5,
            rand_unit() * 5.0,
        );
        let gray = (210.0 + (i % 5) as f32 * 5.0) / 255.0;
        let cloud_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(gray, gray, gray, 0.92),
            base_color_texture: tex_handle.clone(),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let anim_mats = if template_mats.len() > 1 {
            template_mats.clone()
        } else {
            vec![cloud_mat.clone()]
        };

        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(cloud_mat),
            Transform::from_translation(origin + offset).with_scale(Vec3::splat(0.65)),
            bevy::render::view::NoFrustumCulling,
            SmokeBillboardParticle {
                velocity: drift_vel,
                lifetime: 0.0,
                max_lifetime: 29.0,
                hold_fade_at: 15.0,
                start_scale: 0.65,
                end_scale: 1.45,
                grow_duration: 2.2,
                base_rgb: [gray, gray, gray],
                base_alpha: 0.92,
            },
            AnimatedSprite {
                materials: anim_mats,
                current_frame: (i * 3) % template_mats.len().max(1),
                timer: 0.0,
                frame_duration: 1.0 / 18.0,
                is_looping: true,
            },
        ));
    }
}

fn make_spark_material(
    materials: &mut Assets<StandardMaterial>,
    dot_tex: Option<Handle<Image>>,
    base_color: Color,
    emissive: LinearRgba,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color,
        base_color_texture: dot_tex,
        emissive,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    })
}

/// Spawns translucent additive GoldSrc ricochet + flying spark streaks (`TE_SPARKS` + `richo1.spr` + `wall_puff`).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn spawn_wall_sparks(
    commands: &mut Commands,
    hit_pos: Vec3,
    wall_normal: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    // 1. Ricochet flash (`sprites/richo1.spr`, additive translucent)
    if let Some(richo) =
        sprite_store.get_or_load_sprite("sprites/richo1.spr", search_paths, images, materials)
        && let Some(mat) = richo.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(hit_pos + wall_normal * 1.2).with_scale(Vec3::splat(14.0)),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: vec![mat],
                current_frame: 0,
                timer: 0.0,
                frame_duration: 0.08,
                is_looping: false,
            },
        ));
    }

    // 2. Crisp additive `dot.spr` texture for transparent spark streaks (fallback translucent unlit)
    let dot_tex = sprite_store
        .get_or_load_sprite("sprites/dot.spr", search_paths, images, materials)
        .and_then(|s| s.materials.first().cloned())
        .and_then(|h| materials.get(&h).and_then(|m| m.base_color_texture.clone()));

    let streak_mesh = sprite_store.get_or_create_spark_streak_mesh(meshes);
    let mat_bright = if let Some(ref h) = sprite_store.spark_mat_bright {
        h.clone()
    } else {
        let h = make_spark_material(
            materials,
            dot_tex.clone(),
            Color::srgba(1.0, 0.90, 0.45, 0.40),
            LinearRgba::rgb(2.2, 1.7, 0.55),
        );
        sprite_store.spark_mat_bright = Some(h.clone());
        h
    };
    let mat_amber = if let Some(ref h) = sprite_store.spark_mat_amber {
        h.clone()
    } else {
        let h = make_spark_material(
            materials,
            dot_tex,
            Color::srgba(1.0, 0.60, 0.15, 0.35),
            LinearRgba::rgb(1.8, 0.9, 0.2),
        );
        sprite_store.spark_mat_amber = Some(h.clone());
        h
    };

    let up_ref = if wall_normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = wall_normal.cross(up_ref).normalize_or_zero();
    let bitangent = wall_normal.cross(tangent).normalize_or_zero();

    let mut seed = hit_pos.x.to_bits()
        ^ hit_pos.y.to_bits().rotate_left(11)
        ^ hit_pos.z.to_bits().rotate_left(23);
    let mut next_f32 = || -> f32 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        ((seed & 0xFFFF) as f32) / 65536.0
    };

    // GoldSrc R_StreakSplash: spawn 16 airy spark streaks
    for i in 0..16 {
        let lat_a = (next_f32() - 0.5) * 2.0;
        let lat_b = (next_f32() - 0.5) * 2.0;
        let out = 0.5 + next_f32() * 1.1;
        let speed = 90.0 + next_f32() * 190.0;
        let dir = (wall_normal * out + tangent * lat_a + bitangent * lat_b).normalize_or_zero();
        let vel = dir * speed;
        let rot = if vel.length_squared() > 1e-3 {
            Quat::from_rotation_arc(Vec3::Y, vel.normalize())
        } else {
            Quat::IDENTITY
        };

        commands.spawn((
            Mesh3d(streak_mesh.clone()),
            MeshMaterial3d(if i % 2 == 0 {
                mat_bright.clone()
            } else {
                mat_amber.clone()
            }),
            Transform::from_translation(hit_pos + wall_normal * 1.0)
                .with_rotation(rot)
                .with_scale(Vec3::new(0.6, 1.0, 0.6)),
            SparkStreakParticle {
                velocity: vel,
                lifetime: 0.0,
                max_lifetime: 0.14 + next_f32() * 0.16,
            },
        ));
    }

    // 3. Subtle `wall_puff1..4.spr` gunshot smoke puff (`EV_CS16Client_CreateSmoke` with `EV_WallPuff_Wind`)
    let puff_idx = ((hit_pos.x.abs() * 3.0 + hit_pos.z.abs()) as usize % 4) + 1;
    let puff_path = format!("sprites/wall_puff{puff_idx}.spr");
    if let Some(puff) = sprite_store.get_or_load_sprite(&puff_path, search_paths, images, materials)
        && let Some(first_mat) = puff.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(first_mat),
            Transform::from_translation(hit_pos + wall_normal * 1.5).with_scale(Vec3::splat(10.0)),
            bevy::render::view::NoFrustumCulling,
            BillboardSprite,
            AnimatedSprite {
                materials: puff.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 1.0 / 30.0,
                is_looping: false,
            },
            WallPuffParticle {
                velocity: wall_normal * 8.0 + Vec3::Y * 6.0,
                x_wind_direction: true,
                y_wind_direction: true,
                x_wind_magnitude: 0.0,
                y_wind_magnitude: 0.0,
                frame_counter: 0.0,
            },
        ));
    }
}

/// Spawns an authentic black scorch crater decal from `decals.wad` (`{scorch1`..`{scorch3`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_scorch_decal(
    commands: &mut Commands,
    hit_pos: Vec3,
    surface_normal: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let scorch_num = ((hit_pos.x.abs() * 7.0 + hit_pos.z.abs() * 13.0) as usize % 3) + 1;
    let decal_name = format!("{{scorch{scorch_num}");

    let Some(mat) = sprite_store.get_or_load_decal(&decal_name, search_paths, images, materials)
    else {
        return;
    };

    let quad = sprite_store.get_or_create_unit_quad(meshes);
    let decal_pos = hit_pos + surface_normal * 0.06;
    let rot = if surface_normal.dot(Vec3::Z).abs() > 0.999 {
        if surface_normal.z > 0.0 {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_y(std::f32::consts::PI)
        }
    } else {
        Quat::from_rotation_arc(Vec3::Z, surface_normal)
    };

    commands.spawn((
        Mesh3d(quad),
        MeshMaterial3d(mat),
        Transform::from_translation(decal_pos)
            .with_rotation(rot)
            .with_scale(Vec3::splat(112.0)),
        BulletHoleDecal {
            position: decal_pos,
            normal: surface_normal,
            lifetime: 0.0,
            max_lifetime: 90.0,
            custom_mesh: None,
        },
    ));
}

fn decal_plane_basis(normal: Vec3) -> (Vec3, Vec3) {
    let up_ref = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let u_axis = normal.cross(up_ref).normalize_or_zero();
    let v_axis = normal.cross(u_axis).normalize_or_zero();
    (u_axis, v_axis)
}

fn sh_clip_uv_edge(verts: &[(Vec3, [f32; 2])], edge: usize) -> Vec<(Vec3, [f32; 2])> {
    if verts.len() < 3 {
        return Vec::new();
    }
    let inside = |uv: [f32; 2]| -> bool {
        match edge {
            0 => uv[0] >= 0.0,
            1 => uv[0] <= 1.0,
            2 => uv[1] >= 0.0,
            _ => uv[1] <= 1.0,
        }
    };
    let intersect = |a: &(Vec3, [f32; 2]), b: &(Vec3, [f32; 2])| -> (Vec3, [f32; 2]) {
        let (val_a, val_b, target) = match edge {
            0 => (a.1[0], b.1[0], 0.0),
            1 => (a.1[0], b.1[0], 1.0),
            2 => (a.1[1], b.1[1], 0.0),
            _ => (a.1[1], b.1[1], 1.0),
        };
        let denom = val_a - val_b;
        let t = if denom.abs() > 1e-6 {
            ((val_a - target) / denom).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pos = a.0.lerp(b.0, t);
        let mut uv = [
            a.1[0] + (b.1[0] - a.1[0]) * t,
            a.1[1] + (b.1[1] - a.1[1]) * t,
        ];
        match edge {
            0 | 1 => uv[0] = target,
            _ => uv[1] = target,
        }
        (pos, uv)
    };

    let mut out = Vec::with_capacity(verts.len() + 2);
    let mut s = &verts[verts.len() - 1];
    for p in verts {
        if inside(p.1) {
            if !inside(s.1) {
                out.push(intersect(s, p));
            }
            out.push(*p);
        } else if inside(s.1) {
            out.push(intersect(p, s));
        }
        s = p;
    }
    out
}

/// Clips a square decal `[0..1]^2` centered at `proj_center` against a convex BSP face polygon.
/// Returns `(world_or_local_positions, uvs, uv_coverage_area)` when the decal intersects the face.
#[must_use]
pub fn clip_decal_polygon_to_face(
    face: &BspDecalFace,
    proj_center: Vec3,
    size: f32,
) -> Option<(Vec<Vec3>, Vec<[f32; 2]>, f32)> {
    if face.vertices.len() < 3 || size <= 1e-4 {
        return None;
    }
    let (u_axis, v_axis) = decal_plane_basis(face.normal);
    if u_axis.length_squared() < 0.5 || v_axis.length_squared() < 0.5 {
        return None;
    }

    let mut poly: Vec<(Vec3, [f32; 2])> = face
        .vertices
        .iter()
        .map(|&v| {
            let rel = v - proj_center;
            let u = rel.dot(u_axis) / size + 0.5;
            let w = rel.dot(v_axis) / size + 0.5;
            (v, [u, w])
        })
        .collect();

    for edge in 0..4 {
        poly = sh_clip_uv_edge(&poly, edge);
        if poly.len() < 3 {
            return None;
        }
    }

    let mut uv_area = 0.0f32;
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        uv_area += poly[i].1[0] * poly[j].1[1] - poly[j].1[0] * poly[i].1[1];
    }
    uv_area = (uv_area * 0.5).abs();
    if uv_area < 0.015 {
        return None;
    }

    let positions = poly.iter().map(|(p, _)| *p).collect();
    let uvs = poly.iter().map(|(_, uv)| *uv).collect();
    Some((positions, uvs, uv_area))
}

fn build_clipped_decal_mesh(positions: &[Vec3], uvs: &[[f32; 2]], normal: Vec3) -> Mesh {
    let offset_positions: Vec<[f32; 3]> = positions
        .iter()
        .map(|&p| (p + normal * 0.08).to_array())
        .collect();
    let normals: Vec<[f32; 3]> = vec![normal.to_array(); positions.len()];
    let mut indices = Vec::with_capacity((positions.len().saturating_sub(2)) * 3);
    for i in 1..(positions.len() - 1) {
        indices.push(0);
        indices.push(i as u32);
        indices.push((i + 1) as u32);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, offset_positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.to_vec());
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_unclipped_decal_entity(
    commands: &mut Commands,
    quad: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    decal_pos: Vec3,
    wall_normal: Vec3,
    size: f32,
    attach_to: Option<(Entity, Vec3, Quat)>,
) {
    let rot = if wall_normal.dot(Vec3::Z).abs() > 0.999 {
        if wall_normal.z > 0.0 {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_y(std::f32::consts::PI)
        }
    } else {
        Quat::from_rotation_arc(Vec3::Z, wall_normal)
    };

    if let Some((parent_ent, parent_pos, parent_rot)) = attach_to {
        let inv = parent_rot.inverse();
        let local_pos = inv * (decal_pos - parent_pos);
        let local_rot = inv * rot;
        let child = commands
            .spawn((
                Mesh3d(quad),
                MeshMaterial3d(mat),
                Transform::from_translation(local_pos)
                    .with_rotation(local_rot)
                    .with_scale(Vec3::splat(size)),
                BulletHoleDecal {
                    position: decal_pos,
                    normal: wall_normal,
                    lifetime: 0.0,
                    max_lifetime: 60.0,
                    custom_mesh: None,
                },
            ))
            .id();
        commands.entity(parent_ent).add_child(child);
    } else {
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(mat),
            Transform::from_translation(decal_pos)
                .with_rotation(rot)
                .with_scale(Vec3::splat(size)),
            BulletHoleDecal {
                position: decal_pos,
                normal: wall_normal,
                lifetime: 0.0,
                max_lifetime: 60.0,
                custom_mesh: None,
            },
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_clipped_decal_on_index(
    commands: &mut Commands,
    hit_pos: Vec3,
    wall_normal: Vec3,
    size: f32,
    mat: &Handle<StandardMaterial>,
    attach_to: Option<(Entity, Vec3, Quat)>,
    model_index: usize,
    index: &LevelDecalSurfaceIndex,
    sprite_store: &mut SpriteStore,
    meshes: &mut Assets<Mesh>,
) -> bool {
    let half = size * 0.5;
    let (local_hit, local_norm) = if let Some((_, parent_pos, parent_rot)) = attach_to {
        let inv = parent_rot.inverse();
        (
            inv * (hit_pos - parent_pos),
            (inv * wall_normal).normalize_or_zero(),
        )
    } else {
        (hit_pos, wall_normal.normalize_or_zero())
    };

    let search_min = local_hit - Vec3::splat(half + 2.0);
    let search_max = local_hit + Vec3::splat(half + 2.0);
    let mut spawned_any = false;

    for face in &index.faces {
        if face.model_index != model_index
            || face.aabb_max.x < search_min.x
            || face.aabb_min.x > search_max.x
            || face.aabb_max.y < search_min.y
            || face.aabb_min.y > search_max.y
            || face.aabb_max.z < search_min.z
            || face.aabb_min.z > search_max.z
        {
            continue;
        }

        let ndot = face.normal.dot(local_norm);
        if ndot < -0.05 {
            continue;
        }
        let signed_dist = local_hit.dot(face.normal) - face.plane_dist;
        let plane_delta = signed_dist.abs();
        if (ndot > 0.7 && plane_delta > 1.5) || plane_delta >= half {
            continue;
        }

        let proj_center = local_hit - face.normal * signed_dist;
        let Some((positions, uvs, uv_area)) = clip_decal_polygon_to_face(face, proj_center, size)
        else {
            continue;
        };

        spawned_any = true;
        let (world_pos, world_norm) = if let Some((_, parent_pos, parent_rot)) = attach_to {
            (
                parent_pos + parent_rot * (proj_center + face.normal * 0.08),
                parent_rot * face.normal,
            )
        } else {
            (proj_center + face.normal * 0.08, face.normal)
        };

        if uv_area >= 0.99 {
            let quad = sprite_store.get_or_create_unit_quad(meshes);
            spawn_unclipped_decal_entity(
                commands,
                quad,
                mat.clone(),
                world_pos,
                world_norm,
                size,
                attach_to,
            );
            continue;
        }

        let mesh_handle = meshes.add(build_clipped_decal_mesh(&positions, &uvs, face.normal));
        let bundle = (
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::IDENTITY,
            BulletHoleDecal {
                position: world_pos,
                normal: world_norm,
                lifetime: 0.0,
                max_lifetime: 60.0,
                custom_mesh: Some(mesh_handle),
            },
        );
        if let Some((parent_ent, _, _)) = attach_to {
            let child = commands.spawn(bundle).id();
            commands.entity(parent_ent).add_child(child);
        } else {
            commands.spawn(bundle);
        }
    }
    spawned_any
}

/// Spawns a clipped bullet or glass impact decal (`{shot1..5`, `{break1..3`, `{bproof1`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_surface_decal(
    commands: &mut Commands,
    hit_pos: Vec3,
    wall_normal: Vec3,
    kind: ImpactDecalKind,
    attach_to: Option<(Entity, Vec3, Quat)>,
    model_index: usize,
    decal_faces: Option<&LevelDecalSurfaceIndex>,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let decal_name = match kind {
        ImpactDecalKind::BulletShot => {
            let shot_num = ((hit_pos.x.abs() * 10.0 + hit_pos.y.abs()) as usize % 5) + 1;
            format!("{{shot{shot_num}")
        }
        ImpactDecalKind::BreakableGlass => {
            let break_num = ((hit_pos.x.abs() * 11.0 + hit_pos.z.abs() * 7.0) as usize % 3) + 1;
            format!("{{break{break_num}")
        }
        ImpactDecalKind::UnbreakableGlass => "{bproof1".to_string(),
    };

    let Some(mat) = sprite_store.get_or_load_decal(&decal_name, search_paths, images, materials)
    else {
        return;
    };

    // Native GoldSrc `decals.wad` texel proportions (1 texel = 1 world unit):
    // `{shot1..5` (16x16), `{bproof1` (16x16), `{break1..3` (48x48, slightly scaled for panes).
    let size = match kind {
        ImpactDecalKind::BulletShot | ImpactDecalKind::UnbreakableGlass => 16.0,
        ImpactDecalKind::BreakableGlass => 32.0,
    };

    if let Some(index) = decal_faces
        && spawn_clipped_decal_on_index(
            commands,
            hit_pos,
            wall_normal,
            size,
            &mat,
            attach_to,
            model_index,
            index,
            sprite_store,
            meshes,
        )
    {
        return;
    }

    let quad = sprite_store.get_or_create_unit_quad(meshes);
    let decal_pos = hit_pos + wall_normal * 0.08;
    spawn_unclipped_decal_entity(commands, quad, mat, decal_pos, wall_normal, size, attach_to);
}

/// Spawns an authentic bullet hole decal on a struck world wall (`{shot1..5`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_bullet_hole_decal(
    commands: &mut Commands,
    hit_pos: Vec3,
    wall_normal: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    spawn_surface_decal(
        commands,
        hit_pos,
        wall_normal,
        ImpactDecalKind::BulletShot,
        None,
        0,
        None,
        sprite_store,
        search_paths,
        images,
        materials,
        meshes,
    );
}

/// Updates animated billboard sprites and orients world billboards toward the player's camera.
pub fn update_animated_sprites_system(
    mut commands: Commands,
    time: Res<Time>,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<crate::player::PlayerController>)>,
    mut query: Query<(
        Entity,
        &mut AnimatedSprite,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&StickyViewModelSprite>,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok(cam_t) = camera_query.get_single() else {
        return;
    };
    let cam_forward = cam_t.forward().as_vec3();

    for (entity, mut sprite, mut transform, mut mat, sticky) in &mut query {
        // World billboards face camera; viewmodel-attached muzzle flashes stay aligned with the barrel
        if sticky.is_none() {
            transform.rotation = Quat::from_rotation_arc(Vec3::Z, -cam_forward);
        }

        sprite.timer += dt;
        if sprite.timer >= sprite.frame_duration {
            sprite.timer -= sprite.frame_duration;
            sprite.current_frame += 1;

            if sprite.current_frame >= sprite.materials.len() {
                if sprite.is_looping {
                    sprite.current_frame = 0;
                } else {
                    commands.entity(entity).despawn();
                    continue;
                }
            }

            if let Some(next_mat) = sprite.materials.get(sprite.current_frame) {
                mat.0 = next_mat.clone();
            }
        }
    }
}

/// Integrates flying spark streaks with gravity and shrinks them cleanly before despawn.
pub fn update_spark_streaks_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut SparkStreakParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut spark, mut transform) in &mut query {
        spark.lifetime += dt;
        if spark.lifetime >= spark.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        spark.velocity.y -= 800.0 * dt;
        transform.translation += spark.velocity * dt;
        if spark.velocity.length_squared() > 1e-2 {
            transform.rotation = Quat::from_rotation_arc(Vec3::Y, spark.velocity.normalize());
        }
        let fade = (1.0 - spark.lifetime / spark.max_lifetime).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(fade);
    }
}

/// GoldSrc `CShower` / `TE_SPARKS` bouncing spark emitter spawned by HE, Flashbang, and C4 blasts.
#[derive(Component, Debug)]
pub struct SparkShowerParticle {
    pub pos: Vec3,
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub emit_timer: f32,
}

/// Delayed post-explosion steam smoke puff (`steam1.spr` at +0.3s/+0.55s/+0.85s per `ggrenade.cpp`).
#[derive(Component, Debug)]
pub struct DelayedSmokeEffect {
    pub origin: Vec3,
    pub delay: f32,
    pub size: f32,
    pub fps: f32,
    pub sprite_path: &'static str,
}

/// Spawns `2..=3` bouncing `CShower` spark emitters matching ReGameDLL `Explode` / `Explode2` / `Explode3`.
pub fn spawn_spark_shower(commands: &mut Commands, origin: Vec3, normal: Vec3, seed_bits: u32) {
    let count = 2 + (seed_bits % 2) as usize;
    let n = if normal.length_squared() > 0.1 {
        normal.normalize()
    } else {
        Vec3::Y
    };
    let mut s = seed_bits ^ 0x9E37_79B9;
    let mut rand_range = |lo: f32, hi: f32| -> f32 {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        let t = ((s & 0xFFFF) as f32) / 65536.0;
        lo + t * (hi - lo)
    };
    for _ in 0..count {
        let base_spd = rand_range(280.0, 420.0);
        let vel = n * base_spd
            + Vec3::new(
                rand_range(-140.0, 140.0),
                rand_range(160.0, 320.0),
                rand_range(-140.0, 140.0),
            );
        commands.spawn(SparkShowerParticle {
            pos: origin,
            velocity: vel,
            lifetime: 0.0,
            max_lifetime: rand_range(0.8, 1.6),
            emit_timer: 0.0,
        });
    }
}

/// Schedules a delayed `sprites/steam1.spr` post-explosion smoke cloud.
pub fn spawn_delayed_smoke(
    commands: &mut Commands,
    origin: Vec3,
    delay: f32,
    size: f32,
    fps: f32,
    sprite_path: &'static str,
) {
    commands.spawn(DelayedSmokeEffect {
        origin,
        delay,
        size,
        fps,
        sprite_path,
    });
}

/// Spawns the 4 overlapping C4 fireball sprites matching `CGrenade::Explode2` (`fexplo`/`eexplo`/`fexplo`/`zerogxplode`).
pub fn spawn_c4_explosion_sprites(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let entries: [(&str, Vec3, f32, f32); 4] = [
        ("sprites/fexplo.spr", Vec3::new(0.0, 40.0, 0.0), 300.0, 30.0),
        (
            "sprites/eexplo.spr",
            Vec3::new(40.0, 50.0, -35.0),
            340.0,
            26.0,
        ),
        (
            "sprites/fexplo.spr",
            Vec3::new(-45.0, 45.0, 40.0),
            280.0,
            28.0,
        ),
        (
            "sprites/zerogxplode.spr",
            Vec3::new(0.0, 65.0, 0.0),
            360.0,
            24.0,
        ),
    ];
    for (path, offset, sz, fps) in entries {
        if let Some(spr) = sprite_store.get_or_load_sprite(path, search_paths, images, materials)
            && let Some(first_mat) = spr.materials.first().cloned()
        {
            let quad = sprite_store.get_or_create_unit_quad(meshes);
            commands.spawn((
                Mesh3d(quad),
                MeshMaterial3d(first_mat),
                Transform::from_translation(origin + offset).with_scale(Vec3::splat(sz)),
                bevy::render::view::NoFrustumCulling,
                AnimatedSprite {
                    materials: spr.materials,
                    current_frame: 0,
                    timer: 0.0,
                    frame_duration: 1.0 / fps,
                    is_looping: false,
                },
            ));
        }
    }
}

/// Spawns the 2-second red C4 LED blink glow sprite (`sprites/ledglow.spr`, `TE_GLOWSPRITE` parity).
pub fn spawn_ledglow_sprite(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    if let Some(spr) =
        sprite_store.get_or_load_sprite("sprites/ledglow.spr", search_paths, images, materials)
        && let Some(first_mat) = spr.materials.first().cloned()
    {
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin + Vec3::Y * 5.0).with_scale(Vec3::splat(24.0)),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: spr.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 0.1,
                is_looping: false,
            },
        ));
    }
}

/// Spawns continuous small gray-black flowing smoke puffs emitted from a resting smoke canister (`black_smoke1..4.spr`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_black_smoke_puff(
    commands: &mut Commands,
    origin: Vec3,
    variant_idx: usize,
    angle_deg: f32,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let idx = (variant_idx % 4) + 1;
    let spr_path = format!("sprites/black_smoke{idx}.spr");
    let tex_handle = sprite_store
        .get_or_load_sprite(&spr_path, search_paths, images, materials)
        .or_else(|| {
            sprite_store.get_or_load_sprite(
                "sprites/gas_puff_01.spr",
                search_paths,
                images,
                materials,
            )
        })
        .and_then(|s| {
            s.materials
                .first()
                .and_then(|h| materials.get(h).and_then(|m| m.base_color_texture.clone()))
        });
    if let Some(tex) = tex_handle {
        let rad = angle_deg.to_radians();
        let quad = sprite_store.get_or_create_unit_quad(meshes);
        let dir = Vec3::new(rad.cos(), 0.25, -rad.sin()).normalize_or_zero();
        let puff_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.85, 0.85, 0.65),
            base_color_texture: Some(tex),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(quad),
            MeshMaterial3d(puff_mat.clone()),
            Transform::from_translation(origin + dir * 6.0).with_scale(Vec3::splat(72.0 * 0.32)),
            bevy::render::view::NoFrustumCulling,
            SmokeBillboardParticle {
                velocity: dir * 32.0 + Vec3::Y * 14.0,
                lifetime: 0.0,
                max_lifetime: 1.9,
                hold_fade_at: 0.45,
                start_scale: 0.32,
                end_scale: 1.20,
                grow_duration: 1.4,
                base_rgb: [0.85, 0.85, 0.85],
                base_alpha: 0.65,
            },
            AnimatedSprite {
                materials: vec![puff_mat],
                current_frame: 0,
                timer: 0.0,
                frame_duration: 2.0,
                is_looping: false,
            },
        ));
    }
}

/// Integrates drifting/expanding smoke clouds and nozzle wisps, facing camera with gentle world wall damping.
pub fn update_smoke_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<crate::player::PlayerController>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(
        Entity,
        &mut SmokeBillboardParticle,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let cam_forward = camera_query
        .get_single()
        .map_or(Vec3::NEG_Z, |c| c.forward().as_vec3());
    let facing = Quat::from_rotation_arc(Vec3::Z, -cam_forward);

    for (entity, mut p, mut tf, mat_h) in &mut query {
        p.lifetime += dt;
        if p.lifetime >= p.max_lifetime {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        tf.rotation = facing;
        let next_pos = tf.translation + p.velocity * dt;
        if let Some(col_res) = collision_res.as_ref()
            && let Some(ref col) = col_res.model
            && p.velocity.length_squared() > 1.0
        {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col);
            let tr = bsp.trace_player_hull(
                [tf.translation.x, -tf.translation.z, tf.translation.y],
                [next_pos.x, -next_pos.z, next_pos.y],
                0,
            );
            if tr.fraction < 1.0 {
                let n = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                    .normalize_or_zero();
                tf.translation =
                    tf.translation + (next_pos - tf.translation) * tr.fraction + n * 2.0;
                let vn = p.velocity.dot(n);
                if vn < 0.0 {
                    p.velocity = (p.velocity - n * (1.2 * vn)) * 0.4;
                }
            } else {
                tf.translation = next_pos;
            }
        } else {
            tf.translation = next_pos;
        }

        // Gentle exponential air drag matching EV_CreateSmoke billowing
        p.velocity *= (1.0 - 0.18 * dt).max(0.0);

        let grow_t = (p.lifetime / p.grow_duration.max(0.1)).clamp(0.0, 1.0);
        let smooth_grow = 1.0 - (1.0 - grow_t) * (1.0 - grow_t);
        let s = p.start_scale + (p.end_scale - p.start_scale) * smooth_grow;
        tf.scale = Vec3::splat(s);

        let alpha = if p.lifetime <= p.hold_fade_at {
            p.base_alpha
        } else {
            let fade_span = (p.max_lifetime - p.hold_fade_at).max(0.1);
            let fade_frac = 1.0 - ((p.lifetime - p.hold_fade_at) / fade_span).clamp(0.0, 1.0);
            p.base_alpha * fade_frac
        };
        if let Some(mat) = materials.get_mut(&mat_h.0) {
            mat.base_color = Color::srgba(p.base_rgb[0], p.base_rgb[1], p.base_rgb[2], alpha);
        }
    }
}

/// Updates `SparkShowerParticle` emitters, bouncing along surfaces and emitting golden sparks every 0.1s.
#[allow(clippy::too_many_arguments)]
pub fn update_spark_showers_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    mut sprite_store: ResMut<SpriteStore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(Entity, &mut SparkShowerParticle)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut shower) in &mut query {
        shower.lifetime += dt;
        if shower.lifetime >= shower.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        shower.velocity.y -= 400.0 * dt; // CS 1.6 `CShower`: pev->gravity = 0.5 (400 u/s^2)
        let next_pos = shower.pos + shower.velocity * dt;
        if let Some(col_res) = collision_res.as_ref()
            && let Some(ref col) = col_res.model
        {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col);
            let tr = bsp.trace_player_hull(
                [shower.pos.x, -shower.pos.z, shower.pos.y],
                [next_pos.x, -next_pos.z, next_pos.y],
                0,
            );
            if tr.fraction < 1.0 {
                let n = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                    .normalize_or_zero();
                shower.pos = shower.pos + (next_pos - shower.pos) * tr.fraction + n * 1.5;
                let vn = shower.velocity.dot(n);
                if vn < 0.0 {
                    shower.velocity = (shower.velocity - n * (1.5 * vn)) * 0.6;
                }
            } else {
                shower.pos = next_pos;
            }
        } else {
            shower.pos = next_pos;
        }

        shower.emit_timer -= dt;
        if shower.emit_timer <= 0.0 {
            shower.emit_timer = 0.05;
            let streak_mesh = sprite_store.get_or_create_spark_streak_mesh(&mut meshes);
            let mat = if let Some(ref h) = sprite_store.spark_mat_amber {
                h.clone()
            } else {
                let h = materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 0.85, 0.35, 0.75),
                    emissive: LinearRgba::rgb(3.5, 2.2, 0.6),
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                });
                sprite_store.spark_mat_amber = Some(h.clone());
                h
            };
            let mut seed = shower.pos.x.to_bits() ^ shower.lifetime.to_bits().rotate_left(7);
            for _ in 0..6 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let rx = (((seed & 0xFF) as f32) / 128.0) - 1.0;
                let ry = ((((seed >> 8) & 0xFF) as f32) / 128.0) - 0.5;
                let rz = ((((seed >> 16) & 0xFF) as f32) / 128.0) - 1.0;
                let vel = Vec3::new(rx * 200.0, ry * 200.0 + 60.0, rz * 200.0);
                let rot = if vel.length_squared() > 1e-2 {
                    Quat::from_rotation_arc(Vec3::Y, vel.normalize())
                } else {
                    Quat::IDENTITY
                };
                commands.spawn((
                    Mesh3d(streak_mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(shower.pos).with_rotation(rot),
                    SparkStreakParticle {
                        velocity: vel,
                        lifetime: 0.0,
                        max_lifetime: 0.35,
                    },
                ));
            }
        }
    }
}

/// Spawns scheduled `steam1.spr` post-explosion smoke clouds when their delay expires.
#[allow(clippy::too_many_arguments)]
pub fn update_delayed_smokes_system(
    mut commands: Commands,
    time: Res<Time>,
    mut sprite_store: ResMut<SpriteStore>,
    search_paths: Res<GameSearchPaths>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(Entity, &mut DelayedSmokeEffect)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, mut eff) in &mut query {
        eff.delay -= dt;
        if eff.delay <= 0.0 {
            if let Some(spr) = sprite_store.get_or_load_sprite(
                eff.sprite_path,
                &search_paths,
                &mut images,
                &mut materials,
            ) && let Some(first_mat) = spr.materials.first().cloned()
            {
                let quad = sprite_store.get_or_create_unit_quad(&mut meshes);
                commands.spawn((
                    Mesh3d(quad),
                    MeshMaterial3d(first_mat),
                    Transform::from_translation(eff.origin + Vec3::Y * (eff.size * 0.25))
                        .with_scale(Vec3::splat(eff.size)),
                    bevy::render::view::NoFrustumCulling,
                    AnimatedSprite {
                        materials: spr.materials,
                        current_frame: 0,
                        timer: 0.0,
                        frame_duration: 1.0 / eff.fps.max(1.0),
                        is_looping: false,
                    },
                ));
            }
            commands.entity(entity).despawn();
        }
    }
}

/// Despawns bullet decals when their lifetime expires or when active count exceeds 128 (FIFO bound).
pub fn update_bullet_decals_system(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut query: Query<(Entity, &mut BulletHoleDecal)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let mut total_count = 0;
    let mut oldest_entity: Option<(Entity, BulletHoleDecal)> = None;
    let mut max_lifetime_so_far = -1.0f32;

    for (entity, mut decal) in &mut query {
        total_count += 1;
        decal.lifetime += dt;
        if decal.lifetime >= decal.max_lifetime {
            despawn_decal_entity(&mut commands, meshes.as_deref_mut(), entity, &decal);
        } else if decal.lifetime > max_lifetime_so_far {
            max_lifetime_so_far = decal.lifetime;
            oldest_entity = Some((entity, decal.clone()));
        }
    }

    if total_count > 128
        && let Some((oldest, oldest_decal)) = oldest_entity
    {
        despawn_decal_entity(&mut commands, meshes.as_deref_mut(), oldest, &oldest_decal);
    }
}

/// Orients non-animated billboard quad sprites (like embers/flicker particles) toward the player's camera.
pub fn update_billboard_sprites_system(
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<crate::player::PlayerController>)>,
    mut query: Query<&mut Transform, (With<BillboardSprite>, Without<AnimatedSprite>)>,
) {
    let Ok(cam_t) = camera_query.get_single() else {
        return;
    };
    let cam_forward = cam_t.forward().as_vec3();
    let facing = Quat::from_rotation_arc(Vec3::Z, -cam_forward);
    for mut transform in &mut query {
        transform.rotation = facing;
    }
}

pub fn update_rising_smoke_sprites_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &RisingSmokeSprite)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut tf, rising) in &mut query {
        tf.translation.y += rising.speed * dt;
    }
}

pub fn update_flicker_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    mut query: Query<(Entity, &mut FlickerParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, mut p, mut tf) in &mut query {
        p.lifetime += dt;
        if p.lifetime >= p.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        // GoldSrc cl_efx.c:2159: grav * 5.0f where grav = frametime * 800.0 * 0.05 = 40.0 u/s^2 -> 200.0 u/s^2 downward acceleration
        p.velocity.y -= 200.0 * dt;
        p.velocity.x *= (1.0 - 0.35 * dt).max(0.0);
        p.velocity.z *= (1.0 - 0.35 * dt).max(0.0);

        let old_pos = tf.translation;
        let new_pos = old_pos + p.velocity * dt;
        if let Some(col_res) = collision_res.as_ref()
            && let Some(ref col) = col_res.model
        {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col);
            let tr = bsp.trace_player_hull(
                [old_pos.x, -old_pos.z, old_pos.y],
                [new_pos.x, -new_pos.z, new_pos.y],
                0,
            );
            if tr.fraction < 1.0 {
                // GoldSrc pt_blob2: gently bounce/settle on floor rather than instantly vanishing
                p.velocity.y = (-p.velocity.y * 0.25).max(0.0);
                p.velocity.x *= 0.6;
                p.velocity.z *= 0.6;
                tf.translation.y = old_pos.y + p.velocity.y * dt;
                continue;
            }
        }
        tf.translation = new_pos;
    }
}

/// Spawns 15 yellow/amber flickering particles falling under heavy gravity matching canonical `R_FlickerParticles`.
pub fn spawn_flicker_particles(
    commands: &mut Commands,
    origin: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    sprite_store: &mut SpriteStore,
) {
    let dot_mesh = sprite_store.get_or_create_unit_quad(meshes);
    let dot_mat = sprite_store.get_or_create_spark_dot_mat(
        materials,
        None,
        Color::srgba(1.0, 0.88, 0.25, 0.95),
        LinearRgba::rgb(4.0, 3.2, 0.6),
    );
    let mut seed = origin.x.to_bits() ^ origin.z.to_bits().rotate_left(11);
    for _ in 0..15 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let rx = (((seed & 0xFF) as f32) / 128.0) - 1.0;
        let rz = ((((seed >> 8) & 0xFF) as f32) / 128.0) - 1.0;
        let ry = (((seed >> 16) & 0xFF) as f32) / 255.0;
        let vx = rx * 32.0;
        let vz = rz * 32.0;
        let vy = 80.0 + ry * 63.0; // 80..143 u/s (canonical R_FlickerParticles)

        commands.spawn((
            Mesh3d(dot_mesh.clone()),
            MeshMaterial3d(dot_mat.clone()),
            Transform::from_translation(origin + Vec3::new(rx * 4.0, 4.0, rz * 4.0))
                .with_scale(Vec3::splat(3.5)),
            bevy::render::view::NoFrustumCulling,
            BillboardSprite,
            FlickerParticle {
                velocity: Vec3::new(vx, vy, vz),
                lifetime: 0.0,
                max_lifetime: 1.8 + ry * 0.4,
            },
        ));
    }
}

pub fn update_wall_puff_particles_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut WallPuffParticle)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut tf, mut p) in &mut query {
        p.frame_counter += dt * 30.0;
        let scale = 10.0 + (p.frame_counter * 1.6).min(18.0);
        tf.scale = Vec3::splat(scale);
        if p.frame_counter > 7.0 {
            p.velocity.x *= 0.97;
            p.velocity.z *= 0.97;
            p.velocity.y = (p.velocity.y * 0.97 + 0.7).min(70.0);
        }
        if p.frame_counter > 6.0 {
            p.x_wind_magnitude = (p.x_wind_magnitude + 0.075).min(5.0);
            p.y_wind_magnitude = (p.y_wind_magnitude + 0.075).min(5.0);
            let x_dir = if p.x_wind_direction { 1.0 } else { -1.0 };
            let z_dir = if p.y_wind_direction { 1.0 } else { -1.0 };
            p.velocity.x += p.x_wind_magnitude * x_dir * dt * 30.0;
            p.velocity.z += p.y_wind_magnitude * z_dir * dt * 30.0;
            if p.y_wind_magnitude > 3.0 && ((p.frame_counter * 10.0) as usize).is_multiple_of(11) {
                p.y_wind_direction = !p.y_wind_direction;
                p.y_wind_magnitude = 0.0;
            }
            if p.x_wind_magnitude > 3.0 && ((p.frame_counter * 10.0) as usize).is_multiple_of(7) {
                p.x_wind_direction = !p.x_wind_direction;
                p.x_wind_magnitude = 0.0;
            }
        }
        tf.translation += p.velocity * dt;
    }
}

fn find_ground_y(
    x: f32,
    start_y: f32,
    z: f32,
    collision_res: Option<&Res<crate::player::LevelCollisionResource>>,
) -> f32 {
    if let Some(col_res) = collision_res
        && let Some(col) = &col_res.model
    {
        use goldhub_physics::WorldTrace;
        let bsp = goldhub_physics::BspWorldCollision::new(col);
        let start = [x, start_y, z];
        let end = [x, start_y - 2048.0, z];
        let tr = bsp.trace_player_hull(start, end, 0);
        if tr.fraction < 0.99 {
            return tr.endpos[1];
        }
    }
    start_y - 600.0
}

fn recycle_streak_to_top(
    tf: &mut Transform,
    streak: &mut RainStreakParticle,
    player_pos: Vec3,
    collision_res: Option<&Res<crate::player::LevelCollisionResource>>,
    seed: &mut u32,
) {
    let mut next_f32 = || -> f32 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        ((*seed & 0xFFFF) as f32) / 65536.0
    };

    let angle = next_f32() * std::f32::consts::TAU;
    let dist = next_f32().sqrt() * 680.0;
    let spawn_x = player_pos.x + angle.cos() * dist;
    let spawn_z = player_pos.z + angle.sin() * dist;
    let spawn_y = player_pos.y + 220.0 + next_f32() * 260.0;

    streak.ground_y = find_ground_y(spawn_x, spawn_y, spawn_z, collision_res);
    tf.translation = Vec3::new(spawn_x, spawn_y, spawn_z);
}

fn ensure_rain_assets(
    rain_store: &mut WeatherRainStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> (
    Handle<Mesh>,
    Handle<StandardMaterial>,
    Handle<Mesh>,
    Handle<StandardMaterial>,
) {
    const DIM: usize = 32;
    let streak_mesh = rain_store
        .streak_mesh
        .get_or_insert_with(|| meshes.add(Rectangle::new(0.65, 24.0).mesh()))
        .clone();

    let streak_mat = rain_store
        .streak_mat
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.85, 0.92, 1.0, 0.38),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            })
        })
        .clone();

    let splash_mesh = rain_store
        .splash_mesh
        .get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0).mesh()))
        .clone();

    let splash_mat = rain_store
        .splash_mat
        .get_or_insert_with(|| {
            let mut data = Vec::with_capacity(DIM * DIM * 4);
            let center = (DIM as f32 - 1.0) * 0.5;
            let radius = center;
            for y in 0..DIM {
                for x in 0..DIM {
                    let dx = (x as f32 - center) / radius;
                    let dy = (y as f32 - center) / radius;
                    let dist = dx.hypot(dy);
                    let ring = if dist <= 1.0 {
                        let diff = (dist - 0.72).abs();
                        (-diff * diff / (2.0 * 0.12 * 0.12)).exp()
                    } else {
                        0.0
                    };
                    let a = (ring * 190.0) as u8;
                    data.extend_from_slice(&[210, 230, 255, a]);
                }
            }
            let img = Image::new(
                Extent3d {
                    width: DIM as u32,
                    height: DIM as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                data,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            let img_handle = images.add(img);
            materials.add(StandardMaterial {
                base_color_texture: Some(img_handle),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            })
        })
        .clone();

    (streak_mesh, streak_mat, splash_mesh, splash_mat)
}

/// Simulates authentic CS 1.6 Aztec rain streaks and ground/water ripple splash particles.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_weather_rain_system(
    mut commands: Commands,
    time: Res<Time>,
    current_map_opt: Option<Res<crate::level::CurrentMapInfo>>,
    mut rain_store: ResMut<WeatherRainStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    collision_res: Option<Res<crate::player::LevelCollisionResource>>,
    player_query: Query<&Transform, With<crate::player::PlayerController>>,
    mut streak_query: Query<
        (Entity, &mut Transform, &mut RainStreakParticle),
        (
            Without<RainSplashParticle>,
            Without<crate::player::PlayerController>,
        ),
    >,
    mut splash_query: Query<
        (Entity, &mut Transform, &mut RainSplashParticle),
        (
            Without<RainStreakParticle>,
            Without<crate::player::PlayerController>,
        ),
    >,
) {
    const TARGET_STREAKS: usize = 160;
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // 1. Update active ground/water ripple splash particles
    for (ent, mut tf, mut splash) in &mut splash_query {
        splash.timer += dt;
        if splash.timer >= splash.max_lifetime {
            commands.entity(ent).despawn();
        } else {
            let frac = splash.timer / splash.max_lifetime;
            let s = splash.start_scale + (splash.end_scale - splash.start_scale) * frac;
            tf.scale = Vec3::splat(s);
        }
    }

    let is_aztec = current_map_opt.is_some_and(|m| m.is_aztec);
    if !is_aztec {
        for (ent, _, _) in &streak_query {
            commands.entity(ent).despawn();
        }
        return;
    }

    let Ok(player_tf) = player_query.get_single() else {
        return;
    };
    let player_pos = player_tf.translation;

    // 2. Ensure cached rain streak & ripple assets exist
    let (streak_mesh, streak_mat, splash_mesh, splash_mat) =
        ensure_rain_assets(&mut rain_store, &mut meshes, &mut materials, &mut images);

    if rain_store.rng_seed == 0 {
        rain_store.rng_seed = 0xDEA2_7EC1;
    }
    let mut seed = rain_store.rng_seed;

    let rain_vel = Vec3::new(-35.0, -920.0, 15.0);
    let streak_rot = Quat::from_rotation_arc(Vec3::Y, rain_vel.normalize());

    // 3. Update existing rain streaks
    let mut active_streak_count = 0;
    for (_ent, mut tf, mut streak) in &mut streak_query {
        active_streak_count += 1;
        tf.translation += streak.velocity * dt;
        if tf.translation.y <= streak.ground_y {
            // Spawn expanding ripple ring on floor or riverbed
            commands.spawn((
                Mesh3d(splash_mesh.clone()),
                MeshMaterial3d(splash_mat.clone()),
                Transform::from_translation(Vec3::new(
                    tf.translation.x,
                    streak.ground_y + 0.4,
                    tf.translation.z,
                ))
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(2.5)),
                RainSplashParticle {
                    timer: 0.0,
                    max_lifetime: 0.16,
                    start_scale: 2.5,
                    end_scale: 8.5,
                },
                bevy::render::view::NoFrustumCulling,
            ));

            // Recycle streak back up into cylinder around player
            recycle_streak_to_top(
                &mut tf,
                &mut streak,
                player_pos,
                collision_res.as_ref(),
                &mut seed,
            );
        }
    }

    // 4. Spawn missing streaks up to 160 drops around player
    if active_streak_count < TARGET_STREAKS {
        let batch = (TARGET_STREAKS - active_streak_count).min(24);
        for _ in 0..batch {
            let mut tf = Transform::from_rotation(streak_rot);
            let mut streak = RainStreakParticle {
                velocity: rain_vel,
                ground_y: player_pos.y - 300.0,
            };
            recycle_streak_to_top(
                &mut tf,
                &mut streak,
                player_pos,
                collision_res.as_ref(),
                &mut seed,
            );
            commands.spawn((
                Mesh3d(streak_mesh.clone()),
                MeshMaterial3d(streak_mat.clone()),
                tf,
                streak,
                bevy::render::view::NoFrustumCulling,
            ));
        }
    }

    rain_store.rng_seed = seed;
}

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteStore>();
        app.init_resource::<WeatherRainStore>();
        app.add_systems(
            Update,
            (
                update_animated_sprites_system,
                update_billboard_sprites_system,
                update_smoke_particles_system,
                update_spark_streaks_system,
                update_spark_showers_system,
                update_delayed_smokes_system,
                update_rising_smoke_sprites_system,
                update_wall_puff_particles_system,
                update_weather_rain_system,
                update_flicker_particles_system,
                update_bullet_decals_system,
            ),
        );
    }
}

/// Default search directories for resolving game assets.
const DEFAULT_ROOTS: &[&str] = &[
    "C:/Games/CS 1.6 GoldClient/cstrike",
    "C:/Games/CS 1.6 GoldClient/valve",
    "C:/Program Files (x86)/Steam/steamapps/common/Half-Life/cstrike",
    "C:/Program Files (x86)/Steam/steamapps/common/Half-Life/valve",
    "C:/Games/Counter-Strike 1.6/cstrike",
    "C:/Games/Counter-Strike 1.6/valve",
    "assets",
];

pub fn resolve_file_path(relative_path: &str, search_paths: &GameSearchPaths) -> Option<PathBuf> {
    let clean = relative_path.replace('\\', "/");
    let rel = clean.strip_prefix('/').unwrap_or(&clean);

    for base_dir in &search_paths.dirs {
        let candidate = base_dir.join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    for root in DEFAULT_ROOTS {
        let candidate = Path::new(root).join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wall_puff_scale_expansion() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);

        let entity = app
            .world_mut()
            .spawn((
                Transform::from_scale(Vec3::splat(10.0)),
                WallPuffParticle {
                    velocity: Vec3::ZERO,
                    x_wind_direction: true,
                    y_wind_direction: true,
                    x_wind_magnitude: 0.0,
                    y_wind_magnitude: 0.0,
                    frame_counter: 0.0,
                },
            ))
            .id();

        app.add_systems(Update, update_wall_puff_particles_system);

        // Advance several frames
        for _ in 0..10 {
            app.update();
        }

        let tf = app.world().get::<Transform>(entity).unwrap();
        // Scale must have grown from initial 10.0
        assert!(
            tf.scale.x > 10.0,
            "Wall puff must expand as it dissipates: {:?}",
            tf.scale
        );
        assert!(
            tf.scale.x <= 28.0,
            "Wall puff scale must be bounded: {:?}",
            tf.scale
        );
    }

    #[test]
    fn test_bullet_decals_fifo_pruning() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);

        // Spawn 135 decals
        for i in 0..135 {
            app.world_mut().spawn(BulletHoleDecal {
                position: Vec3::new(i as f32, 0.0, 0.0),
                normal: Vec3::Z,
                lifetime: i as f32 * 0.1, // oldest has largest lifetime
                max_lifetime: 60.0,
                custom_mesh: None,
            });
        }

        app.add_systems(Update, update_bullet_decals_system);

        // Run system enough times to prune to 128
        for _ in 0..10 {
            app.update();
        }

        let remaining = app
            .world_mut()
            .query::<&BulletHoleDecal>()
            .iter(app.world())
            .count();
        assert_eq!(remaining, 128, "Active decals must be bounded to 128 max");
    }

    #[test]
    fn test_sutherland_hodgman_edge_and_corner_clipping() {
        // Wall face ending at X = 100.0 (facing +Z)
        let front_face = BspDecalFace {
            model_index: 0,
            aabb_min: Vec3::new(0.0, 0.0, -0.5),
            aabb_max: Vec3::new(100.0, 128.0, 0.5),
            normal: Vec3::Z,
            plane_dist: 0.0,
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(100.0, 0.0, 0.0),
                Vec3::new(100.0, 128.0, 0.0),
                Vec3::new(0.0, 128.0, 0.0),
            ],
        };
        // Return L-corner face at X = 100.0 going back along -Z (facing +X)
        let return_face = BspDecalFace {
            model_index: 0,
            aabb_min: Vec3::new(99.5, 0.0, -100.0),
            aabb_max: Vec3::new(100.5, 128.0, 0.0),
            normal: Vec3::X,
            plane_dist: 100.0,
            vertices: vec![
                Vec3::new(100.0, 0.0, 0.0),
                Vec3::new(100.0, 0.0, -100.0),
                Vec3::new(100.0, 128.0, -100.0),
                Vec3::new(100.0, 128.0, 0.0),
            ],
        };

        // Shoot 2 units away from vertical corner edge X = 100 with a 16-unit decal (half = 8)
        let hit_pos = Vec3::new(98.0, 64.0, 0.0);

        let (front_pts, _, front_area) =
            clip_decal_polygon_to_face(&front_face, hit_pos, 16.0).expect("clips front face");
        // Must clip strictly at X = 100.0 without protruding past edge!
        assert!(front_pts.iter().all(|p| p.x <= 100.0 + 1e-4));
        assert!(front_area > 0.55 && front_area < 0.70);

        // Project onto return face at X = 100.0
        let proj_return = Vec3::new(100.0, 64.0, 0.0);
        let (ret_pts, _, ret_area) =
            clip_decal_polygon_to_face(&return_face, proj_return, 16.0).expect("wraps corner");
        assert!(ret_pts.iter().all(|p| p.z <= 1e-4));
        assert!(ret_area > 0.45 && ret_area < 0.55);
    }

    #[test]
    fn test_aztec_weather_rain_streaks_and_splashes() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.add_plugins(bevy::asset::AssetPlugin::default());
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<WeatherRainStore>();

        app.insert_resource(crate::level::CurrentMapInfo {
            map_name: "de_aztec".to_string(),
            is_aztec: true,
        });

        app.world_mut().spawn((
            crate::player::PlayerController::default(),
            Transform::from_xyz(0.0, 50.0, 0.0),
        ));

        app.add_systems(Update, update_weather_rain_system);
        app.update();
        app.update();

        // Streaks should have spawned
        let streak_count = app
            .world_mut()
            .query::<&RainStreakParticle>()
            .iter(app.world())
            .count();
        assert!(
            streak_count > 0,
            "Aztec rain must spawn rain streak particles"
        );

        // Manually place a streak at ground level and simulate frame
        let streak_ent = app
            .world_mut()
            .query_filtered::<Entity, With<RainStreakParticle>>()
            .iter(app.world())
            .next()
            .unwrap();

        app.world_mut()
            .get_mut::<Transform>(streak_ent)
            .unwrap()
            .translation
            .y = -700.0;
        app.update();

        // A ripple splash particle should be spawned
        let splash_count = app
            .world_mut()
            .query::<&RainSplashParticle>()
            .iter(app.world())
            .count();
        assert!(
            splash_count > 0,
            "Ground impact must spawn a RainSplashParticle"
        );
    }
}
