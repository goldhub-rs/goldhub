//! Canonical 3D studio model representation (geometry, meshes, textures, and skeletal rig).

use super::skeleton::SkeletalRig;

/// 3D Studio model vertex with bone skinning binding and texture coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StudioVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bone_index: u16,
}

/// Mesh partition binding triangles to a material/texture index.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioMesh {
    pub skin_ref: usize,
    pub vertices: Vec<StudioVertex>,
    pub indices: Vec<u32>,
}

/// Submodel (e.g. hands, weapon body) containing meshes.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioSubModel {
    pub name: String,
    pub meshes: Vec<StudioMesh>,
}

/// Body part containing alternative submodels.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioBodyPart {
    pub name: String,
    pub models: Vec<StudioSubModel>,
    pub base: i32,
}

/// Decoded 8-bit paletted texture converted into canonical 32-bit RGBA8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioTexture {
    pub name: String,
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub rgba_pixels: Vec<u8>,
}

/// Fully decoded Studio Model (.mdl).
#[derive(Debug, Clone, PartialEq)]
pub struct StudioModel {
    pub name: String,
    pub eye_position: [f32; 3],
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
    pub flags: u32,
    pub rig: SkeletalRig,
    pub body_parts: Vec<StudioBodyPart>,
    pub textures: Vec<StudioTexture>,
    pub skin_families: Vec<Vec<u16>>,
}
