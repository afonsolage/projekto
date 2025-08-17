use bevy::prelude::*;
use projekto_archive::{ArchiveServer, ArchiveTask};
use projekto_core::chunk::Chunk;

use crate::{
    WorldSet,
    asset::ChunkAsset,
    bundle::{
        ChunkBundle, ChunkFacesOcclusion, ChunkFacesSoftLight, ChunkKind, ChunkLight, ChunkLocal,
        ChunkMap, ChunkVertex,
    },
    genesis::{ChunkCreation, GenesisServer, GenesisTask},
};

pub struct ChunkManagementPlugin;

impl Plugin for ChunkManagementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_systems(PreStartup, init_servers)
            .add_systems(
                Update,
                (chunks_unload.run_if(on_event::<ChunkUnload>), chunks_load)
                    .chain()
                    .in_set(WorldSet::ChunkManagement),
            );
    }
}

#[derive(Event, Debug, Clone, Copy)]
pub struct ChunkUnload(pub Chunk);

#[derive(Event, Debug, Clone, Copy)]
pub struct ChunkLoad(pub Chunk);

#[derive(Event, Debug, Clone, Copy)]
pub struct ChunkGen(pub Chunk);

fn init_servers(mut commands: Commands) {
    commands.insert_resource(ArchiveServer::<ChunkAsset>::new("archive/region/"));
    commands.insert_resource(GenesisServer::new(1));
}

fn chunks_unload(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut reader: EventReader<ChunkUnload>,
) {
    let mut count = 0;

    reader.read().for_each(|evt| {
        if let Some(entity) = chunk_map.remove(&evt.0) {
            commands.entity(entity).despawn();
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
    mut reader: EventReader<ChunkLoad>,
    mut archive: ResMut<ArchiveServer<ChunkAsset>>,
    genesis: ResMut<GenesisServer>,
    mut load_tasks: Local<Vec<ArchiveTask<ChunkAsset>>>,
    mut gen_tasks: Local<Vec<GenesisTask>>,
    mut map: ResMut<ChunkMap>,
    time: Res<Time>,
) -> Result {
    for &ChunkLoad(chunk) in reader.read() {
        load_tasks.push(archive.load_chunk(chunk)?);
    }

    let mut loaded = 0;
    let mut generated = 0;

    load_tasks.retain_mut(|task| {
        if let Some(result) = task.try_get_result() {
            match result {
                Ok(maybe_asset) => {
                    if let Some(asset) = maybe_asset {
                        let chunk = asset.chunk;
                        let entity = spawn_chunk(&mut commands, asset);
                        map.insert(chunk, entity);

                        loaded += 1;
                    } else {
                        let task = genesis.generate(task.chunk());
                        gen_tasks.push(task);
                    }
                }
                Err(e) => error!("Unabled to load chunk ({}). {e}", task.chunk()),
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
            match result {
                Ok(creation) => {
                    let entity = spawn_created_chunk(&mut commands, chunk, creation);
                    map.insert(chunk, entity);

                    generated += 1;
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
        occlusion,
        soft_light,
        vertex,
    } = asset;

    commands
        .spawn(ChunkBundle {
            kind: ChunkKind(kind),
            light: ChunkLight(light),
            local: ChunkLocal(chunk),
            occlusion: ChunkFacesOcclusion(occlusion),
            soft_light: ChunkFacesSoftLight(soft_light),
            vertex: ChunkVertex(vertex),
        })
        .id()
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
