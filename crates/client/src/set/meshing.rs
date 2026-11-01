use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use projekto_core::voxel;
use projekto_messages::{ChunkUnload, ChunkVertex};
use projekto_proto::RegisterMessageHandler;

use crate::{
    ChunkBundle, ChunkMap, ChunkMaterialHandle, bundle::ChunkLocal, material::ChunkMaterial,
    net::ServerDisconnected,
};

pub(crate) struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.set_message_handler(update_chunk_mesh)
            .add_message_handler(remove_chunk_mesh)
            .add_systems(
                Update,
                despawn_chunks_on_server_disconnect.run_if(on_message::<ServerDisconnected>),
            );
    }
}

fn remove_chunk_mesh(
    In(ChunkUnload { chunk }): In<ChunkUnload>,
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
) {
    if let Some(entity) = map.remove(&chunk) {
        commands.entity(entity).despawn();
        trace!("[remove_chunk_mesh] chunk {chunk:?} despawned");
    }
}

fn despawn_chunks_on_server_disconnect(
    mut map: ResMut<ChunkMap>,
    mut reader: MessageReader<ServerDisconnected>,
    mut commands: Commands,
) {
    reader.clear();
    for (_, entity) in map.drain() {
        commands.entity(entity).despawn();
    }
}

fn update_chunk_mesh(
    In(vertex): In<ChunkVertex>,
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<ChunkMaterialHandle>,
) {
    let ChunkVertex { chunk, vertex } = vertex;

    let mesh_handler = Mesh3d(meshes.add(generate_mesh(&vertex)));

    if let Some(&entity) = map.get(&chunk) {
        commands.entity(entity).insert(mesh_handler);
    } else {
        let entity = commands
            .spawn(ChunkBundle {
                chunk: ChunkLocal(chunk),
                mesh: mesh_handler,
                material: MeshMaterial3d(material.0.clone()),
                transform: Transform::from_translation(chunk.into()),
            })
            .insert(Name::new(format!("Client Chunk {chunk}")))
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
    // Each 4 vertices form a voxel face, which needs 6 indices.
    let mut res = Vec::with_capacity((vertex_count / 4) * 6);
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

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use projekto_core::coords::Chunk;
    use projekto_proto::{ClientId, RunMessageHandlers};

    use super::*;

    #[test]
    fn compute_indices_does_not_pad() {
        assert_eq!(compute_indices(8), vec![0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4]);
    }

    #[test]
    fn chunk_unload_despawns_entity_and_removes_map_entry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>();
        app.add_message_handler(remove_chunk_mesh);

        let chunk = Chunk::new(1, -2);
        let entity = app.world_mut().spawn(ChunkLocal(chunk)).id();
        app.world_mut()
            .resource_mut::<ChunkMap>()
            .insert(chunk, entity);

        app.world_mut()
            .run_handlers::<ChunkUnload>(ClientId::default(), Box::new(ChunkUnload { chunk }));
        app.update();

        assert!(!app.world().resource::<ChunkMap>().contains_key(&chunk));
        assert!(app.world().get_entity(entity).is_err());
    }
}
