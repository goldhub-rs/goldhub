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
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{SpriteData, SpriteDecoder, Wad3Decoder};

use crate::audio::GameSearchPaths;

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
}

impl SpriteStore {
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

            let mat = StandardMaterial {
                base_color_texture: Some(img_handle),
                cull_mode: None,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
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

/// Component tagging bullet hole decals with a fade-out timer.
#[derive(Component, Debug, Clone)]
pub struct BulletHoleDecal {
    pub lifetime: f32,
    pub max_lifetime: f32,
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
        let mesh = meshes.add(Rectangle::new(26.0, 26.0).mesh());
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin),
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
        let mesh = meshes.add(Rectangle::new(16.0, 16.0).mesh());
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin + Vec3::new(0.0, 1.5, 0.0)),
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

/// Spawns a CS 1.6 muzzle flash billboard sprite at the weapon barrel.
#[allow(clippy::too_many_arguments)]
pub fn spawn_muzzleflash_sprite(
    commands: &mut Commands,
    cam_pos: Vec3,
    cam_forward: Vec3,
    cam_right: Vec3,
    cam_up: Vec3,
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

    let total_frames = sprite.materials.len().max(1);
    let frame_idx = ((cam_pos.x.abs() * 100.0) as usize) % total_frames;
    let Some(mat) = sprite
        .materials
        .get(frame_idx)
        .cloned()
        .or_else(|| sprite.materials.first().cloned())
    else {
        return;
    };
    let mesh = meshes.add(Rectangle::new(14.0, 14.0).mesh());

    // Position at gun barrel based on handedness
    let side_offset = if right_handed { 6.0 } else { -6.0 };
    let barrel_pos = cam_pos + cam_forward * 28.0 + cam_right * side_offset - cam_up * 5.0;

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat.clone()),
        Transform::from_translation(barrel_pos),
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

/// Spawns an explosion fireball sprite (`sprites/zerogxplode.spr`).
pub fn spawn_explosion_sprite(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let Some(sprite) =
        sprite_store.get_or_load_sprite("sprites/zerogxplode.spr", search_paths, images, materials)
    else {
        return;
    };

    let first_mat = sprite.materials.first().cloned().unwrap_or_default();
    let mesh = meshes.add(Rectangle::new(64.0, 64.0).mesh());

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(first_mat),
        Transform::from_translation(origin),
        bevy::render::view::NoFrustumCulling,
        AnimatedSprite {
            materials: sprite.materials,
            current_frame: 0,
            timer: 0.0,
            frame_duration: 1.0 / 20.0, // 20 FPS
            is_looping: false,
        },
    ));
}

/// Spawns an authentic bullet hole decal on a struck wall.
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
    let shot_num = ((hit_pos.x.abs() * 10.0 + hit_pos.y.abs()) as usize % 5) + 1;
    let decal_name = format!("{{shot{shot_num}");

    let Some(mat) = sprite_store.get_or_load_decal(&decal_name, search_paths, images, materials)
    else {
        return;
    };

    let mesh = meshes.add(Rectangle::new(14.0, 14.0).mesh());
    // Offset slightly along normal to prevent z-fighting with the wall
    let decal_pos = hit_pos + wall_normal * 0.08;

    // Orient quad so its normal (+Z) aligns with wall_normal
    let rot = if wall_normal.dot(Vec3::Z).abs() > 0.999 {
        if wall_normal.z > 0.0 {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_y(std::f32::consts::PI)
        }
    } else {
        Quat::from_rotation_arc(Vec3::Z, wall_normal)
    };

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_translation(decal_pos).with_rotation(rot),
        BulletHoleDecal {
            lifetime: 0.0,
            max_lifetime: 60.0,
        },
    ));
}

/// Updates animated billboard sprites and orients them toward the player's camera.
pub fn update_animated_sprites_system(
    mut commands: Commands,
    time: Res<Time>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut query: Query<(
        Entity,
        &mut AnimatedSprite,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
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

    for (entity, mut sprite, mut transform, mut mat) in &mut query {
        // Billboard: face camera
        transform.rotation = Quat::from_rotation_arc(Vec3::Z, -cam_forward);

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

/// Despawns bullet decals when their lifetime expires.
pub fn update_bullet_decals_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut BulletHoleDecal)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, mut decal) in &mut query {
        decal.lifetime += dt;
        if decal.lifetime >= decal.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteStore>();
        app.add_systems(
            Update,
            (update_animated_sprites_system, update_bullet_decals_system),
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
