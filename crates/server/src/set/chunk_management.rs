use bevy::{
    platform::collections::HashSet,
    prelude::*,
    tasks::{Task, block_on},
};
use projekto_archive::{ArchiveError, ArchiveServer, ArchiveTask, MaintenanceResult};
use projekto_archive::{CHUNK_MESH_VERSION, ChunkAsset};
use projekto_core::{chunk, coords::Chunk};

use crate::{
    DerivedCaches, DirtyChunks, WorldSet, affected_chunks,
    bundle::{
        CachedMesh, ChunkBundle, ChunkKind, ChunkLight, ChunkLocal, ChunkLod, ChunkMap, ChunkQuery,
        ChunkVertex,
    },
    genesis::{ChunkCreation, GenesisServer, GenesisTask},
    net::{ClientSync, Clients},
};

use super::{
    Landscape, MeshTasks,
    meshing::{desired_lod, next_lod},
};

/// Chunks whose latest vertices still need to be persisted.
#[derive(Resource, Default)]
pub struct PendingSaves(HashSet<Chunk>);

/// Counters describing the streaming and persistence work performed by the server.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct TerrainStats {
    pub generated: u64,
    pub archive_loaded: u64,
    pub unloaded: u64,
    pub saved: u64,
    pub meshed_full: u64,
    pub meshed_coarse: u64,
    pub mesh_nanos: u64,
}

const SAVE_INTERVAL_SECS: f32 = 0.25;
const SAVE_BATCH_SIZE: usize = 64;
const MAINTENANCE_INTERVAL_SECS: f32 = 2.0;

pub struct ChunkManagementPlugin;

impl Plugin for ChunkManagementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .init_resource::<DerivedCaches>()
            .init_resource::<DirtyChunks>()
            .init_resource::<PendingSaves>()
            .init_resource::<TerrainStats>()
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

    let threads = std::env::var("PROJEKTO_GENESIS_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|threads| threads.get())
                .unwrap_or(1)
        })
        .clamp(1, 32);

    commands.insert_resource(GenesisServer::new(threads));
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
    mut stats: ResMut<TerrainStats>,
    clients: Res<Clients>,
    mut sync: ResMut<ClientSync>,
    mut meshing: Option<ResMut<MeshTasks>>,
    mut reader: MessageReader<ChunkUnload>,
) {
    let mut count = 0;

    reader.read().for_each(|evt| {
        if let Some(entity) = chunk_map.remove(&evt.0) {
            commands.entity(entity).despawn();

            if let Some(meshing) = meshing.as_mut() {
                meshing.cancel(evt.0);
            }

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
            stats.unloaded += 1;

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
    mut dirty: ResMut<DirtyChunks>,
    mut stats: ResMut<TerrainStats>,
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

                    if is_desired(chunk) && !map.0.contains_key(&chunk) {
                        let saved_lod = asset.lod;
                        let desired_lod = landscape
                            .as_ref()
                            .map(|landscape| next_lod(saved_lod, chunk, landscape.center))
                            .unwrap_or(saved_lod);
                        let lod_changed = saved_lod != desired_lod;
                        let stale_mesh = asset.mesh_version != CHUNK_MESH_VERSION;
                        let suspect_hole =
                            asset.vertex.is_empty() && !asset.kind.all(|kind| kind.is_none());
                        let entity = spawn_chunk(&mut commands, asset, desired_lod);
                        map.insert(chunk, entity);
                        stats.archive_loaded += 1;

                        if suspect_hole
                            || stale_mesh
                            || lod_changed
                            || needs_seam_remesh(chunk, landscape.as_deref())
                        {
                            dirty.0.insert(chunk);
                        }

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
                    warn!(
                        "Chunk {chunk} was saved with an unsupported archive format version; regenerating it."
                    );

                    if is_desired(chunk) {
                        gen_tasks.push(genesis.generate(chunk));
                    } else {
                        loading.remove(&chunk);
                    }
                }
                Err(ArchiveError::Decode(err)) => {
                    warn!("Chunk {chunk} could not be decoded ({err}); regenerating it.");

                    if is_desired(chunk) {
                        gen_tasks.push(genesis.generate(chunk));
                    } else {
                        loading.remove(&chunk);
                    }
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
                    if is_desired(chunk) && !map.0.contains_key(&chunk) {
                        let desired_lod = landscape
                            .as_ref()
                            .map(|landscape| desired_lod(chunk, landscape.center))
                            .unwrap_or(0);
                        let entity =
                            spawn_created_chunk(&mut commands, chunk, creation, desired_lod);
                        map.insert(chunk, entity);
                        dirty.0.insert(chunk);
                        stats.generated += 1;

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

fn needs_seam_remesh(chunk: Chunk, landscape: Option<&Landscape>) -> bool {
    let Some(landscape) = landscape else {
        return false;
    };

    let center = landscape.center;

    chunk::SIDES.iter().any(|side| {
        let neighbor = chunk.neighbor(side.dir());

        !landscape.contains(neighbor)
            || (desired_lod(chunk, center) == 0 && desired_lod(neighbor, center) > 0)
    })
}

fn spawn_created_chunk(
    commands: &mut Commands,
    chunk: Chunk,
    creation: ChunkCreation,
    lod: u8,
) -> Entity {
    let ChunkCreation { kind, light } = creation;
    commands
        .spawn((
            ChunkBundle {
                local: ChunkLocal(chunk),
                kind: ChunkKind(kind),
                light: ChunkLight(light),
                ..Default::default()
            },
            ChunkLod(lod),
        ))
        .id()

    // TODO: save chunk
}

fn spawn_chunk(commands: &mut Commands, asset: ChunkAsset, desired_lod: u8) -> Entity {
    let ChunkAsset {
        chunk,
        kind,
        light,
        vertex,
        lod,
        mesh_version,
    } = asset;

    let suspect_hole = vertex.is_empty() && !kind.all(|kind| kind.is_none());
    let stale_mesh = mesh_version != CHUNK_MESH_VERSION;

    if suspect_hole {
        warn!("chunk {chunk} was loaded with an empty mesh over solid terrain");
    }
    if stale_mesh {
        debug!(
            "chunk {chunk} was saved with mesh version {mesh_version}; remeshing instead of \
             streaming the stale mesh"
        );
    }

    // A mesh saved with older tile bindings would sample the generated atlas with the wrong
    // offsets and tints until the async remesh lands. Stream an empty mesh instead; the dirty
    // flag queues the remesh and the changed vertex buffer is sent when it finishes.
    let vertex = if stale_mesh { Vec::new() } else { vertex };
    let trust_mesh = !suspect_hole && !stale_mesh && lod == desired_lod;

    let mut entity = commands.spawn(ChunkBundle {
        kind: ChunkKind(kind),
        light: ChunkLight(light),
        local: ChunkLocal(chunk),
        vertex: ChunkVertex(vertex),
    });

    // An empty saved mesh over solid terrain is not a valid cache. Let the meshing
    // pipeline rebuild it instead of trusting it forever.
    if trust_mesh {
        entity.insert(CachedMesh);
    }

    entity.insert(ChunkLod(desired_lod));
    entity.id()
}

fn queue_chunks_to_save(
    q_chunks: Query<
        (
            &ChunkLocal,
            &ChunkKind,
            Ref<ChunkVertex>,
            Option<&CachedMesh>,
            Option<&ChunkLod>,
        ),
        Changed<ChunkVertex>,
    >,
    mut pending: ResMut<PendingSaves>,
) {
    for (ChunkLocal(chunk), kind, vertex, cached, lod) in &q_chunks {
        // The archive already holds this mesh; only save it if it is rebuilt.
        if vertex.is_added() && cached.is_some() {
            continue;
        }

        let lod = lod.map(|lod| lod.0).unwrap_or(0);

        // A generated chunk starts with an empty buffer before its first mesh. Do not persist
        // that placeholder; a real empty mesh is also not worth caching because it can be
        // regenerated from the chunk data.
        if vertex.0.is_empty() && cached.is_none() {
            continue;
        }

        // A full-detail chunk starts with an empty vertex buffer before its first mesh, and an
        // empty mesh over solid terrain is always a hole. Coarse chunks may legitimately be
        // empty when every coarse cell contains some air.
        if lod == 0 && vertex.0.is_empty() && !kind.0.all(|kind| kind.is_none()) {
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
#[allow(clippy::too_many_arguments)]
fn chunks_save(
    mut archive_server: ResMut<ArchiveServer<ChunkAsset>>,
    mut pending: ResMut<PendingSaves>,
    chunks: ChunkQuery<(
        &ChunkKind,
        &ChunkLight,
        &ChunkVertex,
        Option<&ChunkLod>,
        Option<&CachedMesh>,
    )>,
    dirty: Res<DirtyChunks>,
    meshing: Option<Res<MeshTasks>>,
    mut stats: ResMut<TerrainStats>,
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

    let candidates = pending
        .0
        .iter()
        .filter(|chunk| !dirty.0.contains(*chunk))
        .filter(|chunk| {
            !meshing
                .as_ref()
                .is_some_and(|tasks| tasks.is_pending(**chunk))
        })
        .take(SAVE_BATCH_SIZE)
        .copied()
        .collect::<Vec<_>>();

    for chunk in candidates {
        if let Some((kind, light, vertex, lod, cached)) = chunks.get_chunk(chunk) {
            let Some(lod) = lod else {
                warn!("chunk {chunk} has no ChunkLod component; dropping its pending save");
                handled.push(chunk);
                continue;
            };

            if vertex.0.is_empty() && cached.is_none() {
                handled.push(chunk);
                continue;
            }

            let asset = ChunkAsset {
                chunk,
                kind: kind.0.clone(),
                light: light.0.clone(),
                vertex: vertex.0.clone(),
                lod: lod.0,
                mesh_version: CHUNK_MESH_VERSION,
            };
            let _task = archive_server.save_chunk(chunk, asset);
            saved += 1;
        }

        handled.push(chunk);
    }

    for chunk in handled {
        pending.0.remove(&chunk);
    }

    stats.saved += saved as u64;

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

#[cfg(test)]
mod save_tests {
    use projekto_core::{
        chunk::ChunkStorage,
        coords::{Chunk, ChunkVoxel},
        voxel::{self, Kind},
    };

    use crate::bundle::{ChunkKind, ChunkLocal, ChunkLod, ChunkVertex};

    use super::*;

    fn solid_kind() -> ChunkKind {
        let mut kind = ChunkStorage::<Kind>::default();
        kind.set(ChunkVoxel::new(0, 0, 0), Kind::id(3));
        ChunkKind(kind)
    }

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<PendingSaves>()
            .add_systems(Update, queue_chunks_to_save);
        app
    }

    #[test]
    fn full_chunk_next_to_coarse_neighbour_needs_seam_remesh() {
        let landscape = Landscape {
            center: bevy::math::IVec2::ZERO,
            radius: 32,
        };

        assert!(needs_seam_remesh(Chunk::new(16, 0), Some(&landscape)));
        assert!(!needs_seam_remesh(Chunk::new(0, 0), Some(&landscape)));
        assert!(!needs_seam_remesh(Chunk::new(16, 0), None));

        let small = Landscape {
            center: bevy::math::IVec2::ZERO,
            radius: 4,
        };
        assert!(needs_seam_remesh(Chunk::new(4, 0), Some(&small)));
        assert!(needs_seam_remesh(Chunk::new(-4, 0), Some(&small)));
    }

    #[test]
    fn does_not_save_unmeshed_empty_solid_chunk() {
        let mut app = app();
        let chunk = Chunk::new(0, 0);

        app.world_mut()
            .spawn((ChunkLocal(chunk), solid_kind(), ChunkVertex::default()));

        app.update();

        assert!(!app.world().resource::<PendingSaves>().0.contains(&chunk));
    }

    #[test]
    fn does_not_save_unmeshed_empty_coarse_chunk() {
        let mut app = app();
        let chunk = Chunk::new(0, 0);

        app.world_mut().spawn((
            ChunkLocal(chunk),
            solid_kind(),
            ChunkVertex::default(),
            ChunkLod(1),
        ));

        app.update();

        assert!(!app.world().resource::<PendingSaves>().0.contains(&chunk));
    }

    #[test]
    fn saves_non_empty_chunk() {
        let mut app = app();
        let chunk = Chunk::new(0, 0);

        app.world_mut().spawn((
            ChunkLocal(chunk),
            solid_kind(),
            ChunkVertex(vec![voxel::Vertex::default()]),
        ));

        app.update();

        assert!(app.world().resource::<PendingSaves>().0.contains(&chunk));
    }
}
