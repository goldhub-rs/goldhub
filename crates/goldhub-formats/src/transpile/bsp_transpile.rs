//! High-performance polygon triangulation and geometry compiler for BSP30 levels.

use crate::codecs::legacy::bsp::{BspFace, BspLevel};
use crate::error::FormatError;
use crate::ir::lightmap::LightmapAtlas;
use crate::ir::{MeshBuffer, SubMesh, Vertex};

/// Metadata computed for a BSP face's lightmap block before packing into the global atlas.
struct FaceLightmapInfo {
    face_idx: usize,
    s_min: f32,
    t_min: f32,
    w: u32,
    h: u32,
    light_offset: u32,
}

/// Face lightmap placement: (s_min, t_min, [atlas_x, atlas_y], [atlas_w, atlas_h])
type FaceUvMapping = std::collections::HashMap<usize, (f32, f32, [f32; 2], [f32; 2])>;

/// Computes the lightmap extents and allocates positions in a single RGBA8 Lightmap Atlas (2048px width).
fn build_lightmap_atlas(
    bsp: &BspLevel<'_>,
    invisible_faces: &std::collections::HashSet<usize>,
    textures: &[crate::ir::TextureDescriptor],
) -> (LightmapAtlas, FaceUvMapping) {
    let mut face_infos = Vec::new();

    for (face_idx, face) in bsp.faces.iter().enumerate() {
        if invisible_faces.contains(&face_idx) || face.lightmap_offset == u32::MAX {
            continue;
        }

        let (s_vec, s_shift, t_vec, t_shift, tex_id) = resolve_texture_projection(face, bsp);

        if let Some(tex) = textures.get(tex_id as usize) {
            let tex_lower = tex.name.to_ascii_lowercase();
            if tex_lower == "aaatrigger"
                || tex_lower == "clip"
                || tex_lower == "origin"
                || tex_lower == "{invisible"
                || tex_lower == "sky"
            {
                continue;
            }
        }

        let face_positions = collect_face_positions(face, bsp);
        if face_positions.len() < 3 {
            continue;
        }

        let mut s_min = f32::MAX;
        let mut s_max = f32::MIN;
        let mut t_min = f32::MAX;
        let mut t_max = f32::MIN;

        for &pos in &face_positions {
            let s = pos[0] * s_vec[0] + pos[1] * s_vec[1] + pos[2] * s_vec[2] + s_shift;
            let t = pos[0] * t_vec[0] + pos[1] * t_vec[1] + pos[2] * t_vec[2] + t_shift;
            s_min = s_min.min(s);
            s_max = s_max.max(s);
            t_min = t_min.min(t);
            t_max = t_max.max(t);
        }

        let b_s_min = (s_min / 16.0).floor();
        let b_s_max = (s_max / 16.0).ceil();
        let b_t_min = (t_min / 16.0).floor();
        let b_t_max = (t_max / 16.0).ceil();

        let w = ((b_s_max - b_s_min) as i32 + 1).max(1) as u32;
        let h = ((b_t_max - b_t_min) as i32 + 1).max(1) as u32;

        face_infos.push(FaceLightmapInfo {
            face_idx,
            s_min: b_s_min * 16.0,
            t_min: b_t_min * 16.0,
            w,
            h,
            light_offset: face.lightmap_offset,
        });
    }

    // Shelf packing: width = 2048, packed row by row
    let atlas_width = 2048u32;
    let mut current_x = 0u32;
    let mut current_y = 0u32;
    let mut row_h = 0u32;

    // Face UV mapping: face_idx -> (s_min, t_min, [atlas_x, atlas_y], [atlas_w, atlas_h])
    let mut face_uv_map = std::collections::HashMap::with_capacity(face_infos.len());
    let mut placements = Vec::with_capacity(face_infos.len());

    for info in &face_infos {
        if current_x + info.w > atlas_width {
            current_x = 0;
            current_y += row_h;
            row_h = 0;
        }

        placements.push((info, current_x, current_y));
        row_h = row_h.max(info.h);
        current_x += info.w;
    }

    let total_height = (current_y + row_h).max(1).next_power_of_two();
    let mut atlas = LightmapAtlas::new(atlas_width, total_height);

    for (info, x_pos, y_pos) in placements {
        face_uv_map.insert(
            info.face_idx,
            (
                info.s_min,
                info.t_min,
                [x_pos as f32, y_pos as f32],
                [atlas_width as f32, total_height as f32],
            ),
        );

        // Copy raw RGB luxels into the atlas RGBA8 buffer
        let luxel_count = (info.w * info.h) as usize;
        let src_start = info.light_offset as usize;
        let src_end = src_start + luxel_count * 3;

        if src_end <= bsp.lighting.len() {
            let luxels = &bsp.lighting[src_start..src_end];
            for ly in 0..info.h {
                for lx in 0..info.w {
                    let src_idx = ((ly * info.w + lx) * 3) as usize;
                    let dst_idx = (((y_pos + ly) * atlas_width + (x_pos + lx)) * 4) as usize;

                    if src_idx + 2 < luxels.len() && dst_idx + 3 < atlas.rgba_bytes.len() {
                        atlas.rgba_bytes[dst_idx] = luxels[src_idx];
                        atlas.rgba_bytes[dst_idx + 1] = luxels[src_idx + 1];
                        atlas.rgba_bytes[dst_idx + 2] = luxels[src_idx + 2];
                        atlas.rgba_bytes[dst_idx + 3] = 255;
                    }
                }
            }
        }
    }

    (atlas, face_uv_map)
}

/// Parses entities lump string to collect face ranges of invisible entities (triggers, collision walls).
fn collect_invisible_face_ranges(bsp: &BspLevel<'_>) -> std::collections::HashSet<usize> {
    let mut invis_faces = std::collections::HashSet::new();
    let Ok(entities_str) = std::str::from_utf8(bsp.entities) else {
        return invis_faces;
    };

    for block in entities_str.split('}') {
        let mut model_idx = None;
        let mut rendermode = 0u32;
        let mut is_trigger = false;

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"model\"")
                && let Some(m_str) = trimmed.split('"').nth(3)
                && let Some(num) = m_str.strip_prefix('*')
            {
                model_idx = num.parse::<usize>().ok();
            } else if trimmed.starts_with("\"rendermode\"") {
                if let Some(r_str) = trimmed.split('"').nth(3) {
                    rendermode = r_str.parse::<u32>().unwrap_or(0);
                }
            } else if trimmed.starts_with("\"classname\"")
                && let Some(c_str) = trimmed.split('"').nth(3)
                && (c_str == "func_ladder"
                    || c_str == "func_bomb_target"
                    || c_str.starts_with("trigger_"))
            {
                is_trigger = true;
            }
        }

        // In GoldSrc: rendermode 2 (kRenderTransTexture) on func_wall is an invisible collision wall
        if (is_trigger || rendermode == 2)
            && let Some(m_idx) = model_idx
            && let Some(model) = bsp.models.get(m_idx)
        {
            let first = model.first_face.max(0) as usize;
            let num = model.num_faces.max(0) as usize;
            for f in first..first + num {
                invis_faces.insert(f);
            }
        }
    }

    invis_faces
}

/// Compiles raw BSP30 level lumps into a contiguous, cache-line aligned Canonical `MeshBuffer`.
pub fn compile_bsp_geometry(bsp: &BspLevel<'_>) -> Result<MeshBuffer, FormatError> {
    let textures = bsp.extract_embedded_textures();
    let (mesh, _) = compile_bsp_geometry_and_lightmap(bsp, &textures)?;
    Ok(mesh)
}

/// Compiles raw BSP30 level into partitioned submeshes with unconstrained, repeating UV coordinates.
pub fn compile_bsp_geometry_submeshes(
    bsp: &BspLevel<'_>,
    textures: &[crate::ir::TextureDescriptor],
) -> Result<MeshBuffer, FormatError> {
    let (mesh, _) = compile_bsp_geometry_and_lightmap(bsp, textures)?;
    Ok(mesh)
}

/// Compiles raw BSP30 level geometry and generates the hardware-accelerated packed lightmap atlas.
pub fn compile_bsp_geometry_and_lightmap(
    bsp: &BspLevel<'_>,
    textures: &[crate::ir::TextureDescriptor],
) -> Result<(MeshBuffer, LightmapAtlas), FormatError> {
    let invisible_faces = collect_invisible_face_ranges(bsp);
    let (atlas, face_uv_map) = build_lightmap_atlas(bsp, &invisible_faces, textures);

    let mut vertices: Vec<Vertex> = Vec::with_capacity(bsp.faces.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(bsp.faces.len() * 6);
    let mut submeshes: Vec<SubMesh> = Vec::with_capacity(bsp.faces.len());

    for (face_idx, face) in bsp.faces.iter().enumerate() {
        if invisible_faces.contains(&face_idx) {
            continue;
        }

        let (s_vec, s_shift, t_vec, t_shift, tex_id) = resolve_texture_projection(face, bsp);

        // Filter out invisible engine tool textures (AAATRIGGER, CLIP, ORIGIN) and SKY brushes
        if let Some(tex) = textures.get(tex_id as usize) {
            let tex_lower = tex.name.to_ascii_lowercase();
            if tex_lower == "aaatrigger"
                || tex_lower == "clip"
                || tex_lower == "origin"
                || tex_lower == "{invisible"
                || tex_lower == "sky"
            {
                continue;
            }
        }

        let face_positions = collect_face_positions(face, bsp);
        if face_positions.len() < 3 {
            continue;
        }
        let normal = compute_face_normal(&face_positions);
        let tangent = [s_vec[0], s_vec[1], s_vec[2], 1.0];

        let submesh_start = indices.len() as u32;
        let base_vert_idx = vertices.len() as u32;

        let (tex_w, tex_h) = textures
            .get(tex_id as usize)
            .map_or((128.0, 128.0), |t| (t.width as f32, t.height as f32));

        // Lightmap atlas mapping parameters for this face
        let lm_info = face_uv_map.get(&face_idx);

        // Triangulate convex polygon fan: (0, 1, 2), (0, 2, 3), (0, 3, 4), ...
        for (i, &pos) in face_positions.iter().enumerate() {
            // Correct GoldSrc texture projection in world coordinates:
            let raw_u =
                ((pos[0] * s_vec[0] + pos[1] * s_vec[1] + pos[2] * s_vec[2]) + s_shift) / tex_w;
            let raw_v =
                ((pos[0] * t_vec[0] + pos[1] * t_vec[1] + pos[2] * t_vec[2]) + t_shift) / tex_h;

            // Correct GoldSrc lightmap UV calculation:
            let uv1_lightmap =
                if let Some(&(s_min, t_min, [atlas_x, atlas_y], [atlas_w, atlas_h])) = lm_info {
                    let s = pos[0] * s_vec[0] + pos[1] * s_vec[1] + pos[2] * s_vec[2] + s_shift;
                    let t = pos[0] * t_vec[0] + pos[1] * t_vec[1] + pos[2] * t_vec[2] + t_shift;
                    let lm_u = (atlas_x + ((s - s_min) / 16.0) + 0.5) / atlas_w;
                    let lm_v = (atlas_y + ((t - t_min) / 16.0) + 0.5) / atlas_h;
                    [lm_u, lm_v]
                } else {
                    [0.0, 0.0]
                };

            // Coordinate system transformation:
            // GoldSrc: X = Right, Y = Forward, Z = Up
            // Bevy / Modern: X = Right, Y = Up, Z = -Forward (or -Y)
            let bevy_pos = [pos[0], pos[2], -pos[1]];
            let bevy_normal = [normal[0], normal[2], -normal[1]];
            let bevy_tangent = [tangent[0], tangent[2], -tangent[1], 1.0];

            vertices.push(Vertex {
                position: bevy_pos,
                normal: bevy_normal,
                tangent: bevy_tangent,
                uv0: [raw_u, raw_v],
                uv1_lightmap,
                style_indices: face.styles,
                style_weights: [255, 0, 0, 0],
            });

            if i >= 2 {
                indices.push(base_vert_idx);
                indices.push(base_vert_idx + (i as u32) - 1);
                indices.push(base_vert_idx + (i as u32));
            }
        }

        let submesh_count = (indices.len() as u32) - submesh_start;
        submeshes.push(SubMesh {
            name: format!("face_{face_idx}"),
            index_start: submesh_start,
            index_count: submesh_count,
            material_id: tex_id,
        });
    }

    let mut mesh = MeshBuffer {
        vertices,
        indices,
        submeshes,
        aabb_min: [0.0; 3],
        aabb_max: [0.0; 3],
    };

    mesh.recompute_bounds();
    Ok((mesh, atlas))
}

/// Compiles raw BSP30 level lumps with atlas-mapped UV coordinates (Legacy Fallback).
pub fn compile_bsp_geometry_with_atlas(
    bsp: &BspLevel<'_>,
    atlas: Option<&crate::ir::TextureAtlas>,
) -> Result<MeshBuffer, FormatError> {
    let mut vertices: Vec<Vertex> = Vec::with_capacity(bsp.faces.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(bsp.faces.len() * 6);
    let mut submeshes: Vec<SubMesh> = Vec::with_capacity(bsp.faces.len());

    for (face_idx, face) in bsp.faces.iter().enumerate() {
        let (s_vec, s_shift, t_vec, t_shift, tex_id) = resolve_texture_projection(face, bsp);

        let face_positions = collect_face_positions(face, bsp);
        if face_positions.len() < 3 {
            continue;
        }
        let normal = compute_face_normal(&face_positions);
        let tangent = [s_vec[0], s_vec[1], s_vec[2], 1.0];

        let submesh_start = indices.len() as u32;
        let base_vert_idx = vertices.len() as u32;

        let region = atlas.and_then(|a| a.regions.get(tex_id as usize));
        let (tex_w, tex_h) = region.map_or((128.0, 128.0), |r| (r.width as f32, r.height as f32));

        // Triangulate convex polygon fan: (0, 1, 2), (0, 2, 3), (0, 3, 4), ...
        for (i, &pos) in face_positions.iter().enumerate() {
            let raw_u =
                ((pos[0] * s_vec[0] + pos[1] * s_vec[1] + pos[2] * s_vec[2]) + s_shift) / tex_w;
            let raw_v =
                ((pos[0] * t_vec[0] + pos[1] * t_vec[1] + pos[2] * t_vec[2]) + t_shift) / tex_h;

            let [uv_u, uv_v] = region.map_or([raw_u, raw_v], |r| {
                let u_fract = raw_u.rem_euclid(1.0);
                let v_fract = raw_v.rem_euclid(1.0);
                [
                    r.u_min + u_fract * (r.u_max - r.u_min),
                    r.v_min + v_fract * (r.v_max - r.v_min),
                ]
            });

            // Coordinate system transformation:
            // GoldSrc: X = Right, Y = Forward, Z = Up
            // Bevy / Modern: X = Right, Y = Up, Z = -Forward (or -Y)
            let bevy_pos = [pos[0], pos[2], -pos[1]];
            let bevy_normal = [normal[0], normal[2], -normal[1]];
            let bevy_tangent = [tangent[0], tangent[2], -tangent[1], 1.0];

            vertices.push(Vertex {
                position: bevy_pos,
                normal: bevy_normal,
                tangent: bevy_tangent,
                uv0: [uv_u, uv_v],
                uv1_lightmap: [0.0, 0.0],
                style_indices: face.styles,
                style_weights: [255, 0, 0, 0],
            });

            if i >= 2 {
                indices.push(base_vert_idx);
                indices.push(base_vert_idx + (i as u32) - 1);
                indices.push(base_vert_idx + (i as u32));
            }
        }

        let submesh_count = (indices.len() as u32) - submesh_start;
        submeshes.push(SubMesh {
            name: format!("face_{face_idx}"),
            index_start: submesh_start,
            index_count: submesh_count,
            material_id: tex_id,
        });
    }

    let mut mesh = MeshBuffer {
        vertices,
        indices,
        submeshes,
        aabb_min: [0.0; 3],
        aabb_max: [0.0; 3],
    };

    mesh.recompute_bounds();
    Ok(mesh)
}

fn collect_face_positions(face: &BspFace, bsp: &BspLevel<'_>) -> Vec<[f32; 3]> {
    let first_edge = face.first_surfedge_id as usize;
    let num_edges = face.surfedges_num as usize;

    if first_edge + num_edges > bsp.surfedges.len() {
        return Vec::new();
    }

    let mut face_positions = Vec::with_capacity(num_edges);
    for &surfedge in &bsp.surfedges[first_edge..first_edge + num_edges] {
        let (edge_idx, reverse) = if surfedge >= 0 {
            (surfedge as usize, false)
        } else {
            (surfedge.unsigned_abs() as usize, true)
        };

        if edge_idx >= bsp.edges.len() {
            continue;
        }

        let edge = &bsp.edges[edge_idx];
        let vert_idx = if reverse {
            edge.vertices[1] as usize
        } else {
            edge.vertices[0] as usize
        };

        if vert_idx < bsp.vertices.len() {
            let pos = bsp.vertices[vert_idx];
            if pos[0].is_finite() && pos[1].is_finite() && pos[2].is_finite() {
                face_positions.push(pos);
            }
        }
    }
    face_positions
}

fn resolve_texture_projection(
    face: &BspFace,
    bsp: &BspLevel<'_>,
) -> ([f32; 3], f32, [f32; 3], f32, u32) {
    let tex_info_idx = face.texture_info_id as usize;
    if tex_info_idx < bsp.texture_infos.len() {
        let ti = &bsp.texture_infos[tex_info_idx];
        let s_finite = ti.s[0].is_finite()
            && ti.s[1].is_finite()
            && ti.s[2].is_finite()
            && ti.s_shift.is_finite();
        let t_finite = ti.t[0].is_finite()
            && ti.t[1].is_finite()
            && ti.t[2].is_finite()
            && ti.t_shift.is_finite();

        if s_finite && t_finite {
            (ti.s, ti.s_shift, ti.t, ti.t_shift, ti.texture_id)
        } else {
            ([1.0, 0.0, 0.0], 0.0, [0.0, 1.0, 0.0], 0.0, ti.texture_id)
        }
    } else {
        ([1.0, 0.0, 0.0], 0.0, [0.0, 1.0, 0.0], 0.0, 0)
    }
}

fn compute_face_normal(face_positions: &[[f32; 3]]) -> [f32; 3] {
    let p0 = face_positions[0];
    let p1 = face_positions[1];
    let p2 = face_positions[2];

    let v01 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let v02 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

    let mut normal = [
        v01[1] * v02[2] - v01[2] * v02[1],
        v01[2] * v02[0] - v01[0] * v02[2],
        v01[0] * v02[1] - v01[1] * v02[0],
    ];

    let len_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    if len_sq > 1e-6 {
        let inv_len = 1.0 / len_sq.sqrt();
        normal[0] *= inv_len;
        normal[1] *= inv_len;
        normal[2] *= inv_len;
    } else {
        normal = [0.0, 1.0, 0.0];
    }
    normal
}
