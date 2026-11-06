//! Canonical mesh buffers and vertex representations.

use bytemuck::{Pod, Zeroable};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Standard 64-byte cache-line aligned vertex for static world and level geometry.
///
/// Designed with mechanical sympathy: exactly fits a single 64-byte L1 CPU/GPU cache line.
/// Layout:
/// - `position`: 12 bytes
/// - `normal`: 12 bytes
/// - `tangent`: 16 bytes (xyz + sign w)
/// - `uv0`: 8 bytes (base material coordinates)
/// - `uv1_lightmap`: 8 bytes (radiosity luxel coordinates)
/// - `style_indices`: 4 bytes (dynamic lightstyle layer bindings)
/// - `style_weights`: 4 bytes (linear blend factors)
/// Total = 64 bytes.
#[repr(C)]
#[derive(
    Debug, Clone, Copy, PartialEq, Pod, Zeroable, FromBytes, IntoBytes, KnownLayout, Immutable,
)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv0: [f32; 2],
    pub uv1_lightmap: [f32; 2],
    pub style_indices: [u8; 4],
    pub style_weights: [u8; 4],
}

// Compile-time assertion: Vertex MUST be exactly 64 bytes (1 cache line)
const _: () = assert!(core::mem::size_of::<Vertex>() == 64);

/// Sub-mesh draw call partition within a contiguous mesh buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubMesh {
    pub name: String,
    pub index_start: u32,
    pub index_count: u32,
    pub material_id: u32,
}

/// Contiguous canonical mesh container ready for direct DMA upload to GPU buffers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshBuffer {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub submeshes: Vec<SubMesh>,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
}

impl MeshBuffer {
    /// Creates an empty mesh buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the total vertex count.
    #[inline(always)]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Returns the total index count.
    #[inline(always)]
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// Computes the bounding box min/max from vertices.
    pub fn recompute_bounds(&mut self) {
        if self.vertices.is_empty() {
            self.aabb_min = [0.0; 3];
            self.aabb_max = [0.0; 3];
            return;
        }

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];

        for v in &self.vertices {
            for i in 0..3 {
                if v.position[i] < min[i] {
                    min[i] = v.position[i];
                }
                if v.position[i] > max[i] {
                    max[i] = v.position[i];
                }
            }
        }

        self.aabb_min = min;
        self.aabb_max = max;
    }
}
