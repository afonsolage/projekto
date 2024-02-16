use std::time::Duration;

use async_channel::Receiver;
use bevy::prelude::*;

use crate::{
    app::AsyncRunnnerPlugin,
    asset::{ChunkAsset, ChunkAssetGenRequest},
    bundle::{ChunkKind, ChunkMap},
};

mod genesis;

#[derive(Component, Debug, Deref, DerefMut)]
struct ChunkRequest(ChunkAssetGenRequest);

#[derive(Resource, Deref, DerefMut)]
pub(crate) struct ChunkAssetGenReceiver(pub Receiver<ChunkAssetGenRequest>);

const TICK_EVERY_MILLIS: u64 = 1000;

pub(crate) fn create(receiver: Receiver<ChunkAssetGenRequest>) -> App {
    let mut app = App::new();

    app.add_plugins((
        AssetPlugin::default(),
        MinimalPlugins,
        AsyncRunnnerPlugin::new("WorldGen", Duration::from_millis(TICK_EVERY_MILLIS)),
    ));

    app.insert_resource(ChunkAssetGenReceiver(receiver));
    app.init_resource::<ChunkMap>();

    app.add_systems(First, collect_requests);
    app.add_systems(Update, generate_structure);
    app.add_systems(Last, dispatch_requests);

    app
}

fn collect_requests(
    mut commands: Commands,
    receiver: Res<ChunkAssetGenReceiver>,
    mut chunk_map: ResMut<ChunkMap>,
) {
    let mut count = receiver.len();
    if count == 0 {
        return;
    }

    while let Ok(msg) = receiver.try_recv() {
        let chunk = msg.chunk;
        let entity = commands
            .spawn((ChunkRequest(msg), ChunkKind::default()))
            .id();

        let existing = chunk_map.insert(chunk, entity);
        debug_assert_eq!(existing, None, "Can't replace existing chunk {chunk}");
        count += 1;
    }

    trace!("[collect_request] {count} chunks requests received.");
}

fn generate_structure(mut q: Query<(&mut ChunkKind, &ChunkRequest)>) {
    if q.is_empty() {
        return;
    }

    let mut count = 0;
    for (mut kind, req) in q.iter_mut() {
        count += 1;
        genesis::generate_chunk(req.chunk, &mut kind);
    }

    trace!("[generate_structure] {count} chunks structures generated.");
}

fn dispatch_requests(world: &mut World) {
    let entities = world
        .query_filtered::<Entity, With<ChunkRequest>>()
        .iter(world)
        .collect::<Vec<_>>();

    entities.into_iter().for_each(|entity| {
        let (ChunkRequest(req), ChunkKind(kind)) = world
            .entity_mut(entity)
            .take::<(ChunkRequest, ChunkKind)>()
            .expect("All components to exists");

        let asset = ChunkAsset {
            chunk: req.chunk,
            kind,
            ..Default::default()
        };

        if let Ok(bytes) = bincode::serialize(&asset) {
            req.finish(Ok(bytes));
        } else {
            let chunk = asset.chunk;
            error!("Failed to serialize chunk {chunk:?}.");
        }
    });
}
