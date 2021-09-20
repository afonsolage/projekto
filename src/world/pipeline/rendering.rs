use bevy::{
    prelude::*,
    render::{mesh::Indices, pipeline::PrimitiveTopology},
    tasks::{AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use futures_lite::future;

use crate::world::{
    mesh,
    storage::{
        chunk::{self, ChunkKind},
        voxel::{self, VoxelFace, VoxelVertex},
        VoxWorld,
    },
};

use super::{ChunkEntityMap, ChunkFacesOcclusion, EvtChunkMeshDirty, Pipeline};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(Pipeline::Rendering, mesh_generation_system);
    }
}

fn faces_occlusion(chunk: &ChunkKind) -> ChunkFacesOcclusion {
    let mut occlusion = ChunkFacesOcclusion::default();
    for voxel in chunk::voxels() {
        let mut voxel_faces = occlusion.get(voxel);

        if chunk.get(voxel).is_empty() {
            voxel_faces.set_all(true);
        } else {
            for side in voxel::SIDES {
                let dir = side.get_side_dir();
                let neighbor_pos = voxel + dir;

                let neighbor_kind = if !chunk::is_within_bounds(neighbor_pos) {
                    // let (next_chunk_dir, next_chunk_voxel) = chunk::overlap_voxel(neighbor_pos);

                    // if let Some(neighbor_chunk) = world.get(*local + next_chunk_dir) {
                    //     neighbor_chunk.get(next_chunk_voxel)
                    // } else {
                    continue;
                    // }
                } else {
                    chunk.get(neighbor_pos)
                };

                if !neighbor_kind.is_empty() {
                    voxel_faces[side as usize] = true;
                }
            }
        }

        occlusion.set(voxel, voxel_faces);
    }

    occlusion
}

fn faces_merging(chunk: &ChunkKind, occlusion: &ChunkFacesOcclusion) -> Vec<VoxelFace> {
    mesh::merge_faces(occlusion, chunk)
}

fn vertices_computation(faces: Vec<VoxelFace>) -> Vec<VoxelVertex> {
    let mut vertices = vec![];

    for face in faces {
        let normal = face.side.get_side_normal();

        for (i, v) in face.vertices.iter().enumerate() {
            let base_vertex_idx = mesh::VERTICES_INDICES[face.side as usize][i];
            let base_vertex: Vec3 = mesh::VERTICES[base_vertex_idx].into();
            vertices.push(VoxelVertex {
                position: base_vertex + v.as_f32(),
                normal,
            })
        }
    }

    vertices
}

#[derive(Default)]
struct MeshGenerationMeta {
    tasks: HashMap<IVec3, Task<Vec<VoxelVertex>>>,
}

fn mesh_generation_system(
    mut commands: Commands,
    mut reader: EventReader<EvtChunkMeshDirty>,
    mut meshes: ResMut<Assets<Mesh>>,
    vox_world: Res<VoxWorld>,
    entity_map: Res<ChunkEntityMap>,
    task_pool: Res<AsyncComputeTaskPool>,
    mut meta: Local<MeshGenerationMeta>,
) {
    let mut _perf = perf_fn!();

    for EvtChunkMeshDirty(local) in reader.iter() {
        trace_system_run!(local);
        perf_scope!(_perf);

        let chunk = match vox_world.get(*local) {
            None => {
                warn!(
                    "Skipping faces occlusion since chunk {} wasn't found on world",
                    *local
                );
                continue;
            }
            Some(&c) => c,
        };

        let task = task_pool.spawn(async move {
            let occlusion = faces_occlusion(&chunk);
            let faces = faces_merging(&chunk, &occlusion);
            let vertices = vertices_computation(faces);
            vertices
        });
        meta.tasks.insert(*local, task);
    }

    let completed_tasks = meta
        .tasks
        .iter_mut()
        .filter_map(|(&local, task)| {
            future::block_on(future::poll_once(task)).map(|v| {
                match entity_map.0.get(&local) {
                    None => {
                        warn!(
                            "Skipping mesh generation since chunk {} wasn't found on entity map",
                            local
                        );
                    }
                    Some(&e) => generate_mesh(v, &mut commands, e, &mut meshes),
                };
                local
            })
        })
        .collect::<Vec<_>>();

    completed_tasks.iter().for_each(|v| {
        meta.tasks
            .remove(v)
            .expect("Task for load cache must exists");
    });
}

fn generate_mesh(
    vertices: Vec<VoxelVertex>,
    commands: &mut Commands,
    entity: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
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

#[cfg(test)]
mod test {
    // use bevy::{app::Events, utils::HashMap};

    // use super::*;

    // #[test]
    // fn faces_occlusion_system_occlude_empty_voxel() {
    //     // Arrange
    //     let local = (3, 2, 1).into();

    //     let mut events = Events::<EvtChunkDirty>::default();
    //     events.send(EvtChunkDirty(local));

    //     let mut voxel_world = storage::VoxWorld::default();
    //     voxel_world.add(local, ChunkKind::default());

    //     let mut world = World::default();
    //     world.insert_resource(voxel_world);
    //     world.insert_resource(events);

    //     let mut entity_map = ChunkEntityMap(HashMap::default());

    //     entity_map.0.insert(
    //         local,
    //         world
    //             .spawn()
    //             .insert_bundle(ChunkBuildingBundle::default())
    //             .id(),
    //     );

    //     world.insert_resource(entity_map);

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::faces_occlusion_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    //     let faces_occlusion = world
    //         .query::<&ChunkFacesOcclusion>()
    //         .iter(&world)
    //         .next()
    //         .unwrap();

    //     assert!(
    //         faces_occlusion.iter().all(|a| a.is_fully_occluded()),
    //         "A chunk full of empty-kind voxels should be fully occluded"
    //     );
    // }

    // #[test]
    // fn faces_occlusion_system() {
    //     // Arrange
    //     let local = (3, 2, 1).into();

    //     let mut events = Events::<EvtChunkDirty>::default();
    //     events.send(EvtChunkDirty(local));

    //     let mut voxel_world = storage::VoxWorld::default();
    //     voxel_world.add(local, ChunkKind::default());

    //     let chunk = voxel_world.get_mut(local).unwrap();
    //     // Top-Bottom occlusion
    //     chunk.set((1, 1, 1).into(), 1.into());
    //     chunk.set((1, 2, 1).into(), 1.into());

    //     // Full occluded voxel at (10, 10, 10)
    //     chunk.set((10, 10, 10).into(), 1.into());
    //     chunk.set((9, 10, 10).into(), 1.into());
    //     chunk.set((11, 10, 10).into(), 1.into());
    //     chunk.set((10, 9, 10).into(), 1.into());
    //     chunk.set((10, 11, 10).into(), 1.into());
    //     chunk.set((10, 10, 9).into(), 1.into());
    //     chunk.set((10, 10, 11).into(), 1.into());

    //     let mut world = World::default();
    //     world.insert_resource(voxel_world);
    //     world.insert_resource(events);

    //     let mut entity_map = ChunkEntityMap(HashMap::default());

    //     entity_map.0.insert(
    //         local,
    //         world
    //             .spawn()
    //             .insert_bundle(ChunkBuildingBundle::default())
    //             .id(),
    //     );

    //     world.insert_resource(entity_map);

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::faces_occlusion_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    //     let faces_occlusion = world
    //         .query::<&ChunkFacesOcclusion>()
    //         .iter(&world)
    //         .next()
    //         .unwrap();

    //     let faces = faces_occlusion.get((1, 2, 1).into());

    //     assert_eq!(
    //         faces,
    //         [false, false, false, true, false, false].into(),
    //         "Only down face should be occluded by the bottom voxel"
    //     );

    //     let faces = faces_occlusion.get((1, 1, 1).into());

    //     assert_eq!(
    //         faces,
    //         [false, false, true, false, false, false].into(),
    //         "Only down face should be occluded by the bottom voxel"
    //     );

    //     let faces = faces_occlusion.get((10, 10, 10).into());

    //     assert_eq!(
    //         faces,
    //         [true; voxel::SIDE_COUNT].into(),
    //         "Voxel fully surrounded by another non-empty voxels should be fully occluded"
    //     );
    // }

    // #[test]
    // fn vertices_computation_system() {
    //     // Arrange
    //     let local = (1, 2, 3).into();

    //     let mut events = Events::<EvtChunkDirty>::default();
    //     events.send(EvtChunkDirty(local));

    //     let mut world = World::default();
    //     world.insert_resource(events);

    //     let mut entity_map = ChunkEntityMap(HashMap::default());

    //     let side = voxel::Side::Up;
    //     let faces = ChunkFaces(vec![VoxelFace {
    //         side,
    //         vertices: [
    //             (0, 0, 0).into(),
    //             (0, 0, 1).into(),
    //             (1, 0, 1).into(),
    //             (1, 0, 0).into(),
    //         ],
    //     }]);

    //     let entity = world
    //         .spawn()
    //         .insert_bundle(ChunkBuildingBundle {
    //             faces,
    //             ..Default::default()
    //         })
    //         .id();

    //     entity_map.0.insert(local, entity);

    //     world.insert_resource(entity_map);

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::vertices_computation_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    //     let vertices = world.query::<&ChunkVertices>().iter(&world).next().unwrap();

    //     let normal = side.get_side_normal();
    //     assert_eq!(
    //         vertices.0,
    //         vec![
    //             VoxelVertex {
    //                 normal: normal,
    //                 position: (0.0, 1.0, 0.0).into(),
    //             },
    //             VoxelVertex {
    //                 normal: normal,
    //                 position: (0.0, 1.0, 2.0).into(),
    //             },
    //             VoxelVertex {
    //                 normal: normal,
    //                 position: (2.0, 1.0, 2.0).into(),
    //             },
    //             VoxelVertex {
    //                 normal: normal,
    //                 position: (2.0, 1.0, 0.0).into(),
    //             },
    //         ]
    //     );
    // }

    // #[test]
    // fn mesh_generation_system() {
    //     // Arrange
    //     let local = (1, 2, 3).into();

    //     let mut events = Events::<EvtChunkDirty>::default();
    //     events.send(EvtChunkDirty(local));

    //     let mut world = World::default();
    //     world.insert_resource(events);

    //     let mut entity_map = ChunkEntityMap(HashMap::default());

    //     let asset_server = AssetServer::new(
    //         FileAssetIo::new(AssetServerSettings::default().asset_folder),
    //         TaskPool::new(),
    //     );

    //     // what now...

    //     world.insert_resource(asset_server);

    //     let entity = world
    //         .spawn()
    //         .insert_bundle(ChunkBuildingBundle {
    //             ..Default::default()
    //         })
    //         .id();

    //     entity_map.0.insert(local, entity);

    //     world.insert_resource(entity_map);

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::mesh_generation_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    // }

    // #[test]
    // fn clean_up_system() {
    //     // Arrange
    //     let local = (1, 2, 3).into();

    //     let mut events = Events::<EvtChunkDirty>::default();
    //     events.send(EvtChunkDirty(local));

    //     let mut world = World::default();
    //     world.insert_resource(events);

    //     let mut entity_map = ChunkEntityMap(HashMap::default());

    //     let entity = world
    //         .spawn()
    //         .insert_bundle(ChunkBuildingBundle {
    //             ..Default::default()
    //         })
    //         .id();

    //     entity_map.0.insert(local, entity);

    //     world.insert_resource(entity_map);

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::clean_up_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    //     assert!(world.get::<ChunkVertices>(entity).is_none());
    // }
}
