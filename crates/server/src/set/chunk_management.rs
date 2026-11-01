use bevy::{
    platform::collections::HashSet,
    prelude::*,
    tasks::{Task, block_on},
};
use projekto_archive::{ArchiveError, ArchiveServer, ArchiveTask, MaintenanceResult};
use projekto_core::coords::Chunk;

use crate::{
    DerivedCaches, DirtyChunks, WorldSet, affected_chunks,
    asset::ChunkAsset,
    bundle::{
        CachedMesh, ChunkBundle, ChunkKind, ChunkLight, ChunkLocal, ChunkMap, ChunkQuery,
        ChunkVertex,
    },
    genesis::{ChunkCreation, GenesisServer, GenesisTask},
    net::{ClientSync, Clients},
};

use super::Landscape;

/// Chunks whose latest vertices still need to be persisted.
#[derive(Resource, Default)]
pub struct PendingSaves(HashSet<Chunk>);

const SAVE_INTERVAL_SECS: f32 = 0.5;
const SAVE_BATCH_SIZE: usize = 32;
const MAINTENANCE_INTERVAL_SECS: f32 = 2.0;

pub struct ChunkManagementPlugin;

impl Plugin for ChunkManagementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .init_resource::<DerivedCaches>()
            .init_resource::<DirtyChunks>()
            .init_resource::<PendingSaves>()
            .init_resource::<Clients>()
            .init_resource::<ClientSync>()
            .add_message::<ChunkUnload>()
            .add_message::<ChunkLoad>()
            .add_message::<ChunkGen>()
            .add_systems(PreStartup, init_servers)
            .add_systems(
                Update,
                (
                    chunks_unload.run_if(on_message::<ChunkUnload>),
                    chunks_load,
                    queue_chunks_to_save,
                    chunks_save,
                    archive_maintenance,
                )
                    .chain()
                    .in_set(WorldSet::ChunkManagement),
            );
    }
}

#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkUnload(pub Chunk);

#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkLoad(pub Chunk);

#[derive(Message, Debug, Clone, Copy)]
pub struct ChunkGen(pub Chunk);

fn init_servers(mut commands: Commands) {
    commands.insert_resource(ArchiveServer::<ChunkAsset>::new("archive/region/"));
    commands.insert_resource(GenesisServer::new(1));
}

/// Periodically flushes the archive headers.
///
/// `Archive::write` only updates the in-memory header; without this, saved chunks are never
/// indexed and every run regenerates the whole world.
fn archive_maintenance(
    mut archive: ResMut<ArchiveServer<ChunkAsset>>,
    pending: Res<PendingSaves>,
    time: Res<Time>,
    mut timer: Local<Option<Timer>>,
    mut task: Local<Option<Task<MaintenanceResult>>>,
    mut was_empty: Local<bool>,
    mut force_flush: Local<bool>,
) {
    if task.as_ref().is_some_and(Task::is_finished)
        && let Some(task) = task.take()
    {
        for (region, error) in block_on(task) {
            if let Some(error) = error {
                error!("Archive maintenance failed for {region:?}: {error}");
            }
        }
    }

    let pending_empty = pending.0.is_empty();

    if !*was_empty && pending_empty {
        // The save burst finished: flush a header that includes every queued write.
        *force_flush = true;
    }
    *was_empty = pending_empty;

    if task.is_some() {
        return;
    }

    let timer = timer.get_or_insert_with(|| {
        Timer::from_seconds(MAINTENANCE_INTERVAL_SECS, TimerMode::Repeating)
    });
    let timer_due = timer.tick(time.delta()).just_finished();

    if *force_flush || timer_due {
        *force_flush = false;
        *task = Some(archive.do_maintenance_stuff());
    }
}

#[allow(clippy::too_many_arguments)]
fn chunks_unload(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut caches: ResMut<DerivedCaches>,
    mut dirty: ResMut<DirtyChunks>,
    mut pending_saves: ResMut<PendingSaves>,
    clients: Res<Clients>,
    mut sync: ResMut<ClientSync>,
    mut reader: MessageReader<ChunkUnload>,
) {
    let mut count = 0;

    reader.read().for_each(|evt| {
        if let Some(entity) = chunk_map.remove(&evt.0) {
            commands.entity(entity).despawn();

            for (id, client) in clients.iter() {
                if !sync.remove_chunk(*id, evt.0) {
                    continue;
                }

                if client
                    .channel()
                    .send(projekto_messages::ChunkUnload { chunk: evt.0 })
                    .is_err()
                {
                    sync.queue_unload(*id, evt.0);
                }
            }

            caches.invalidate(evt.0);
            dirty.0.remove(&evt.0);
            pending_saves.0.remove(&evt.0);

            for neighbor in affected_chunks(evt.0).filter(|&chunk| chunk != evt.0) {
                caches.invalidate(neighbor);
                dirty.0.insert(neighbor);
            }

            count += 1;
        } else {
            let local = evt.0;
            warn!("Chunk {local} entity not found.");
        }
    });

    trace!("[chunks_unload] {count} chunks despawned");
}

#[allow(clippy::too_many_arguments)]
fn chunks_load(
    mut commands: Commands,
    mut reader: MessageReader<ChunkLoad>,
    mut archive: ResMut<ArchiveServer<ChunkAsset>>,
    genesis: ResMut<GenesisServer>,
    landscape: Option<Res<Landscape>>,
    mut load_tasks: Local<Vec<ArchiveTask<ChunkAsset>>>,
    mut gen_tasks: Local<Vec<GenesisTask>>,
    mut loading: Local<HashSet<Chunk>>,
    mut map: ResMut<ChunkMap>,
    time: Res<Time>,
) -> Result {
    let is_desired = |chunk: Chunk| landscape.as_ref().is_some_and(|l| l.contains(chunk));

    for &ChunkLoad(chunk) in reader.read() {
        if map.0.contains_key(&chunk) || loading.contains(&chunk) {
            continue;
        }

        match archive.load_chunk(chunk) {
            Ok(task) => {
                loading.insert(chunk);
                load_tasks.push(task);
            }
            Err(e) => error!("Unable to request chunk ({chunk}). {e}"),
        }
    }

    let mut loaded = 0;
    let mut generated = 0;

    load_tasks.retain_mut(|task| {
        if let Some(result) = task.try_get_result() {
            let chunk = task.chunk();

            match result {
                Ok(Some(asset)) => {
                    loading.remove(&chunk);

                    if is_desired(chunk) {
                        let entity = spawn_chunk(&mut commands, asset);
                        map.insert(chunk, entity);

                        loaded += 1;
                    }
                }
                Ok(None) => {
                    // Keep the chunk in `loading` while genesis runs.
                    if is_desired(chunk) {
                        gen_tasks.push(genesis.generate(chunk));
                    } else {
                        loading.remove(&chunk);
                    }
                }
                Err(ArchiveError::UnsupportedVersion) => {
                    loading.remove(&chunk);
                    error!(
                        "Chunk {chunk} was saved with an unsupported archive format version. Delete or migrate archive/region before loading."
                    );
                }
                Err(e) => {
                    loading.remove(&chunk);
                    error!("Unable to load chunk ({chunk}). {e}");
                }
            }
            // remove task
            false
        } else {
            // keep task
            true
        }
    });

    gen_tasks.retain_mut(|task| {
        if let Some(result) = task.try_get_result() {
            let chunk = task.chunk();
            loading.remove(&chunk);

            match result {
                Ok(creation) => {
                    if is_desired(chunk) {
                        let entity = spawn_created_chunk(&mut commands, chunk, creation);
                        map.insert(chunk, entity);

                        generated += 1;
                    }
                }
                Err(e) => error!("Unable to generate chunk ({chunk}). {e}"),
            }
            // remove task
            false
        } else {
            // keep task
            true
        }
    });

    if time.elapsed_wrapped().as_secs() % 10 == 0 {
        trace!(
            "[chunks_spawn] Load tasks: {}. Gen tasks: {}",
            load_tasks.len(),
            gen_tasks.len()
        );
    }

    Ok(())
}

fn spawn_created_chunk(commands: &mut Commands, chunk: Chunk, creation: ChunkCreation) -> Entity {
    let ChunkCreation { kind, light } = creation;
    commands
        .spawn(ChunkBundle {
            local: ChunkLocal(chunk),
            kind: ChunkKind(kind),
            light: ChunkLight(light),
            ..Default::default()
        })
        .id()

    // TODO: save chunk
}

fn spawn_chunk(commands: &mut Commands, asset: ChunkAsset) -> Entity {
    let ChunkAsset {
        chunk,
        kind,
        light,
        vertex,
    } = asset;

    commands
        .spawn(ChunkBundle {
            kind: ChunkKind(kind),
            light: ChunkLight(light),
            local: ChunkLocal(chunk),
            vertex: ChunkVertex(vertex),
        })
        .insert(CachedMesh)
        .id()
}

fn queue_chunks_to_save(
    q_chunks: Query<(&ChunkLocal, Ref<ChunkVertex>, Option<&CachedMesh>), Changed<ChunkVertex>>,
    mut pending: ResMut<PendingSaves>,
) {
    for (ChunkLocal(chunk), vertex, cached) in &q_chunks {
        // The archive already holds this mesh; only save it if it is rebuilt.
        if vertex.is_added() && cached.is_some() {
            continue;
        }

        pending.0.insert(*chunk);
    }
}

/// Persists at most [`SAVE_BATCH_SIZE`] chunks every [`SAVE_INTERVAL_SECS`] seconds.
///
/// Initial meshing changes thousands of chunks; saving them immediately competes with meshing
/// for the ECS thread and the archive workers. Deferring keeps the render path responsive while
/// the archive catches up in the background.
fn chunks_save(
    mut archive_server: ResMut<ArchiveServer<ChunkAsset>>,
    mut pending: ResMut<PendingSaves>,
    chunks: ChunkQuery<(&ChunkKind, &ChunkLight, &ChunkVertex)>,
    time: Res<Time>,
    mut timer: Local<Option<Timer>>,
) {
    if pending.0.is_empty() {
        return;
    }

    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(SAVE_INTERVAL_SECS, TimerMode::Repeating));

    if !timer.tick(time.delta()).just_finished() {
        return;
    }

    let mut saved = 0;
    let mut handled = Vec::with_capacity(SAVE_BATCH_SIZE);

    for chunk in pending.0.iter().take(SAVE_BATCH_SIZE) {
        if let Some((kind, light, vertex)) = chunks.get_chunk(*chunk) {
            let asset = ChunkAsset {
                chunk: *chunk,
                kind: kind.0.clone(),
                light: light.0.clone(),
                vertex: vertex.0.clone(),
            };
            let _task = archive_server.save_chunk(*chunk, asset);
            saved += 1;
        }

        handled.push(*chunk);
    }

    for chunk in handled {
        pending.0.remove(&chunk);
    }

    if saved > 0 {
        trace!("[chunks_save] saved {saved} chunks.",);
    }
}

// #[cfg(test)]
// mod tests {
//     use bevy::app::ScheduleRunnerPlugin;
//
//     use crate::{
//         bundle::{ChunkKind, ChunkMap},
//         set::Landscape,
//     };
//
//     use super::*;
//
//     #[test]
//     fn chunk_load() {
//         // arrange
//         let mut app = App::new();
//
//         app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
//             .init_resource::<Landscape>()
//             .add_plugins(ChunkManagementPlugin);
//
//         app.world.send_event(ChunkLoad((0, 0).into()));
//
//         // act
//         app.update();
//
//         // assert
//         assert_eq!(
//             app.world.entities().len(),
//             1,
//             "One entity should be spawned"
//         );
//         assert_eq!(
//             app.world.get_resource::<ChunkMap>().unwrap().len(),
//             1,
//             "One entity should be inserted on map"
//         );
//     }
//
//     #[test]
//     fn chunk_gen() {
//         // arrange
//         let mut app = App::new();
//
//         app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
//             .init_resource::<Landscape>()
//             .add_plugins(ChunkManagementPlugin);
//
//         app.world.send_event(ChunkLoad((0, 0).into()));
//
//         // act
//         app.update();
//
//         // assert
//         let kind = app
//             .world
//             .query::<&ChunkKind>()
//             .get_single(&app.world)
//             .unwrap();
//
//         assert!(
//             kind.iter().any(|kind| kind.is_opaque()),
//             "Every chunk should have at least on solid block"
//         );
//     }
// }
