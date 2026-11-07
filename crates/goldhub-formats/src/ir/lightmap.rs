//! Canonical lightmap atlas representations and dynamic lightstyle bindings.

/// Hardware-accelerated lightmap atlas backed by RGBA8 texels.
///
/// Designed as a 2D Texture where lightmaps from all BSP faces are packed tightly
/// with shelf packing, ready for instant GPU upload and sampling in pixel shaders.
#[derive(Debug, Clone, PartialEq)]
pub struct LightmapAtlas {
    pub width: u32,
    pub height: u32,
    /// Raw byte buffer holding RGBA8 data (each luxel is 4 bytes).
    pub rgba_bytes: Vec<u8>,
}

impl LightmapAtlas {
    /// Creates an empty lightmap atlas with default black (0, 0, 0, 255) pixels.
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize) * (height as usize) * 4;
        let mut rgba_bytes = vec![0; size];
        // Set alpha channel to 255 for all texels
        for chunk in rgba_bytes.as_chunks_mut::<4>().0 {
            chunk[3] = 255;
        }
        Self {
            width,
            height,
            rgba_bytes,
        }
    }

    /// Access raw RGBA8 bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.rgba_bytes
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
