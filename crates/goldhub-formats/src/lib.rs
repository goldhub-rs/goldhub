//! Canonical Data IR, pluggable asset codecs, and high-performance caching for GoldHub.

pub mod cache;
pub mod codecs;
pub mod error;
pub mod ir;
pub mod pipeline;

#[cfg(feature = "goldsrc")]
pub mod transpile;

pub use cache::AssetCache;
pub use codecs::AssetDecoder;
pub use error::FormatError;
pub use ir::*;
pub use pipeline::AssetPipeline;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::modern::{RawBlobDecoder, encode_raw_blob};

    #[test]
    fn test_canonical_mesh_and_blob_roundtrip() {
        let mut mesh = MeshBuffer::new();
        mesh.vertices.push(Vertex {
            position: [1.0, 2.0, 3.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [0.5, 0.5],
            uv1_lightmap: [0.1, 0.1],
            style_indices: [0, 1, 2, 3],
            style_weights: [255, 0, 0, 0],
        });
        mesh.indices.extend_from_slice(&[0, 1, 2]);
        mesh.recompute_bounds();

        assert_eq!(mesh.aabb_min, [1.0, 2.0, 3.0]);
        assert_eq!(mesh.aabb_max, [1.0, 2.0, 3.0]);

        // Encode to 64-byte aligned blob
        let encoded = encode_raw_blob(&mesh);
        assert!(encoded.len() >= 64);

        // Decode back
        let decoder = RawBlobDecoder;
        let decoded = decoder.decode(&encoded).expect("blob decode succeeds");

        assert_eq!(decoded.vertices.len(), 1);
        assert_eq!(decoded.vertices[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(decoded.indices, vec![0, 1, 2]);
        assert_eq!(decoded.aabb_min, mesh.aabb_min);
    }

    #[test]
    fn test_texture_descriptor_creation() {
        let tex =
            TextureDescriptor::new_2d("test_tex", TextureFormat::Rgba8Unorm, 2, 2, vec![255; 16]);
        assert_eq!(tex.name, "test_tex");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 2);
        assert_eq!(tex.mips.len(), 1);
        assert_eq!(tex.mips[0].data.len(), 16);
    }
}
