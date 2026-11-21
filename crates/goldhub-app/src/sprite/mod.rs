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

            let alpha_mode = match sprite.tex_format {
                1 => AlphaMode::Add,       // SPR_ADDITIVE
                3 => AlphaMode::Mask(0.5), // SPR_ALPHTEST
                0 => AlphaMode::Opaque,    // SPR_NORMAL
                _ => AlphaMode::Blend,     // SPR_INDEXALPHA
            };

            let mat = StandardMaterial {
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

/// Component tagging world decals with a fade-out timer (for world surfaces or attached brush submodels).
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
    let mesh = meshes.add(Rectangle::new(14.0, 14.0).mesh());

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
    let mesh = meshes.add(Rectangle::new(12.0, 12.0).mesh());

    // Remember `vm_root` carries `scale = Vec3::new(-1.0, 1.0, 1.0)` when right_handed!
    // In `vm_root` local space, `x = -6.0` * scale.x(-1.0) places muzzle +6.0 right in camera space.
    let local_x = if right_handed { -6.0 } else { 6.0 };
    let flash_child = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(Vec3::new(local_x, -4.5, -28.0)),
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
        let mesh = meshes.add(Rectangle::new(280.0, 280.0).mesh());
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin + Vec3::Y * 45.0),
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
        let mesh2 = meshes.add(Rectangle::new(320.0, 320.0).mesh());
        commands.spawn((
            Mesh3d(mesh2),
            MeshMaterial3d(first_mat2),
            Transform::from_translation(origin + Vec3::new(14.0, 75.0, -10.0)),
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

    let mesh = meshes.add(Rectangle::new(210.0, 210.0).mesh());
    let mut seed = origin.x.to_bits()
        ^ origin.y.to_bits().rotate_left(13)
        ^ origin.z.to_bits().rotate_left(25);
    let mut rand_unit = || -> f32 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (((seed & 0xFFFF) as f32) / 65536.0) * 2.0 - 1.0
    };

    // ReGameDLL/EV_CreateSmoke parity: 20 drifting expanding clouds, 15s hold + 15s fade = 30s lifetime.
    for i in 0..20 {
        let ang = (i as f32) * std::f32::consts::TAU / 20.0 + rand_unit() * 0.25;
        let rad = if i == 0 {
            0.0
        } else {
            20.0 + ((i * 19) % 78) as f32
        };
        let offset = Vec3::new(
            ang.cos() * rad + rand_unit() * 14.0,
            24.0 + (i % 4) as f32 * 18.0 + rand_unit() * 8.0,
            ang.sin() * rad + rand_unit() * 14.0,
        );
        let drift_vel = Vec3::new(
            ang.cos() * (12.0 + (i % 3) as f32 * 6.0) + rand_unit() * 6.0,
            3.5 + rand_unit() * 2.5,
            ang.sin() * (12.0 + (i % 3) as f32 * 6.0) + rand_unit() * 6.0,
        );
        let gray = (210.0 + (i % 5) as f32 * 5.0) / 255.0;
        let cloud_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(gray, gray, gray, 0.78),
            base_color_texture: tex_handle.clone(),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(cloud_mat.clone()),
            Transform::from_translation(origin + offset).with_scale(Vec3::splat(0.45)),
            bevy::render::view::NoFrustumCulling,
            SmokeBillboardParticle {
                velocity: drift_vel,
                lifetime: 0.0,
                max_lifetime: 30.0,
                hold_fade_at: 15.0,
                start_scale: 0.45,
                end_scale: 1.30,
                grow_duration: 3.5,
                base_rgb: [gray, gray, gray],
                base_alpha: 0.78,
            },
            AnimatedSprite {
                materials: vec![cloud_mat],
                current_frame: 0,
                timer: 0.0,
                frame_duration: 32.0,
                is_looping: false,
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
#[allow(clippy::too_many_arguments)]
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
        let rmesh = meshes.add(Rectangle::new(14.0, 14.0).mesh());
        commands.spawn((
            Mesh3d(rmesh),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(hit_pos + wall_normal * 1.2),
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

    let streak_mesh = meshes.add(Rectangle::new(1.4, 5.5).mesh());
    let mat_bright = make_spark_material(
        materials,
        dot_tex.clone(),
        Color::srgba(1.0, 0.90, 0.45, 0.65),
        LinearRgba::rgb(2.2, 1.7, 0.55),
    );
    let mat_amber = make_spark_material(
        materials,
        dot_tex,
        Color::srgba(1.0, 0.60, 0.15, 0.55),
        LinearRgba::rgb(1.8, 0.9, 0.2),
    );

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

    for i in 0..8 {
        let lat_a = (next_f32() - 0.5) * 2.0;
        let lat_b = (next_f32() - 0.5) * 2.0;
        let out = 0.5 + next_f32() * 1.1;
        let speed = 110.0 + next_f32() * 170.0;
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
            Transform::from_translation(hit_pos + wall_normal * 1.0).with_rotation(rot),
            SparkStreakParticle {
                velocity: vel,
                lifetime: 0.0,
                max_lifetime: 0.16 + next_f32() * 0.14,
            },
        ));
    }

    // 3. Subtle `wall_puff1..4.spr` gunshot smoke puff (`EV_CS16Client_CreateSmoke`)
    let puff_idx = ((hit_pos.x.abs() * 3.0 + hit_pos.z.abs()) as usize % 4) + 1;
    let puff_path = format!("sprites/wall_puff{puff_idx}.spr");
    if let Some(puff) = sprite_store.get_or_load_sprite(&puff_path, search_paths, images, materials)
        && let Some(first_mat) = puff.materials.first().cloned()
    {
        let pmesh = meshes.add(Rectangle::new(22.0, 22.0).mesh());
        commands.spawn((
            Mesh3d(pmesh),
            MeshMaterial3d(first_mat),
            Transform::from_translation(hit_pos + wall_normal * 3.0),
            bevy::render::view::NoFrustumCulling,
            AnimatedSprite {
                materials: puff.materials,
                current_frame: 0,
                timer: 0.0,
                frame_duration: 1.0 / 35.0,
                is_looping: false,
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

    let mesh = meshes.add(Rectangle::new(112.0, 112.0).mesh());
    let decal_pos = hit_pos + surface_normal * 0.25;
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
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_translation(decal_pos).with_rotation(rot),
        BulletHoleDecal {
            lifetime: 0.0,
            max_lifetime: 90.0,
        },
    ));
}

/// Spawns a bullet or glass impact decal (`{shot1..5`, `{break1..3`, `{bproof1`),
/// optionally attached in local space to a moving BSP submodel parent (e.g., `func_door`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_surface_decal(
    commands: &mut Commands,
    hit_pos: Vec3,
    wall_normal: Vec3,
    kind: ImpactDecalKind,
    attach_to: Option<(Entity, Vec3, Quat)>,
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

    let size = match kind {
        ImpactDecalKind::BreakableGlass => 48.0,
        ImpactDecalKind::BulletShot | ImpactDecalKind::UnbreakableGlass => 22.0,
    };
    let mesh = meshes.add(Rectangle::new(size, size).mesh());
    let decal_pos = hit_pos + wall_normal * 0.12;
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
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(local_pos).with_rotation(local_rot),
                BulletHoleDecal {
                    lifetime: 0.0,
                    max_lifetime: 60.0,
                },
            ))
            .id();
        commands.entity(parent_ent).add_child(child);
    } else {
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

/// Spawns `0..=3` bouncing `CShower` spark emitters matching ReGameDLL `Explode` / `Explode2` / `Explode3`.
pub fn spawn_spark_shower(commands: &mut Commands, origin: Vec3, normal: Vec3, seed_bits: u32) {
    let count = (seed_bits % 4) as usize;
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
        let base_spd = rand_range(200.0, 300.0);
        let vel = n * base_spd
            + Vec3::new(
                rand_range(-100.0, 100.0),
                rand_range(-200.0, 200.0),
                rand_range(-100.0, 100.0),
            );
        commands.spawn(SparkShowerParticle {
            pos: origin,
            velocity: vel,
            lifetime: 0.0,
            max_lifetime: rand_range(0.5, 1.5),
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
            let mesh = meshes.add(Rectangle::new(sz, sz).mesh());
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(first_mat),
                Transform::from_translation(origin + offset),
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
        let mesh = meshes.add(Rectangle::new(24.0, 24.0).mesh());
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(first_mat),
            Transform::from_translation(origin + Vec3::Y * 5.0),
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
        let mesh = meshes.add(Rectangle::new(72.0, 72.0).mesh());
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
            Mesh3d(mesh),
            MeshMaterial3d(puff_mat.clone()),
            Transform::from_translation(origin + dir * 6.0).with_scale(Vec3::splat(0.32)),
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
        shower.velocity.y -= 400.0 * dt;
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
            shower.emit_timer = 0.1;
            let streak_mesh = meshes.add(Rectangle::new(1.6, 5.2).mesh());
            let mat = materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.85, 0.35, 0.65),
                emissive: LinearRgba::rgb(2.5, 1.6, 0.4),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                ..default()
            });
            let mut seed = shower.pos.x.to_bits() ^ shower.lifetime.to_bits().rotate_left(7);
            for _ in 0..6 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let rx = (((seed & 0xFF) as f32) / 128.0) - 1.0;
                let ry = ((((seed >> 8) & 0xFF) as f32) / 128.0) - 0.5;
                let rz = ((((seed >> 16) & 0xFF) as f32) / 128.0) - 1.0;
                let vel = Vec3::new(rx * 180.0, ry * 180.0 + 40.0, rz * 180.0);
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
                        max_lifetime: 0.22,
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
                let mesh = meshes.add(Rectangle::new(eff.size, eff.size).mesh());
                commands.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(first_mat),
                    Transform::from_translation(eff.origin + Vec3::Y * (eff.size * 0.25)),
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
            (
                update_animated_sprites_system,
                update_smoke_particles_system,
                update_spark_streaks_system,
                update_spark_showers_system,
                update_delayed_smokes_system,
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
