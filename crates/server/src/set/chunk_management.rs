use bevy::prelude::*;
use projekto_core::chunk::Chunk;

use crate::{
    WorldSet,
    archive::{ArchiveServer, ArchiveTask},
    asset::ChunkAsset,
    bundle::{
        ChunkBundle, ChunkFacesOcclusion, ChunkFacesSoftLight, ChunkKind, ChunkLight, ChunkLocal,
        ChunkMap, ChunkVertex,
    },
    r#gen,
};

pub struct ChunkManagementPlugin;

impl Plugin for ChunkManagementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_systems(PreStartup, init_archive_server)
            .add_systems(
                Update,
                (
                    chunks_unload.run_if(on_event::<ChunkUnload>),
                    chunks_load.run_if(on_event::<ChunkLoad>),
                )
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

fn init_archive_server(mut commands: Commands) {
    commands.insert_resource(ArchiveServer::<ChunkAsset>::new("archive/region/"));
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

fn chunks_load(
    mut commands: Commands,
    mut reader: EventReader<ChunkLoad>,
    mut archive: ResMut<ArchiveServer<ChunkAsset>>,
    mut load_tasks: Local<Vec<ArchiveTask<ChunkAsset>>>,
    mut map: ResMut<ChunkMap>,
) -> Result {
    for &ChunkLoad(chunk) in reader.read() {
        load_tasks.push(archive.load_chunk(chunk)?);
    }

    let mut loaded = 0;

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
                        todo!("Request chunk gen")
                    }
                }
                Err(e) => error!("Unabled to load chunk. {e}"),
            }
            false
        } else {
            true
        }
    });

    if loaded > 0 {
        trace!("[chunks_spawn] Loaded {loaded} chunks!");
    }

    Ok(())
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
