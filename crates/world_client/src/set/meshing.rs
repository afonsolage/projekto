use bevy::{
    prelude::*,
    render::{mesh::Indices, render_asset::RenderAssetUsages, render_resource::PrimitiveTopology},
};
use projekto_core::{chunk, voxel};
use projekto_server::{
    bundle::ChunkLocal,
    proto::{server, RegisterMessageHandler},
};

use crate::{material::ChunkMaterial, ChunkBundle, ChunkMap, ChunkMaterialHandle, PlayerLandscape};

pub(crate) struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.set_message_handler(update_chunk_mesh);
    }
}

fn update_chunk_mesh(
    In(vertex): In<server::ChunkVertex>,
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<ChunkMaterialHandle>,
) {
    let server::ChunkVertex { chunk, vertex } = vertex;

    let mesh_handler = meshes.add(generate_mesh(&vertex));

    if let Some(&entity) = map.get(&chunk) {
        commands.entity(entity).insert(mesh_handler);
    } else {
        let entity = commands
            .spawn(ChunkBundle {
                chunk: ChunkLocal(chunk),
                mesh: MaterialMeshBundle {
                    mesh: mesh_handler,
                    transform: Transform::from_translation(chunk::to_world(chunk)),
                    material: material.0.clone(),
                    ..Default::default()
                },
            })
            .insert(Name::new(format!("Client Chunk {}", chunk)))
            .id();
        map.insert(chunk, entity);
    }

    trace!("[update_chunk_mesh] chunk {chunk:?} mesh updated");
}

fn generate_mesh(vertices: &[voxel::Vertex]) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    let mut positions: Vec<[f32; 3]> = vec![];
    let mut normals: Vec<[f32; 3]> = vec![];
    let mut uvs: Vec<[f32; 2]> = vec![];
    let mut tile_coord_start: Vec<[f32; 2]> = vec![];
    let mut lights: Vec<[f32; 3]> = vec![];

    let vertex_count = vertices.len();

    for vertex in vertices {
        positions.push(vertex.position.into());
        normals.push(vertex.normal.into());
        uvs.push(vertex.uv.into());
        tile_coord_start.push(vertex.tile_coord_start.into());
        lights.push(vertex.light.into());
    }

    mesh.insert_indices(Indices::U32(compute_indices(vertex_count)));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_TILE_COORD_START, tile_coord_start);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_LIGHT, lights);
    mesh
}

/// Computes indices of a triangle list mesh.
///
/// This function assumes 4 vertices per face, 3 indices per triangles and all vertices are placed
/// in CCW order.
///
/// It generates indices in the following order: _*0 1 2 2 3 0*_ where 0 is the first vertice and 3
/// is the last one
///
/// Returns** a list of indices in the CCW order
pub fn compute_indices(vertex_count: usize) -> Vec<u32> {
    // Each 4 vertex is a voxel face and each voxel face has 6 indices, so we can multiply the
    // vertex count by 1.5
    let index_count = (vertex_count as f32 * 1.5) as usize;

    let mut res = vec![0; index_count];
    let mut i = 0u32;

    while i < vertex_count as u32 {
        res.push(i);
        res.push(i + 1);
        res.push(i + 2);

        res.push(i + 2);
        res.push(i + 3);
        res.push(i);

        i += 4;
    }

    res
}
