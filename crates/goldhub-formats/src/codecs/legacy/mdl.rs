//! Studio MDL model parser and bone extractor.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;
use crate::ir::{BoneData, SequenceData, SkeletalRig};

/// Studio MDL magic ("IDST").
pub const MDL_MAGIC: [u8; 4] = *b"IDST";
/// Studio MDL format version.
pub const MDL_VERSION: u32 = 10;

/// Studio MDL file header (studiohdr_t).
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
}

/// Bone descriptor inside Studio MDL (mstudiobone_t).
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

/// Animation sequence descriptor (mstudioseqdesc_t).
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
}

/// Decoder for Studio MDL models into Canonical `SkeletalRig`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StudioMdlDecoder;

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

        // Extract bones
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
                reason: "misaligned bone table".into(),
            })?;

        let mut bones = Vec::with_capacity(raw_bones.len());
        for b in raw_bones {
            let name_len = b.name.iter().position(|&c| c == 0).unwrap_or(b.name.len());
            let name = String::from_utf8_lossy(&b.name[..name_len]).into_owned();

            // Identity 4x4 matrix placeholder for local transform
            let mut local_matrix = [0.0; 16];
            local_matrix[0] = 1.0;
            local_matrix[5] = 1.0;
            local_matrix[10] = 1.0;
            local_matrix[15] = 1.0;
            // Fill position offset from b.value[0..3]
            local_matrix[12] = b.value[0];
            local_matrix[13] = b.value[1];
            local_matrix[14] = b.value[2];

            bones.push(BoneData {
                name,
                parent_index: b.parent,
                local_matrix,
            });
        }

        // Extract sequences
        let seq_start = header.sequence_index as usize;
        let seq_len = (header.num_sequences as usize) * core::mem::size_of::<MdlSequenceDesc>();
        let mut sequences = Vec::new();

        if seq_start + seq_len <= bytes.len() {
            let seq_slice = &bytes[seq_start..seq_start + seq_len];
            if let Ok(raw_seqs) = <[MdlSequenceDesc]>::ref_from_bytes(seq_slice) {
                for s in raw_seqs {
                    let label_len = s
                        .label
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(s.label.len());
                    let label = String::from_utf8_lossy(&s.label[..label_len]).into_owned();
                    sequences.push(SequenceData {
                        name: label,
                        fps: s.fps,
                        frame_count: s.num_frames as u32,
                    });
                }
            }
        }

        Ok(SkeletalRig { bones, sequences })
    }
}
