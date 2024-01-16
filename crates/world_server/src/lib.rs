use bevy_app::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_log::warn;
use bevy_math::prelude::*;
use bevy_tasks::Task;
use bevy_utils::HashMap;
use futures_lite::future;
use projekto_core::chunk::Chunk;
use projekto_genesis::task;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadTasks>()
            .add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_systems(
                Update,
                (
                    chunks_unload.run_if(on_event::<ChunkUnload>()),
                    chunks_load.run_if(on_event::<ChunkLoad>()),
                    chunks_handle_load_tasks,
                ),
            );
    }
}

#[derive(Resource, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<IVec3, Entity>);

#[derive(Event, Debug, Clone, Copy)]
struct ChunkUnload(IVec3);

fn chunks_unload(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut reader: EventReader<ChunkUnload>,
) {
    for ChunkUnload(local) in reader.read() {
        if let Some(e) = chunk_map.remove(local) {
            commands.entity(e).despawn();
        } else {
            warn!("Failed to unload chunk {local}. Chunk not found in entity map.");
        }
    }
}

#[derive(Event, Debug, Clone, Copy)]
struct ChunkLoad(IVec3);

type LoadTask = Task<Vec<(IVec3, Chunk)>>;

#[derive(Resource, Default, Debug, Deref, DerefMut)]
struct LoadTasks(Vec<LoadTask>);

fn chunks_load(
    mut reader: EventReader<ChunkLoad>,
    mut writer: EventWriter<ChunkGen>,
    mut load_tasks: ResMut<LoadTasks>,
) {
    let locals = reader.read().map(|evt| evt.0).collect::<Vec<_>>();

    let task::LoadChunksResult {
        not_found,
        load_task,
    } = task::load_chunks(&locals);

    if let Some(new_load_task) = load_task {
        load_tasks.extend(new_load_task);
    }

    not_found
        .into_iter()
        .for_each(|local| writer.send(ChunkGen(local)));
}

fn chunks_handle_load_tasks(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut running_tasks: ResMut<LoadTasks>,
) {
    running_tasks.retain_mut(|task| {
        future::block_on(future::poll_once(task)).is_some_and(|result| {
            result.into_iter().for_each(|(local, _chunk)| {
                let entity = commands.spawn_empty().id();
                // TODO: Spawn chunk bundle
                chunk_map.insert(local, entity);
            });
            true
        })
    });
}

#[derive(Event, Debug, Clone, Copy)]
struct ChunkGen(IVec3);
