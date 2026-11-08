//! GoldSrc StudioModel (`.mdl`) binary parser and IR converter.
//!
//! Replicates 1-to-1 parsing logic from `ReHLDS`, `HLSDK`, and `Xash3D FWGS`.
//! Supports:
//! - Magic validation (`IDST`, version 10).
//! - Skeletal bones hierarchy (`mstudiobone_t`), default poses and parent chains.
//! - Hitboxes (`mstudiobbox_t`), attachments (`mstudioattachment_t`), bone controllers (`mstudiobonecontroller_t`).
//! - Animation sequences (`mstudioseqdesc_t`), events (`mstudioevent_t`), and RLE compressed frame decoding (`mstudioanim_t`).
//! - Submodels and bodyparts (`mstudiobodyparts_t`, `mstudiomodel_t`).
//! - OpenGL triangle fans and strips (`mstudiomesh_t`, `short *ptricmds`) converted to canonical index buffers.
//! - Paletted 8-bit embedded textures (`mstudiotexture_t`) converted to 32-bit RGBA8 with optional external `T.mdl` support.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;
use crate::ir::{
    AnimEventData, AttachmentData, BoneControllerData, BoneData, HitboxData, SequenceData,
    SkeletalRig, StudioBodyPart, StudioMesh, StudioModel, StudioSubModel, StudioTexture,
    StudioVertex,
};

/// Studio MDL magic bytes ("IDST").
pub const MDL_MAGIC: [u8; 4] = *b"IDST";

/// Studio MDL supported format version (GoldSrc v10).
pub const MDL_VERSION: u32 = 10;

/// Studio texture flag: transparent / alpha masked (e.g. `{` prefix).
pub const STUDIO_NF_MASKED: i32 = 0x0040;

/// Header structure of a GoldSrc StudioModel file (`studiohdr_t`, 244 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub name: [u8; 64],
    pub length: u32,
    pub eye_position: [f32; 3],
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
    pub flags: u32,
    pub num_bones: u32,
    pub bone_index: u32,
    pub num_bone_controllers: u32,
    pub bone_controller_index: u32,
    pub num_hitboxes: u32,
    pub hitbox_index: u32,
    pub num_sequences: u32,
    pub sequence_index: u32,
    pub num_seq_groups: u32,
    pub seq_group_index: u32,
    pub num_textures: u32,
    pub texture_index: u32,
    pub texture_data_index: u32,
    pub num_skin_ref: u32,
    pub num_skin_families: u32,
    pub skin_index: u32,
    pub num_body_parts: u32,
    pub body_part_index: u32,
    pub num_attachments: u32,
    pub attachment_index: u32,
    pub sound_table: i32,
    pub sound_index: i32,
    pub sound_groups: i32,
    pub sound_group_index: i32,
    pub num_transitions: i32,
    pub transition_index: i32,
}

const _: () = assert!(core::mem::size_of::<MdlHeader>() == 244);

/// Raw bone record (`mstudiobone_t`, 112 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlBone {
    pub name: [u8; 32],
    pub parent: i32,
    pub flags: i32,
    pub bone_controller: [i32; 6],
    pub value: [f32; 6],
    pub scale: [f32; 6],
}

const _: () = assert!(core::mem::size_of::<MdlBone>() == 112);

/// Raw bone controller (`mstudiobonecontroller_t`, 24 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlBoneController {
    pub bone: i32,
    pub motion_type: i32,
    pub start: f32,
    pub end: f32,
    pub rest: i32,
    pub index: i32,
}

const _: () = assert!(core::mem::size_of::<MdlBoneController>() == 24);

/// Raw hitbox bounding box (`mstudiobbox_t`, 32 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlBBox {
    pub bone: i32,
    pub group: i32,
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
}

const _: () = assert!(core::mem::size_of::<MdlBBox>() == 32);

/// Raw attachment descriptor (`mstudioattachment_t`, 88 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlAttachment {
    pub name: [u8; 32],
    pub type_flags: i32,
    pub bone: i32,
    pub org: [f32; 3],
    pub vectors: [[f32; 3]; 3],
}

const _: () = assert!(core::mem::size_of::<MdlAttachment>() == 88);

/// Raw animation sequence descriptor (`mstudioseqdesc_t`, 176 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlSequenceDesc {
    pub label: [u8; 32],
    pub fps: f32,
    pub flags: i32,
    pub activity: i32,
    pub act_weight: i32,
    pub num_events: i32,
    pub event_index: i32,
    pub num_frames: i32,
    pub num_pivots: i32,
    pub pivot_index: i32,
    pub motion_type: i32,
    pub motion_bone: i32,
    pub linear_movement: [f32; 3],
    pub automove_pos_index: i32,
    pub automove_angle_index: i32,
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
    pub num_blends: i32,
    pub anim_index: i32,
    pub blend_type: [i32; 2],
    pub blend_start: [f32; 2],
    pub blend_end: [f32; 2],
    pub blend_parent: i32,
    pub seq_group: i32,
    pub entry_node: i32,
    pub exit_node: i32,
    pub node_flags: i32,
    pub next_seq: i32,
}

const _: () = assert!(core::mem::size_of::<MdlSequenceDesc>() == 176);

/// Raw sequence event (`mstudioevent_t`, 76 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlEvent {
    pub frame: i32,
    pub event: i32,
    pub event_type: i32,
    pub options: [u8; 64],
}

const _: () = assert!(core::mem::size_of::<MdlEvent>() == 76);

/// Raw body part descriptor (`mstudiobodyparts_t`, 76 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlBodyPart {
    pub name: [u8; 64],
    pub num_models: i32,
    pub base: i32,
    pub model_index: i32,
}

const _: () = assert!(core::mem::size_of::<MdlBodyPart>() == 76);

/// Raw submodel descriptor (`mstudiomodel_t`, 112 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlModel {
    pub name: [u8; 64],
    pub model_type: i32,
    pub bounding_radius: f32,
    pub num_mesh: i32,
    pub mesh_index: i32,
    pub num_verts: i32,
    pub vert_info_index: i32,
    pub vert_index: i32,
    pub num_norms: i32,
    pub norm_info_index: i32,
    pub norm_index: i32,
    pub num_groups: i32,
    pub group_index: i32,
}

const _: () = assert!(core::mem::size_of::<MdlModel>() == 112);

/// Raw mesh descriptor (`mstudiomesh_t`, 20 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlMesh {
    pub num_tris: i32,
    pub tri_index: i32,
    pub skin_ref: i32,
    pub num_norms: i32,
    pub norm_index: i32,
}

const _: () = assert!(core::mem::size_of::<MdlMesh>() == 20);

/// Raw texture descriptor (`mstudiotexture_t`, 80 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MdlTexture {
    pub name: [u8; 64],
    pub flags: i32,
    pub width: i32,
    pub height: i32,
    pub index: i32,
}

const _: () = assert!(core::mem::size_of::<MdlTexture>() == 80);

/// Decodes Studio MDL models into canonical `SkeletalRig` or full `StudioModel`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StudioMdlDecoder;

impl StudioMdlDecoder {
    /// Decodes a full `StudioModel` from raw file bytes, with optional external texture file bytes.
    pub fn decode_model_with_textures(
        &self,
        bytes: &[u8],
        external_textures: Option<&[u8]>,
    ) -> Result<StudioModel, FormatError> {
        let (header, _) =
            MdlHeader::ref_from_prefix(bytes).map_err(|_| FormatError::BufferTooSmall {
                expected: core::mem::size_of::<MdlHeader>(),
                got: bytes.len(),
            })?;

        if header.magic != MDL_MAGIC {
            return Err(FormatError::InvalidMagic {
                expected: MDL_MAGIC.to_vec(),
                got: header.magic.to_vec(),
            });
        }

        if header.version != MDL_VERSION {
            return Err(FormatError::UnsupportedVersion {
                expected: MDL_VERSION,
                got: header.version,
            });
        }

        let name = null_terminated_ascii(&header.name);

        // 1. Bones & Rig
        let rig = self.decode_rig_internal(bytes, header)?;

        // 2. Textures
        let (textures, skin_families) =
            self.decode_textures_internal(bytes, header, external_textures)?;

        // 3. Body parts & Submodels
        let body_parts = self.decode_body_parts_internal(bytes, header, &textures)?;

        Ok(StudioModel {
            name,
            eye_position: header.eye_position,
            min: header.min,
            max: header.max,
            bbmin: header.bbmin,
            bbmax: header.bbmax,
            flags: header.flags,
            rig,
            body_parts,
            textures,
            skin_families,
        })
    }

    #[allow(clippy::unused_self, clippy::too_many_lines)]
    fn decode_rig_internal(
        &self,
        bytes: &[u8],
        header: &MdlHeader,
    ) -> Result<SkeletalRig, FormatError> {
        // Bones
        let bone_start = header.bone_index as usize;
        let bone_len = (header.num_bones as usize) * core::mem::size_of::<MdlBone>();
        if bone_start + bone_len > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: bone_start + bone_len,
                got: bytes.len(),
            });
        }
        let bones_slice = &bytes[bone_start..bone_start + bone_len];
        let raw_bones =
            <[MdlBone]>::ref_from_bytes(bones_slice).map_err(|_| FormatError::MalformedData {
                location: "mdl bones",
                reason: "misaligned or invalid bone table".into(),
            })?;

        let mut bones = Vec::with_capacity(raw_bones.len());
        for b in raw_bones {
            let bone_name = null_terminated_ascii(&b.name);
            let local_matrix = compute_bone_matrix(
                [b.value[0], b.value[1], b.value[2]],
                [b.value[3], b.value[4], b.value[5]],
            );
            bones.push(BoneData {
                name: bone_name,
                parent_index: b.parent,
                flags: b.flags,
                local_matrix,
            });
        }

        // Hitboxes
        let mut hitboxes = Vec::new();
        if header.num_hitboxes > 0 {
            let hb_start = header.hitbox_index as usize;
            let hb_len = (header.num_hitboxes as usize) * core::mem::size_of::<MdlBBox>();
            if hb_start + hb_len <= bytes.len() {
                let hb_slice = &bytes[hb_start..hb_start + hb_len];
                if let Ok(raw_hitboxes) = <[MdlBBox]>::ref_from_bytes(hb_slice) {
                    hitboxes.reserve(raw_hitboxes.len());
                    for hb in raw_hitboxes {
                        hitboxes.push(HitboxData {
                            bone_index: hb.bone,
                            hit_group: hb.group,
                            bbmin: hb.bbmin,
                            bbmax: hb.bbmax,
                        });
                    }
                }
            }
        }

        // Attachments
        let mut attachments = Vec::new();
        if header.num_attachments > 0 {
            let att_start = header.attachment_index as usize;
            let att_len = (header.num_attachments as usize) * core::mem::size_of::<MdlAttachment>();
            if att_start + att_len <= bytes.len() {
                let att_slice = &bytes[att_start..att_start + att_len];
                if let Ok(raw_atts) = <[MdlAttachment]>::ref_from_bytes(att_slice) {
                    attachments.reserve(raw_atts.len());
                    for att in raw_atts {
                        attachments.push(AttachmentData {
                            name: null_terminated_ascii(&att.name),
                            bone_index: att.bone,
                            origin: att.org,
                            vectors: att.vectors,
                        });
                    }
                }
            }
        }

        // Controllers
        let mut controllers = Vec::new();
        if header.num_bone_controllers > 0 {
            let ctrl_start = header.bone_controller_index as usize;
            let ctrl_len =
                (header.num_bone_controllers as usize) * core::mem::size_of::<MdlBoneController>();
            if ctrl_start + ctrl_len <= bytes.len() {
                let ctrl_slice = &bytes[ctrl_start..ctrl_start + ctrl_len];
                if let Ok(raw_ctrls) = <[MdlBoneController]>::ref_from_bytes(ctrl_slice) {
                    controllers.reserve(raw_ctrls.len());
                    for c in raw_ctrls {
                        controllers.push(BoneControllerData {
                            bone_index: c.bone,
                            motion_type: c.motion_type,
                            start: c.start,
                            end: c.end,
                            rest: c.rest,
                            index: c.index,
                        });
                    }
                }
            }
        }

        // Sequences
        let mut sequences = Vec::new();
        if header.num_sequences > 0 {
            let seq_start = header.sequence_index as usize;
            let seq_len = (header.num_sequences as usize) * core::mem::size_of::<MdlSequenceDesc>();
            if seq_start + seq_len <= bytes.len() {
                let seq_slice = &bytes[seq_start..seq_start + seq_len];
                if let Ok(raw_seqs) = <[MdlSequenceDesc]>::ref_from_bytes(seq_slice) {
                    sequences.reserve(raw_seqs.len());
                    for s in raw_seqs {
                        let label = null_terminated_ascii(&s.label);

                        // Decode events
                        let mut events = Vec::new();
                        if s.num_events > 0 && s.event_index > 0 {
                            let ev_start = s.event_index as usize;
                            let ev_len = (s.num_events as usize) * core::mem::size_of::<MdlEvent>();
                            if ev_start + ev_len <= bytes.len()
                                && let Ok(raw_events) = <[MdlEvent]>::ref_from_bytes(
                                    &bytes[ev_start..ev_start + ev_len],
                                )
                            {
                                events.reserve(raw_events.len());
                                for ev in raw_events {
                                    events.push(AnimEventData {
                                        frame: ev.frame,
                                        event: ev.event,
                                        event_type: ev.event_type,
                                        options: null_terminated_ascii(&ev.options),
                                    });
                                }
                            }
                        }

                        // Sample animation bone matrices for each frame
                        let bone_matrices = sample_sequence_bone_matrices(bytes, raw_bones, s);

                        sequences.push(SequenceData {
                            name: label,
                            fps: s.fps,
                            flags: s.flags,
                            activity: s.activity,
                            act_weight: s.act_weight,
                            frame_count: s.num_frames.max(0) as u32,
                            events,
                            bbmin: s.bbmin,
                            bbmax: s.bbmax,
                            bone_matrices,
                        });
                    }
                }
            }
        }

        Ok(SkeletalRig {
            bones,
            hitboxes,
            attachments,
            controllers,
            sequences,
        })
    }

    #[allow(clippy::unused_self)]
    fn decode_textures_internal(
        &self,
        bytes: &[u8],
        header: &MdlHeader,
        external_textures: Option<&[u8]>,
    ) -> Result<(Vec<StudioTexture>, Vec<Vec<u16>>), FormatError> {
        let (tex_bytes, tex_header) = if header.num_textures > 0 {
            (bytes, header)
        } else if let Some(ext) = external_textures {
            let (ext_hdr, _) =
                MdlHeader::ref_from_prefix(ext).map_err(|_| FormatError::BufferTooSmall {
                    expected: core::mem::size_of::<MdlHeader>(),
                    got: ext.len(),
                })?;
            (ext, ext_hdr)
        } else {
            return Ok((Vec::new(), Vec::new()));
        };

        let num_tex = tex_header.num_textures as usize;
        let tex_start = tex_header.texture_index as usize;
        let tex_meta_len = num_tex * core::mem::size_of::<MdlTexture>();

        if tex_start + tex_meta_len > tex_bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: tex_start + tex_meta_len,
                got: tex_bytes.len(),
            });
        }

        let raw_tex_slice = &tex_bytes[tex_start..tex_start + tex_meta_len];
        let raw_textures = <[MdlTexture]>::ref_from_bytes(raw_tex_slice).map_err(|_| {
            FormatError::MalformedData {
                location: "mdl textures",
                reason: "invalid texture lump header".into(),
            }
        })?;

        let mut textures = Vec::with_capacity(num_tex);
        for t in raw_textures {
            let tex_name = null_terminated_ascii(&t.name);
            let width = t.width.max(0) as usize;
            let height = t.height.max(0) as usize;
            let pixel_count = width * height;
            let pal_offset = t.index as usize + pixel_count;

            if pal_offset + 768 > tex_bytes.len() {
                return Err(FormatError::BufferTooSmall {
                    expected: pal_offset + 768,
                    got: tex_bytes.len(),
                });
            }

            let index_data = &tex_bytes[t.index as usize..pal_offset];
            let palette = &tex_bytes[pal_offset..pal_offset + 768];

            let is_masked = (t.flags & STUDIO_NF_MASKED) != 0;
            let mut rgba_pixels = Vec::with_capacity(pixel_count * 4);

            for &idx in index_data {
                let pal_idx = (idx as usize) * 3;
                let red = palette[pal_idx];
                let green = palette[pal_idx + 1];
                let blue = palette[pal_idx + 2];
                let alpha = if is_masked && idx == 255 { 0 } else { 255 };
                rgba_pixels.extend_from_slice(&[red, green, blue, alpha]);
            }

            textures.push(StudioTexture {
                name: tex_name,
                flags: t.flags as u32,
                width: width as u32,
                height: height as u32,
                rgba_pixels,
            });
        }

        // Skin families
        let mut skin_families = Vec::new();
        if tex_header.num_skin_families > 0 && tex_header.num_skin_ref > 0 {
            let num_fam = tex_header.num_skin_families as usize;
            let num_ref = tex_header.num_skin_ref as usize;
            let total_skins = num_fam * num_ref;
            let skin_start = tex_header.skin_index as usize;
            let skin_bytes_len = total_skins * core::mem::size_of::<i16>();

            if skin_start + skin_bytes_len <= tex_bytes.len() {
                let skin_slice = &tex_bytes[skin_start..skin_start + skin_bytes_len];
                if let Ok(raw_skins) = <[i16]>::ref_from_bytes(skin_slice) {
                    for fam_idx in 0..num_fam {
                        let fam_start = fam_idx * num_ref;
                        let fam_slice = &raw_skins[fam_start..fam_start + num_ref];
                        skin_families.push(fam_slice.iter().map(|&s| s.max(0) as u16).collect());
                    }
                }
            }
        }

        Ok((textures, skin_families))
    }

    #[allow(clippy::unused_self, clippy::too_many_lines)]
    fn decode_body_parts_internal(
        &self,
        bytes: &[u8],
        header: &MdlHeader,
        textures: &[StudioTexture],
    ) -> Result<Vec<StudioBodyPart>, FormatError> {
        let num_parts = header.num_body_parts as usize;
        if num_parts == 0 {
            return Ok(Vec::new());
        }

        let bp_start = header.body_part_index as usize;
        let bp_len = num_parts * core::mem::size_of::<MdlBodyPart>();
        if bp_start + bp_len > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: bp_start + bp_len,
                got: bytes.len(),
            });
        }

        let raw_bp_slice = &bytes[bp_start..bp_start + bp_len];
        let raw_bodyparts = <[MdlBodyPart]>::ref_from_bytes(raw_bp_slice).map_err(|_| {
            FormatError::MalformedData {
                location: "mdl bodyparts",
                reason: "invalid bodypart array".into(),
            }
        })?;

        let mut body_parts = Vec::with_capacity(num_parts);

        for bp in raw_bodyparts {
            let bp_name = null_terminated_ascii(&bp.name);
            let num_models = bp.num_models.max(0) as usize;
            let mut submodels = Vec::with_capacity(num_models);

            for m_idx in 0..num_models {
                let model_offset =
                    bp.model_index as usize + m_idx * core::mem::size_of::<MdlModel>();
                if model_offset + core::mem::size_of::<MdlModel>() > bytes.len() {
                    return Err(FormatError::BufferTooSmall {
                        expected: model_offset + core::mem::size_of::<MdlModel>(),
                        got: bytes.len(),
                    });
                }

                let (raw_m, _) =
                    MdlModel::ref_from_prefix(&bytes[model_offset..]).map_err(|_| {
                        FormatError::MalformedData {
                            location: "mdl model",
                            reason: "invalid submodel struct".into(),
                        }
                    })?;

                let model_name = null_terminated_ascii(&raw_m.name);
                let num_verts = raw_m.num_verts.max(0) as usize;
                let num_norms = raw_m.num_norms.max(0) as usize;

                // Vertices
                let vert_bytes_start = raw_m.vert_index as usize;
                let vert_bytes_len = num_verts * 12; // 3 * f32
                if vert_bytes_start + vert_bytes_len > bytes.len() {
                    return Err(FormatError::BufferTooSmall {
                        expected: vert_bytes_start + vert_bytes_len,
                        got: bytes.len(),
                    });
                }
                let raw_verts_slice = &bytes[vert_bytes_start..vert_bytes_start + vert_bytes_len];
                let verts = <[[f32; 3]]>::ref_from_bytes(raw_verts_slice).map_err(|_| {
                    FormatError::MalformedData {
                        location: "mdl vertices",
                        reason: "misaligned vertex coordinates".into(),
                    }
                })?;

                // Normals
                let norm_bytes_start = raw_m.norm_index as usize;
                let norm_bytes_len = num_norms * 12;
                if norm_bytes_start + norm_bytes_len > bytes.len() {
                    return Err(FormatError::BufferTooSmall {
                        expected: norm_bytes_start + norm_bytes_len,
                        got: bytes.len(),
                    });
                }
                let raw_norms_slice = &bytes[norm_bytes_start..norm_bytes_start + norm_bytes_len];
                let norms = <[[f32; 3]]>::ref_from_bytes(raw_norms_slice).map_err(|_| {
                    FormatError::MalformedData {
                        location: "mdl normals",
                        reason: "misaligned normal coordinates".into(),
                    }
                })?;

                // Bone bindings
                let vert_bone_start = raw_m.vert_info_index as usize;
                if vert_bone_start + num_verts > bytes.len() {
                    return Err(FormatError::BufferTooSmall {
                        expected: vert_bone_start + num_verts,
                        got: bytes.len(),
                    });
                }
                let vert_bones = &bytes[vert_bone_start..vert_bone_start + num_verts];

                // Meshes
                let num_mesh = raw_m.num_mesh.max(0) as usize;
                let mesh_start = raw_m.mesh_index as usize;
                let mesh_meta_len = num_mesh * core::mem::size_of::<MdlMesh>();
                if mesh_start + mesh_meta_len > bytes.len() {
                    return Err(FormatError::BufferTooSmall {
                        expected: mesh_start + mesh_meta_len,
                        got: bytes.len(),
                    });
                }

                let raw_mesh_slice = &bytes[mesh_start..mesh_start + mesh_meta_len];
                let raw_meshes = <[MdlMesh]>::ref_from_bytes(raw_mesh_slice).map_err(|_| {
                    FormatError::MalformedData {
                        location: "mdl mesh",
                        reason: "invalid mesh table".into(),
                    }
                })?;

                let mut meshes = Vec::with_capacity(num_mesh);
                for mesh in raw_meshes {
                    let skin_ref = mesh.skin_ref.max(0) as usize;
                    let (tex_w, tex_h) = textures.get(skin_ref).map_or((1.0, 1.0), |tex| {
                        (tex.width.max(1) as f32, tex.height.max(1) as f32)
                    });

                    let tri_cmd_start = mesh.tri_index as usize;
                    let (mesh_verts, mesh_indices) = decode_triangle_commands(
                        bytes,
                        tri_cmd_start,
                        verts,
                        norms,
                        vert_bones,
                        tex_w,
                        tex_h,
                    )?;

                    meshes.push(StudioMesh {
                        skin_ref,
                        vertices: mesh_verts,
                        indices: mesh_indices,
                    });
                }

                submodels.push(StudioSubModel {
                    name: model_name,
                    meshes,
                });
            }

            body_parts.push(StudioBodyPart {
                name: bp_name,
                models: submodels,
                base: bp.base,
            });
        }

        Ok(body_parts)
    }
}

impl<'a> AssetDecoder<'a, SkeletalRig> for StudioMdlDecoder {
    fn decode(&self, bytes: &'a [u8]) -> Result<SkeletalRig, FormatError> {
        let (header, _) =
            MdlHeader::ref_from_prefix(bytes).map_err(|_| FormatError::BufferTooSmall {
                expected: core::mem::size_of::<MdlHeader>(),
                got: bytes.len(),
            })?;

        if header.magic != MDL_MAGIC {
            return Err(FormatError::InvalidMagic {
                expected: MDL_MAGIC.to_vec(),
                got: header.magic.to_vec(),
            });
        }

        if header.version != MDL_VERSION {
            return Err(FormatError::UnsupportedVersion {
                expected: MDL_VERSION,
                got: header.version,
            });
        }

        self.decode_rig_internal(bytes, header)
    }
}

impl<'a> AssetDecoder<'a, StudioModel> for StudioMdlDecoder {
    fn decode(&self, bytes: &'a [u8]) -> Result<StudioModel, FormatError> {
        self.decode_model_with_textures(bytes, None)
    }
}

/// Helper to decode null-terminated ASCII string from byte slice.
fn null_terminated_ascii(bytes: &[u8]) -> String {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).into_owned()
}

/// Computes a column-major 4x4 matrix from GoldSrc Euler angles (radians) and origin.
fn compute_bone_matrix(origin: [f32; 3], angles: [f32; 3]) -> [f32; 16] {
    let q = angle_to_quaternion(angles);
    quaternion_to_matrix(q, origin)
}

/// GoldSrc AngleQuaternion conversion (`r_studio.cpp`).
fn angle_to_quaternion(angles: [f32; 3]) -> [f32; 4] {
    let angle_z = angles[2] * 0.5;
    let sy = angle_z.sin();
    let cy = angle_z.cos();

    let angle_y = angles[1] * 0.5;
    let sp = angle_y.sin();
    let cp = angle_y.cos();

    let angle_x = angles[0] * 0.5;
    let sr = angle_x.sin();
    let cr = angle_x.cos();

    [
        sr * cp * cy - cr * sp * sy, // x
        cr * sp * cy + sr * cp * sy, // y
        cr * cp * sy - sr * sp * cy, // z
        cr * cp * cy + sr * sp * sy, // w
    ]
}

/// GoldSrc QuaternionMatrix conversion (`r_studio.cpp`) into 4x4 column-major matrix.
fn quaternion_to_matrix(q: [f32; 4], pos: [f32; 3]) -> [f32; 16] {
    let [qx, qy, qz, qw] = q;

    let m00 = 1.0 - 2.0 * qy * qy - 2.0 * qz * qz;
    let m10 = 2.0 * qx * qy + 2.0 * qw * qz;
    let m20 = 2.0 * qx * qz - 2.0 * qw * qy;

    let m01 = 2.0 * qx * qy - 2.0 * qw * qz;
    let m11 = 1.0 - 2.0 * qx * qx - 2.0 * qz * qz;
    let m21 = 2.0 * qy * qz + 2.0 * qw * qx;

    let m02 = 2.0 * qx * qz + 2.0 * qw * qy;
    let m12 = 2.0 * qy * qz - 2.0 * qw * qx;
    let m22 = 1.0 - 2.0 * qx * qx - 2.0 * qy * qy;

    [
        m00, m10, m20, 0.0, // column 0
        m01, m11, m21, 0.0, // column 1
        m02, m12, m22, 0.0, // column 2
        pos[0], pos[1], pos[2], 1.0, // column 3 (translation)
    ]
}

/// Multiplies two 4x4 column-major affine matrices (`a * b`).
fn multiply_4x4(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

/// Samples bone transform matrices for every frame of a sequence.
fn sample_sequence_bone_matrices(
    bytes: &[u8],
    bones: &[MdlBone],
    seq: &MdlSequenceDesc,
) -> Vec<Vec<[f32; 16]>> {
    let num_frames = seq.num_frames.max(1) as usize;
    let num_bones = bones.len();
    if seq.anim_index <= 0 || (seq.anim_index as usize) >= bytes.len() {
        return Vec::new();
    }

    let anim_base = seq.anim_index as usize;
    let mut all_frame_matrices = Vec::with_capacity(num_frames);

    // Temp buffers for bone poses
    let mut local_matrices = vec![[0.0f32; 16]; num_bones];
    let mut world_matrices = vec![[0.0f32; 16]; num_bones];

    for frame in 0..num_frames {
        for (bone_idx, bone) in bones.iter().enumerate() {
            let bone_panim_offset = anim_base + bone_idx * 12; // 6 * u16
            let (pos, angles) = evaluate_bone_frame(bytes, bone_panim_offset, bone, frame);
            local_matrices[bone_idx] = compute_bone_matrix(pos, angles);
        }

        // Concatenate hierarchy (parent -> child)
        for (bone_idx, bone) in bones.iter().enumerate() {
            if bone.parent < 0 || (bone.parent as usize) >= num_bones {
                world_matrices[bone_idx] = local_matrices[bone_idx];
            } else {
                let parent_mat = &world_matrices[bone.parent as usize];
                let local_mat = &local_matrices[bone_idx];
                world_matrices[bone_idx] = multiply_4x4(parent_mat, local_mat);
            }
        }

        all_frame_matrices.push(world_matrices.clone());
    }

    all_frame_matrices
}

/// Evaluates position `[x, y, z]` and angles `[xr, yr, zr]` for a single bone frame.
fn evaluate_bone_frame(
    bytes: &[u8],
    bone_panim_offset: usize,
    bone: &MdlBone,
    frame: usize,
) -> ([f32; 3], [f32; 3]) {
    let mut pos = [bone.value[0], bone.value[1], bone.value[2]];
    let mut angles = [bone.value[3], bone.value[4], bone.value[5]];

    if bone_panim_offset + 12 > bytes.len() {
        return (pos, angles);
    }

    // Read 6 channel offsets: 0..3 are pos (X, Y, Z), 3..6 are angles (XR, YR, ZR)
    let offsets_slice = &bytes[bone_panim_offset..bone_panim_offset + 12];
    if let Ok(offsets) = <[u16; 6]>::ref_from_bytes(offsets_slice) {
        // Translation channels
        for ch in 0..3 {
            if offsets[ch] != 0 {
                let track_offset = bone_panim_offset + (offsets[ch] as usize);
                if let Some(val) = evaluate_rle_track(bytes, track_offset, frame) {
                    pos[ch] = bone.value[ch] + f32::from(val) * bone.scale[ch];
                }
            }
        }
        // Rotation channels
        for (ch, angle_val) in angles.iter_mut().enumerate() {
            let angle_ch = ch + 3;
            if offsets[angle_ch] != 0 {
                let track_offset = bone_panim_offset + (offsets[angle_ch] as usize);
                if let Some(val) = evaluate_rle_track(bytes, track_offset, frame) {
                    *angle_val = bone.value[angle_ch] + f32::from(val) * bone.scale[angle_ch];
                }
            }
        }
    }

    (pos, angles)
}

/// Evaluates RLE anim value stream at frame `frame`.
fn evaluate_rle_track(bytes: &[u8], mut offset: usize, target_frame: usize) -> Option<i16> {
    let mut cur_frame = target_frame;

    while offset + 2 <= bytes.len() {
        let valid = bytes[offset] as usize;
        let total = bytes[offset + 1] as usize;
        if total == 0 {
            break;
        }

        let span_values_offset = offset + 2;
        let span_byte_len = valid * 2;
        if span_values_offset + span_byte_len > bytes.len() {
            return None;
        }

        if total <= cur_frame {
            cur_frame -= total;
            offset += 2 + span_byte_len;
        } else {
            // Target frame is inside this run
            if cur_frame < valid {
                let val_pos = span_values_offset + cur_frame * 2;
                return Some(i16::from_le_bytes([bytes[val_pos], bytes[val_pos + 1]]));
            } else if valid > 0 {
                // Repeat last valid value
                let val_pos = span_values_offset + (valid - 1) * 2;
                return Some(i16::from_le_bytes([bytes[val_pos], bytes[val_pos + 1]]));
            }
            return None;
        }
    }

    None
}

/// Decodes OpenGL triangle strips and fans into standard triangles.
fn decode_triangle_commands(
    bytes: &[u8],
    mut offset: usize,
    verts: &[[f32; 3]],
    norms: &[[f32; 3]],
    vert_bones: &[u8],
    tex_w: f32,
    tex_h: f32,
) -> Result<(Vec<StudioVertex>, Vec<u32>), FormatError> {
    let mut mesh_vertices = Vec::new();
    let mut mesh_indices = Vec::new();

    let inv_w = 1.0 / tex_w;
    let inv_h = 1.0 / tex_h;

    while offset + 2 <= bytes.len() {
        let cmd = i16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;
        if cmd == 0 {
            break;
        }

        let is_fan = cmd < 0;
        let count = cmd.unsigned_abs() as usize;

        let required_bytes = count * 8; // 4 * i16
        if offset + required_bytes > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: offset + required_bytes,
                got: bytes.len(),
            });
        }

        let run_start_idx = mesh_vertices.len() as u32;

        for _ in 0..count {
            let v_idx = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
            let n_idx = u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
            let s = f32::from(i16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]));
            let t = f32::from(i16::from_le_bytes([bytes[offset + 6], bytes[offset + 7]]));
            offset += 8;

            let pos = *verts.get(v_idx).unwrap_or(&[0.0, 0.0, 0.0]);
            let normal = *norms.get(n_idx).unwrap_or(&[0.0, 0.0, 1.0]);
            let bone_idx = u16::from(*vert_bones.get(v_idx).unwrap_or(&0));
            let uv = [s * inv_w, t * inv_h];

            mesh_vertices.push(StudioVertex {
                position: pos,
                normal,
                uv,
                bone_index: bone_idx,
            });
        }

        if is_fan {
            // Triangle fan: (v0, v1, v2), (v0, v2, v3), ...
            for k in 1..count.saturating_sub(1) {
                mesh_indices.push(run_start_idx);
                mesh_indices.push(run_start_idx + (k as u32));
                mesh_indices.push(run_start_idx + (k as u32) + 1);
            }
        } else {
            // Triangle strip: alternating winding order
            for k in 0..count.saturating_sub(2) {
                let (first, second) = if k % 2 == 0 {
                    (run_start_idx + (k as u32), run_start_idx + (k as u32) + 1)
                } else {
                    (run_start_idx + (k as u32) + 1, run_start_idx + (k as u32))
                };
                mesh_indices.push(first);
                mesh_indices.push(second);
                mesh_indices.push(run_start_idx + (k as u32) + 2);
            }
        }
    }

    Ok((mesh_vertices, mesh_indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mdl_magic_and_version_validation() {
        let decoder = StudioMdlDecoder;

        // Truncated buffer
        let small_buf = [0u8; 100];
        let err = AssetDecoder::<SkeletalRig>::decode(&decoder, &small_buf).unwrap_err();
        match err {
            FormatError::BufferTooSmall { expected, got } => {
                assert_eq!(expected, core::mem::size_of::<MdlHeader>());
                assert_eq!(got, 100);
            }
            other => panic!("expected BufferTooSmall, got: {other:?}"),
        }

        // Invalid magic
        let mut invalid_magic_buf = vec![0u8; 244];
        invalid_magic_buf[0..4].copy_from_slice(b"BADM");
        let err = AssetDecoder::<SkeletalRig>::decode(&decoder, &invalid_magic_buf).unwrap_err();
        match err {
            FormatError::InvalidMagic { expected, got } => {
                assert_eq!(expected, MDL_MAGIC);
                assert_eq!(got, b"BADM");
            }
            other => panic!("expected InvalidMagic, got: {other:?}"),
        }

        // Unsupported version
        let mut bad_version_buf = vec![0u8; 244];
        bad_version_buf[0..4].copy_from_slice(b"IDST");
        bad_version_buf[4..8].copy_from_slice(&11u32.to_le_bytes());
        let err = AssetDecoder::<SkeletalRig>::decode(&decoder, &bad_version_buf).unwrap_err();
        match err {
            FormatError::UnsupportedVersion { expected, got } => {
                assert_eq!(expected, MDL_VERSION);
                assert_eq!(got, 11);
            }
            other => panic!("expected UnsupportedVersion, got: {other:?}"),
        }
    }

    #[test]
    fn test_quaternion_and_matrix_math() {
        // Identity Euler angles (0, 0, 0)
        let angles = [0.0, 0.0, 0.0];
        let pos = [10.0, 20.0, 30.0];
        let mat = compute_bone_matrix(pos, angles);

        // Column 0
        assert!((mat[0] - 1.0).abs() < 1e-5);
        assert!(mat[1].abs() < 1e-5);
        assert!(mat[2].abs() < 1e-5);
        // Column 3 (translation)
        assert!((mat[12] - 10.0).abs() < 1e-5);
        assert!((mat[13] - 20.0).abs() < 1e-5);
        assert!((mat[14] - 30.0).abs() < 1e-5);
        assert!((mat[15] - 1.0).abs() < 1e-5);

        // Concatenate transforms
        let parent = compute_bone_matrix([5.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        let child = compute_bone_matrix([2.0, 3.0, 0.0], [0.0, 0.0, 0.0]);
        let combined = multiply_4x4(&parent, &child);
        assert!((combined[12] - 7.0).abs() < 1e-5);
        assert!((combined[13] - 3.0).abs() < 1e-5);
    }
}
