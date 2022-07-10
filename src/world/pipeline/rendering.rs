use bevy::{
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
    tasks::{AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use futures_lite::future;

use crate::world::{
    mesh,
    storage::{
        chunk::{self, ChunkKind},
        landscape,
        voxel::{self, FacesOcclusion, VoxelFace, VoxelVertex},
    },
};

use super::{genesis::WorldRes, ChunkEntityMap, ChunkFacesOcclusion, EvtChunkMeshDirty, Pipeline};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(Pipeline::Rendering, mesh_generation_system);
    }
}

fn faces_occlusion(chunk: &ChunkKind) -> ChunkFacesOcclusion {
    perf_fn_scope!();

    let mut occlusion = ChunkFacesOcclusion::default();
    for voxel in chunk::voxels() {
        let mut voxel_faces = FacesOcclusion::default();

        if chunk.get(voxel).is_empty() {
            voxel_faces.set_all(true);
        } else {
            for side in voxel::SIDES {
                let dir = side.dir();
                let neighbor_pos = voxel + dir;

                let neighbor_kind = if !chunk::is_within_bounds(neighbor_pos) {
                    let (_, next_chunk_voxel) = chunk::overlap_voxel(neighbor_pos);

                    match chunk.neighborhood.get(side, next_chunk_voxel) {
                        Some(k) => k,
                        None => continue,
                    }
                } else {
                    chunk.get(neighbor_pos)
                };

                voxel_faces.set(side, !neighbor_kind.is_empty());
            }
        }

        occlusion.set(voxel, voxel_faces);
    }

    occlusion
}

fn faces_merging(chunk: &ChunkKind, occlusion: &ChunkFacesOcclusion) -> Vec<VoxelFace> {
    perf_fn_scope!();

    mesh::merge_faces(occlusion, chunk)
}

fn vertices_computation(faces: Vec<VoxelFace>) -> Vec<VoxelVertex> {
    perf_fn_scope!();

    let mut vertices = vec![];

    for face in faces {
        let normal = face.side.normal();

        for (i, v) in face.vertices.iter().enumerate() {
            let base_vertex_idx = mesh::VERTICES_INDICES[face.side as usize][i];
            let base_vertex: Vec3 = mesh::VERTICES[base_vertex_idx].into();
            vertices.push(VoxelVertex {
                position: base_vertex + v.as_vec3(),
                normal,
            })
        }
    }

    vertices
}

type BatchResult = Vec<(IVec3, Vec<VoxelVertex>)>;

#[derive(Default)]
struct MeshGenerationMeta {
    tasks: HashMap<usize, Task<BatchResult>>,
    batch_id: usize,
}

const MESH_BATCH_SIZE: usize = landscape::HORIZONTAL_SIZE * landscape::HORIZONTAL_SIZE;

fn mesh_generation_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    vox_world: Res<WorldRes>,
    entity_map: Res<ChunkEntityMap>,
    task_pool: Res<AsyncComputeTaskPool>,
    mut meta: Local<MeshGenerationMeta>,
    mut reader: EventReader<EvtChunkMeshDirty>,
) {
    let mut _perf = perf_fn!();

    let chunks = reader
        .iter()
        .filter_map(|evt| vox_world.get(evt.0).map(|c| (evt.0, c)))
        .collect::<Vec<_>>();

    for batch in chunks.chunks(MESH_BATCH_SIZE) {
        let id = meta.batch_id;
        meta.batch_id += 1;

        let owned_batch = batch
            .iter()
            .map(|(local, c)| (*local, (*c).clone()))
            .collect();

        let task = task_pool.spawn(async move { generate_vertices(owned_batch) });

        meta.tasks.insert(id, task);
    }

    let completed_tasks = meta
        .tasks
        .iter_mut()
        .filter_map(|(local, task)| {
            future::block_on(future::poll_once(task)).map(|vertices| (*local, vertices))
        })
        .collect::<Vec<_>>();

    for (batch_id, batch_result) in completed_tasks {
        for (local, vertices) in batch_result {
            if let Some(&e) = entity_map.0.get(&local) {
                generate_mesh(vertices, &mut commands, e, &mut meshes)
            } else {
                warn!(
                    "Skipping mesh generation since chunk {} wasn't found on entity map",
                    local
                );
            }
        }

        meta.tasks.remove(&batch_id);
    }
}

fn generate_vertices(chunks: Vec<(IVec3, ChunkKind)>) -> BatchResult {
    let mut _perf = perf_fn!();

    let mut result = vec![];

    for (local, chunk) in chunks {
        perf_scope!(_perf);

        let occlusion = faces_occlusion(&chunk);
        let vertices = if !occlusion.iter().all(|oc| oc.is_fully_occluded()) {
            let faces = faces_merging(&chunk, &occlusion);
            vertices_computation(faces)
        } else {
            vec![]
        };

        result.push((local, vertices));
    }

    result
}

fn generate_mesh(
    vertices: Vec<VoxelVertex>,
    commands: &mut Commands,
    entity: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
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
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![0; vertex_count]);


        commands.entity(entity).insert(meshes.add(mesh));
    }
}

#[cfg(test)]
mod test {
    use crate::world::storage::VoxWorld;

    use super::*;

    #[test]
    fn faces_occlusion_occlude_empty_chunk() {
        // Arrange
        let chunk = ChunkKind::default();

        // Act
        let occlusions = super::faces_occlusion(&chunk);

        // Assert
        assert!(
            occlusions.iter().all(|a| a.is_fully_occluded()),
            "A chunk full of empty-kind voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion() {
        // Arrange
        let mut chunk = ChunkKind::default();

        // Top-Bottom occlusion
        chunk.set((1, 1, 1).into(), 1.into());
        chunk.set((1, 2, 1).into(), 1.into());

        // Full occluded voxel at (10, 10, 10)
        chunk.set((10, 10, 10).into(), 1.into());
        chunk.set((9, 10, 10).into(), 1.into());
        chunk.set((11, 10, 10).into(), 1.into());
        chunk.set((10, 9, 10).into(), 1.into());
        chunk.set((10, 11, 10).into(), 1.into());
        chunk.set((10, 10, 9).into(), 1.into());
        chunk.set((10, 10, 11).into(), 1.into());

        // Act
        let faces_occlusion = super::faces_occlusion(&chunk);

        // Assert
        let faces = faces_occlusion.get((1, 2, 1).into());

        assert_eq!(
            faces,
            [false, false, false, true, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((1, 1, 1).into());

        assert_eq!(
            faces,
            [false, false, true, false, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((10, 10, 10).into());

        assert_eq!(
            faces,
            [true; voxel::SIDE_COUNT].into(),
            "Voxel fully surrounded by another non-empty voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion_neighborhood() {
        let mut world = VoxWorld::default();

        let mut top = ChunkKind::default();
        top.set_all(2.into());

        let mut down = ChunkKind::default();
        down.set_all(3.into());

        let mut center = ChunkKind::default();
        center.set((0, chunk::Y_END as i32, 0).into(), 1.into());
        center.set((1, 0, 1).into(), 1.into());

        world.add((0, 1, 0).into(), top);
        world.add((0, 0, 0).into(), center);
        world.add((0, -1, 0).into(), down);

        world.update_neighborhood((0, 0, 0).into());
        let center = world.get((0, 0, 0).into()).unwrap();

        let faces_occlusion = super::faces_occlusion(center);

        let faces = faces_occlusion.get((0, chunk::Y_END as i32, 0).into());
        assert_eq!(faces, [false, false, true, false, false, false].into());

        let faces = faces_occlusion.get((1, 0, 1).into());
        assert_eq!(faces, [false, false, false, true, false, false].into());
    }

    #[test]
    fn vertices_computation() {
        // Arrange
        let side = voxel::Side::Up;
        let faces = vec![VoxelFace {
            side,
            vertices: [
                (0, 0, 0).into(),
                (0, 0, 1).into(),
                (1, 0, 1).into(),
                (1, 0, 0).into(),
            ],
        }];

        // Act
        let vertices = super::vertices_computation(faces);

        // Assert
        let normal = side.normal();
        assert_eq!(
            vertices,
            vec![
                VoxelVertex {
                    normal,
                    position: (0.0, 1.0, 0.0).into(),
                },
                VoxelVertex {
                    normal,
                    position: (0.0, 1.0, 2.0).into(),
                },
                VoxelVertex {
                    normal,
                    position: (2.0, 1.0, 2.0).into(),
                },
                VoxelVertex {
                    normal,
                    position: (2.0, 1.0, 0.0).into(),
                },
            ]
        );
    }
}
