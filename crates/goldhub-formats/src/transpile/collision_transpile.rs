//! Collision hull and visibility compiler for BSP30 levels.

use crate::codecs::legacy::bsp::BspLevel;
use crate::error::FormatError;
use crate::ir::{ClipNode, CollisionHull, CollisionModel, CollisionPlane};

/// Extracts spatial collision planes and clipping hulls from parsed BSP level data.
pub fn compile_bsp_collision(bsp: &BspLevel<'_>) -> Result<CollisionModel, FormatError> {
    let mut planes = Vec::with_capacity(bsp.planes.len());
    for p in bsp.planes {
        planes.push(CollisionPlane {
            normal: p.normal,
            distance: p.distance,
            plane_type: p.plane_type,
        });
    }

    let mut clip_nodes = Vec::with_capacity(bsp.clip_nodes.len());
    for cn in bsp.clip_nodes {
        clip_nodes.push(ClipNode {
            plane_index: cn.plane_index,
            children: cn.children,
        });
    }

    let world_model = bsp.models.first();
    let headnodes = world_model.map_or([0; 4], |m| m.headnode);

    // Extract all 4 hulls (0: Point/Ray, 1: Standing Player, 2: Large, 3: Ducking Player)
    let mut hulls = Vec::with_capacity(4);
    for (hull_idx, &headnode) in headnodes.iter().enumerate() {
        hulls.push(CollisionHull {
            hull_index: hull_idx as u32,
            root_node: headnode,
            clip_nodes: clip_nodes.clone(),
        });
    }

    Ok(CollisionModel {
        hulls,
        planes,
        visdata: bsp.visdata.to_vec(),
    })
}
