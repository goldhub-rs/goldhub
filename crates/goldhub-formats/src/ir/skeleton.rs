//! Canonical skeletal animation models, bones, and keyframe sequences.

/// Bone descriptor with bind-pose transform.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneData {
    pub name: String,
    pub parent_index: i32,
    pub flags: i32,
    pub local_matrix: [f32; 16],
}

/// Bounding hitbox associated with a specific bone.
#[derive(Debug, Clone, PartialEq)]
pub struct HitboxData {
    pub bone_index: i32,
    pub hit_group: i32,
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
}

/// Queryable attachment point on a model bone.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentData {
    pub name: String,
    pub bone_index: i32,
    pub origin: [f32; 3],
    pub vectors: [[f32; 3]; 3],
}

/// Bone controller for procedural rotations and offsets (e.g. mouth, head yaw).
#[derive(Debug, Clone, PartialEq)]
pub struct BoneControllerData {
    pub bone_index: i32,
    pub motion_type: i32,
    pub start: f32,
    pub end: f32,
    pub rest: i32,
    pub index: i32,
}

/// Animation sequence event (e.g. muzzle flash, footsteps, sounds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimEventData {
    pub frame: i32,
    pub event: i32,
    pub event_type: i32,
    pub options: String,
}

/// Evaluated bone pose at a given animation keyframe.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneTransformKeyframe {
    pub position: [f32; 3],
    pub rotation_quat: [f32; 4],
}

/// Animation sequence descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceData {
    pub name: String,
    pub fps: f32,
    pub flags: i32,
    pub activity: i32,
    pub act_weight: i32,
    pub frame_count: u32,
    pub events: Vec<AnimEventData>,
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
    /// Optional sampled keyframe matrices per frame per bone: `[frame][bone]`
    pub bone_matrices: Vec<Vec<[f32; 16]>>,
}

/// Canonical skeletal rig and animation database.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkeletalRig {
    pub bones: Vec<BoneData>,
    pub hitboxes: Vec<HitboxData>,
    pub attachments: Vec<AttachmentData>,
    pub controllers: Vec<BoneControllerData>,
    pub sequences: Vec<SequenceData>,
}
