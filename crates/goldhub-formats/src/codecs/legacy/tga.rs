//! TGA (Truevision TGA) image decoder for legacy GoldSrc skybox textures.
//!
//! Complies with the Scrooge Systems Mindset: zero-copy parsing, contiguous memory allocation,
//! explicit fail-fast validation and no external heavy dependencies.

use crate::error::FormatError;

/// Decoded uncompressed true-color TGA image.
#[derive(Debug, Clone)]
pub struct TgaImage {
    pub width: u32,
    pub height: u32,
    /// RGBA8 pixels buffer (4 bytes per pixel, length = width * height * 4).
    pub rgba: Vec<u8>,
}

/// Decodes an uncompressed 24-bit (BGR) or 32-bit (BGRA) TGA image from bytes.
pub fn decode_tga(bytes: &[u8]) -> Result<TgaImage, FormatError> {
    if bytes.len() < 18 {
        return Err(FormatError::BufferTooSmall {
            expected: 18,
            got: bytes.len(),
        });
    }

    let id_len = bytes[0] as usize;
    let color_map_type = bytes[1];
    let image_type = bytes[2];

    // Image type 2: Uncompressed True-Color Image
    if image_type != 2 {
        return Err(FormatError::UnsupportedFeature(format!(
            "Only uncompressed true-color TGA (type 2) is supported, got type {image_type}"
        )));
    }

    // Color map should be absent for type 2
    if color_map_type != 0 {
        return Err(FormatError::UnsupportedFeature("Color-mapped TGA is not supported".into()));
    }

    let width = u32::from(u16::from_le_bytes([bytes[12], bytes[13]]));
    let height = u32::from(u16::from_le_bytes([bytes[14], bytes[15]]));
    let pixel_depth = bytes[16];
    let descriptor = bytes[17];

    if width == 0 || height == 0 {
        return Err(FormatError::MalformedData {
            location: "tga_header",
            reason: "Zero dimension in TGA image".into(),
        });
    }

    if pixel_depth != 24 && pixel_depth != 32 {
        return Err(FormatError::UnsupportedFeature(format!(
            "Unsupported TGA bit depth: {pixel_depth} (expected 24 or 32)"
        )));
    }

    let bytes_per_pixel = (pixel_depth / 8) as usize;
    let pixel_data_offset = 18 + id_len;
    let expected_data_len = (width * height) as usize * bytes_per_pixel;

    if bytes.len() < pixel_data_offset + expected_data_len {
        return Err(FormatError::BufferTooSmall {
            expected: pixel_data_offset + expected_data_len,
            got: bytes.len(),
        });
    }

    let raw_pixels = &bytes[pixel_data_offset..pixel_data_offset + expected_data_len];
    let total_pixels = (width * height) as usize;
    let mut rgba = vec![255u8; total_pixels * 4];

    // Bit 5 of descriptor: 0 = origin at bottom-left, 1 = origin at top-left
    let top_to_bottom = (descriptor & 0x20) != 0;

    for y in 0..height {
        let src_y = if top_to_bottom { y } else { height - 1 - y };
        for x in 0..width {
            let src_idx = ((src_y * width + x) as usize) * bytes_per_pixel;
            let dst_idx = ((y * width + x) as usize) * 4;

            let b = raw_pixels[src_idx];
            let g = raw_pixels[src_idx + 1];
            let r = raw_pixels[src_idx + 2];
            let a = if bytes_per_pixel == 4 {
                raw_pixels[src_idx + 3]
            } else {
                255
            };

            rgba[dst_idx] = r;
            rgba[dst_idx + 1] = g;
            rgba[dst_idx + 2] = b;
            rgba[dst_idx + 3] = a;
        }
    }

    Ok(TgaImage {
        width,
        height,
        rgba,
    })
}
