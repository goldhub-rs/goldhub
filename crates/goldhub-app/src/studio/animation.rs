//! High-Hz (144+ FPS) skeletal animation evaluator with quaternion SLERP
//! and GoldSrc upper/lower body (`gaitsequence`) skeletal bone splitting.

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use goldhub_formats::ir::{BoneData, SequenceData};

use super::store::StudioModelStore;

/// Snapshot of the last skeletal pose committed to GPU vertex buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinnedPoseKey {
    pub sequence_index: usize,
    pub frame_progress: f32,
    pub gait_sequence_index: Option<usize>,
    pub gait_frame_progress: f32,
    pub blending: [u8; 2],
}

/// Component attached to entities playing a StudioModel skeletal animation.
#[derive(Component, Debug, Clone)]
pub struct StudioAnimatedModel {
    pub model_id: String,
    pub sequence_index: usize,
    pub frame_progress: f32,
    pub fps: f32,
    pub is_looping: bool,
    pub hold_at_end: bool,
    pub idle_sequence_index: usize,
    pub is_viewmodel: bool,
    pub last_event_frame: i32,
    /// Animation blend parameters (`[yaw_or_0, pitch]` in `0..=255`; `128` = neutral/level).
    pub blending: [u8; 2],
    /// Optional lower-body locomotion gait sequence (`pev->gaitsequence` parity).
    /// When `Some`, bones from pelvis down to legs use the gait sequence while bones from
    /// `Bip01 Spine` upward use `sequence_index`.
    pub gait_sequence_index: Option<usize>,
    pub gait_frame_progress: f32,
    pub gait_fps: f32,
    /// List of (child_entity, mesh_asset_handle, submesh_index)
    pub mesh_entities: Vec<(Entity, Handle<Mesh>, usize)>,
    /// Interpolated bone transforms evaluated during the current frame.
    pub current_bone_transforms: Vec<Mat4>,
    /// Previous sequence pose snapshot used for `0.2s` `StudioSlerpBones` cross-fade.
    pub prev_bone_transforms: Vec<Mat4>,
    /// Remaining duration (`0.0..=0.2s`) of cross-sequence bone SLERP blend.
    pub blend_timer: f32,
    /// Last pose state committed to GPU meshes (`None` before first skin pass).
    pub last_skinned_pose: Option<SkinnedPoseKey>,
}

impl StudioAnimatedModel {
    /// GoldSrc `StudioSetupBones` cross-sequence transition blend duration.
    pub const CROSSFADE_DURATION: f32 = 0.2;

    /// Creates a new animation controller for a given model.
    #[must_use]
    pub fn new(model_id: impl Into<String>, is_viewmodel: bool) -> Self {
        Self {
            model_id: model_id.into(),
            sequence_index: 0,
            frame_progress: 0.0,
            fps: 30.0,
            is_looping: true,
            hold_at_end: false,
            idle_sequence_index: 0,
            is_viewmodel,
            last_event_frame: -1,
            blending: [128, 128],
            gait_sequence_index: None,
            gait_frame_progress: 0.0,
            gait_fps: 30.0,
            mesh_entities: Vec::new(),
            current_bone_transforms: Vec::new(),
            prev_bone_transforms: Vec::new(),
            blend_timer: 0.0,
            last_skinned_pose: None,
        }
    }

    fn snapshot_blend_transition(&mut self) {
        if !self.current_bone_transforms.is_empty() {
            self.prev_bone_transforms
                .clone_from(&self.current_bone_transforms);
            self.blend_timer = Self::CROSSFADE_DURATION;
        }
    }

    /// Sets the active primary sequence, resetting frame progress and updating FPS.
    pub fn set_sequence(&mut self, seq_idx: usize, fps: f32, looping: bool) {
        if self.sequence_index != seq_idx || !looping {
            if self.sequence_index != seq_idx {
                self.snapshot_blend_transition();
            }
            self.sequence_index = seq_idx;
            self.frame_progress = 0.0;
            self.fps = fps;
            self.is_looping = looping;
            self.hold_at_end = false;
            self.last_event_frame = -1;
        }
    }

    /// Sets a one-shot primary sequence that freezes on its last frame when finished
    /// (used for grenade pin-pull cocked pose while LMB is held).
    pub fn set_sequence_hold(&mut self, seq_idx: usize, fps: f32) {
        if self.sequence_index != seq_idx || !self.hold_at_end {
            if self.sequence_index != seq_idx {
                self.snapshot_blend_transition();
            }
            self.sequence_index = seq_idx;
            self.frame_progress = 0.0;
            self.fps = fps;
            self.is_looping = false;
            self.hold_at_end = true;
            self.last_event_frame = -1;
        }
    }

    /// Sets the lower-body gait locomotion sequence (`gaitsequence`) without resetting
    /// `gait_frame_progress` unless the sequence index actually changed.
    pub fn set_gait_sequence(&mut self, seq_idx: usize, fps: f32) {
        if self.gait_sequence_index != Some(seq_idx) {
            self.gait_sequence_index = Some(seq_idx);
            self.gait_frame_progress = 0.0;
        }
        self.gait_fps = fps;
    }
}

/// Interpolates a single bone `b` between `(f0, f1, alpha)` for a given blend slice.
#[inline]
fn eval_bone_at_frames(
    blend_frames: &[Vec<[f32; 16]>],
    f0: usize,
    f1: usize,
    alpha: f32,
    b: usize,
) -> (Quat, Vec3) {
    let m0 = Mat4::from_cols_array(&blend_frames[f0][b]);
    let m1 = Mat4::from_cols_array(&blend_frames[f1][b]);
    let (_s0, r0, t0) = m0.to_scale_rotation_translation();
    let (_s1, r1, t1) = m1.to_scale_rotation_translation();
    (r0.slerp(r1, alpha), t0.lerp(t1, alpha))
}

/// Samples SLERP-interpolated world bone matrices (supporting 9-way blend grids) into `out`.
fn sample_interpolated_bones(
    seq: &SequenceData,
    frame_progress: f32,
    looping: bool,
    blending: [u8; 2],
    out: &mut Vec<Mat4>,
) {
    out.clear();
    let num_frames = seq.bone_matrices.len();
    if num_frames == 0 {
        return;
    }

    let f0 = (frame_progress.floor() as usize).min(num_frames - 1);
    let f1 = if looping {
        (f0 + 1) % num_frames
    } else {
        (f0 + 1).min(num_frames - 1)
    };
    let alpha = (frame_progress - frame_progress.floor()).clamp(0.0, 1.0);

    // Full 9-way GoldSrc blend grid evaluation (e.g., player aim pitch rows 0..2)
    if seq.blend_count == 9
        && seq.blend_bone_matrices.len() == 9
        && seq.blend_bone_matrices.iter().all(|b| b.len() > f1)
    {
        let u = (f32::from(blending[0]) / 255.0) * 2.0;
        let v = (f32::from(blending[1]) / 255.0) * 2.0;
        let bx0 = (u.floor() as usize).min(1);
        let bx1 = bx0 + 1;
        let tx = (u - bx0 as f32).clamp(0.0, 1.0);
        let by0 = (v.floor() as usize).min(1);
        let by1 = by0 + 1;
        let ty = (v - by0 as f32).clamp(0.0, 1.0);

        let b00 = &seq.blend_bone_matrices[by0 * 3 + bx0];
        let b10 = &seq.blend_bone_matrices[by0 * 3 + bx1];
        let b01 = &seq.blend_bone_matrices[by1 * 3 + bx0];
        let b11 = &seq.blend_bone_matrices[by1 * 3 + bx1];

        let num_bones = b00[f0].len().min(b11[f1].len());
        out.reserve(num_bones);
        for b in 0..num_bones {
            let (r00, t00) = eval_bone_at_frames(b00, f0, f1, alpha, b);
            let (r10, t10) = eval_bone_at_frames(b10, f0, f1, alpha, b);
            let (r01, t01) = eval_bone_at_frames(b01, f0, f1, alpha, b);
            let (r11, t11) = eval_bone_at_frames(b11, f0, f1, alpha, b);

            let r_top = r00.slerp(r10, tx);
            let t_top = t00.lerp(t10, tx);
            let r_bot = r01.slerp(r11, tx);
            let t_bot = t01.lerp(t11, tx);

            let rot = r_top.slerp(r_bot, ty);
            let trans = t_top.lerp(t_bot, ty);
            out.push(Mat4::from_rotation_translation(rot, trans));
        }
        return;
    }

    let m0_slice = &seq.bone_matrices[f0];
    let m1_slice = &seq.bone_matrices[f1];
    let num_bones = m0_slice.len().min(m1_slice.len());

    out.reserve(num_bones);
    for b in 0..num_bones {
        let mat0 = Mat4::from_cols_array(&m0_slice[b]);
        let mat1 = Mat4::from_cols_array(&m1_slice[b]);

        let (_s0, r0, t0) = mat0.to_scale_rotation_translation();
        let (_s1, r1, t1) = mat1.to_scale_rotation_translation();

        let rot = r0.slerp(r1, alpha);
        let trans = t0.lerp(t1, alpha);
        out.push(Mat4::from_rotation_translation(rot, trans));
    }
}

fn is_gait_lower_bone(bones: &[BoneData], idx: usize) -> bool {
    let name = &bones[idx].name;
    if name.eq_ignore_ascii_case("Bip01") || name.eq_ignore_ascii_case("Bip01 Pelvis") {
        return true;
    }
    // Walk ancestors up the tree; lower-body leg chain passes through Thigh/Calf/Foot/Toe
    // and excludes Bip01 Spine, torso, arms, head, and root props such as `Object04`.
    let mut cur = Some(idx);
    for _ in 0..bones.len() {
        let Some(c) = cur else {
            break;
        };
        let bname = &bones[c].name;
        if bname.eq_ignore_ascii_case("Bip01 Spine") {
            return false;
        }
        if bname.contains("Thigh")
            || bname.contains("Calf")
            || bname.contains("Foot")
            || bname.contains("Toe")
        {
            return true;
        }
        cur = usize::try_from(bones[c].parent_index)
            .ok()
            .filter(|&p| p < bones.len());
    }
    false
}

/// Merges lower-body gait bones (`Bip01` + `Bip01 Pelvis` + legs) with upper-body primary sequence bones
/// (`Bip01 Spine` + torso + arms + head + `Object*`), matching `GameStudioModelRenderer.cpp:355-368`.
fn merge_gait_and_upper_bones(bones: &[BoneData], upper: &mut [Mat4], gait: &[Mat4]) {
    let count = bones.len().min(upper.len()).min(gait.len());
    if count == 0 {
        return;
    }

    let pelvis_idx = bones
        .iter()
        .position(|b| b.name.eq_ignore_ascii_case("Bip01 Pelvis"))
        .unwrap_or(0);

    let has_spine = bones
        .iter()
        .any(|b| b.name.eq_ignore_ascii_case("Bip01 Spine"));
    if !has_spine {
        return;
    }

    let gait_pelvis_pos = gait[pelvis_idx].w_axis.truncate();
    let upper_pelvis_pos = upper[pelvis_idx].w_axis.truncate();
    let pelvis_translation_delta = Vec4::from((gait_pelvis_pos - upper_pelvis_pos, 0.0));

    for i in 0..count {
        if is_gait_lower_bone(bones, i) {
            upper[i] = gait[i];
        } else {
            upper[i].w_axis += pelvis_translation_delta;
        }
    }
}

/// Applies GoldSrc `StudioSlerpBones` cross-sequence fade from `prev` into `cur`.
fn apply_crossfade_slerp(prev: &[Mat4], cur: &mut [Mat4], alpha: f32) {
    let t = alpha.clamp(0.0, 1.0);
    for (p_mat, c_mat) in prev.iter().zip(cur.iter_mut()) {
        let (_s0, r0, t0) = p_mat.to_scale_rotation_translation();
        let (_s1, r1, t1) = c_mat.to_scale_rotation_translation();
        *c_mat = Mat4::from_rotation_translation(r0.slerp(r1, t), t0.lerp(t1, t));
    }
}

/// Evaluates skeletal bone poses at sub-frame precision and skins vertex attributes in-place.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn studio_animation_system(
    mut commands: Commands,
    time: Res<Time>,
    model_store: Res<StudioModelStore>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut audio_bank: Option<ResMut<crate::audio::GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
    mut query: Query<(
        &mut StudioAnimatedModel,
        Option<&Visibility>,
        Option<&InheritedVisibility>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let mut interp_bones = Vec::with_capacity(64);
    let mut gait_bones = Vec::with_capacity(64);

    for (mut anim, vis_opt, inherited_vis_opt) in &mut query {
        // Hidden entities (e.g. 3rd-person player mesh in 1st person or dead chickens) still advance
        // timers below if needed, or skip vertex mesh buffer uploads completely until visible.
        let is_hidden = vis_opt.is_some_and(|v| *v == Visibility::Hidden)
            || inherited_vis_opt.is_some_and(|iv| !iv.get());
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

        // 1. Advance primary frame time
        anim.frame_progress += dt * anim.fps;

        let total_f = num_frames as f32;
        if anim.is_looping {
            if anim.frame_progress >= total_f {
                anim.last_event_frame = -1;
            }
            anim.frame_progress = anim.frame_progress.rem_euclid(total_f);
        } else if anim.frame_progress >= (total_f - 1.0).max(0.0) {
            anim.frame_progress = (total_f - 1.0).max(0.0);
            // Non-looping animation reached its end: return viewmodel to idle sequence 0
            // UNLESS hold_at_end is set (e.g. pulled grenade pin held ready to throw).
            if anim.is_viewmodel
                && !anim.hold_at_end
                && anim.sequence_index != anim.idle_sequence_index
            {
                let target_idle = anim.idle_sequence_index;
                let idle_fps = sequences.get(target_idle).map_or(30.0, |s| s.fps);
                anim.set_sequence(target_idle, idle_fps, true);
                continue;
            }
        }

        // Advance gait sequence time if configured (for 3rd-person player models)
        if let Some(gait_idx) = anim.gait_sequence_index
            && let Some(gait_seq) = sequences.get(gait_idx)
            && !gait_seq.bone_matrices.is_empty()
        {
            anim.gait_frame_progress += dt * anim.gait_fps;
            let gait_total = gait_seq.bone_matrices.len() as f32;
            anim.gait_frame_progress = anim.gait_frame_progress.rem_euclid(gait_total);
        }

        // 2. Dispatch sequence audio events (mstudioevent_t 5004 sound playback)
        let cur_frame_i32 = anim.frame_progress.floor() as i32;
        if cur_frame_i32 != anim.last_event_frame {
            for ev in &seq.events {
                if ev.frame > anim.last_event_frame
                    && ev.frame <= cur_frame_i32
                    && !ev.options.is_empty()
                    && (std::path::Path::new(&ev.options)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
                        || ev.options.contains('/'))
                    && let (Some(paths), Some(ref mut bank), Some(ref mut sources)) = (
                        search_paths.as_ref(),
                        audio_bank.as_mut(),
                        audio_sources.as_mut(),
                    )
                    && let Some(h) = bank.get_or_load_sound(&ev.options, &paths.dirs, sources)
                {
                    commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                }
            }
            anim.last_event_frame = cur_frame_i32;
        }

        // 3. Skip GPU mesh mutation when hidden or frozen at an identical pose
        let pose_unchanged = anim.blend_timer <= 0.0
            && anim.last_skinned_pose.is_some_and(|prev| {
                prev.sequence_index == anim.sequence_index
                    && (prev.frame_progress - anim.frame_progress).abs() < 1e-4
                    && prev.gait_sequence_index == anim.gait_sequence_index
                    && prev.blending == anim.blending
                    && (anim.gait_sequence_index.is_none()
                        || (prev.gait_frame_progress - anim.gait_frame_progress).abs() < 1e-4)
            });
        if is_hidden || pose_unchanged {
            if anim.blend_timer > 0.0 {
                anim.blend_timer = (anim.blend_timer - dt).max(0.0);
            }
            continue;
        }

        // Evaluate primary sequence bones and optionally split with gait lower-body sequence
        sample_interpolated_bones(
            seq,
            anim.frame_progress,
            anim.is_looping,
            anim.blending,
            &mut interp_bones,
        );

        if let Some(gait_idx) = anim.gait_sequence_index
            && let Some(gait_seq) = sequences.get(gait_idx)
            && !gait_seq.bone_matrices.is_empty()
        {
            sample_interpolated_bones(
                gait_seq,
                anim.gait_frame_progress,
                true,
                [128, 128],
                &mut gait_bones,
            );
            merge_gait_and_upper_bones(&model_data.model.rig.bones, &mut interp_bones, &gait_bones);
        }

        if anim.blend_timer > 0.0 && anim.prev_bone_transforms.len() == interp_bones.len() {
            let blend_alpha =
                1.0 - (anim.blend_timer / StudioAnimatedModel::CROSSFADE_DURATION).clamp(0.0, 1.0);
            apply_crossfade_slerp(&anim.prev_bone_transforms, &mut interp_bones, blend_alpha);
            anim.blend_timer = (anim.blend_timer - dt).max(0.0);
        } else if anim.blend_timer > 0.0 {
            anim.blend_timer = 0.0;
        }

        anim.current_bone_transforms.clone_from(&interp_bones);
        anim.last_skinned_pose = Some(SkinnedPoseKey {
            sequence_index: anim.sequence_index,
            frame_progress: anim.frame_progress,
            gait_sequence_index: anim.gait_sequence_index,
            gait_frame_progress: anim.gait_frame_progress,
            blending: anim.blending,
        });

        // 4. Deform each submesh's vertices and normals in-place
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
