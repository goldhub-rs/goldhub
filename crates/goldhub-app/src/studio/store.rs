//! StudioModel (.mdl) asset store, texture conversion, and template mesh cache.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::StudioMdlDecoder;
use goldhub_formats::ir::{StudioModel, StudioVertex};

use crate::audio::GameSearchPaths;

/// Pre-processed template for a single mesh partition in a submodel.
#[derive(Clone)]
pub struct SubmeshTemplate {
    pub skin_ref: usize,
    pub material: Handle<StandardMaterial>,
    pub rest_vertices: Vec<StudioVertex>,
    pub indices: Vec<u32>,
}

/// Fully decoded and GPU-prepared StudioModel data.
pub struct StudioModelData {
    pub model: StudioModel,
    pub submeshes: Vec<SubmeshTemplate>,
}

/// Resource managing loaded Studio Models, shared textures, and materials.
#[derive(Resource, Default)]
pub struct StudioModelStore {
    pub models: HashMap<String, Arc<StudioModelData>>,
}

impl StudioModelStore {
    /// Loads or retrieves a cached StudioModel by logical relative path (e.g. "models/v_knife.mdl").
    pub fn get_or_load(
        &mut self,
        model_name: &str,
        search_paths: &GameSearchPaths,
        images: &mut ResMut<Assets<Image>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> Option<Arc<StudioModelData>> {
        let normalized_key = model_name.replace('\\', "/").to_lowercase();
        if let Some(cached) = self.models.get(&normalized_key) {
            return Some(Arc::clone(cached));
        }

        let file_path = resolve_model_file_path(model_name, search_paths)?;
        let raw_bytes = fs::read(&file_path).ok()?;
        let decoder = StudioMdlDecoder;

        let model: StudioModel = match decoder.decode(&raw_bytes) {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(path = ?file_path, error = ?err, "Failed to decode StudioModel");
                return None;
            }
        };

        // 1. Upload textures to Bevy Image assets and create StandardMaterials
        let mut material_handles = Vec::with_capacity(model.textures.len());
        for tex in &model.textures {
            let image = Image::new(
                Extent3d {
                    width: tex.width,
                    height: tex.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                tex.rgba_pixels.clone(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            let img_handle = images.add(image);

            let is_masked = (tex.flags & 0x0040) != 0;
            let mat = StandardMaterial {
                base_color_texture: Some(img_handle),
                perceptual_roughness: 0.75,
                metallic: 0.15,
                cull_mode: None, // Two-sided rendering
                alpha_mode: if is_masked {
                    AlphaMode::Mask(0.5)
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            };
            material_handles.push(materials.add(mat));
        }

        // 2. Extract submeshes from the first bodypart and primary submodel
        let mut submeshes = Vec::new();
        if let Some(body_part) = model.body_parts.first()
            && let Some(submodel) = body_part.models.first()
        {
            for mesh in &submodel.meshes {
                let mat_handle = if mesh.skin_ref < material_handles.len() {
                    material_handles[mesh.skin_ref].clone()
                } else {
                    // Fallback neutral material if skin_ref is out of range
                    materials.add(StandardMaterial {
                        base_color: Color::srgb(0.7, 0.7, 0.7),
                        cull_mode: None,
                        ..default()
                    })
                };

                submeshes.push(SubmeshTemplate {
                    skin_ref: mesh.skin_ref,
                    material: mat_handle,
                    rest_vertices: mesh.vertices.clone(),
                    indices: mesh.indices.clone(),
                });
            }
        }

        let data = Arc::new(StudioModelData { model, submeshes });
        self.models.insert(normalized_key, Arc::clone(&data));
        Some(data)
    }
}

/// Creates a new GPU Bevy `Mesh` initialized with rest-pose vertices from the template.
#[must_use]
pub fn create_initial_bevy_mesh(template: &SubmeshTemplate, is_viewmodel: bool) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    let mut positions = Vec::with_capacity(template.rest_vertices.len());
    let mut normals = Vec::with_capacity(template.rest_vertices.len());
    let mut uvs = Vec::with_capacity(template.rest_vertices.len());

    for v in &template.rest_vertices {
        let (p, n) = if is_viewmodel {
            // Camera local coordinates: X_cam = -Y_gs, Y_cam = Z_gs, Z_cam = -X_gs
            ([-v.position[1], v.position[2], -v.position[0]], [-v.normal[1], v.normal[2], -v.normal[0]])
        } else {
            // World entity coordinates: X_bevy = X_gs, Y_bevy = Z_gs, Z_bevy = -Y_gs
            ([v.position[0], v.position[2], -v.position[1]], [v.normal[0], v.normal[2], -v.normal[1]])
        };
        positions.push(p);
        normals.push(n);
        uvs.push(v.uv);
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(template.indices.clone()));

    mesh
}

/// Default root directories for locating CS 1.6 game assets.
const DEFAULT_ROOTS: &[&str] = &[
    "C:/Games/CS 1.6 GoldClient/cstrike",
    "C:/Program Files (x86)/Steam/steamapps/common/Half-Life/cstrike",
    "C:/Games/Counter-Strike 1.6/cstrike",
    "D:/Games/CS 1.6/cstrike",
    "assets",
];

/// Resolves a relative GoldSrc model path across search paths and default locations.
fn resolve_model_file_path(model_name: &str, search_paths: &GameSearchPaths) -> Option<PathBuf> {
    let clean = model_name.replace('\\', "/");
    let relative_name = clean.strip_prefix('/').unwrap_or(&clean);

    // 1. Check configured game search paths
    for base_dir in &search_paths.dirs {
        let candidate = base_dir.join(relative_name);
        if candidate.exists() {
            return Some(candidate);
        }
        // Also check if stripped "models/" exists under base_dir/models
        if let Some(stripped) = relative_name.strip_prefix("models/") {
            let candidate_models = base_dir.join("models").join(stripped);
            if candidate_models.exists() {
                return Some(candidate_models);
            }
        }
    }

    // 2. Fallback to standard game directories
    for root in DEFAULT_ROOTS {
        let candidate = Path::new(root).join(relative_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}
