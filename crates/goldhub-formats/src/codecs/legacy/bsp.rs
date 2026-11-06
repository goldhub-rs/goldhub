//! BSP30 level format parser and lump extractor.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;

pub const BSP30_VERSION: u32 = 30;
pub const BSP_LUMPS_COUNT: usize = 15;

pub const LUMP_ENTITIES: usize = 0;
pub const LUMP_PLANES: usize = 1;
pub const LUMP_TEXTURES: usize = 2;
pub const LUMP_VERTICES: usize = 3;
pub const LUMP_VISIBILITY: usize = 4;
pub const LUMP_NODES: usize = 5;
pub const LUMP_TEXINFO: usize = 6;
pub const LUMP_FACES: usize = 7;
pub const LUMP_LIGHTING: usize = 8;
pub const LUMP_CLIPNODES: usize = 9;
pub const LUMP_LEAVES: usize = 10;
pub const LUMP_MARKSURFACES: usize = 11;
pub const LUMP_EDGES: usize = 12;
pub const LUMP_SURFEDGES: usize = 13;
pub const LUMP_MODELS: usize = 14;

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspLump {
    pub offset: u32,
    pub length: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspHeader {
    pub version: u32,
    pub lumps: [BspLump; BSP_LUMPS_COUNT],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspPlane {
    pub normal: [f32; 3],
    pub distance: f32,
    pub plane_type: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspTextureInfo {
    pub s: [f32; 3],
    pub s_shift: f32,
    pub t: [f32; 3],
    pub t_shift: f32,
    pub texture_id: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspFace {
    pub plane_id: u16,
    pub side: u16,
    pub first_surfedge_id: u32,
    pub surfedges_num: u16,
    pub texture_info_id: u16,
    pub styles: [u8; 4],
    pub lightmap_offset: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspClipNode {
    pub plane_index: u32,
    pub children: [i16; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BspEdge {
    pub vertices: [u16; 2],
}

/// In-memory view of parsed BSP30 lumps.
pub struct BspLevel<'a> {
    pub planes: &'a [BspPlane],
    pub vertices: &'a [[f32; 3]],
    pub texture_infos: &'a [BspTextureInfo],
    pub faces: &'a [BspFace],
    pub clip_nodes: &'a [BspClipNode],
    pub edges: &'a [BspEdge],
    pub surfedges: &'a [i32],
    pub lighting: &'a [u8],
    pub visdata: &'a [u8],
    pub entities: &'a [u8],
}

fn extract_lump_slice<'a, T: FromBytes + Immutable + KnownLayout>(
    bytes: &'a [u8],
    lump: &BspLump,
    name: &'static str,
) -> Result<&'a [T], FormatError> {
    let start = lump.offset as usize;
    let len = lump.length as usize;
    let end = start + len;

    if end > bytes.len() {
        return Err(FormatError::BufferTooSmall {
            expected: end,
            got: bytes.len(),
        });
    }

    let lump_bytes = &bytes[start..end];
    <[T]>::ref_from_bytes(lump_bytes).map_err(|_| FormatError::MalformedData {
        location: name,
        reason: "lump data alignment or size mismatch".into(),
    })
}

fn extract_lump_raw<'a>(
    bytes: &'a [u8],
    lump: &BspLump,
    _name: &'static str,
) -> Result<&'a [u8], FormatError> {
    let start = lump.offset as usize;
    let len = lump.length as usize;
    let end = start + len;

    if end > bytes.len() {
        return Err(FormatError::BufferTooSmall {
            expected: end,
            got: bytes.len(),
        });
    }

    Ok(&bytes[start..end])
}

/// Decoder for GoldSrc BSP30 levels.
#[derive(Debug, Default, Clone, Copy)]
pub struct Bsp30Decoder;

impl<'a> AssetDecoder<'a, BspLevel<'a>> for Bsp30Decoder {
    fn decode(&self, bytes: &'a [u8]) -> Result<BspLevel<'a>, FormatError> {
        let (header, _) =
            BspHeader::ref_from_prefix(bytes).map_err(|_| FormatError::BufferTooSmall {
                expected: core::mem::size_of::<BspHeader>(),
                got: bytes.len(),
            })?;

        if header.version != BSP30_VERSION {
            return Err(FormatError::UnsupportedVersion {
                expected: BSP30_VERSION,
                got: header.version,
            });
        }

        let entities = extract_lump_raw(bytes, &header.lumps[LUMP_ENTITIES], "entities")?;
        let planes = extract_lump_slice(bytes, &header.lumps[LUMP_PLANES], "planes")?;
        let vertices = extract_lump_slice(bytes, &header.lumps[LUMP_VERTICES], "vertices")?;
        let visdata = extract_lump_raw(bytes, &header.lumps[LUMP_VISIBILITY], "visdata")?;
        let texture_infos = extract_lump_slice(bytes, &header.lumps[LUMP_TEXINFO], "texinfo")?;
        let faces = extract_lump_slice(bytes, &header.lumps[LUMP_FACES], "faces")?;
        let lighting = extract_lump_raw(bytes, &header.lumps[LUMP_LIGHTING], "lighting")?;
        let clip_nodes = extract_lump_slice(bytes, &header.lumps[LUMP_CLIPNODES], "clipnodes")?;
        let edges = extract_lump_slice(bytes, &header.lumps[LUMP_EDGES], "edges")?;
        let surfedges = extract_lump_slice(bytes, &header.lumps[LUMP_SURFEDGES], "surfedges")?;

        Ok(BspLevel {
            planes,
            vertices,
            texture_infos,
            faces,
            clip_nodes,
            edges,
            surfedges,
            lighting,
            visdata,
            entities,
        })
    }
}
