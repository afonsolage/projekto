use bevy::prelude::*;

use crate::world::storage::{self, chunk, voxel};

use super::{entity_managing::ChunkEntityMap, ChunkFacesOcclusion, EvtChunkDirty, Pipeline};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_to_stage(
            Pipeline::Rendering,
            faces_occlusion_system.label("faces_occlusion"),
        );
    }
}

fn faces_occlusion_system(
    world: Res<storage::World>,
    entity_map: Res<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkDirty>,
    mut q: Query<&mut ChunkFacesOcclusion>,
) {
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

        faces_occlusion.0.fill(voxel::FacesOcclusion::default());

        for voxel in chunk.voxels() {
            let voxel_faces = &mut faces_occlusion.0[chunk::to_index(voxel)];

            if chunk.get_kind(voxel) == 0 {
                voxel_faces.fill(true);
                continue;
            }

            for side in voxel::SIDES {
                let dir = voxel::get_side_dir(side);
                let neighbor_pos = voxel + dir;

                if !chunk::is_within_bounds(neighbor_pos) {
                    // TODO: Check neighborhood
                    continue;
                }

                if chunk.get_kind(neighbor_pos) == 1 {
                    voxel_faces[side as usize] = true;
                }
            }
        }
    }
}

fn compute_vertices_system(
    mut world: ResMut<storage::World>,
    mut reader: EventReader<EvtChunkDirty>,
) {
    //Component or Resource?
}

#[cfg(test)]
mod test {
    use bevy::{app::Events, utils::HashMap};

    use crate::world::pipeline::ChunkBuildingBundle;

    use super::*;

    #[test]
    fn faces_occlusion_system_occlude_empty_voxel() {
        // Arrange
        let local = (3, 2, 1).into();

        let mut events = Events::<EvtChunkDirty>::default();
        events.send(EvtChunkDirty(local));

        let mut voxel_world = storage::World::default();
        voxel_world.add(local);

        let mut world = World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);

        let mut entity_map = ChunkEntityMap(HashMap::default());

        entity_map.0.insert(
            local,
            world
                .spawn()
                .insert(ChunkBuildingBundle::default().faces_occlusion)
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
            faces_occlusion
                .0
                .iter()
                .all(|a| a.iter().all(|b| *b == true)),
            "A chunk full of empty-kind voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion_system() {
        // Arrange
        let local = (3, 2, 1).into();

        let mut events = Events::<EvtChunkDirty>::default();
        events.send(EvtChunkDirty(local));

        let mut voxel_world = storage::World::default();
        voxel_world.add(local);

        let chunk = voxel_world.get_mut(local).unwrap();
        // Top-Bottom occlusion
        chunk.set_kind((1, 1, 1).into(), 1);
        chunk.set_kind((1, 2, 1).into(), 1);

        // Full occluded voxel at (10, 10, 10)
        chunk.set_kind((10, 10, 10).into(), 1);
        chunk.set_kind((9, 10, 10).into(), 1);
        chunk.set_kind((11, 10, 10).into(), 1);
        chunk.set_kind((10, 9, 10).into(), 1);
        chunk.set_kind((10, 11, 10).into(), 1);
        chunk.set_kind((10, 10, 9).into(), 1);
        chunk.set_kind((10, 10, 11).into(), 1);

        let mut world = World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);

        let mut entity_map = ChunkEntityMap(HashMap::default());

        entity_map.0.insert(
            local,
            world
                .spawn()
                .insert(ChunkBuildingBundle::default().faces_occlusion)
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

        let faces = faces_occlusion.0[chunk::to_index((1, 2, 1).into())];

        assert_eq!(
            faces,
            [false, false, false, true, false, false],
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.0[chunk::to_index((1, 1, 1).into())];

        assert_eq!(
            faces,
            [false, false, true, false, false, false],
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.0[chunk::to_index((10, 10, 10).into())];

        assert_eq!(
            faces,
            [true; voxel::SIDE_COUNT],
            "Voxel fully surrounded by another non-empty voxels should be fully occluded"
        );
    }
}
