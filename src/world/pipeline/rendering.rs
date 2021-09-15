use bevy::{
    prelude::*,
    render::{mesh::Indices, pipeline::PrimitiveTopology},
};

#[cfg(perf_counter)]
use crate::debug::{PerfCounter, PerfCounterRes};

use crate::world::{
    mesh,
    storage::{
        self, chunk,
        voxel::{self, VoxelVertex},
        VoxWorld,
    },
};

use super::{
    ChunkBuildingBundle, ChunkEntityMap, ChunkFaces, ChunkFacesOcclusion, ChunkVertices,
    EvtChunkDirty, Pipeline,
};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(
            Pipeline::Rendering,
            faces_occlusion_system.label("faces_occlusion"),
        )
        .add_system_to_stage(
            Pipeline::Rendering,
            faces_merging_system
                .label("faces_merging")
                .after("faces_occlusion"),
        )
        .add_system_to_stage(
            Pipeline::Rendering,
            vertices_computation_system
                .label("vertices")
                .after("faces_merging"),
        )
        .add_system_to_stage(
            Pipeline::Rendering,
            mesh_generation_system
                .label("mesh_generation")
                .after("vertices"),
        )
        .add_system_to_stage(
            Pipeline::Rendering,
            clean_up_system.after("mesh_generation"),
        );
    }
}

fn faces_occlusion_system(
    #[cfg(perf_counter)] perf_res: Res<PerfCounterRes>,
    world: Res<storage::VoxWorld>,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkDirty>,
    mut q: Query<&mut ChunkFacesOcclusion>,
) {
    #[cfg(perf_counter)]
    let mut perf_counter = PerfCounter::new("Faces Occlusion");

    for EvtChunkDirty(local) in reader.iter() {
        let chunk = match world.get(*local) {
            None => {
                warn!(
                    "Skipping faces occlusion since chunk {} wasn't found on world",
                    *local
                );
                continue;
            }
            Some(c) => c,
        };

        let entity = match entity_map.0.get(local) {
            None => {
                warn!(
                    "Skipping faces occlusion since chunk {} wasn't found on entity map",
                    *local
                );
                continue;
            }
            Some(&e) => e,
        };

        let mut faces_occlusion = match q.get_mut(entity) {
            Err(e) => {
                warn!(
                    "Skipping faces occlusion for chunk {}. Error: {}",
                    *local, e
                );
                continue;
            }
            Ok(f) => f,
        };
        #[cfg(perf_counter)]
        let _perf = perf_counter.measure();

        faces_occlusion.set_all(voxel::FacesOcclusion::default());

        for voxel in chunk::voxels() {
            let mut voxel_faces = faces_occlusion.get(voxel);

            if chunk.get(voxel).is_empty() {
                voxel_faces.set_all(true);
            } else {
                for side in voxel::SIDES {
                    let dir = side.get_side_dir();
                    let neighbor_pos = voxel + dir;

                    let neighbor_kind = if !chunk::is_within_bounds(neighbor_pos) {
                        let (next_chunk_dir, next_chunk_voxel) = chunk::overlap_voxel(neighbor_pos);

                        if let Some(neighbor_chunk) = world.get(*local + next_chunk_dir) {
                            neighbor_chunk.get(next_chunk_voxel)
                        } else {
                            continue;
                        }
                    } else {
                        chunk.get(neighbor_pos)
                    };

                    if !neighbor_kind.is_empty() {
                        voxel_faces[side as usize] = true;
                    }
                }
            }

            faces_occlusion.set(voxel, voxel_faces);
        }
    }
    #[cfg(perf_counter)]
    {
        perf_counter.calc_meta();
        perf_res.lock().unwrap().add(perf_counter);
    }
}

fn vertices_computation_system(
    #[cfg(perf_counter)] perf_res: Res<PerfCounterRes>,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkDirty>,
    mut q: Query<(&ChunkFaces, &mut ChunkVertices)>,
) {
    #[cfg(perf_counter)]
    let mut perf_counter = PerfCounter::new("Vertices Computation");

    for EvtChunkDirty(local) in reader.iter() {
        let entity = match entity_map.0.get(local) {
            None => {
                warn!(
                    "Skipping vertices computation since chunk {} wasn't found on entity map",
                    *local
                );
                continue;
            }
            Some(&e) => e,
        };

        let (faces, mut vertices) = match q.get_mut(entity) {
            Err(e) => {
                warn!(
                    "Skipping vertices computation for chunk {}. Error: {}",
                    *local, e
                );
                continue;
            }
            Ok(f) => f,
        };

        #[cfg(perf_counter)]
        let _perf = perf_counter.measure();
        trace!("Processing vertices computation of chunk entity {}", *local);

        vertices.0.clear();

        for face in faces.0.iter() {
            let normal = face.side.get_side_normal();

            for (i, v) in face.vertices.iter().enumerate() {
                let base_vertex_idx = mesh::VERTICES_INDICES[face.side as usize][i];
                let base_vertex: Vec3 = mesh::VERTICES[base_vertex_idx].into();
                vertices.0.push(VoxelVertex {
                    position: base_vertex + v.as_f32(),
                    normal,
                })
            }
        }
    }

    #[cfg(perf_counter)]
    {
        perf_counter.calc_meta();
        perf_res.lock().unwrap().add(perf_counter);
    }
}

fn mesh_generation_system(
    #[cfg(perf_counter)] perf_res: Res<PerfCounterRes>,
    mut commands: Commands,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkDirty>,
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<&ChunkVertices>,
) {
    #[cfg(perf_counter)]
    let mut perf_counter = PerfCounter::new("Mesh Generation");

    for EvtChunkDirty(local) in reader.iter() {
        let entity = match entity_map.0.get(local) {
            None => {
                warn!(
                    "Skipping mesh generation since chunk {} wasn't found on entity map",
                    *local
                );
                continue;
            }
            Some(&e) => e,
        };

        let vertices = match query.get(entity) {
            Err(e) => {
                warn!(
                    "Skipping vertices computation for chunk {}. Error: {}",
                    *local, e
                );
                continue;
            }
            Ok(v) => &v.0,
        };
        #[cfg(perf_counter)]
        let _perf = perf_counter.measure();

        trace!("Processing mesh generation of chunk entity {}", *local);

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        let mut positions: Vec<[f32; 3]> = vec![];
        let mut normals: Vec<[f32; 3]> = vec![];

        let vertex_count = vertices.len();

        for vertex in vertices {
            positions.push([vertex.position.x, vertex.position.y, vertex.position.z]);
            normals.push([vertex.normal.x, vertex.normal.y, vertex.normal.z]);
        }

        mesh.set_indices(Some(Indices::U32(mesh::compute_indices(vertex_count))));
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

        commands.entity(entity).insert(meshes.add(mesh));
    }
    #[cfg(perf_counter)]
    {
        perf_counter.calc_meta();
        perf_res.lock().unwrap().add(perf_counter);
    }
}

fn clean_up_system(
    #[cfg(perf_counter)] perf_res: Res<PerfCounterRes>,
    mut commands: Commands,
    mut reader: EventReader<EvtChunkDirty>,
    entity_map: Res<ChunkEntityMap>,
) {
    #[cfg(perf_counter)]
    let mut perf_counter = PerfCounter::new("Clean Up");

    for EvtChunkDirty(local) in reader.iter() {
        let entity = match entity_map.0.get(local) {
            None => {
                warn!(
                    "Skipping clean up since chunk {} wasn't found on entity map",
                    *local
                );
                continue;
            }
            Some(&e) => e,
        };
        #[cfg(perf_counter)]
        let _perf = perf_counter.measure();

        trace!("Clearing up chunk entity {}", *local);

        commands
            .entity(entity)
            .remove_bundle::<ChunkBuildingBundle>();
    }
    #[cfg(perf_counter)]
    {
        perf_counter.calc_meta();
        perf_res.lock().unwrap().add(perf_counter);
    }
}

fn faces_merging_system(
    #[cfg(perf_counter)] perf_res: Res<PerfCounterRes>,
    mut reader: EventReader<EvtChunkDirty>,
    vox_world: Res<VoxWorld>,
    entity_map: Res<ChunkEntityMap>,
    mut query: Query<(&mut ChunkFaces, &ChunkFacesOcclusion)>,
) {
    #[cfg(perf_counter)]
    let mut perf_counter = PerfCounter::new("Faces Merging");

    for EvtChunkDirty(local) in reader.iter() {
        let entity = match entity_map.0.get(local) {
            Some(&e) => e,
            None => {
                warn!(
                    "Skipping faces merging since chunk {} wasn't found on entity map",
                    *local
                );
                continue;
            }
        };

        let (mut faces, occlusion) = match query.get_mut(entity) {
            Ok(v) => v,
            Err(e) => {
                warn!("Skipping faces merging for chunk {}. Error: {}", *local, e);
                continue;
            }
        };

        let chunk = match vox_world.get(*local) {
            None => {
                warn!(
                    "Skipping faces occlusion since chunk {} wasn't found on world",
                    *local
                );
                continue;
            }
            Some(c) => c,
        };

        #[cfg(perf_counter)]
        let _perf = perf_counter.measure();

        let merged_faces = mesh::merge_faces(&occlusion, chunk);
        faces.0 = merged_faces;
    }

    #[cfg(perf_counter)]
    {
        perf_counter.calc_meta();
        perf_res.lock().unwrap().add(perf_counter);
    }
}

#[cfg(test)]
mod test {
    use bevy::{app::Events, utils::HashMap};

    use crate::world::{pipeline::ChunkBuildingBundle, storage::voxel::VoxelFace};

    use super::*;

    #[test]
    fn faces_occlusion_system_occlude_empty_voxel() {
        // Arrange
        let local = (3, 2, 1).into();

        let mut events = Events::<EvtChunkDirty>::default();
        events.send(EvtChunkDirty(local));

        let mut voxel_world = storage::VoxWorld::default();
        voxel_world.add(local);

        let mut world = World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);

        let mut entity_map = ChunkEntityMap(HashMap::default());

        entity_map.0.insert(
            local,
            world
                .spawn()
                .insert_bundle(ChunkBuildingBundle::default())
                .id(),
        );

        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::faces_occlusion_system);

        // Act
        stage.run(&mut world);

        // Assert
        let faces_occlusion = world
            .query::<&ChunkFacesOcclusion>()
            .iter(&world)
            .next()
            .unwrap();

        assert!(
            faces_occlusion.iter().all(|a| a.is_fully_occluded()),
            "A chunk full of empty-kind voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion_system() {
        // Arrange
        let local = (3, 2, 1).into();

        let mut events = Events::<EvtChunkDirty>::default();
        events.send(EvtChunkDirty(local));

        let mut voxel_world = storage::VoxWorld::default();
        voxel_world.add(local);

        let chunk = voxel_world.get_mut(local).unwrap();
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

        let mut world = World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);

        let mut entity_map = ChunkEntityMap(HashMap::default());

        entity_map.0.insert(
            local,
            world
                .spawn()
                .insert_bundle(ChunkBuildingBundle::default())
                .id(),
        );

        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::faces_occlusion_system);

        // Act
        stage.run(&mut world);

        // Assert
        let faces_occlusion = world
            .query::<&ChunkFacesOcclusion>()
            .iter(&world)
            .next()
            .unwrap();

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
    fn vertices_computation_system() {
        // Arrange
        let local = (1, 2, 3).into();

        let mut events = Events::<EvtChunkDirty>::default();
        events.send(EvtChunkDirty(local));

        let mut world = World::default();
        world.insert_resource(events);

        let mut entity_map = ChunkEntityMap(HashMap::default());

        let side = voxel::Side::Up;
        let faces = ChunkFaces(vec![VoxelFace {
            side,
            vertices: [
                (0, 0, 0).into(),
                (0, 0, 1).into(),
                (1, 0, 1).into(),
                (1, 0, 0).into(),
            ],
        }]);

        let entity = world
            .spawn()
            .insert_bundle(ChunkBuildingBundle {
                faces,
                ..Default::default()
            })
            .id();

        entity_map.0.insert(local, entity);

        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::vertices_computation_system);

        // Act
        stage.run(&mut world);

        // Assert
        let vertices = world.query::<&ChunkVertices>().iter(&world).next().unwrap();

        let normal = side.get_side_normal();
        assert_eq!(
            vertices.0,
            vec![
                VoxelVertex {
                    normal: normal,
                    position: (0.0, 1.0, 0.0).into(),
                },
                VoxelVertex {
                    normal: normal,
                    position: (0.0, 1.0, 2.0).into(),
                },
                VoxelVertex {
                    normal: normal,
                    position: (2.0, 1.0, 2.0).into(),
                },
                VoxelVertex {
                    normal: normal,
                    position: (2.0, 1.0, 0.0).into(),
                },
            ]
        );
    }

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

    #[test]
    fn clean_up_system() {
        // Arrange
        let local = (1, 2, 3).into();

        let mut events = Events::<EvtChunkDirty>::default();
        events.send(EvtChunkDirty(local));

        let mut world = World::default();
        world.insert_resource(events);

        let mut entity_map = ChunkEntityMap(HashMap::default());

        let entity = world
            .spawn()
            .insert_bundle(ChunkBuildingBundle {
                ..Default::default()
            })
            .id();

        entity_map.0.insert(local, entity);

        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::clean_up_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert!(world.get::<ChunkVertices>(entity).is_none());
    }
}
