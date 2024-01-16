use bevy_app::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
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
            .add_systems(Update, (unload_chunks, load_chunks, handle_load_tasks));
    }
}

#[derive(Resource, Debug, Clone, Deref, DerefMut)]
struct ChunkMap(HashMap<IVec3, Entity>);

#[derive(Event, Debug, Clone, Copy)]
struct ChunkUnload(IVec3);

fn unload_chunks(
    mut commands: Commands,
    chunk_map: Res<ChunkMap>,
    mut reader: EventReader<ChunkUnload>,
) {
    for ChunkUnload(local) in reader.read() {
        if let Some(&e) = chunk_map.get(local) {
            commands.entity(e).despawn();
        }
    }
}

#[derive(Event, Debug, Clone, Copy)]
struct ChunkLoad(IVec3);

type LoadTask = Task<Vec<(IVec3, Chunk)>>;

#[derive(Resource, Default, Debug, Deref, DerefMut)]
struct LoadTasks(Vec<LoadTask>);

fn load_chunks(
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

fn handle_load_tasks(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut load_tasks: ResMut<LoadTasks>,
    mut running_tasks: Local<Vec<LoadTask>>,
) {
    running_tasks.extend(load_tasks.bypass_change_detection().drain(..));

    running_tasks.retain_mut(|task| {
        if let Some(result) = future::block_on(future::poll_once(task)) {
            result.into_iter().for_each(|(local, _chunk)| {
                let entity = commands.spawn_empty().id();
                chunk_map.insert(local, entity);
            });
            false
        } else {
            true
        }
    });
}

#[derive(Event, Debug, Clone, Copy)]
struct ChunkGen(IVec3);
