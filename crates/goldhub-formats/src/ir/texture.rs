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
