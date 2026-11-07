//! WAD3 texture package decoder and miptex indexing.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::codecs::AssetDecoder;
use crate::error::FormatError;
use crate::ir::{TextureDescriptor, TextureFormat};

/// WAD3 Magic ("WAD3").
pub const WAD3_MAGIC: [u8; 4] = *b"WAD3";

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

/// In-memory indexed WAD3 archive.
pub struct WadArchive<'a> {
    pub entries: &'a [WadEntry],
    pub raw_bytes: &'a [u8],
}

impl<'a> WadArchive<'a> {
    /// Finds a texture entry by name (case-insensitive ASCII match).
    pub fn find_entry(&self, name: &str) -> Option<&'a WadEntry> {
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
        let (header, _) =
            MiptexHeader::ref_from_prefix(bytes).map_err(|_| FormatError::BufferTooSmall {
                expected: core::mem::size_of::<MiptexHeader>(),
                got: bytes.len(),
            })?;

        let width = header.width;
        let height = header.height;
        let pixel_count = (width * height) as usize;

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
        for &idx in pixel_indices {
            let p_idx = (idx as usize) * 3;
            let is_chroma_blue = palette_slice[p_idx] == 0
                && palette_slice[p_idx + 1] == 0
                && palette_slice[p_idx + 2] == 255;
            let is_transparent = ((header.name[0] == b'{'
                || (palette_slice[255 * 3] == 0
                    && palette_slice[255 * 3 + 1] == 0
                    && palette_slice[255 * 3 + 2] == 255))
                && idx == 255)
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
        let entries = <[WadEntry]>::ref_from_bytes(entries_slice).map_err(|_| {
            FormatError::MalformedData {
                location: "wad entries",
                reason: "misaligned directory table".into(),
            }
        })?;

        Ok(WadArchive {
            entries,
            raw_bytes: bytes,
        })
    }
}
