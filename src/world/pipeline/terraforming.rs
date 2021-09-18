use bevy::prelude::*;

use crate::world::storage::{chunk, voxel, VoxWorld};

pub(super) struct TerraformingPlugin;

impl Plugin for TerraformingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkAdd>()
            .add_event::<CmdChunkRemove>()
            .add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkAdded>()
            .add_event::<EvtChunkUpdated>()
            .add_event::<EvtChunkRemoved>()
            .add_system_set_to_stage(
                super::Pipeline::Terraforming,
                SystemSet::new()
                    // .with_system(process_add_chunks_system.label("add"))
                    // .with_system(process_remove_chunks_system.label("remove").after("add"))
                    .with_system(process_update_chunks_system),
            );
    }
}

#[derive(Clone)]
pub struct CmdChunkAdd(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct CmdChunkRemove(pub IVec3);

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct EvtChunkAdded(pub IVec3);

#[derive(Clone, Copy)]
pub struct EvtChunkRemoved(pub IVec3);

#[derive(Clone, Copy)]
pub struct EvtChunkUpdated(pub IVec3);

// fn process_add_chunks_system(
//     mut world: ResMut<VoxWorld>,
//     mut reader: EventReader<CmdChunkAdd>,
//     mut writer: EventWriter<EvtChunkAdded>,
// ) {
//     let mut _perf = perf_fn!();

//     for CmdChunkAdd(local, voxels) in reader.iter() {
//         perf_scope!(_perf);

//         trace!("Adding chunk {} to world", *local);
//         world.add(*local, ChunkKind::default());
//         let chunk = world.get_mut(*local).unwrap();

//         for &(voxel, kind) in voxels {
//             chunk.set(voxel, kind);
//         }

//         writer.send(EvtChunkAdded(*local));
//     }
// }

// fn process_remove_chunks_system(
//     mut world: ResMut<VoxWorld>,
//     mut reader: EventReader<CmdChunkRemove>,
//     mut writer: EventWriter<EvtChunkRemoved>,
// ) {
//     let mut _perf = perf_fn!();
//     for CmdChunkRemove(local) in reader.iter() {
//         perf_scope!(_perf);

//         trace!("Removing chunk {} from world", *local);
//         world.remove(*local);
//         writer.send(EvtChunkRemoved(*local));
//     }
// }

fn process_update_chunks_system(
    mut world: ResMut<VoxWorld>,
    mut reader: EventReader<CmdChunkUpdate>,
    mut writer: EventWriter<EvtChunkUpdated>,
) {
    let mut _perf = perf_fn!();

    for CmdChunkUpdate(chunk_local, voxels) in reader.iter() {
        let chunk = match world.get_mut(*chunk_local) {
            None => {
                warn!(
                    "Skipping update on {} since the chunk doesn't exists",
                    *chunk_local
                );
                continue;
            }
            Some(c) => c,
        };

        trace_system_run!(chunk_local);
        perf_scope!(_perf);

        let mut neighbor_chunks = vec![];

        for (voxel, kind) in voxels {
            chunk.set(*voxel, *kind);
        }

        drop(chunk);

        for (voxel, _) in voxels {
            if chunk::is_at_bounds(*voxel) {
                let dir = chunk::get_boundary_dir(*voxel);
                let neighbor_chunk = *chunk_local + dir;

                if world.get(neighbor_chunk).is_some() {
                    neighbor_chunks.push(neighbor_chunk);
                }
            }
        }

        debug!("Updating chunk {}", *chunk_local);
        writer.send(EvtChunkUpdated(*chunk_local));

        for neighbor in neighbor_chunks {
            debug!("Notifying neighbor chunk {}", neighbor);
            writer.send(EvtChunkUpdated(neighbor));
        }
    }
}

#[cfg(test)]
mod test {
    use bevy::{
        app::Events,
        prelude::{self, *},
    };

    use crate::world::storage::{self, chunk::ChunkKind};

    use super::*;

    // #[test]
    // fn process_add_chunks_system() {
    //     // Arrange
    //     let mut events = Events::<CmdChunkAdd>::default();
    //     events.send(CmdChunkAdd((1, 2, 3).into(), vec![]));

    //     let mut world = prelude::World::default();
    //     world.insert_resource(storage::VoxWorld::default());
    //     world.insert_resource(events);
    //     world.insert_resource(Events::<EvtChunkAdded>::default());

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::process_add_chunks_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    //     assert!(world
    //         .get_resource::<storage::VoxWorld>()
    //         .unwrap()
    //         .get((1, 2, 3).into())
    //         .is_some());

    //     assert_eq!(
    //         world
    //             .get_resource_mut::<Events::<EvtChunkAdded>>()
    //             .unwrap()
    //             .iter_current_update_events()
    //             .next()
    //             .unwrap()
    //             .0,
    //         (1, 2, 3).into()
    //     );
    // }

    // #[test]
    // fn process_remove_chunks_system() {
    //     // Arrange
    //     let mut events = Events::<CmdChunkRemove>::default();
    //     events.send(CmdChunkRemove((1, 2, 3).into()));

    //     let mut voxel_world = storage::VoxWorld::default();
    //     voxel_world.add((1, 2, 3).into(), ChunkKind::default());

    //     let mut world = prelude::World::default();
    //     world.insert_resource(voxel_world);
    //     world.insert_resource(events);
    //     world.insert_resource(Events::<EvtChunkRemoved>::default());

    //     let mut stage = SystemStage::parallel();
    //     stage.add_system(super::process_remove_chunks_system);

    //     // Act
    //     stage.run(&mut world);

    //     // Assert
    //     assert!(!world
    //         .get_resource::<storage::VoxWorld>()
    //         .unwrap()
    //         .get((1, 2, 3).into())
    //         .is_some());

    //     assert_eq!(
    //         world
    //             .get_resource_mut::<Events::<EvtChunkRemoved>>()
    //             .unwrap()
    //             .iter_current_update_events()
    //             .next()
    //             .unwrap()
    //             .0,
    //         (1, 2, 3).into()
    //     );
    // }

    #[test]
    fn process_update_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkUpdate>::default();
        events.send(CmdChunkUpdate(
            (1, 2, 3).into(),
            vec![(IVec3::ONE, 2.into())],
        ));

        let mut voxel_world = storage::VoxWorld::default();
        voxel_world.add((1, 2, 3).into(), ChunkKind::default());

        let mut world = prelude::World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);
        world.insert_resource(Events::<EvtChunkUpdated>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::process_update_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<storage::VoxWorld>()
                .unwrap()
                .get((1, 2, 3).into())
                .unwrap()
                .get(IVec3::ONE),
            2.into()
        );

        let evt = world
            .get_resource_mut::<Events<EvtChunkUpdated>>()
            .unwrap()
            .iter_current_update_events()
            .next()
            .unwrap()
            .clone();

        assert_eq!(evt.0, (1, 2, 3).into());
    }
}
