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

#[inline]
const fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Returns the half-extents bounding box for a given player collision hull.
#[inline]
#[must_use]
pub fn player_hull_bounds(hull_idx: u32) -> ([f32; 3], [f32; 3]) {
    match hull_idx {
        crate::constants::HULL_DUCKING => (
            crate::constants::PLAYER_HULL_MIN_DUCKING,
            crate::constants::PLAYER_HULL_MAX_DUCKING,
        ),
        crate::constants::HULL_POINT => ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        crate::constants::HULL_LARGE => ([-32.0, -32.0, -32.0], [32.0, 32.0, 32.0]),
        _ => (
            crate::constants::PLAYER_HULL_MIN_STANDING,
            crate::constants::PLAYER_HULL_MAX_STANDING,
        ),
    }
}

/// Contact information when player intersects a ladder entity (`func_ladder`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LadderContact {
    /// Outward facing surface normal of the ladder pointing toward the player.
    pub normal: [f32; 3],
    /// Geometric center of the ladder brush.
    pub center: [f32; 3],
    /// Entity ID of the ladder.
    pub entity_id: u32,
}

/// Abstract spatial query provider for player movement hulls.
pub trait WorldTrace {
    /// Traces an AABB hull from `start` to `end` against the world and all active solid brush entities.
    fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], hull_idx: u32) -> TraceResult;

    /// Tests whether the designated player hull intersects any solid world or brush geometry at `pos`.
    /// Returns `Some(hit_entity)` if inside solid (0 for worldspawn, entity_id for brush entity), or `None` if free.
    fn test_player_position(&self, pos: [f32; 3], hull_idx: u32) -> Option<u32>;

    /// Probes for contact with a ladder entity (`func_ladder`) intersecting the player hull.
    fn find_ladder(&self, _origin: [f32; 3], _hull_idx: u32) -> Option<LadderContact> {
        None
    }
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
        root_node: i32,
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
                root_node,
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
                    root_node,
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
            while self.hull_point_contents(hull, root_node, mid) == CONTENTS_SOLID {
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
    fn test_player_position(&self, pos: [f32; 3], hull_idx: u32) -> Option<u32> {
        let world_hull = self
            .model
            .hulls
            .iter()
            .find(|h| h.hull_index == hull_idx)
            .or_else(|| self.model.hulls.first());

        if let Some(hull) = world_hull
            && self.hull_point_contents(hull, hull.root_node, pos) == CONTENTS_SOLID
        {
            return Some(0);
        }

        let (pmins, pmaxs) = player_hull_bounds(hull_idx);
        for ent in &self.model.brush_entities {
            if ent.solid != goldhub_formats::ir::SOLID_BSP || ent.classname == "func_ladder" {
                continue;
            }

            // Broadphase AABB test with player hull expansion
            if pos[0] < ent.mins[0] - pmaxs[0]
                || pos[0] > ent.maxs[0] - pmins[0]
                || pos[1] < ent.mins[1] - pmaxs[1]
                || pos[1] > ent.maxs[1] - pmins[1]
                || pos[2] < ent.mins[2] - pmaxs[2]
                || pos[2] > ent.maxs[2] - pmins[2]
            {
                continue;
            }

            let root_node = ent.headnodes.get(hull_idx as usize).copied().unwrap_or(-1);
            if root_node < 0 {
                if root_node == i32::from(CONTENTS_SOLID) {
                    return Some(ent.entity_id);
                }
                continue;
            }

            let mut local_pos = [
                pos[0] - ent.origin[0],
                pos[1] - ent.origin[1],
                pos[2] - ent.origin[2],
            ];

            if ent.angles[0] != 0.0 || ent.angles[1] != 0.0 || ent.angles[2] != 0.0 {
                let (forward, right, up) = crate::movement::angle_vectors(ent.angles);
                let p = local_pos;
                local_pos = [dot(forward, p), -dot(right, p), dot(up, p)];
            }

            if let Some(hull) = world_hull
                && self.hull_point_contents(hull, root_node, local_pos) == CONTENTS_SOLID
            {
                return Some(ent.entity_id);
            }
        }

        None
    }

    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
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

        let world_hull = self
            .model
            .hulls
            .iter()
            .find(|h| h.hull_index == hull_idx)
            .or_else(|| self.model.hulls.first());

        let Some(hull) = world_hull else {
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
        } else {
            self.recursive_hull_check(
                hull,
                hull.root_node,
                hull.root_node,
                0.0,
                1.0,
                start,
                end,
                &mut trace,
            );
            if trace.allsolid {
                trace.startsolid = true;
            }
        }

        // Trace against active solid brush entities (*1..*N)
        let (pmins, pmaxs) = player_hull_bounds(hull_idx);
        let ray_mins = [
            start[0].min(end[0]) + pmins[0],
            start[1].min(end[1]) + pmins[1],
            start[2].min(end[2]) + pmins[2],
        ];
        let ray_maxs = [
            start[0].max(end[0]) + pmaxs[0],
            start[1].max(end[1]) + pmaxs[1],
            start[2].max(end[2]) + pmaxs[2],
        ];

        for ent in &self.model.brush_entities {
            if ent.solid != goldhub_formats::ir::SOLID_BSP || ent.classname == "func_ladder" {
                continue;
            }

            // Broadphase AABB overlap test
            if ray_maxs[0] < ent.mins[0]
                || ray_mins[0] > ent.maxs[0]
                || ray_maxs[1] < ent.mins[1]
                || ray_mins[1] > ent.maxs[1]
                || ray_maxs[2] < ent.mins[2]
                || ray_mins[2] > ent.maxs[2]
            {
                continue;
            }

            let root_node = ent.headnodes.get(hull_idx as usize).copied().unwrap_or(-1);
            if root_node == i32::from(CONTENTS_EMPTY) {
                continue;
            }

            let mut start_l = [
                start[0] - ent.origin[0],
                start[1] - ent.origin[1],
                start[2] - ent.origin[2],
            ];
            let mut end_l = [
                end[0] - ent.origin[0],
                end[1] - ent.origin[1],
                end[2] - ent.origin[2],
            ];

            let rotated = ent.angles[0] != 0.0 || ent.angles[1] != 0.0 || ent.angles[2] != 0.0;
            let (forward, right, up) = if rotated {
                let (f, r, u) = crate::movement::angle_vectors(ent.angles);
                let s = start_l;
                start_l = [dot(f, s), -dot(r, s), dot(u, s)];
                let e = end_l;
                end_l = [dot(f, e), -dot(r, e), dot(u, e)];
                (f, r, u)
            } else {
                ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0])
            };

            let mut sub_trace = TraceResult {
                startsolid: false,
                allsolid: true,
                fraction: 1.0,
                endpos: end_l,
                plane_normal: [0.0, 0.0, 1.0],
                plane_dist: 0.0,
                hit_entity: Some(ent.entity_id),
            };

            if root_node < 0 {
                if root_node == i32::from(CONTENTS_SOLID) {
                    sub_trace.startsolid = true;
                    sub_trace.allsolid = true;
                    sub_trace.fraction = 0.0;
                    sub_trace.endpos = start_l;
                } else {
                    sub_trace.allsolid = false;
                }
            } else {
                self.recursive_hull_check(
                    hull,
                    root_node,
                    root_node,
                    0.0,
                    1.0,
                    start_l,
                    end_l,
                    &mut sub_trace,
                );
                if sub_trace.allsolid {
                    sub_trace.startsolid = true;
                }
            }

            if sub_trace.startsolid {
                trace.startsolid = true;
                trace.fraction = 0.0;
                trace.hit_entity = Some(ent.entity_id);
            }

            if sub_trace.fraction < trace.fraction {
                trace.fraction = sub_trace.fraction;
                trace.hit_entity = Some(ent.entity_id);
                if rotated {
                    let n = sub_trace.plane_normal;
                    trace.plane_normal = [
                        forward[0] * n[0] - right[0] * n[1] + up[0] * n[2],
                        forward[1] * n[0] - right[1] * n[1] + up[1] * n[2],
                        forward[2] * n[0] - right[2] * n[1] + up[2] * n[2],
                    ];
                } else {
                    trace.plane_normal = sub_trace.plane_normal;
                }
                trace.plane_dist = sub_trace.plane_dist;
                trace.endpos = [
                    start[0] + (end[0] - start[0]) * trace.fraction,
                    start[1] + (end[1] - start[1]) * trace.fraction,
                    start[2] + (end[2] - start[2]) * trace.fraction,
                ];
            }
        }

        if trace.fraction >= 1.0 {
            trace.endpos = end;
        }

        trace
    }

    #[allow(clippy::many_single_char_names, clippy::too_many_lines)]
    fn find_ladder(&self, origin: [f32; 3], hull_idx: u32) -> Option<LadderContact> {
        let (hull_mins, hull_maxs) = player_hull_bounds(hull_idx);

        let world_hull = self
            .model
            .hulls
            .iter()
            .find(|h| h.hull_index == hull_idx)
            .or_else(|| self.model.hulls.first())?;

        for ent in &self.model.brush_entities {
            if ent.classname != "func_ladder" {
                continue;
            }

            // Quick AABB overlap with expanded player hull
            let exp_min = [
                ent.mins[0] - hull_maxs[0],
                ent.mins[1] - hull_maxs[1],
                ent.mins[2] - hull_maxs[2],
            ];
            let exp_max = [
                ent.maxs[0] - hull_mins[0],
                ent.maxs[1] - hull_mins[1],
                ent.maxs[2] - hull_mins[2],
            ];

            if origin[0] < exp_min[0]
                || origin[0] > exp_max[0]
                || origin[1] < exp_min[1]
                || origin[1] > exp_max[1]
                || origin[2] < exp_min[2]
                || origin[2] > exp_max[2]
            {
                continue;
            }

            let root_node = ent.headnodes.get(hull_idx as usize).copied().unwrap_or(-1);
            if root_node == i32::from(CONTENTS_EMPTY) {
                continue;
            }

            let mut local_origin = [
                origin[0] - ent.origin[0],
                origin[1] - ent.origin[1],
                origin[2] - ent.origin[2],
            ];

            let rotated = ent.angles[0] != 0.0 || ent.angles[1] != 0.0 || ent.angles[2] != 0.0;
            let (forward, right, up) = if rotated {
                let (f, r, u) = crate::movement::angle_vectors(ent.angles);
                let p = local_origin;
                local_origin = [dot(f, p), -dot(r, p), dot(u, p)];
                (f, r, u)
            } else {
                ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0])
            };

            let contents = if root_node < 0 {
                root_node as i16
            } else {
                self.hull_point_contents(world_hull, root_node, local_origin)
            };

            if contents != CONTENTS_EMPTY {
                let ladder_center = [
                    f32::midpoint(ent.mins[0], ent.maxs[0]),
                    f32::midpoint(ent.mins[1], ent.maxs[1]),
                    f32::midpoint(ent.mins[2], ent.maxs[2]),
                ];

                let mut local_center = [
                    ladder_center[0] - ent.origin[0],
                    ladder_center[1] - ent.origin[1],
                    ladder_center[2] - ent.origin[2],
                ];
                if rotated {
                    let c = local_center;
                    local_center = [dot(forward, c), -dot(right, c), dot(up, c)];
                }

                let mut sub_trace = TraceResult {
                    startsolid: false,
                    allsolid: true,
                    fraction: 1.0,
                    endpos: local_center,
                    plane_normal: [0.0, 0.0, 1.0],
                    plane_dist: 0.0,
                    hit_entity: Some(ent.entity_id),
                };

                let point_hull = self
                    .model
                    .hulls
                    .iter()
                    .find(|h| h.hull_index == 0)
                    .unwrap_or(world_hull);
                let point_root_node = ent.headnodes.first().copied().unwrap_or(root_node);

                if point_root_node >= 0 {
                    self.recursive_hull_check(
                        point_hull,
                        point_root_node,
                        point_root_node,
                        0.0,
                        1.0,
                        local_origin,
                        local_center,
                        &mut sub_trace,
                    );
                }

                let normal = if sub_trace.fraction < 1.0 {
                    if rotated {
                        let n = sub_trace.plane_normal;
                        [
                            forward[0] * n[0] - right[0] * n[1] + up[0] * n[2],
                            forward[1] * n[0] - right[1] * n[1] + up[1] * n[2],
                            forward[2] * n[0] - right[2] * n[1] + up[2] * n[2],
                        ]
                    } else {
                        sub_trace.plane_normal
                    }
                } else {
                    // Start is inside the ladder brush (common because player hull overlaps the brush).
                    // In GoldSrc/CS 1.6, func_ladder is an oriented bounding box whose face normal is
                    // along its thinnest horizontal dimension (depth of ladder is usually 2-8 units).
                    let dx = (ent.maxs[0] - ent.mins[0]).abs();
                    let dy = (ent.maxs[1] - ent.mins[1]).abs();

                    let face_normal = if dx < dy {
                        // Thin in X: ladder surface plane is perpendicular to X axis
                        if origin[0] < ladder_center[0] {
                            [-1.0, 0.0, 0.0]
                        } else {
                            [1.0, 0.0, 0.0]
                        }
                    } else {
                        // Thin in Y: ladder surface plane is perpendicular to Y axis
                        if origin[1] < ladder_center[1] {
                            [0.0, -1.0, 0.0]
                        } else {
                            [0.0, 1.0, 0.0]
                        }
                    };

                    if rotated {
                        [
                            forward[0] * face_normal[0] - right[0] * face_normal[1]
                                + up[0] * face_normal[2],
                            forward[1] * face_normal[0] - right[1] * face_normal[1]
                                + up[1] * face_normal[2],
                            forward[2] * face_normal[0] - right[2] * face_normal[1]
                                + up[2] * face_normal[2],
                        ]
                    } else {
                        face_normal
                    }
                };

                return Some(LadderContact {
                    normal,
                    center: ladder_center,
                    entity_id: ent.entity_id,
                });
            }
        }

        None
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
            brush_entities: Vec::new(),
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

    #[test]
    fn test_bsp_brush_entity_collision() {
        use goldhub_formats::ir::{BrushEntityCollision, SOLID_BSP};

        let planes = vec![
            // World floor at Z = 0
            CollisionPlane {
                normal: [0.0, 0.0, 1.0],
                distance: 0.0,
                plane_type: 2,
            },
            // Crate plane: front face at X = 50 facing -X (normal = [-1, 0, 0], dist = -50)
            CollisionPlane {
                normal: [-1.0, 0.0, 0.0],
                distance: -50.0,
                plane_type: 0,
            },
        ];
        let clip_nodes = vec![
            // Node 0: Floor
            ClipNode {
                plane_index: 0,
                children: [CONTENTS_EMPTY, CONTENTS_SOLID],
            },
            // Node 1: Crate front face (front is empty, back is solid)
            ClipNode {
                plane_index: 1,
                children: [CONTENTS_EMPTY, CONTENTS_SOLID],
            },
        ];
        let hulls = vec![CollisionHull {
            hull_index: 1,
            root_node: 0,
            clip_nodes,
        }];
        let brush_entities = vec![BrushEntityCollision {
            entity_id: 42,
            model_index: 1,
            classname: "func_wall".to_string(),
            origin: [0.0, 0.0, 0.0],
            angles: [0.0, 0.0, 0.0],
            mins: [50.0, -20.0, 0.0],
            maxs: [90.0, 20.0, 40.0],
            headnodes: [1, 1, 1, 1],
            solid: SOLID_BSP,
            rendermode: 0,
            renderamt: 255,
        }];
        let model = CollisionModel {
            hulls,
            planes,
            brush_entities,
            visdata: Vec::new(),
        };

        let col = BspWorldCollision::new(&model);

        // 1. Ray moving from X = 0 towards X = 100 at Z = 10 (should hit crate at X = 50)
        let tr = col.trace_player_hull([0.0, 0.0, 10.0], [100.0, 0.0, 10.0], 1);
        assert!(!tr.allsolid);
        assert_eq!(tr.hit_entity, Some(42), "Must collide with brush entity 42");
        assert!(tr.fraction < 1.0);
        assert_eq!(tr.plane_normal, [-1.0, 0.0, 0.0]);

        // 2. Position test inside crate (X = 60, Y = 0, Z = 20)
        assert_eq!(col.test_player_position([60.0, 0.0, 20.0], 1), Some(42));

        // 3. Position test outside crate in empty world air (X = 10, Y = 0, Z = 20)
        assert_eq!(col.test_player_position([10.0, 0.0, 20.0], 1), None);
    }
}
