//! Spatial collision tracing against BSP clipnode trees.

use goldhub_formats::ir::CollisionModel;

/// Content types matching GoldSrc BSP specification.
pub const CONTENTS_EMPTY: i16 = -1;
pub const CONTENTS_SOLID: i16 = -2;
pub const CONTENTS_WATER: i16 = -3;
pub const CONTENTS_SLIME: i16 = -4;
pub const CONTENTS_LAVA: i16 = -5;
pub const CONTENTS_SKY: i16 = -6;

/// Result of tracing a player hull through the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceResult {
    /// True if the trace started inside solid geometry.
    pub startsolid: bool,
    /// True if the entire trace line is inside solid.
    pub allsolid: bool,
    /// Normalized fraction along the ray [0.0, 1.0] where collision occurred (1.0 = no hit).
    pub fraction: f32,
    /// Final end position of the hull.
    pub endpos: [f32; 3],
    /// Surface plane normal at the collision point (facing outward).
    pub plane_normal: [f32; 3],
    /// Surface plane distance.
    pub plane_dist: f32,
    /// Optional hit entity identifier (None for world brush).
    pub hit_entity: Option<u32>,
}

impl Default for TraceResult {
    fn default() -> Self {
        Self {
            startsolid: false,
            allsolid: false,
            fraction: 1.0,
            endpos: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            plane_dist: 0.0,
            hit_entity: None,
        }
    }
}

/// Abstract spatial query provider for player movement hulls.
pub trait WorldTrace {
    /// Traces an AABB hull from `start` to `end` against the world.
    fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], hull_idx: u32) -> TraceResult;
}

/// Hardware-accelerated zero-copy BSP collision trace engine.
#[derive(Debug, Clone)]
pub struct BspWorldCollision<'a> {
    pub model: &'a CollisionModel,
}

impl<'a> BspWorldCollision<'a> {
    #[must_use]
    pub fn new(model: &'a CollisionModel) -> Self {
        Self { model }
    }

    /// Recursively traces a ray through a BSP clipnode tree.
    #[allow(clippy::too_many_arguments)]
    fn recursive_trace(
        &self,
        node_idx: i32,
        p1f: f32,
        p2f: f32,
        p1: [f32; 3],
        p2: [f32; 3],
        clip_nodes: &[goldhub_formats::ir::ClipNode],
        trace: &mut TraceResult,
    ) -> bool {
        if node_idx < 0 {
            // Reached a leaf
            if node_idx == i32::from(CONTENTS_SOLID) {
                trace.allsolid = true;
                return true;
            }
            trace.allsolid = false;
            return false;
        }

        let Some(node) = clip_nodes.get(node_idx as usize) else {
            return false;
        };

        let Some(plane) = self.model.planes.get(node.plane_index as usize) else {
            return false;
        };

        let d1 = (p1[0] * plane.normal[0] + p1[1] * plane.normal[1] + p1[2] * plane.normal[2])
            - plane.distance;
        let d2 = (p2[0] * plane.normal[0] + p2[1] * plane.normal[1] + p2[2] * plane.normal[2])
            - plane.distance;

        if d1 >= 0.0 && d2 >= 0.0 {
            return self.recursive_trace(
                i32::from(node.children[0]),
                p1f,
                p2f,
                p1,
                p2,
                clip_nodes,
                trace,
            );
        }
        if d1 < 0.0 && d2 < 0.0 {
            return self.recursive_trace(
                i32::from(node.children[1]),
                p1f,
                p2f,
                p1,
                p2,
                clip_nodes,
                trace,
            );
        }

        // Crosses the plane: calculate intersection fraction
        let idist = 1.0 / (d1 - d2);
        let side = usize::from(d1 < 0.0);
        let frac = (d1 * idist).clamp(0.0, 1.0);

        let mid = [
            p1[0] + (p2[0] - p1[0]) * frac,
            p1[1] + (p2[1] - p1[1]) * frac,
            p1[2] + (p2[2] - p1[2]) * frac,
        ];
        let midf = p1f + (p2f - p1f) * frac;

        // Traverse near side first
        if self.recursive_trace(
            i32::from(node.children[side]),
            p1f,
            midf,
            p1,
            mid,
            clip_nodes,
            trace,
        ) {
            return true;
        }

        // If near side was not solid, save impact plane if far side is solid
        let other_side = 1 - side;
        if Self::check_contents(i32::from(node.children[other_side]), clip_nodes)
            == CONTENTS_SOLID
        {
            if trace.allsolid {
                return true;
            }

            if d1 < 0.0 {
                trace.plane_normal = [-plane.normal[0], -plane.normal[1], -plane.normal[2]];
                trace.plane_dist = -plane.distance;
            } else {
                trace.plane_normal = plane.normal;
                trace.plane_dist = plane.distance;
            }

            trace.fraction = midf;
            trace.endpos = mid;
            return true;
        }

        // Traverse far side
        self.recursive_trace(
            i32::from(node.children[other_side]),
            midf,
            p2f,
            mid,
            p2,
            clip_nodes,
            trace,
        )
    }

    fn check_contents(node_idx: i32, clip_nodes: &[goldhub_formats::ir::ClipNode]) -> i16 {
        let mut curr = node_idx;
        while curr >= 0 {
            let Some(node) = clip_nodes.get(curr as usize) else {
                return CONTENTS_EMPTY;
            };
            curr = i32::from(node.children[0]);
        }
        curr as i16
    }
}

impl WorldTrace for BspWorldCollision<'_> {
    fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], hull_idx: u32) -> TraceResult {
        let mut trace = TraceResult {
            startsolid: false,
            allsolid: false,
            fraction: 1.0,
            endpos: end,
            plane_normal: [0.0, 0.0, 1.0],
            plane_dist: 0.0,
            hit_entity: None,
        };

        let hull = self
            .model
            .hulls
            .iter()
            .find(|h| h.hull_index == hull_idx)
            .or_else(|| self.model.hulls.first());

        let Some(hull) = hull else {
            return trace;
        };

        if hull.root_node < 0 {
            if hull.root_node == i32::from(CONTENTS_SOLID) {
                trace.startsolid = true;
                trace.allsolid = true;
                trace.fraction = 0.0;
                trace.endpos = start;
            }
            return trace;
        }

        self.recursive_trace(
            hull.root_node,
            0.0,
            1.0,
            start,
            end,
            &hull.clip_nodes,
            &mut trace,
        );

        if trace.fraction < 1.0 {
            // Nudge endpos slightly along plane normal to prevent getting stuck on surface boundaries
            const DIST_EPSILON: f32 = 0.03125;
            trace.endpos[0] += trace.plane_normal[0] * DIST_EPSILON;
            trace.endpos[1] += trace.plane_normal[1] * DIST_EPSILON;
            trace.endpos[2] += trace.plane_normal[2] * DIST_EPSILON;
        } else {
            trace.endpos = end;
        }

        trace
    }
}
