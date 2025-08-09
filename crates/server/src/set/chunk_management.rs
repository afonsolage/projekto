use bevy::prelude::*;
use projekto_core::chunk::Chunk;

use crate::{
    asset::{ChunkAsset, ChunkAssetHandle},
    bundle::{
        ChunkBundle, ChunkFacesOcclusion, ChunkFacesSoftLight, ChunkKind, ChunkLight, ChunkLocal,
        ChunkMap, ChunkVertex,
    },
    WorldSet,
};

pub struct ChunkManagementPlugin;

impl Plugin for ChunkManagementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .add_event::<ChunkUnload>()
            .add_event::<ChunkLoad>()
            .add_event::<ChunkGen>()
            .add_systems(
                Update,
                (
                    chunks_unload.run_if(on_event::<ChunkUnload>),
                    chunks_load.run_if(on_event::<ChunkLoad>),
                    chunks_spawn.run_if(any_chunk_to_spawn),
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
    asset_server: Res<AssetServer>,
) {
    for &ChunkLoad(chunk) in reader.read() {
        let handle = asset_server.load::<ChunkAsset>(chunk.path());
        commands.spawn(ChunkAssetHandle(handle));
    }
}

fn any_chunk_to_spawn(q: Query<(Entity, &ChunkAssetHandle), Without<ChunkLocal>>) -> bool {
    !q.is_empty()
}

fn chunks_spawn(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    asset_server: Res<AssetServer>,
    mut assets: ResMut<Assets<ChunkAsset>>,
    q: Query<(Entity, &ChunkAssetHandle), Without<ChunkLocal>>,
) {
    let mut count = 0;
    for (entity, handle) in &q {
        let loaded = match asset_server.load_state(&handle.0) {
            bevy::asset::LoadState::Loading => continue,
            bevy::asset::LoadState::NotLoaded => {
                let path = handle.path().expect("All chunk assets must have a path");
                warn!("Chunk not loaded: {path:?}");
                false
            }
            bevy::asset::LoadState::Loaded => true,
            bevy::asset::LoadState::Failed(_) => false,
        };

        if loaded {
            let ChunkAsset {
                chunk,
                kind,
                light,
                occlusion,
                soft_light,
                vertex,
            } = assets.remove(&handle.0).expect("Chunk asset exists");

            let entity = commands
                .spawn((
                    ChunkBundle {
                        kind: ChunkKind(kind),
                        light: ChunkLight(light),
                        local: ChunkLocal(chunk),
                        occlusion: ChunkFacesOcclusion(occlusion),
                        soft_light: ChunkFacesSoftLight(soft_light),
                        vertex: ChunkVertex(vertex),
                    },
                    Name::new(format!("Server Chunk {chunk:?}")),
                ))
                .id();

            if chunk_map.insert(chunk, entity).is_some() {
                warn!("Chunk {chunk:?} overwritten an existing entity on map.");
            }

            count += 1;
        }

        commands.entity(entity).despawn();
    }

    if count > 0 {
        trace!("[chunks_spawn] Spawned {count} chunks!");
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
