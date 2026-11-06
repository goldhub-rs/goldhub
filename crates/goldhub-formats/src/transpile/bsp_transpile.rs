//! High-performance polygon triangulation and geometry compiler for BSP30 levels.

use crate::codecs::legacy::bsp::{BspFace, BspLevel};
use crate::error::FormatError;
use crate::ir::{MeshBuffer, SubMesh, Vertex};

/// Compiles raw BSP30 level lumps into a contiguous, cache-line aligned Canonical `MeshBuffer`.
pub fn compile_bsp_geometry(bsp: &BspLevel<'_>) -> Result<MeshBuffer, FormatError> {
    compile_bsp_geometry_with_atlas(bsp, None)
}

/// Compiles raw BSP30 level lumps with atlas-mapped UV coordinates.
pub fn compile_bsp_geometry_with_atlas(
    bsp: &BspLevel<'_>,
    atlas: Option<&crate::ir::TextureAtlas>,
) -> Result<MeshBuffer, FormatError> {
    let mut vertices: Vec<Vertex> = Vec::with_capacity(bsp.faces.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(bsp.faces.len() * 6);
    let mut submeshes: Vec<SubMesh> = Vec::with_capacity(bsp.faces.len());

    for (face_idx, face) in bsp.faces.iter().enumerate() {
        let face_positions = collect_face_positions(face, bsp);
        if face_positions.len() < 3 {
            continue;
        }

        let (s_vec, s_shift, t_vec, t_shift, tex_id) = resolve_texture_projection(face, bsp);
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

            vertices.push(Vertex {
                position: pos,
                normal,
                tangent,
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
