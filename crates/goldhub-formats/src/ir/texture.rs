//! Canonical texture descriptors and block-compressed image formats.

/// Hardware-supported GPU texture compression formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    /// 32-bit uncompressed linear RGBA (8 bits per channel).
    Rgba8Unorm,
    /// DXT1 / BC1 block compression (4 bpp, optional 1-bit alpha).
    Bc1Rgba,
    /// DXT5 / BC3 block compression (8 bpp, interpolated alpha).
    Bc3Rgba,
    /// BC5 dual-channel normal map compression (8 bpp, RG tangent space).
    Bc5Rg,
    /// BC7 high-quality modern HDR/LDR texture compression (8 bpp).
    Bc7Rgba,
}

/// A single mipmap level of a texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Canonical GPU-ready texture resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureDescriptor {
    pub name: String,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub mips: Vec<MipLevel>,
}

impl TextureDescriptor {
    /// Creates a new texture descriptor with a single base mip level.
    pub fn new_2d(
        name: impl Into<String>,
        format: TextureFormat,
        width: u32,
        height: u32,
        data: Vec<u8>,
    ) -> Self {
        Self {
            name: name.into(),
            format,
            width,
            height,
            mips: vec![MipLevel {
                width,
                height,
                data,
            }],
        }
    }
}

/// A UV bounding rectangle inside a packed texture atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasRegion {
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
    pub width: u32,
    pub height: u32,
}

/// Packed texture atlas containing stitched sub-textures and UV coordinate offsets.
#[derive(Debug, Clone)]
pub struct TextureAtlas {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Vec<u8>,
    pub regions: Vec<AtlasRegion>,
}

impl TextureAtlas {
    /// Stitches a collection of uncompressed RGBA8 textures into a compact power-of-two grid atlas.
    pub fn build(textures: &[TextureDescriptor], padding: u32) -> Self {
        if textures.is_empty() {
            return Self {
                width: 4,
                height: 4,
                rgba_data: vec![255; 4 * 4 * 4],
                regions: Vec::new(),
            };
        }

        // Simple shelf / grid packing algorithm
        // 1. Calculate approximate total area
        let mut total_area: u64 = 0;
        for tex in textures {
            total_area += u64::from(tex.width + padding * 2) * u64::from(tex.height + padding * 2);
        }

        // 2. Pick atlas size (power of two, min 512, max 4096)
        let mut atlas_width = 1024_u32;
        let mut atlas_height = 1024_u32;
        while (u64::from(atlas_width) * u64::from(atlas_height)) < total_area * 2
            && atlas_width < 4096
        {
            if atlas_width <= atlas_height {
                atlas_width *= 2;
            } else {
                atlas_height *= 2;
            }
        }

        let mut atlas_pixels = vec![0_u8; (atlas_width * atlas_height * 4) as usize];
        let mut regions = Vec::with_capacity(textures.len());

        let mut current_x = padding;
        let mut current_y = padding;
        let mut row_max_h = 0_u32;

        for tex in textures {
            let tw = tex.width;
            let th = tex.height;

            if current_x + tw + padding > atlas_width {
                current_x = padding;
                current_y += row_max_h + padding;
                row_max_h = 0;
            }

            if current_y + th + padding > atlas_height {
                // Grow atlas vertically if needed
                let new_height = (atlas_height * 2).min(8192);
                let extra_bytes = ((new_height - atlas_height) * atlas_width * 4) as usize;
                atlas_pixels.extend(core::iter::repeat_n(0_u8, extra_bytes));
                atlas_height = new_height;
            }

            row_max_h = row_max_h.max(th);

            // Copy pixels into atlas
            if let Some(base_mip) = tex.mips.first() {
                for y in 0..th {
                    let src_start = ((y * tw) * 4) as usize;
                    let src_end = src_start + (tw * 4) as usize;
                    let dst_start = (((current_y + y) * atlas_width + current_x) * 4) as usize;
                    let dst_end = dst_start + (tw * 4) as usize;

                    if src_end <= base_mip.data.len() && dst_end <= atlas_pixels.len() {
                        atlas_pixels[dst_start..dst_end]
                            .copy_from_slice(&base_mip.data[src_start..src_end]);
                    }
                }
            }

            let u_min = (current_x as f32) / (atlas_width as f32);
            let v_min = (current_y as f32) / (atlas_height as f32);
            let u_max = ((current_x + tw) as f32) / (atlas_width as f32);
            let v_max = ((current_y + th) as f32) / (atlas_height as f32);

            regions.push(AtlasRegion {
                u_min,
                v_min,
                u_max,
                v_max,
                width: tw,
                height: th,
            });

            current_x += tw + padding;
        }

        Self {
            width: atlas_width,
            height: atlas_height,
            rgba_data: atlas_pixels,
            regions,
        }
    }
}
