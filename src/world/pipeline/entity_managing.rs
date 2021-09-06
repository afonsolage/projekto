use bevy::{prelude::*, utils::HashMap};

use super::{ChunkLocal, EvtChunkAdded, EvtChunkRemoved, EvtChunkUpdated};

pub(super) struct EntityManagingPlugin;

impl Plugin for EntityManagingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkDirty>()
            .add_startup_system_to_stage(
                super::PipelineStartup::EntityManaging,
                setup_chunk_entity_map,
            )
            .add_system_set_to_stage(
                super::Pipeline::EntityManaging,
                SystemSet::new()
                    .with_system(despawn_chunks_system.label("despawn"))
                    .with_system(spawn_chunks_system.label("spawn").after("despawn"))
                    .with_system(update_chunks_system.after("spawn")),
            );
    }
}

pub struct EvtChunkDirty(pub IVec3);

#[derive(Bundle)]
pub struct ChunkBundle {
    local: ChunkLocal,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            local: ChunkLocal(IVec3::ZERO),
        }
    }
}

pub struct ChunkEntityMap(pub HashMap<IVec3, Entity>);

fn setup_chunk_entity_map(mut commands: Commands) {
    commands.insert_resource(ChunkEntityMap(HashMap::default()));
}

fn spawn_chunks_system(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkAdded>,
    mut writer: EventWriter<EvtChunkDirty>,
) {
    for EvtChunkAdded(local) in reader.iter() {
        let entity = commands
            .spawn_bundle(ChunkBundle {
                local: ChunkLocal(*local),
            })
            .id();
        entity_map.0.insert(*local, entity);
        writer.send(EvtChunkDirty(*local));
    }
}

fn despawn_chunks_system(
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut reader: EventReader<EvtChunkRemoved>,
) {
    for EvtChunkRemoved(local) in reader.iter() {
        if let Some(entity) = entity_map.0.remove(local) {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn update_chunks_system(
    mut writer: EventWriter<EvtChunkDirty>,
    mut reader: EventReader<EvtChunkUpdated>,
) {
    for EvtChunkUpdated(chunk_local, _) in reader.iter() {
        writer.send(EvtChunkDirty(*chunk_local));
    }
}

#[cfg(test)]
mod test {
    use bevy::{app::Events, prelude::*, utils::HashMap};

    use crate::world::pipeline::{entity_managing::ChunkBundle, ChunkLocal, EvtChunkRemoved};

    use super::{ChunkEntityMap, EvtChunkAdded, EvtChunkDirty, EvtChunkUpdated};

    #[test]
    fn spawn_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkAdded>::default();
        added_events.send(EvtChunkAdded(IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(ChunkEntityMap(HashMap::default()));
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkDirty>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::spawn_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<Events<EvtChunkDirty>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            IVec3::ONE
        );

        assert_eq!(world.query::<&ChunkLocal>().iter(&world).len(), 1);
    }

    #[test]
    fn despawn_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkRemoved>::default();
        added_events.send(EvtChunkRemoved(IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkDirty>::default());

        let entity = world
            .spawn()
            .insert_bundle(ChunkBundle {
                local: ChunkLocal(IVec3::ONE),
                ..Default::default()
            })
            .id();

        let mut entity_map = ChunkEntityMap(HashMap::default());
        entity_map.0.insert(IVec3::ONE, entity);
        world.insert_resource(entity_map);

        let mut stage = SystemStage::parallel();
        stage.add_system(super::despawn_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(world.query::<&ChunkLocal>().iter(&world).len(), 0);
        assert!(world.get_resource::<ChunkEntityMap>().unwrap().0.is_empty());
    }

    #[test]
    fn update_chunks_system() {
        // Arrange
        let mut added_events = Events::<EvtChunkUpdated>::default();
        added_events.send(EvtChunkUpdated((1, 2, 3).into(), IVec3::ONE));

        let mut world = World::default();
        world.insert_resource(added_events);
        world.insert_resource(Events::<super::EvtChunkDirty>::default());

        let mut stage = SystemStage::parallel();
        stage.add_system(super::update_chunks_system);

        // Act
        stage.run(&mut world);

        // Assert
        assert_eq!(
            world
                .get_resource::<Events<EvtChunkDirty>>()
                .unwrap()
                .iter_current_update_events()
                .next()
                .unwrap()
                .0,
            (1, 2, 3).into()
        );
    }
}
