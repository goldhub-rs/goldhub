//! GoldSrc Sprite (`.spr`) binary decoder.
//!
//! Replicates 1-to-1 parsing logic from `ReHLDS`, `HLSDK`, and `Xash3D FWGS`.
//! Supports:
//! - Magic validation (`IDSP`, version 2 for Half-Life / CS 1.6).
//! - Texture format modes:
//!   - `SPR_NORMAL` (0): Opaque palette lookup
//!   - `SPR_ADDITIVE` (1): Additive glow blend
//!   - `SPR_INDEXALPHA` (2): Index is alpha channel
//!   - `SPR_ALPHTEST` (3): Transparent color key (index 255)
//! - Single frames and animated group frames.
//! - Conversion of indexed 8-bit pixels into 32-bit RGBA pixels.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;

/// Sprite magic bytes ("IDSP").
pub const SPRITE_MAGIC: [u8; 4] = *b"IDSP";

/// Sprite supported format version (GoldSrc v2).
pub const SPRITE_VERSION_HL: u32 = 2;

/// Sprite camera alignment type.
pub const SPR_VP_PARALLEL_UPRIGHT: u32 = 0;
pub const SPR_FACING_UPRIGHT: u32 = 1;
pub const SPR_VP_PARALLEL: u32 = 2;
pub const SPR_ORIENTED: u32 = 3;
pub const SPR_VP_PARALLEL_ORIENTED: u32 = 4;

/// Sprite texture blending mode.
pub const SPR_NORMAL: u32 = 0;
pub const SPR_ADDITIVE: u32 = 1;
pub const SPR_INDEXALPHA: u32 = 2;
pub const SPR_ALPHTEST: u32 = 3;

/// Header structure of a GoldSrc Half-Life Sprite file (`dsprite_hl_t`, 40 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SpriteHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub sprite_type: u32,
    pub tex_format: u32,
    pub bounding_radius: f32,
    pub width: u32,
    pub height: u32,
    pub num_frames: u32,
    pub facetype: u32,
    pub synctype: u32,
}

const _: () = assert!(core::mem::size_of::<SpriteHeader>() == 40);

/// Decoded individual frame of a sprite.
#[derive(Debug, Clone)]
pub struct SpriteFrame {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub rgba_pixels: Vec<u8>,
}

/// Decoded Sprite package holding all metadata and animation frames.
#[derive(Debug, Clone)]
pub struct SpriteData {
    pub sprite_type: u32,
    pub tex_format: u32,
    pub bounding_radius: f32,
    pub max_width: u32,
    pub max_height: u32,
    pub frames: Vec<SpriteFrame>,
}

/// GoldSrc `.spr` asset decoder.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteDecoder;

impl<'a> AssetDecoder<'a, SpriteData> for SpriteDecoder {
    #[allow(clippy::too_many_lines)]
    fn decode(&self, bytes: &'a [u8]) -> Result<SpriteData, FormatError> {
        let header_len = core::mem::size_of::<SpriteHeader>();
        if bytes.len() < header_len {
            return Err(FormatError::BufferTooSmall {
                expected: header_len,
                got: bytes.len(),
            });
        }

        let (header_bytes, _) = bytes.split_at(header_len);
        let header =
            SpriteHeader::ref_from_bytes(header_bytes).map_err(|_| FormatError::MalformedData {
                location: "sprite header",
                reason: "misaligned header bytes".into(),
            })?;

        if header.magic != SPRITE_MAGIC {
            return Err(FormatError::InvalidMagic {
                expected: SPRITE_MAGIC.to_vec(),
                got: header.magic.to_vec(),
            });
        }

        if header.version != SPRITE_VERSION_HL {
            return Err(FormatError::UnsupportedVersion {
                expected: SPRITE_VERSION_HL,
                got: header.version,
            });
        }

        let mut offset = header_len;

        // Read 16-bit palette entry count
        if offset + 2 > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: offset + 2,
                got: bytes.len(),
            });
        }
        let palette_count = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2;

        let palette_bytes_len = palette_count * 3;
        if offset + palette_bytes_len > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: offset + palette_bytes_len,
                got: bytes.len(),
            });
        }

        let palette = &bytes[offset..offset + palette_bytes_len];
        offset += palette_bytes_len;

        let num_frames = header.num_frames.max(1) as usize;
        let mut frames = Vec::with_capacity(num_frames);

        for _ in 0..num_frames {
            if offset + 4 > bytes.len() {
                break;
            }
            let frame_type = i32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;

            if frame_type == 0 {
                // FRAME_SINGLE
                if offset + 16 > bytes.len() {
                    break;
                }
                let origin_x = i32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]);
                let origin_y = i32::from_le_bytes([
                    bytes[offset + 4],
                    bytes[offset + 5],
                    bytes[offset + 6],
                    bytes[offset + 7],
                ]);
                let w = u32::from_le_bytes([
                    bytes[offset + 8],
                    bytes[offset + 9],
                    bytes[offset + 10],
                    bytes[offset + 11],
                ]);
                let h = u32::from_le_bytes([
                    bytes[offset + 12],
                    bytes[offset + 13],
                    bytes[offset + 14],
                    bytes[offset + 15],
                ]);
                offset += 16;

                if w > 4096 || h > 4096 {
                    break;
                }
                let Some(pixel_count) = w.checked_mul(h).map(|cnt| cnt as usize) else {
                    break;
                };
                if offset + pixel_count > bytes.len() {
                    break;
                }

                let index_data = &bytes[offset..offset + pixel_count];
                offset += pixel_count;

                let rgba_pixels = decode_pixels_to_rgba(index_data, palette, header.tex_format);
                frames.push(SpriteFrame {
                    origin_x,
                    origin_y,
                    width: w,
                    height: h,
                    rgba_pixels,
                });
            } else {
                // FRAME_GROUP
                if offset + 4 > bytes.len() {
                    break;
                }
                let group_num = i32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]) as usize;
                offset += 4;

                // Skip intervals (group_num * 4 bytes)
                offset += group_num * 4;

                for _ in 0..group_num {
                    if offset + 16 > bytes.len() {
                        break;
                    }
                    let origin_x = i32::from_le_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ]);
                    let origin_y = i32::from_le_bytes([
                        bytes[offset + 4],
                        bytes[offset + 5],
                        bytes[offset + 6],
                        bytes[offset + 7],
                    ]);
                    let w = u32::from_le_bytes([
                        bytes[offset + 8],
                        bytes[offset + 9],
                        bytes[offset + 10],
                        bytes[offset + 11],
                    ]);
                    let h = u32::from_le_bytes([
                        bytes[offset + 12],
                        bytes[offset + 13],
                        bytes[offset + 14],
                        bytes[offset + 15],
                    ]);
                    offset += 16;

                    if w > 4096 || h > 4096 {
                        break;
                    }
                    let Some(pixel_count) = w.checked_mul(h).map(|cnt| cnt as usize) else {
                        break;
                    };
                    if offset + pixel_count > bytes.len() {
                        break;
                    }
                    let index_data = &bytes[offset..offset + pixel_count];
                    offset += pixel_count;

                    let rgba_pixels = decode_pixels_to_rgba(index_data, palette, header.tex_format);
                    frames.push(SpriteFrame {
                        origin_x,
                        origin_y,
                        width: w,
                        height: h,
                        rgba_pixels,
                    });
                }
            }
        }

        Ok(SpriteData {
            sprite_type: header.sprite_type,
            tex_format: header.tex_format,
            bounding_radius: header.bounding_radius,
            max_width: header.width,
            max_height: header.height,
            frames,
        })
    }
}

/// Converts indexed pixel bytes to 32-bit RGBA according to the sprite texture format.
fn decode_pixels_to_rgba(indices: &[u8], palette: &[u8], tex_format: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(indices.len() * 4);
    let pal_colors = palette.len() / 3;

    // For INDEXALPHA, the last palette color provides the base RGB
    let last_pal_idx = pal_colors.saturating_sub(1) * 3;
    let base_rgb = if last_pal_idx + 3 <= palette.len() {
        [
            palette[last_pal_idx],
            palette[last_pal_idx + 1],
            palette[last_pal_idx + 2],
        ]
    } else {
        [255, 255, 255]
    };

    for &idx in indices {
        let p_idx = (idx as usize) * 3;
        match tex_format {
            SPR_INDEXALPHA => {
                // Index is the alpha value; color is the last palette entry
                out.extend_from_slice(&[base_rgb[0], base_rgb[1], base_rgb[2], idx]);
            }
            SPR_ALPHTEST => {
                if idx == 255 {
                    // Index 255 is canonical transparent color key in GoldSrc.
                    // Strictly zero out RGB to prevent blue texture filtering bleed from palette[255].
                    out.extend_from_slice(&[0, 0, 0, 0]);
                } else if p_idx + 3 <= palette.len() {
                    let r = palette[p_idx];
                    let g = palette[p_idx + 1];
                    let b = palette[p_idx + 2];
                    out.extend_from_slice(&[r, g, b, 255]);
                } else {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
            SPR_ADDITIVE => {
                if p_idx + 3 <= palette.len() {
                    let r = palette[p_idx];
                    let g = palette[p_idx + 1];
                    let b = palette[p_idx + 2];
                    // Additive brightness: alpha proportional to luminance for standard alpha blend
                    let a = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
                    out.extend_from_slice(&[r, g, b, a as u8]);
                } else {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
            _ => {
                // SPR_NORMAL
                if p_idx + 3 <= palette.len() {
                    let r = palette[p_idx];
                    let g = palette[p_idx + 1];
                    let b = palette[p_idx + 2];
                    out.extend_from_slice(&[r, g, b, 255]);
                } else {
                    out.extend_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
    }

    out
}
