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

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(mesh_buffer.indices.clone()));

    mesh
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
            }],
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 0.0, 1.0],
        };

        let bevy_mesh = to_bevy_mesh(&dummy_mesh);
        assert_eq!(bevy_mesh.count_vertices(), 3);
        assert_eq!(bevy_mesh.indices().unwrap().len(), 3);
    }
}
