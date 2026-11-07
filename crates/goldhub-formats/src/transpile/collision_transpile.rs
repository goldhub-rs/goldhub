//! Collision hull and visibility compiler for BSP30 levels.

use crate::codecs::legacy::bsp::BspLevel;
use crate::error::FormatError;
use crate::ir::{
    BrushEntityCollision, ClipNode, CollisionHull, CollisionModel, CollisionPlane, SOLID_BSP,
    SOLID_NOT,
};

/// Extracts spatial collision planes and clipping hulls from parsed BSP level data.
#[allow(clippy::too_many_lines)]
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

    // Extract all brush entities (*1..*N) for multi-entity collision (physents)
    let mut brush_entities = Vec::new();
    if let Ok(entities_str) = std::str::from_utf8(bsp.entities) {
        let mut entity_id = 0u32;
        for block in entities_str.split('}') {
            let mut model_idx = None;
            let mut classname = String::new();
            let mut rendermode = 0u32;
            let mut renderamt = 0u32;
            let mut origin = [0.0f32; 3];
            let mut has_origin = false;
            let mut angles = [0.0f32; 3];

            for line in block.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("\"model\"")
                    && let Some(m_str) = trimmed.split('"').nth(3)
                    && let Some(num) = m_str.strip_prefix('*')
                {
                    model_idx = num.parse::<usize>().ok();
                } else if trimmed.starts_with("\"classname\"")
                    && let Some(c_str) = trimmed.split('"').nth(3)
                {
                    classname = c_str.to_string();
                } else if trimmed.starts_with("\"rendermode\"") {
                    if let Some(r_str) = trimmed.split('"').nth(3) {
                        rendermode = r_str.parse::<u32>().unwrap_or(0);
                    }
                } else if trimmed.starts_with("\"renderamt\"") {
                    if let Some(a_str) = trimmed.split('"').nth(3) {
                        renderamt = a_str.parse::<u32>().unwrap_or(0);
                    }
                } else if trimmed.starts_with("\"origin\"") {
                    if let Some(o_str) = trimmed.split('"').nth(3) {
                        let mut parts = o_str.split_whitespace();
                        if let (Some(x), Some(y), Some(z)) =
                            (parts.next(), parts.next(), parts.next())
                            && let (Ok(fx), Ok(fy), Ok(fz)) =
                                (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>())
                        {
                            origin = [fx, fy, fz];
                            has_origin = true;
                        }
                    }
                } else if trimmed.starts_with("\"angles\"")
                    && let Some(a_str) = trimmed.split('"').nth(3)
                {
                    let mut parts = a_str.split_whitespace();
                    if let (Some(p), Some(y), Some(r)) = (parts.next(), parts.next(), parts.next())
                        && let (Ok(fp), Ok(fy), Ok(fr)) =
                            (p.parse::<f32>(), y.parse::<f32>(), r.parse::<f32>())
                    {
                        angles = [fp, fy, fr];
                    }
                }
            }

            if let Some(m_idx) = model_idx
                && let Some(submodel) = bsp.models.get(m_idx)
            {
                entity_id += 1;
                let is_solid = match classname.as_str() {
                    "func_illusionary" => false,
                    c if c.starts_with("trigger_") => false,
                    "func_bomb_target"
                    | "func_buyzone"
                    | "func_vip_safetyzone"
                    | "func_escapezone" => false,
                    _ => true,
                };
                let solid = if is_solid { SOLID_BSP } else { SOLID_NOT };

                let mins = if has_origin {
                    [
                        submodel.mins[0] + origin[0],
                        submodel.mins[1] + origin[1],
                        submodel.mins[2] + origin[2],
                    ]
                } else {
                    submodel.mins
                };

                let maxs = if has_origin {
                    [
                        submodel.maxs[0] + origin[0],
                        submodel.maxs[1] + origin[1],
                        submodel.maxs[2] + origin[2],
                    ]
                } else {
                    submodel.maxs
                };

                brush_entities.push(BrushEntityCollision {
                    entity_id,
                    model_index: m_idx,
                    classname,
                    origin,
                    angles,
                    mins,
                    maxs,
                    headnodes: submodel.headnode,
                    solid,
                    rendermode,
                    renderamt: renderamt.min(255) as u8,
                });
            }
        }
    }

    Ok(CollisionModel {
        hulls,
        planes,
        brush_entities,
        visdata: bsp.visdata.to_vec(),
    })
}
