//! Zero-cost native binary blob codec for cached meshes (.ghbsp, .ghmdl).

use bytemuck::{Pod, Zeroable};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;
use crate::ir::{MeshBuffer, SubMesh, Vertex};

/// Magic bytes for GoldHub binary cache blobs ("GHBLOB\x01\x00").
pub const BLOB_MAGIC: [u8; 8] = *b"GHBLOB\x01\x00";

/// 64-byte header for GoldHub binary blobs.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BlobHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub submesh_count: u32,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub _pad: [u8; 16],
}

// Ensure BlobHeader is exactly 64 bytes (1 cache line)
const _: () = assert!(core::mem::size_of::<BlobHeader>() == 64);

/// POD layout for serialized submesh draw call metadata.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BlobSubMesh {
    pub name: [u8; 32],
    pub index_start: u32,
    pub index_count: u32,
    pub material_id: u32,
}

/// Decoder for pre-compiled, cache-line aligned native blobs.
#[derive(Debug, Default, Clone, Copy)]
pub struct RawBlobDecoder;

impl<'a> AssetDecoder<'a, MeshBuffer> for RawBlobDecoder {
    fn decode(&self, bytes: &'a [u8]) -> Result<MeshBuffer, FormatError> {
        let (header, rest) =
            BlobHeader::read_from_prefix(bytes).map_err(|e| FormatError::MalformedData {
                location: "blob header",
                reason: format!("failed to read blob header: {e}"),
            })?;

        if header.magic != BLOB_MAGIC {
            return Err(FormatError::InvalidMagic {
                expected: BLOB_MAGIC.to_vec(),
                got: header.magic.to_vec(),
            });
        }

        let vertex_bytes_len = (header.vertex_count as usize)
            .checked_mul(core::mem::size_of::<Vertex>())
            .ok_or_else(|| FormatError::MalformedData {
                location: "blob header",
                reason: "vertex count arithmetic overflow".into(),
            })?;

        if rest.len() < vertex_bytes_len {
            return Err(FormatError::BufferTooSmall {
                expected: vertex_bytes_len,
                got: rest.len(),
            });
        }

        let (vertex_bytes, after_vertices) = rest.split_at(vertex_bytes_len);
        let vertices: &[Vertex] =
            bytemuck::try_cast_slice(vertex_bytes).map_err(|e| FormatError::MalformedData {
                location: "blob vertices",
                reason: format!("misaligned or invalid vertex buffer: {e}"),
            })?;

        let index_bytes_len = (header.index_count as usize)
            .checked_mul(core::mem::size_of::<u32>())
            .ok_or_else(|| FormatError::MalformedData {
                location: "blob header",
                reason: "index count arithmetic overflow".into(),
            })?;

        if after_vertices.len() < index_bytes_len {
            return Err(FormatError::BufferTooSmall {
                expected: index_bytes_len,
                got: after_vertices.len(),
            });
        }

        let (index_bytes, after_indices) = after_vertices.split_at(index_bytes_len);
        let indices: &[u32] =
            bytemuck::try_cast_slice(index_bytes).map_err(|e| FormatError::MalformedData {
                location: "blob indices",
                reason: format!("misaligned index buffer: {e}"),
            })?;

        let submesh_bytes_len = (header.submesh_count as usize)
            .checked_mul(core::mem::size_of::<BlobSubMesh>())
            .ok_or_else(|| FormatError::MalformedData {
                location: "blob header",
                reason: "submesh count arithmetic overflow".into(),
            })?;

        let submeshes =
            if submesh_bytes_len > 0 && after_indices.len() >= submesh_bytes_len {
                let (submesh_bytes, _) = after_indices.split_at(submesh_bytes_len);
                let raw_submeshes: &[BlobSubMesh] = bytemuck::try_cast_slice(submesh_bytes)
                    .map_err(|e| FormatError::MalformedData {
                        location: "blob submeshes",
                        reason: format!("misaligned submesh table: {e}"),
                    })?;

                raw_submeshes
                    .iter()
                    .map(|sm| {
                        let name_len = sm
                            .name
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(sm.name.len());
                        let name = String::from_utf8_lossy(&sm.name[..name_len]).into_owned();
                        SubMesh {
                            name,
                            index_start: sm.index_start,
                            index_count: sm.index_count,
                            material_id: sm.material_id,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };

        Ok(MeshBuffer {
            vertices: vertices.to_vec(),
            indices: indices.to_vec(),
            submeshes,
            aabb_min: header.aabb_min,
            aabb_max: header.aabb_max,
        })
    }
}

/// Serializes a `MeshBuffer` into the 64-byte aligned GoldHub blob format.
pub fn encode_raw_blob(mesh: &MeshBuffer) -> Vec<u8> {
    let header = BlobHeader {
        magic: BLOB_MAGIC,
        version: 1,
        vertex_count: mesh.vertices.len() as u32,
        index_count: mesh.indices.len() as u32,
        submesh_count: mesh.submeshes.len() as u32,
        aabb_min: mesh.aabb_min,
        aabb_max: mesh.aabb_max,
        _pad: [0; 16],
    };

    let header_bytes = bytemuck::bytes_of(&header);
    let vertex_bytes = bytemuck::cast_slice::<Vertex, u8>(&mesh.vertices);
    let index_bytes = bytemuck::cast_slice::<u32, u8>(&mesh.indices);

    let raw_submeshes: Vec<BlobSubMesh> = mesh
        .submeshes
        .iter()
        .map(|sm| {
            let mut name_buf = [0u8; 32];
            let copy_len = sm.name.len().min(31);
            name_buf[..copy_len].copy_from_slice(&sm.name.as_bytes()[..copy_len]);
            BlobSubMesh {
                name: name_buf,
                index_start: sm.index_start,
                index_count: sm.index_count,
                material_id: sm.material_id,
            }
        })
        .collect();

    let submesh_bytes = bytemuck::cast_slice::<BlobSubMesh, u8>(&raw_submeshes);

    let total_len =
        header_bytes.len() + vertex_bytes.len() + index_bytes.len() + submesh_bytes.len();
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(header_bytes);
    out.extend_from_slice(vertex_bytes);
    out.extend_from_slice(index_bytes);
    out.extend_from_slice(submesh_bytes);
    out
}
