use bevy::prelude::*;

use crate::world::storage::{chunk, voxel, VoxWorld};

pub(super) struct TerraformingPlugin;

impl Plugin for TerraformingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkUpdated>()
            .add_system_set_to_stage(
                super::Pipeline::Terraforming,
                SystemSet::new().with_system(process_update_chunks_system),
            );
    }
}

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct EvtChunkUpdated(pub IVec3);

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
