//! Legacy GoldSrc 6-sided Skybox cube loader and renderer.
//!
//! Complies with the Scrooge Systems Mindset: zero allocation during runtime updates,
//! contiguous single-entity hierarchical layout, and unlit GPU materials.

use std::fs;
use std::path::{Path, PathBuf};

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::legacy::decode_tga;

const SKYBOX_HALF_SIZE: f32 = 4000.0;

/// Marker component for the skybox root entity that follows the camera position.
#[derive(Component)]
pub struct SkyboxRoot;

/// System that locks the Skybox entity position to the camera in world space,
/// creating the illusion of infinite distance.
pub fn update_skybox_position(
    camera_query: Query<&Transform, (With<Camera3d>, Without<SkyboxRoot>)>,
    mut skybox_query: Query<&mut Transform, With<SkyboxRoot>>,
) {
    if let Ok(cam_tf) = camera_query.get_single() {
        for mut sky_tf in &mut skybox_query {
            sky_tf.translation = cam_tf.translation;
        }
    }
}

/// Discovers the 6 skybox TGA face textures on disk.
fn find_skybox_file(sky_name: &str, suffix: &str) -> Option<PathBuf> {
    let candidate_dirs = [
        "assets/gfx/env",
        "cstrike/gfx/env",
        "valve/gfx/env",
        "C:/Games/CS 1.6 GoldClient/cstrike/gfx/env",
        "C:/Games/CS 1.6 GoldClient/valve/gfx/env",
        "C:/Games/Counter-Strike 1.6/cstrike/gfx/env",
        "C:/Games/Counter-Strike 1.6/valve/gfx/env",
        "D:/Repo/CS16/hlmsl/gfx/env",
        "C:/Program Files (x86)/Steam/steamapps/common/Half-Life/cstrike/gfx/env",
        "C:/Program Files (x86)/Steam/steamapps/common/Half-Life/valve/gfx/env",
    ];

    let filename = format!("{sky_name}{suffix}.tga");
    let filename_lower = format!("{}{}.tga", sky_name.to_lowercase(), suffix);

    for dir in candidate_dirs {
        let p1 = Path::new(dir).join(&filename);
        if p1.exists() {
            return Some(p1);
        }
        let p2 = Path::new(dir).join(&filename_lower);
        if p2.exists() {
            return Some(p2);
        }
    }
    None
}

/// Creates a quad mesh for one side of the skybox cube pointing inward with exact UVs.
fn create_skybox_face_mesh(positions: [[f32; 3]; 4], uvs: [[f32; 2]; 4]) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    let normals: [[f32; 3]; 4] = [[0.0, 0.0, 0.0]; 4];
    let indices = vec![0u32, 1, 2, 0, 2, 3];

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.to_vec());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.to_vec());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.to_vec());
    mesh.insert_indices(Indices::U32(indices));

    mesh
}

/// Spawns the 6-sided GoldSrc Skybox cube into the Bevy world.
#[allow(clippy::too_many_lines)]
pub fn spawn_skybox(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    sky_name: &str,
) {
    let r = SKYBOX_HALF_SIZE;

    // Mathematically exact GoldSrc skybox projections from Xash3D st_to_vec / MakeSkyVec:
    // Suffixes: rt, lf, bk, ft, up, dn
    let sides = [
        // 0: Right (rt)
        (
            "rt",
            [[r, -r, -r], [r, r, -r], [r, r, r], [r, -r, r]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        ),
        // 1: Left (lf)
        (
            "lf",
            [[-r, -r, r], [-r, r, r], [-r, r, -r], [-r, -r, -r]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        ),
        // 2: Back (bk)
        (
            "bk",
            [[-r, -r, -r], [-r, r, -r], [r, r, -r], [r, -r, -r]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        ),
        // 3: Front (ft)
        (
            "ft",
            [[r, -r, r], [r, r, r], [-r, r, r], [-r, -r, r]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        ),
        // 4: Up (up)
        (
            "up",
            [[r, r, -r], [-r, r, -r], [-r, r, r], [r, r, r]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        ),
        // 5: Down (dn)
        (
            "dn",
            [[-r, -r, -r], [r, -r, -r], [r, -r, r], [-r, -r, r]],
            [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        ),
    ];

    let mut face_entities = Vec::with_capacity(6);

    for (suffix, face_positions, face_uvs) in sides {
        let quad_mesh = create_skybox_face_mesh(face_positions, face_uvs);
        let mesh_handle = meshes.add(quad_mesh);

        let img_handle = if let Some(path) = find_skybox_file(sky_name, suffix)
            && let Ok(file_bytes) = fs::read(&path)
            && let Ok(tga) = decode_tga(&file_bytes)
        {
            let mut img = Image::new(
                Extent3d {
                    width: tga.width,
                    height: tga.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                tga.rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::ClampToEdge,
                address_mode_v: ImageAddressMode::ClampToEdge,
                address_mode_w: ImageAddressMode::ClampToEdge,
                mag_filter: bevy::image::ImageFilterMode::Linear,
                min_filter: bevy::image::ImageFilterMode::Linear,
                ..default()
            });
            Some(images.add(img))
        } else {
            None
        };

        let mat = StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: img_handle,
            unlit: true,
            cull_mode: None,
            depth_bias: -10000.0,
            ..default()
        };
        let mat_handle = materials.add(mat);

        let child = commands
            .spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat_handle),
                Transform::IDENTITY,
            ))
            .id();
        face_entities.push(child);
    }

    commands
        .spawn((
            SkyboxRoot,
            Transform::from_xyz(0.0, 0.0, 0.0),
            Visibility::default(),
        ))
        .add_children(&face_entities);

    tracing::info!(sky_name, "Spawned GoldSrc 6-sided Skybox cube into scene");
}
