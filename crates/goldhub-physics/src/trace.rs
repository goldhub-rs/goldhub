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

    /// Evaluates the content type at point `p` by traversing clipnodes down to a leaf (`PM_HullPointContents`).
    #[must_use]
    pub fn hull_point_contents(
        &self,
        hull: &goldhub_formats::ir::CollisionHull,
        mut node_idx: i32,
        p: [f32; 3],
    ) -> i16 {
        while node_idx >= 0 {
            let Some(node) = hull.clip_nodes.get(node_idx as usize) else {
                return CONTENTS_EMPTY;
            };
            let Some(plane) = self.model.planes.get(node.plane_index as usize) else {
                return CONTENTS_EMPTY;
            };
            let d = (p[0] * plane.normal[0] + p[1] * plane.normal[1] + p[2] * plane.normal[2])
                - plane.distance;
            let side = usize::from(d < 0.0);
            node_idx = i32::from(node.children[side]);
        }
        node_idx as i16
    }

    /// Recursively traces a ray through a BSP clipnode tree (`PM_RecursiveHullCheck`).
    /// Returns `true` if the ray completed without collision, `false` if stopped by a solid surface.
    #[allow(
        clippy::too_many_arguments,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::if_not_else
    )]
    fn recursive_hull_check(
        &self,
        hull: &goldhub_formats::ir::CollisionHull,
        mut node_idx: i32,
        p1f: f32,
        p2f: f32,
        p1: [f32; 3],
        p2: [f32; 3],
        trace: &mut TraceResult,
    ) -> bool {
        const DIST_EPSILON: f32 = 0.03125;

        loop {
            // Leaf node reached
            if node_idx < 0 {
                if node_idx == i32::from(CONTENTS_SOLID) {
                    trace.startsolid = true;
                } else {
                    trace.allsolid = false;
                }
                return true;
            }

            let Some(node) = hull.clip_nodes.get(node_idx as usize) else {
                trace.allsolid = false;
                return true;
            };

            let Some(plane) = self.model.planes.get(node.plane_index as usize) else {
                trace.allsolid = false;
                return true;
            };

            let t1 = (p1[0] * plane.normal[0] + p1[1] * plane.normal[1] + p1[2] * plane.normal[2])
                - plane.distance;
            let t2 = (p2[0] * plane.normal[0] + p2[1] * plane.normal[1] + p2[2] * plane.normal[2])
                - plane.distance;

            if t1 >= 0.0 && t2 >= 0.0 {
                node_idx = i32::from(node.children[0]);
                continue;
            }
            if t1 < 0.0 && t2 < 0.0 {
                node_idx = i32::from(node.children[1]);
                continue;
            }

            // Put the crosspoint DIST_EPSILON on the near side of the plane
            let side = usize::from(t1 < 0.0);
            let denom = t1 - t2;
            let mut frac = if side != 0 {
                (t1 + DIST_EPSILON) / denom
            } else {
                (t1 - DIST_EPSILON) / denom
            };

            if frac.is_nan() || frac < 0.0 {
                frac = 0.0;
            } else if frac > 1.0 {
                frac = 1.0;
            }

            let mut midf = p1f + (p2f - p1f) * frac;
            let mut mid = [
                p1[0] + (p2[0] - p1[0]) * frac,
                p1[1] + (p2[1] - p1[1]) * frac,
                p1[2] + (p2[2] - p1[2]) * frac,
            ];

            // Move up to the node on the near side
            if !self.recursive_hull_check(
                hull,
                i32::from(node.children[side]),
                p1f,
                midf,
                p1,
                mid,
                trace,
            ) {
                return false;
            }

            let other_side = side ^ 1;
            // If the other side is not solid, continue tracing past the plane
            if self.hull_point_contents(hull, i32::from(node.children[other_side]), mid)
                != CONTENTS_SOLID
            {
                return self.recursive_hull_check(
                    hull,
                    i32::from(node.children[other_side]),
                    midf,
                    p2f,
                    mid,
                    p2,
                    trace,
                );
            }

            // Never got out of solid area
            if trace.allsolid {
                return false;
            }

            // The other side is solid: this is the canonical collision impact point!
            if side == 0 {
                trace.plane_normal = plane.normal;
                trace.plane_dist = plane.distance;
            } else {
                trace.plane_normal = [-plane.normal[0], -plane.normal[1], -plane.normal[2]];
                trace.plane_dist = -plane.distance;
            }

            // Step back if mid is still solid inside the root hull
            let mut back_frac = frac;
            while self.hull_point_contents(hull, hull.root_node, mid) == CONTENTS_SOLID {
                back_frac -= 0.1;
                if back_frac.is_nan() || back_frac < 0.0 {
                    trace.fraction = midf;
                    trace.endpos = mid;
                    return false;
                }
                midf = p1f + (p2f - p1f) * back_frac;
                mid = [
                    p1[0] + (p2[0] - p1[0]) * back_frac,
                    p1[1] + (p2[1] - p1[1]) * back_frac,
                    p1[2] + (p2[2] - p1[2]) * back_frac,
                ];
            }

            trace.fraction = midf;
            trace.endpos = mid;
            return false;
        }
    }
}

impl WorldTrace for BspWorldCollision<'_> {
    fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], hull_idx: u32) -> TraceResult {
        let mut trace = TraceResult {
            startsolid: false,
            allsolid: true,
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
            } else {
                trace.allsolid = false;
            }
            return trace;
        }

        self.recursive_hull_check(hull, hull.root_node, 0.0, 1.0, start, end, &mut trace);

        if trace.allsolid {
            trace.startsolid = true;
        }

        if trace.fraction >= 1.0 {
            trace.endpos = end;
        }

        trace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goldhub_formats::ir::{ClipNode, CollisionHull, CollisionModel, CollisionPlane};

    #[test]
    fn test_bsp_world_collision_floor_hit() {
        // Plane at Z = 0, normal = [0, 0, 1]
        let planes = vec![CollisionPlane {
            normal: [0.0, 0.0, 1.0],
            distance: 0.0,
            plane_type: 2,
        }];
        // Root node 0: child[0] (front, Z >= 0) is CONTENTS_EMPTY (-1), child[1] (back, Z < 0) is CONTENTS_SOLID (-2)
        let clip_nodes = vec![ClipNode {
            plane_index: 0,
            children: [CONTENTS_EMPTY, CONTENTS_SOLID],
        }];
        let hulls = vec![CollisionHull {
            hull_index: 1,
            root_node: 0,
            clip_nodes,
        }];
        let model = CollisionModel {
            hulls,
            planes,
            visdata: Vec::new(),
        };

        let col = BspWorldCollision::new(&model);
        // Trace from Z = 10 to Z = -10 (should hit floor)
        let tr = col.trace_player_hull([0.0, 0.0, 10.0], [0.0, 0.0, -10.0], 1);
        assert!(!tr.allsolid);
        assert!(!tr.startsolid);
        assert!(tr.fraction < 1.0);
        assert_eq!(tr.plane_normal, [0.0, 0.0, 1.0]);
        // Point contents at Z = 5 should be empty, at Z = -5 should be solid
        assert_eq!(
            col.hull_point_contents(&col.model.hulls[0], 0, [0.0, 0.0, 5.0]),
            CONTENTS_EMPTY
        );
        assert_eq!(
            col.hull_point_contents(&col.model.hulls[0], 0, [0.0, 0.0, -5.0]),
            CONTENTS_SOLID
        );
    }
}
