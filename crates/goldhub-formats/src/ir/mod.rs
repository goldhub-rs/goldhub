//! Intermediate Representation (IR) for assets.
//!
//! All asset codecs (glTF, KTX2, BSP30, WAD3, Studio MDL) decode into these unified structures.

pub mod collision;
pub mod lightmap;
pub mod mesh;
pub mod model;
pub mod skeleton;
pub mod texture;

pub use collision::{
    BrushEntityCollision, ClipNode, CollisionHull, CollisionModel, CollisionPlane, SOLID_BBOX,
    SOLID_BSP, SOLID_NOT, SOLID_SLIDEBOX, SOLID_TRIGGER,
};
pub use lightmap::{DynamicLightStyles, LightmapAtlas};
pub use mesh::{MeshBuffer, SubMesh, Vertex};
pub use model::{
    StudioBodyPart, StudioMesh, StudioModel, StudioSubModel, StudioTexture, StudioVertex,
};
pub use skeleton::{
    AnimEventData, AttachmentData, BoneControllerData, BoneData, BoneTransformKeyframe, HitboxData,
    SequenceData, SkeletalRig,
};
pub use texture::{
    AtlasRegion, MipLevel, TextureAtlas, TextureDescriptor, TextureFormat, generate_rgba8_mipmaps,
};
