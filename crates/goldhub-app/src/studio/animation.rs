//! High-Hz (144+ FPS) skeletal animation evaluator with quaternion SLERP.

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;

use super::store::StudioModelStore;

/// Component attached to entities playing a StudioModel skeletal animation.
#[derive(Component, Debug, Clone)]
pub struct StudioAnimatedModel {
    pub model_id: String,
    pub sequence_index: usize,
    pub frame_progress: f32,
    pub fps: f32,
    pub is_looping: bool,
    pub is_viewmodel: bool,
    /// List of (child_entity, mesh_asset_handle, submesh_index)
    pub mesh_entities: Vec<(Entity, Handle<Mesh>, usize)>,
}

impl StudioAnimatedModel {
    /// Creates a new animation controller for a given model.
    #[must_use]
    pub fn new(model_id: impl Into<String>, is_viewmodel: bool) -> Self {
        Self {
            model_id: model_id.into(),
            sequence_index: 0,
            frame_progress: 0.0,
            fps: 30.0,
            is_looping: true,
            is_viewmodel,
            mesh_entities: Vec::new(),
        }
    }

    /// Sets the active sequence, resetting frame progress and updating FPS.
    pub fn set_sequence(&mut self, seq_idx: usize, fps: f32, looping: bool) {
        if self.sequence_index != seq_idx {
            self.sequence_index = seq_idx;
            self.frame_progress = 0.0;
            self.fps = fps;
            self.is_looping = looping;
        }
    }
}

/// Evaluates skeletal bone poses at sub-frame precision and skins vertex attributes in-place.
pub fn studio_animation_system(
    time: Res<Time>,
    model_store: Res<StudioModelStore>,
    mut query: Query<&mut StudioAnimatedModel>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for mut anim in &mut query {
        let normalized_id = anim.model_id.replace('\\', "/").to_lowercase();
        let Some(model_data) = model_store.models.get(&normalized_id) else {
            continue;
        };

        let sequences = &model_data.model.rig.sequences;
        if anim.sequence_index >= sequences.len() {
            continue;
        }

        let seq = &sequences[anim.sequence_index];
        let num_frames = seq.bone_matrices.len();
        if num_frames == 0 {
            continue;
        }

        // 1. Advance frame time
        anim.frame_progress += dt * anim.fps;

        let total_f = num_frames as f32;
        if anim.is_looping {
            anim.frame_progress = anim.frame_progress.rem_euclid(total_f);
        } else if anim.frame_progress >= (total_f - 1.0).max(0.0) {
            anim.frame_progress = (total_f - 1.0).max(0.0);
        }

        // 2. Compute SLERP interpolated bone matrices for the current fractional frame
        let f0 = (anim.frame_progress.floor() as usize).min(num_frames - 1);
        let f1 = if anim.is_looping {
            (f0 + 1) % num_frames
        } else {
            (f0 + 1).min(num_frames - 1)
        };
        let alpha = anim.frame_progress - anim.frame_progress.floor();

        let m0_slice = &seq.bone_matrices[f0];
        let m1_slice = &seq.bone_matrices[f1];
        let num_bones = m0_slice.len().min(m1_slice.len());

        // Precompute interpolated bone transforms for this frame
        let mut interp_bones = Vec::with_capacity(num_bones);
        for b in 0..num_bones {
            let mat0 = Mat4::from_cols_array(&m0_slice[b]);
            let mat1 = Mat4::from_cols_array(&m1_slice[b]);

            let (_s0, r0, t0) = mat0.to_scale_rotation_translation();
            let (_s1, r1, t1) = mat1.to_scale_rotation_translation();

            let rot = r0.slerp(r1, alpha);
            let trans = t0.lerp(t1, alpha);
            interp_bones.push(Mat4::from_rotation_translation(rot, trans));
        }

        // 3. Deform each submesh's vertices and normals in-place
        let is_vm = anim.is_viewmodel;
        for &(_, ref mesh_handle, submesh_idx) in &anim.mesh_entities {
            let Some(template) = model_data.submeshes.get(submesh_idx) else {
                continue;
            };
            let Some(mesh) = meshes.get_mut(mesh_handle) else {
                continue;
            };

            let Some(VertexAttributeValues::Float32x3(pos_vec)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };

            // In-place position transformation
            for (v, out_p) in template.rest_vertices.iter().zip(pos_vec.iter_mut()) {
                let bone_idx = v.bone_index as usize;
                if bone_idx < interp_bones.len() {
                    let bm = &interp_bones[bone_idx];
                    let p = bm.transform_point3(Vec3::from(v.position));
                    if is_vm {
                        *out_p = [-p.y, p.z, -p.x];
                    } else {
                        *out_p = [p.x, p.z, -p.y];
                    }
                }
            }

            // In-place normal transformation
            if let Some(VertexAttributeValues::Float32x3(norm_vec)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
            {
                for (v, out_n) in template.rest_vertices.iter().zip(norm_vec.iter_mut()) {
                    let bone_idx = v.bone_index as usize;
                    if bone_idx < interp_bones.len() {
                        let bm = &interp_bones[bone_idx];
                        let n = bm.transform_vector3(Vec3::from(v.normal));
                        if is_vm {
                            *out_n = [-n.y, n.z, -n.x];
                        } else {
                            *out_n = [n.x, n.z, -n.y];
                        }
                    }
                }
            }
        }
    }
}
