//! WAD3 texture package decoder and miptex indexing.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;
use crate::ir::{TextureDescriptor, TextureFormat};

/// WAD3 Magic ("WAD3").
pub const WAD3_MAGIC: [u8; 4] = *b"WAD3";

/// WAD3 entry type for Half-Life bitmap console/UI fonts (`TYP_QFONT`).
pub const TYP_QFONT: u8 = 70;

/// Single character glyph descriptor inside a GoldSrc `qfont_t` (`charinfo_t`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct QCharInfo {
    pub start_offset: u16,
    pub char_width: u16,
}

/// Decoded GoldSrc `qfont_t` bitmap font lump (`FONT0`, `FONT1`, `FONT2`).
#[derive(Debug, Clone)]
pub struct QFontData {
    pub name: String,
    pub sheet_width: u32,
    pub sheet_height: u32,
    pub row_count: u32,
    pub row_height: u32,
    pub glyphs: [QCharInfo; 256],
    pub rgba_pixels: Vec<u8>,
}

const _: () = assert!(core::mem::size_of::<QCharInfo>() == 4);

/// WAD3 file header.
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct WadHeader {
    pub magic: [u8; 4],
    pub entry_count: u32,
    pub dir_offset: u32,
}

/// Directory entry in WAD3 file.
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct WadEntry {
    pub offset: u32,
    pub disk_size: u32,
    pub size: u32,
    pub entry_type: u8,
    pub compression: u8,
    pub pad: u16,
    pub name: [u8; 16],
}

/// Mipmapped texture header inside WAD3 (miptex_t).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MiptexHeader {
    pub name: [u8; 16],
    pub width: u32,
    pub height: u32,
    pub offsets: [u32; 4],
}

/// Raw header inside a `TYP_QFONT` WAD3 lump (`qfont_t` prefix before trailing pixel data).
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
struct RawQFontHeader {
    width: i32,
    height: i32,
    row_count: i32,
    row_height: i32,
}

const _: () = assert!(core::mem::size_of::<RawQFontHeader>() == 16);

/// In-memory indexed WAD3 archive.
pub struct WadArchive<'a> {
    pub entries: Vec<WadEntry>,
    pub raw_bytes: &'a [u8],
}

impl<'a> WadArchive<'a> {
    /// Finds a texture entry by name (case-insensitive ASCII match).
    pub fn find_entry(&self, name: &str) -> Option<&WadEntry> {
        self.entries.iter().find(|e| {
            let len = e.name.iter().position(|&c| c == 0).unwrap_or(e.name.len());
            let entry_name = &e.name[..len];
            entry_name.eq_ignore_ascii_case(name.as_bytes())
        })
    }

    /// Extracts raw bytes of an entry.
    pub fn read_entry(&self, entry: &WadEntry) -> Result<&'a [u8], FormatError> {
        let start = entry.offset as usize;
        let end = start + (entry.disk_size as usize);
        if end > self.raw_bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: end,
                got: self.raw_bytes.len(),
            });
        }
        Ok(&self.raw_bytes[start..end])
    }

    /// Unpacks an 8-bit paletted miptex into an uncompressed RGBA8 texture descriptor.
    pub fn unpack_miptex(&self, entry: &WadEntry) -> Result<TextureDescriptor, FormatError> {
        let bytes = self.read_entry(entry)?;
        let hdr_len = core::mem::size_of::<MiptexHeader>();
        if bytes.len() < hdr_len {
            return Err(FormatError::BufferTooSmall {
                expected: hdr_len,
                got: bytes.len(),
            });
        }
        let header = MiptexHeader::read_from_bytes(&bytes[..hdr_len]).map_err(|_| {
            FormatError::MalformedData {
                location: "wad miptex header",
                reason: "invalid miptex header".into(),
            }
        })?;

        let width = header.width;
        let height = header.height;
        if width > 4096 || height > 4096 {
            return Err(FormatError::MalformedData {
                location: "wad miptex dimensions",
                reason: format!("dimensions {width}x{height} exceed limit of 4096"),
            });
        }
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| FormatError::MalformedData {
                location: "wad miptex dimensions",
                reason: "dimension overflow".into(),
            })? as usize;

        // The palette is stored after mip level 3: [u16 color_count, [u8; 3] * 256]
        // Base mip data starts at header.offsets[0]
        let offset0 = header.offsets[0] as usize;
        if offset0 + pixel_count > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: offset0 + pixel_count,
                got: bytes.len(),
            });
        }

        let pixel_indices = &bytes[offset0..offset0 + pixel_count];

        // Find palette: located after mip3 data
        let offset3 = header.offsets[3] as usize;
        let mip3_len = (width / 8) * (height / 8);
        let palette_offset = offset3 + (mip3_len as usize) + 2; // +2 for color count uint16

        if palette_offset + 768 > bytes.len() {
            return Err(FormatError::MalformedData {
                location: "miptex palette",
                reason: "palette data truncated".into(),
            });
        }

        let palette_slice = &bytes[palette_offset..palette_offset + 768];

        // Unpack into RGBA8
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        let is_chroma_blue_255 = palette_slice[255 * 3] == 0
            && palette_slice[255 * 3 + 1] == 0
            && palette_slice[255 * 3 + 2] == 255;
        let is_gradient_decal = header.name[0] == b'{' && !is_chroma_blue_255;
        let decal_r = palette_slice[255 * 3];
        let decal_g = palette_slice[255 * 3 + 1];
        let decal_b = palette_slice[255 * 3 + 2];

        for &idx in pixel_indices {
            if is_gradient_decal {
                // Classic GoldSrc gradient decal:
                // idx is the direct alpha value (0 = transparent background), RGB is last palette entry.
                let a = idx;
                rgba.extend_from_slice(&[decal_r, decal_g, decal_b, a]);
            } else {
                let p_idx = (idx as usize) * 3;
                let is_chroma_blue = palette_slice[p_idx] == 0
                    && palette_slice[p_idx + 1] == 0
                    && palette_slice[p_idx + 2] == 255;
                let is_transparent = ((header.name[0] == b'{' || is_chroma_blue_255) && idx == 255)
                    || (header.name[0] == b'{' && is_chroma_blue);

                if is_transparent {
                    // Zero out RGB and Alpha to prevent blue fringe bleeding
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                } else {
                    let r = palette_slice[p_idx];
                    let g = palette_slice[p_idx + 1];
                    let b = palette_slice[p_idx + 2];
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
        }

        let name_len = header
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(header.name.len());
        let name_str = String::from_utf8_lossy(&header.name[..name_len]).into_owned();

        Ok(TextureDescriptor::new_2d(
            name_str,
            TextureFormat::Rgba8Unorm,
            width,
            height,
            rgba,
        ))
    }

    /// Unpacks a `TYP_QFONT` (`0x46`) WAD3 bitmap font lump (`FONT0`, `FONT1`, `FONT2`) into `QFontData`.
    pub fn unpack_qfont(&self, entry: &WadEntry) -> Result<QFontData, FormatError> {
        const QFONT_DATA_OFFSET: usize = 16 + 256 * 4;
        if entry.entry_type != TYP_QFONT {
            return Err(FormatError::MalformedData {
                location: "wad qfont",
                reason: format!("expected entry type {TYP_QFONT}, got {}", entry.entry_type),
            });
        }

        let bytes = self.read_entry(entry)?;
        if bytes.len() < QFONT_DATA_OFFSET + 2 + 768 {
            return Err(FormatError::BufferTooSmall {
                expected: QFONT_DATA_OFFSET + 2 + 768,
                got: bytes.len(),
            });
        }

        let hdr = RawQFontHeader::read_from_bytes(&bytes[..16]).map_err(|_| {
            FormatError::MalformedData {
                location: "qfont header",
                reason: "invalid header bytes".into(),
            }
        })?;

        if hdr.width <= 0 || hdr.height <= 0 || hdr.row_count <= 0 || hdr.row_height <= 0 {
            return Err(FormatError::MalformedData {
                location: "qfont header",
                reason: "invalid negative or zero font dimensions".into(),
            });
        }

        let charinfo_bytes = &bytes[16..QFONT_DATA_OFFSET];
        let mut glyphs = [QCharInfo::default(); 256];
        for (dst, chunk) in glyphs.iter_mut().zip(charinfo_bytes.as_chunks::<4>().0) {
            *dst = QCharInfo::read_from_bytes(chunk).map_err(|_| FormatError::MalformedData {
                location: "qfont charinfo",
                reason: "invalid glyph record".into(),
            })?;
        }

        // In HL1/CS1.6 (`img_wad.c`), qfont `width` is stored in units of QCHAR_WIDTH (16px).
        // Old-style lumps use fixed width 256; HL 1.1+ sets sheet_width = width * 16.
        let sheet_width = (hdr.width as u32).saturating_mul(16).clamp(16, 4096);
        let sheet_height = (hdr.height as u32).clamp(1, 4096);
        let pixel_count = (sheet_width as usize) * (sheet_height as usize);
        let palette_count_offset = QFONT_DATA_OFFSET + pixel_count;

        if palette_count_offset + 2 + 768 > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: palette_count_offset + 2 + 768,
                got: bytes.len(),
            });
        }

        let pixel_indices = &bytes[QFONT_DATA_OFFSET..palette_count_offset];
        let palette_slice = &bytes[palette_count_offset + 2..palette_count_offset + 2 + 768];

        let mut rgba_pixels = Vec::with_capacity(pixel_count * 4);
        for &idx in pixel_indices {
            if idx == 255 {
                // `LUMP_MASKED`: palette index 255 is transparent background
                rgba_pixels.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let p_idx = (idx as usize) * 3;
                rgba_pixels.extend_from_slice(&[
                    palette_slice[p_idx],
                    palette_slice[p_idx + 1],
                    palette_slice[p_idx + 2],
                    255,
                ]);
            }
        }

        let name_len = entry
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.name.len());
        let name = String::from_utf8_lossy(&entry.name[..name_len]).into_owned();

        Ok(QFontData {
            name,
            sheet_width,
            sheet_height,
            row_count: hdr.row_count as u32,
            row_height: hdr.row_height as u32,
            glyphs,
            rgba_pixels,
        })
    }
}

/// Decoder for WAD3 texture packages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Wad3Decoder;

impl<'a> AssetDecoder<'a, WadArchive<'a>> for Wad3Decoder {
    fn decode(&self, bytes: &'a [u8]) -> Result<WadArchive<'a>, FormatError> {
        let (header, _) =
            WadHeader::ref_from_prefix(bytes).map_err(|_| FormatError::BufferTooSmall {
                expected: core::mem::size_of::<WadHeader>(),
                got: bytes.len(),
            })?;

        if header.magic != WAD3_MAGIC {
            return Err(FormatError::InvalidMagic {
                expected: WAD3_MAGIC.to_vec(),
                got: header.magic.to_vec(),
            });
        }

        let dir_start = header.dir_offset as usize;
        let entry_size = core::mem::size_of::<WadEntry>();
        let dir_len = (header.entry_count as usize) * entry_size;

        if dir_start + dir_len > bytes.len() {
            return Err(FormatError::BufferTooSmall {
                expected: dir_start + dir_len,
                got: bytes.len(),
            });
        }

        let entries_slice = &bytes[dir_start..dir_start + dir_len];
        let mut entries = Vec::with_capacity(header.entry_count as usize);
        for chunk in entries_slice.chunks_exact(entry_size) {
            let entry =
                WadEntry::read_from_bytes(chunk).map_err(|_| FormatError::MalformedData {
                    location: "wad entries",
                    reason: "invalid directory entry".into(),
                })?;
            entries.push(entry);
        }

        Ok(WadArchive {
            entries,
            raw_bytes: bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_fonts_wad_qfont_decoding() {
        let fonts_path = std::path::Path::new(r"C:\Games\CS 1.6 GoldClient\valve\fonts.wad");
        if !fonts_path.exists() {
            return;
        }
        let bytes = std::fs::read(fonts_path).expect("read fonts.wad");
        let wad = Wad3Decoder.decode(&bytes).expect("decode WAD3");
        for name in ["FONT0", "FONT1", "FONT2"] {
            let entry = wad
                .find_entry(name)
                .unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(entry.entry_type, TYP_QFONT);
            let qfont = wad
                .unpack_qfont(entry)
                .unwrap_or_else(|e| panic!("unpack {name}: {e:?}"));
            assert_eq!(qfont.sheet_width, 256);
            assert_eq!(qfont.sheet_height, 128);
            assert!(qfont.row_height >= 14);
            assert!(qfont.glyphs[b'A' as usize].char_width >= 8);
            assert!(
                qfont
                    .rgba_pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] == 255)
            );
            assert!(
                qfont
                    .rgba_pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] == 0)
            );
        }
    }
}
