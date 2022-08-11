use std::collections::VecDeque;

use bevy::{
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};
use projekto_core::voxel::VoxelVertex;

use crate::world::terraformation::prelude::shaping;

use super::{ChunkEntityMap, ChunkMaterial, EvtChunkMeshDirty, WorldRes};

pub(super) struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(mesh_generation_system);
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
            debug_assert!(!vertices.is_empty());

            let mesh = generate_mesh(vertices);
            commands.entity(e).insert(meshes.add(mesh));
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

    mesh.set_indices(Some(Indices::U32(shaping::compute_indices(vertex_count))));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_TILE_COORD_START, tile_coord_start);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_LIGHT, lights);
    mesh
}
