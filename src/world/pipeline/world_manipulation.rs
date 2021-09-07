use bevy::prelude::*;

use crate::world::storage::{voxel, World};

pub(super) struct WorldManipulationPlugin;

impl Plugin for WorldManipulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkAdd>()
            .add_event::<CmdChunkRemove>()
            .add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkAdded>()
            .add_event::<EvtChunkUpdated>()
            .add_event::<EvtChunkRemoved>()
            .add_startup_system_to_stage(super::PipelineStartup::WorldManipulation, setup_world)
            .add_system_set_to_stage(
                super::Pipeline::WorldManipulation,
                SystemSet::new()
                    .with_system(process_add_chunks_system.label("add"))
                    .with_system(process_remove_chunks_system.label("remove").after("add"))
                    .with_system(process_update_chunks_system.after("remove")),
            );
    }
}

#[derive(Clone, Copy)]
pub struct CmdChunkAdd(pub IVec3);

#[derive(Clone, Copy)]
pub struct CmdChunkRemove(pub IVec3);

#[derive(Clone, Copy)]
pub struct CmdChunkUpdate(pub IVec3, pub IVec3, pub voxel::Kind);

#[derive(Clone, Copy)]
pub struct EvtChunkAdded(pub IVec3);

#[derive(Clone, Copy)]
pub struct EvtChunkRemoved(pub IVec3);

#[derive(Clone, Copy)]
pub struct EvtChunkUpdated(pub IVec3, pub IVec3);

fn setup_world(mut commands: Commands) {
    commands.insert_resource(World::default());
}

fn process_add_chunks_system(
    mut world: ResMut<World>,
    mut reader: EventReader<CmdChunkAdd>,
    mut writer: EventWriter<EvtChunkAdded>,
) {
    for CmdChunkAdd(local) in reader.iter() {
        world.add(*local);
        writer.send(EvtChunkAdded(*local));
    }
}

fn process_remove_chunks_system(
    mut world: ResMut<World>,
    mut reader: EventReader<CmdChunkRemove>,
    mut writer: EventWriter<EvtChunkRemoved>,
) {
    for CmdChunkRemove(local) in reader.iter() {
        world.remove(*local);
        writer.send(EvtChunkRemoved(*local));
    }
}

fn process_update_chunks_system(
    mut world: ResMut<World>,
    mut reader: EventReader<CmdChunkUpdate>,
    mut writer: EventWriter<EvtChunkUpdated>,
) {
    for CmdChunkUpdate(chunk_local, voxel_local, voxel_value) in reader.iter() {
        let chunk = match world.get_mut(*chunk_local) {
            None => {
                warn!(
                    "Skipping update on {} {} since the chunk doesn't exists",
                    *chunk_local, voxel_local
                );
                continue;
            }
            Some(c) => c,
        };

        chunk.set_kind(*voxel_local, *voxel_value);

        writer.send(EvtChunkUpdated(*chunk_local, *voxel_local));
    }
}

#[cfg(test)]
mod test {
    use bevy::{
        app::Events,
        prelude::{self, *},
    };

    use crate::world::{
        pipeline::{CmdChunkRemove, EvtChunkRemoved},
        storage,
    };

    use super::*;

    #[test]
    fn process_add_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkAdd>::default();
        events.send(CmdChunkAdd((1, 2, 3).into()));

        let mut world = prelude::World::default();
        world.insert_resource(storage::World::default());
        world.insert_resource(events);
        world.insert_resource(Events::<EvtChunkAdded>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::process_add_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert!(world
            .get_resource::<storage::World>()
            .unwrap()
            .exists((1, 2, 3).into()));

        assert_eq!(
            world
                .get_resource_mut::<Events::<EvtChunkAdded>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }

    #[test]
    fn process_remove_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkRemove>::default();
        events.send(CmdChunkRemove((1, 2, 3).into()));

        let mut voxel_world = storage::World::default();
        voxel_world.add((1, 2, 3).into());

        let mut world = prelude::World::default();
        world.insert_resource(voxel_world);
        world.insert_resource(events);
        world.insert_resource(Events::<EvtChunkRemoved>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::process_remove_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert!(!world
            .get_resource::<storage::World>()
            .unwrap()
            .exists((1, 2, 3).into()));

        assert_eq!(
            world
                .get_resource_mut::<Events::<EvtChunkRemoved>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }

    #[test]
    fn process_update_chunks_system() {
        // Arrange
        let mut events = Events::<CmdChunkUpdate>::default();
        events.send(CmdChunkUpdate((1, 2, 3).into(), IVec3::ONE, 2));

        let mut voxel_world = storage::World::default();
        voxel_world.add((1, 2, 3).into());

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
                .get_resource::<storage::World>()
                .unwrap()
                .get((1, 2, 3).into())
                .unwrap()
                .get_kind(IVec3::ONE),
            2
        );

        let evt = world
            .get_resource_mut::<Events<EvtChunkUpdated>>()
            .unwrap()
            .iter_current_update_events()
            .next()
            .unwrap()
            .clone();

        assert_eq!(evt.0, (1, 2, 3).into());
        assert_eq!(evt.1, IVec3::ONE);
    }
}
