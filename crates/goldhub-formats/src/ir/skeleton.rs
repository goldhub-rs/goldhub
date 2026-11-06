//! Canonical skeletal animation models, bones, and keyframe sequences.

/// Bone descriptor with bind-pose transform.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneData {
    pub name: String,
    pub parent_index: i32,
    pub local_matrix: [f32; 16],
}

/// Animation sequence descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceData {
    pub name: String,
    pub fps: f32,
    pub frame_count: u32,
}

/// Canonical skeletal rig and animation database.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkeletalRig {
    pub bones: Vec<BoneData>,
    pub sequences: Vec<SequenceData>,
}
