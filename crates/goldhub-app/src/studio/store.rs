//! StudioModel (.mdl) asset store, texture conversion, and template mesh cache.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::legacy::StudioMdlDecoder;
use goldhub_formats::ir::{StudioModel, StudioVertex};

use crate::audio::GameSearchPaths;

/// Pre-processed template for a single mesh partition in a submodel.
#[derive(Clone)]
pub struct SubmeshTemplate {
    pub body_part_name: String,
    pub body_part_index: usize,
    pub submodel_index: usize,
    pub skin_ref: usize,
    pub material: Handle<StandardMaterial>,
    pub lit_material: Handle<StandardMaterial>,
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
    pub static_meshes: HashMap<String, Vec<Handle<Mesh>>>,
}

impl StudioModelStore {
    /// Spawns static child submesh entities under `parent_entity`, reusing cached `Handle<Mesh>` instances
    /// to avoid continuous allocations on hot paths (e.g. bullet brass casings and static world drops).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_static_model_submesh_children(
        &mut self,
        commands: &mut Commands,
        parent_entity: Entity,
        model_data: &StudioModelData,
        model_key: &str,
        is_viewmodel: bool,
        render_layer: Option<usize>,
        meshes: &mut Assets<Mesh>,
    ) {
        let key = model_key.replace('\\', "/").to_lowercase();
        let handles = self.static_meshes.entry(key).or_insert_with(|| {
            model_data
                .submeshes
                .iter()
                .map(|template| {
                    let bevy_mesh =
                        create_sequence_frame0_mesh(model_data, 0, template, is_viewmodel);
                    meshes.add(bevy_mesh)
                })
                .collect()
        });

        for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
            let mesh_handle = handles[sub_idx].clone();
            let initial_vis = if template.submodel_index == 0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            let mat = if is_viewmodel {
                template.material.clone()
            } else {
                template.lit_material.clone()
            };
            let mut child_cmd = commands.spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat),
                Transform::IDENTITY,
                initial_vis,
                bevy::render::view::NoFrustumCulling,
            ));
            if let Some(layer) = render_layer {
                child_cmd.insert(bevy::render::view::RenderLayers::layer(layer));
            }
            let child = child_cmd.id();
            commands.entity(parent_entity).add_child(child);
        }
    }
    /// Loads or retrieves a cached StudioModel by logical relative path (e.g. "models/v_knife.mdl").
    #[allow(clippy::too_many_lines)]
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

        // Check if companion external texture model exists (e.g. scientistT.mdl for scientist.mdl)
        let mut external_tex_bytes = None;
        if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
            let t_name_upper = format!("{stem}T.mdl");
            let t_name_lower = format!("{stem}t.mdl");
            let t_path_direct_upper = file_path.with_file_name(&t_name_upper);
            let t_path_direct_lower = file_path.with_file_name(&t_name_lower);
            if t_path_direct_upper.exists() {
                external_tex_bytes = fs::read(&t_path_direct_upper).ok();
            } else if t_path_direct_lower.exists() {
                external_tex_bytes = fs::read(&t_path_direct_lower).ok();
            } else if let Some(resolved_t) =
                resolve_model_file_path(&format!("models/{t_name_upper}"), search_paths)
            {
                external_tex_bytes = fs::read(&resolved_t).ok();
            } else if let Some(resolved_t) =
                resolve_model_file_path(&format!("models/{t_name_lower}"), search_paths)
            {
                external_tex_bytes = fs::read(&resolved_t).ok();
            }
        }

        let model: StudioModel =
            match decoder.decode_model_with_textures(&raw_bytes, external_tex_bytes.as_deref()) {
                Ok(m) => m,
                Err(err) => {
                    tracing::warn!(path = ?file_path, error = ?err, "Failed to decode StudioModel");
                    return None;
                }
            };

        // 1. Upload textures to Bevy Image assets and create StandardMaterials
        let mut material_handles = Vec::with_capacity(model.textures.len());
        let mut lit_material_handles = Vec::with_capacity(model.textures.len());
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
            let alpha_mode = if is_masked {
                AlphaMode::Mask(0.5)
            } else {
                AlphaMode::Opaque
            };

            // Material for first-person viewmodels (pure Lambertian diffuse without fake specular glare)
            let vm_mat = StandardMaterial {
                base_color_texture: Some(img_handle.clone()),
                cull_mode: None, // Two-sided rendering
                unlit: false,
                perceptual_roughness: 0.95,
                metallic: 0.0,
                reflectance: 0.0,
                alpha_mode,
                ..default()
            };
            material_handles.push(materials.add(vm_mat));

            // Lit material for world models (receives dynamic shadows and scene illumination)
            let lit_mat = StandardMaterial {
                base_color_texture: Some(img_handle),
                cull_mode: None,
                unlit: false,
                perceptual_roughness: 0.95,
                metallic: 0.0,
                reflectance: 0.0,
                alpha_mode,
                ..default()
            };
            lit_material_handles.push(materials.add(lit_mat));
        }

        // 2. Extract submeshes from all bodyparts and their submodels (submodel 0 visible by default;
        // alternate submodels e.g. player `backpack` bodygroup 'defuse' spawned hidden until toggled).
        let mut submeshes = Vec::new();
        for (body_part_index, body_part) in model.body_parts.iter().enumerate() {
            for (submodel_index, submodel) in body_part.models.iter().enumerate() {
                for mesh in &submodel.meshes {
                    let tex_idx = model
                        .skin_families
                        .first()
                        .and_then(|fam| fam.get(mesh.skin_ref).copied())
                        .map_or(mesh.skin_ref, |idx| idx as usize);
                    let mat_handle = if tex_idx < material_handles.len() {
                        material_handles[tex_idx].clone()
                    } else {
                        // Fallback neutral material if skin_ref is out of range
                        materials.add(StandardMaterial {
                            base_color: Color::srgb(0.7, 0.7, 0.7),
                            cull_mode: None,
                            ..default()
                        })
                    };
                    let lit_mat_handle = if tex_idx < lit_material_handles.len() {
                        lit_material_handles[tex_idx].clone()
                    } else {
                        mat_handle.clone()
                    };

                    submeshes.push(SubmeshTemplate {
                        body_part_name: body_part.name.clone(),
                        body_part_index,
                        submodel_index,
                        skin_ref: mesh.skin_ref,
                        material: mat_handle,
                        lit_material: lit_mat_handle,
                        rest_vertices: mesh.vertices.clone(),
                        indices: mesh.indices.clone(),
                    });
                }
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
            (
                [-v.position[1], v.position[2], -v.position[0]],
                [-v.normal[1], v.normal[2], -v.normal[0]],
            )
        } else {
            // World entity coordinates: X_bevy = X_gs, Y_bevy = Z_gs, Z_bevy = -Y_gs
            (
                [v.position[0], v.position[2], -v.position[1]],
                [v.normal[0], v.normal[2], -v.normal[1]],
            )
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

/// Creates a new GPU Bevy `Mesh` initialized with bone-skinned vertices from `seq_idx` frame 0
/// (falling back to `rest_vertices` when no sampled bone matrices exist).
#[must_use]
pub fn create_sequence_frame0_mesh(
    model_data: &StudioModelData,
    seq_idx: usize,
    template: &SubmeshTemplate,
    is_viewmodel: bool,
) -> Mesh {
    let mut mesh = create_initial_bevy_mesh(template, is_viewmodel);
    let Some(m0_slice) = model_data
        .model
        .rig
        .sequences
        .get(seq_idx)
        .and_then(|s| s.bone_matrices.first())
    else {
        return mesh;
    };

    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(pos_vec)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for (v, out_p) in template.rest_vertices.iter().zip(pos_vec.iter_mut()) {
            let bone_idx = v.bone_index as usize;
            if bone_idx < m0_slice.len() {
                let bm = Mat4::from_cols_array(&m0_slice[bone_idx]);
                let p = bm.transform_point3(Vec3::from(v.position));
                *out_p = if is_viewmodel {
                    [-p.y, p.z, -p.x]
                } else {
                    [p.x, p.z, -p.y]
                };
            }
        }
    }
    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(norm_vec)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for (v, out_n) in template.rest_vertices.iter().zip(norm_vec.iter_mut()) {
            let bone_idx = v.bone_index as usize;
            if bone_idx < m0_slice.len() {
                let bm = Mat4::from_cols_array(&m0_slice[bone_idx]);
                let n = bm.transform_vector3(Vec3::from(v.normal));
                *out_n = if is_viewmodel {
                    [-n.y, n.z, -n.x]
                } else {
                    [n.x, n.z, -n.y]
                };
            }
        }
    }
    mesh
}

/// Spawns skinned or posed child submesh entities under `parent_entity` on `layer_opt`
/// and returns `(child_entity, mesh_handle, submesh_idx)` tuples.
pub fn spawn_model_submesh_children(
    commands: &mut Commands,
    parent_entity: Entity,
    model_data: &StudioModelData,
    seq_idx: usize,
    is_viewmodel: bool,
    render_layer: Option<usize>,
    meshes: &mut Assets<Mesh>,
) -> Vec<(Entity, Handle<Mesh>, usize)> {
    let mut children = Vec::with_capacity(model_data.submeshes.len());
    for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
        let bevy_mesh = create_sequence_frame0_mesh(model_data, seq_idx, template, is_viewmodel);
        let mesh_handle = meshes.add(bevy_mesh);
        let initial_vis = if template.submodel_index == 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let mat = if is_viewmodel {
            template.material.clone()
        } else {
            template.lit_material.clone()
        };
        let mut child_cmd = commands.spawn((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            initial_vis,
            bevy::render::view::NoFrustumCulling,
        ));
        if let Some(layer) = render_layer {
            child_cmd.insert(bevy::render::view::RenderLayers::layer(layer));
        }
        let child = child_cmd.id();
        commands.entity(parent_entity).add_child(child);
        children.push((child, mesh_handle, sub_idx));
    }
    children
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
