use std::collections::VecDeque;

use bevy::{
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};

use crate::world::{mesh, storage::voxel::VoxelVertex};

use super::{ChunkEntityMap, EvtChunkMeshDirty, Pipeline, WorldRes};

pub struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(Pipeline::Rendering, mesh_generation_system);
    }
}

#[derive(Default)]
struct MeshGenerationMeta {
    pending_chunks: VecDeque<IVec3>,
}

fn mesh_generation_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    vox_world: Res<WorldRes>,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkMeshDirty>,
    mut meta: Local<MeshGenerationMeta>,
) {
    let mut _perf = perf_fn!();

    meta.pending_chunks.extend(reader.iter().map(|evt| evt.0));

    if !vox_world.is_ready() {
        return;
    }

    let limit = usize::min(meta.pending_chunks.len(), 1);

    let chunks = meta
        .pending_chunks
        .drain(..limit)
        .filter_map(|evt| vox_world.get(evt).map(|c| (evt, &c.vertices)))
        .collect::<Vec<_>>();

    for (local, vertices) in chunks {
        if let Some(&e) = entity_map.0.get(&local) {
            let mesh_handle = {
                debug_assert!(!vertices.is_empty());
                meshes.add(generate_mesh(vertices))
            };

            commands.entity(e).insert(mesh_handle);
        } else {
            warn!(
                "Skipping mesh generation since chunk {} wasn't found on entity map",
                local
            );
        }
    }
}

fn generate_mesh(vertices: &Vec<VoxelVertex>) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

    let mut positions: Vec<[f32; 3]> = vec![];
    let mut normals: Vec<[f32; 3]> = vec![];

    let vertex_count = vertices.len();

    for vertex in vertices {
        positions.push(vertex.position.into());
        normals.push(vertex.normal.into());
    }

    mesh.set_indices(Some(Indices::U32(mesh::compute_indices(vertex_count))));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![0; vertex_count]);
    mesh
}
