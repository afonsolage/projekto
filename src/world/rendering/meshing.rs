use bevy::{
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};

use crate::world::{
    mesh,
    storage::{landscape, voxel::VoxelVertex},
};

use super::{ChunkEntityMap, ChunkMeshDirty, Pipeline, WorldRes};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(Pipeline::Rendering, mesh_generation_system);
    }
}

const MESH_BATCH_SIZE: usize = landscape::HORIZONTAL_RADIUS;

fn mesh_generation_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    vox_world: Res<WorldRes>,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<ChunkMeshDirty>,
) {
    let mut _perf = perf_fn!();

    if !vox_world.is_ready() {
        return;
    }

    let chunks = reader
        .iter()
        // .take(MESH_BATCH_SIZE)
        .filter_map(|evt| vox_world.get(evt.0).map(|c| (evt.0, &c.vertices)))
        .collect::<Vec<_>>();

    for (local, vertices) in chunks {
        if let Some(&e) = entity_map.0.get(&local) {
            let mesh_handle = {
                if vertices.is_empty() {
                    Handle::default()
                } else {
                    meshes.add(generate_mesh(vertices))
                }
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
