use bevy::{
    prelude::*,
    render::{mesh::Indices, pipeline::PrimitiveTopology},
};

use crate::world::mesh;

use super::{ChunkEntityMap, EvtChunkMeshDirty, Pipeline};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(Pipeline::Rendering, mesh_generation_system);
    }
}

fn mesh_generation_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkMeshDirty>,
) {
    let mut _perf = perf_fn!();

    for EvtChunkMeshDirty(local, vertices) in reader.iter() {
        perf_scope!(_perf);

        if let Some(&entity) = entity_map.0.get(local) {
            if vertices.is_empty() {
                commands.entity(entity).insert(Handle::<Mesh>::default());
            } else {
                let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

                let mut positions: Vec<[f32; 3]> = vec![];
                let mut normals: Vec<[f32; 3]> = vec![];

                let vertex_count = vertices.len();

                for vertex in vertices {
                    positions.push(vertex.position.into());
                    normals.push(vertex.normal.into());
                }

                mesh.set_indices(Some(Indices::U32(mesh::compute_indices(vertex_count))));
                mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

                commands.entity(entity).insert(meshes.add(mesh));
            }
        }
    }
}
