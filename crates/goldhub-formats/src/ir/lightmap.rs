//! Canonical lightmap atlas representations and dynamic lightstyle bindings.

/// Hardware-accelerated lightmap atlas backed by FP16 linear HDR texels.
///
/// Designed as a 2D Texture Array (`Texture2DArray<R16G16B16A16_FLOAT>`) where each array slice
/// corresponds to a dynamic lightstyle layer, eliminating CPU atlas rebuilds during animated light sequences.
#[derive(Debug, Clone, PartialEq)]
pub struct LightmapAtlas {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    /// Raw byte buffer holding FP16 RGBA data (each luxel is 4 * 2 = 8 bytes).
    pub fp16_bytes: Vec<u8>,
}

impl LightmapAtlas {
    /// Creates an empty lightmap atlas.
    pub fn new(width: u32, height: u32, layers: u32) -> Self {
        let size = (width as usize) * (height as usize) * (layers as usize) * 8;
        Self {
            width,
            height,
            layers,
            fp16_bytes: vec![0; size],
        }
    }
}

/// Uniform buffer payload holding scalar intensities for up to 64 dynamic lightstyles.
///
/// Dispatched directly to the GPU shader uniform buffer (`array<f32, 64>`) to blend
/// pulsing, flickering, and animated light sources entirely inside the pixel shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicLightStyles {
    pub intensities: [f32; 64],
}

impl Default for DynamicLightStyles {
    fn default() -> Self {
        Self {
            intensities: [1.0; 64],
        }
    }
}

impl DynamicLightStyles {
    /// Creates a default lightstyle state with all 64 styles initialized to full intensity (1.0).
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the scalar intensity for a specific lightstyle index (0..63).
    #[inline(always)]
    pub fn set_intensity(&mut self, style_index: usize, value: f32) {
        if style_index < 64 {
            self.intensities[style_index] = value;
        }
    }
}
