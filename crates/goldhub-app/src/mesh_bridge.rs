//! Conversion bridge between Canonical Data IR `MeshBuffer` and Bevy GPU `Mesh`.

use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use goldhub_formats::ir::MeshBuffer;

/// Converts a Canonical Data IR `MeshBuffer` into a GPU-ready Bevy `Mesh`.
pub fn to_bevy_mesh(mesh_buffer: &MeshBuffer) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    let positions: Vec<[f32; 3]> = mesh_buffer.vertices.iter().map(|v| v.position).collect();
    let normals: Vec<[f32; 3]> = mesh_buffer.vertices.iter().map(|v| v.normal).collect();
    let tangents: Vec<[f32; 4]> = mesh_buffer.vertices.iter().map(|v| v.tangent).collect();
    let uvs: Vec<[f32; 2]> = mesh_buffer.vertices.iter().map(|v| v.uv0).collect();
    let uv1_lightmaps: Vec<[f32; 2]> = mesh_buffer
        .vertices
        .iter()
        .map(|v| v.uv1_lightmap)
        .collect();

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1_lightmaps);
    mesh.insert_indices(Indices::U32(mesh_buffer.indices.clone()));

    mesh
}

/// Partitioned submesh ready for multi-material rendering.
#[derive(Debug)]
pub struct PartitionedMesh {
    pub material_id: u32,
    pub render_mode: u32,
    pub render_amt: u8,
    pub render_color: [u8; 3],
    pub mesh: Mesh,
}

/// Converts a Canonical Data IR `MeshBuffer` into partitioned Bevy `Mesh`es grouped by `(material_id, render_mode, render_amt, render_color)`.
pub fn to_bevy_submeshes(mesh_buffer: &MeshBuffer) -> Vec<PartitionedMesh> {
    use std::collections::HashMap;

    // Group submesh index ranges by (material_id, render_mode, render_amt, render_color)
    let mut material_groups: HashMap<(u32, u32, u8, [u8; 3]), Vec<u32>> = HashMap::new();

    for submesh in &mesh_buffer.submeshes {
        let entry = material_groups
            .entry((
                submesh.material_id,
                submesh.render_mode,
                submesh.render_amt,
                submesh.render_color,
            ))
            .or_default();
        let start = submesh.index_start as usize;
        let end = start + submesh.index_count as usize;
        if end <= mesh_buffer.indices.len() {
            entry.extend_from_slice(&mesh_buffer.indices[start..end]);
        }
    }

    let mut result = Vec::with_capacity(material_groups.len());

    for ((mat_id, r_mode, r_amt, r_color), sub_indices) in material_groups {
        if sub_indices.is_empty() {
            continue;
        }

        // Remap only the vertices referenced by this submesh
        let mut vertex_remap: HashMap<u32, u32> = HashMap::new();
        let mut sub_positions = Vec::new();
        let mut sub_normals = Vec::new();
        let mut sub_tangents = Vec::new();
        let mut sub_uvs = Vec::new();
        let mut sub_uv1_lightmaps = Vec::new();
        let mut new_indices = Vec::with_capacity(sub_indices.len());

        for &orig_idx in &sub_indices {
            let remapped = *vertex_remap.entry(orig_idx).or_insert_with(|| {
                let new_idx = sub_positions.len() as u32;
                if let Some(v) = mesh_buffer.vertices.get(orig_idx as usize) {
                    sub_positions.push(v.position);
                    sub_normals.push(v.normal);
                    sub_tangents.push(v.tangent);
                    sub_uvs.push(v.uv0);
                    sub_uv1_lightmaps.push(v.uv1_lightmap);
                }
                new_idx
            });
            new_indices.push(remapped);
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, sub_positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, sub_normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, sub_tangents);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, sub_uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, sub_uv1_lightmaps);
        mesh.insert_indices(Indices::U32(new_indices));

        result.push(PartitionedMesh {
            material_id: mat_id,
            render_mode: r_mode,
            render_amt: r_amt,
            render_color: r_color,
            mesh,
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use goldhub_formats::ir::{SubMesh, Vertex};

    #[test]
    fn test_to_bevy_mesh_conversion() {
        let dummy_mesh = MeshBuffer {
            vertices: vec![
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    uv0: [0.0, 0.0],
                    uv1_lightmap: [0.0, 0.0],
                    style_indices: [0; 4],
                    style_weights: [255, 0, 0, 0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    uv0: [1.0, 0.0],
                    uv1_lightmap: [0.0, 0.0],
                    style_indices: [0; 4],
                    style_weights: [255, 0, 0, 0],
                },
                Vertex {
                    position: [0.0, 0.0, 1.0],
                    normal: [0.0, 1.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    uv0: [0.0, 1.0],
                    uv1_lightmap: [0.0, 0.0],
                    style_indices: [0; 4],
                    style_weights: [255, 0, 0, 0],
                },
            ],
            indices: vec![0, 1, 2],
            submeshes: vec![SubMesh {
                name: "test_submesh".into(),
                index_start: 0,
                index_count: 3,
                material_id: 1,
                render_mode: 0,
                render_amt: 255,
                render_color: [0, 0, 0],
            }],
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 0.0, 1.0],
        };

        let bevy_mesh = to_bevy_mesh(&dummy_mesh);
        assert_eq!(bevy_mesh.count_vertices(), 3);
        assert_eq!(bevy_mesh.indices().unwrap().len(), 3);
    }
}
