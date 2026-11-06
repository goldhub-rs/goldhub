//! Canonical collision geometry and visibility partitioning.

/// Splitting plane for spatial partitioning and collision queries.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionPlane {
    pub normal: [f32; 3],
    pub distance: f32,
    pub plane_type: u32,
}

/// Clip node in a collision hull BSP tree.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipNode {
    pub plane_index: u32,
    pub children: [i16; 2],
}

/// Independent collision hull (e.g. Hull 0: Point, Hull 1: Standing Player, Hull 2: Large Monster, Hull 3: Ducking Player).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionHull {
    pub hull_index: u32,
    pub root_node: i32,
    pub clip_nodes: Vec<ClipNode>,
}

/// Canonical world collision model including spatial trees and visibility bitsets.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollisionModel {
    pub hulls: Vec<CollisionHull>,
    pub planes: Vec<CollisionPlane>,
    pub visdata: Vec<u8>,
}
