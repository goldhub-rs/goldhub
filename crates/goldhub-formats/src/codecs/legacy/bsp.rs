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
pub struct BspModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub headnode: [i32; 4],
    pub visleafs: i32,
    pub first_face: i32,
    pub num_faces: i32,
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
    pub models: &'a [BspModel],
    pub lighting: &'a [u8],
    pub visdata: &'a [u8],
    pub entities: &'a [u8],
    pub textures_lump: &'a [u8],
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
        let models = extract_lump_slice(bytes, &header.lumps[LUMP_MODELS], "models")?;
        let textures_lump = extract_lump_raw(bytes, &header.lumps[LUMP_TEXTURES], "textures")?;

        Ok(BspLevel {
            planes,
            vertices,
            texture_infos,
            faces,
            clip_nodes,
            edges,
            surfedges,
            models,
            lighting,
            visdata,
            entities,
            textures_lump,
        })
    }
}

impl BspLevel<'_> {
    /// Extracts all embedded miptex textures into RGBA8 TextureDescriptors.
    pub fn extract_embedded_textures(&self) -> Vec<crate::ir::TextureDescriptor> {
        let lump = self.textures_lump;
        if lump.len() < 4 {
            return Vec::new();
        }

        let num_textures = u32::from_le_bytes([lump[0], lump[1], lump[2], lump[3]]) as usize;
        let mut descriptors = Vec::with_capacity(num_textures);

        for i in 0..num_textures {
            let offset_pos = 4 + i * 4;
            if offset_pos + 4 > lump.len() {
                break;
            }
            let tex_offset = u32::from_le_bytes([
                lump[offset_pos],
                lump[offset_pos + 1],
                lump[offset_pos + 2],
                lump[offset_pos + 3],
            ]) as usize;

            if tex_offset == 0 || tex_offset >= lump.len() {
                // External WAD texture reference (no embedded pixels)
                descriptors.push(crate::ir::TextureDescriptor::new_2d(
                    format!("external_tex_{i}"),
                    crate::ir::TextureFormat::Rgba8Unorm,
                    16,
                    16,
                    vec![180; 16 * 16 * 4],
                ));
                continue;
            }

            let miptex_bytes = &lump[tex_offset..];
            if miptex_bytes.len() < core::mem::size_of::<crate::codecs::legacy::wad::MiptexHeader>()
            {
                continue;
            }

            let Ok((mip_header, _)) =
                crate::codecs::legacy::wad::MiptexHeader::ref_from_prefix(miptex_bytes)
            else {
                continue;
            };

            let tw = mip_header.width;
            let th = mip_header.height;
            let pixel_count = (tw * th) as usize;

            if mip_header.offsets[0] == 0 || tw == 0 || th == 0 {
                descriptors.push(crate::ir::TextureDescriptor::new_2d(
                    format!("missing_tex_{i}"),
                    crate::ir::TextureFormat::Rgba8Unorm,
                    16,
                    16,
                    vec![200; 16 * 16 * 4],
                ));
                continue;
            }

            let offset0 = mip_header.offsets[0] as usize;
            let offset3 = mip_header.offsets[3] as usize;
            let mip3_len = (tw / 8) * (th / 8);
            let palette_offset = offset3 + (mip3_len as usize) + 2;

            if offset0 + pixel_count > miptex_bytes.len()
                || palette_offset + 768 > miptex_bytes.len()
            {
                descriptors.push(crate::ir::TextureDescriptor::new_2d(
                    format!("corrupt_tex_{i}"),
                    crate::ir::TextureFormat::Rgba8Unorm,
                    16,
                    16,
                    vec![220; 16 * 16 * 4],
                ));
                continue;
            }

            let pixel_indices = &miptex_bytes[offset0..offset0 + pixel_count];
            let palette = &miptex_bytes[palette_offset..palette_offset + 768];

            let mut rgba = Vec::with_capacity(pixel_count * 4);
            let is_transparent_tex = mip_header.name[0] == b'{'
                || (palette[255 * 3] == 0
                    && palette[255 * 3 + 1] == 0
                    && palette[255 * 3 + 2] == 255);
            for &idx in pixel_indices {
                let p_idx = (idx as usize) * 3;
                let r = palette[p_idx];
                let g = palette[p_idx + 1];
                let b = palette[p_idx + 2];
                let is_transparent = is_transparent_tex && idx == 255;
                let a = if is_transparent { 0 } else { 255 };
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }

            let name_len = mip_header
                .name
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(mip_header.name.len());
            let name_str = String::from_utf8_lossy(&mip_header.name[..name_len]).into_owned();

            descriptors.push(crate::ir::TextureDescriptor::new_2d(
                name_str,
                crate::ir::TextureFormat::Rgba8Unorm,
                tw,
                th,
                rgba,
            ));
        }

        descriptors
    }

    /// Extracts the skybox name defined in the worldspawn entity (e.g. "green", "desert").
    pub fn extract_sky_name(&self) -> Option<String> {
        let entities_str = std::str::from_utf8(self.entities).ok()?;
        for line in entities_str.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"skyname\"")
                && let Some(val) = trimmed.split('"').nth(3)
                && !val.trim().is_empty()
            {
                return Some(val.trim().to_string());
            }
        }
        None
    }
}
