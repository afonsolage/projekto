use std::time::Duration;

use async_channel::Receiver;
use bevy::{app::ScheduleRunnerPlugin, ecs::schedule::ExecutorKind, prelude::*};

use crate::{
    asset::{ChunkAsset, ChunkAssetGenRequest},
    bundle::{ChunkKind, ChunkLight, ChunkMap},
};

use self::noise::Noise;

mod genesis;
pub mod noise;

#[derive(Component, Debug, Deref, DerefMut)]
struct ChunkRequest(ChunkAssetGenRequest);

#[derive(Resource, Deref, DerefMut)]
pub(crate) struct ChunkAssetGenReceiver(pub Receiver<ChunkAssetGenRequest>);

const TICK_EVERY_MILLIS: u64 = 1000;

pub(crate) fn start(receiver: Receiver<ChunkAssetGenRequest>) {
    // Force schedules to be single threaded, to avoid using thread pool.
    let (mut first_schedule, mut update_schedule, mut last_schedule) = (
        Schedule::new(First),
        Schedule::new(Update),
        Schedule::new(Last),
    );

    first_schedule.set_executor_kind(ExecutorKind::SingleThreaded);
    update_schedule.set_executor_kind(ExecutorKind::SingleThreaded);
    last_schedule.set_executor_kind(ExecutorKind::SingleThreaded);

    let mut app = App::new();

    app.add_plugins((
        AssetPlugin::default(),
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
    ))
    .insert_resource(ChunkAssetGenReceiver(receiver))
    .init_resource::<ChunkMap>()
    .add_schedule(first_schedule)
    .add_schedule(update_schedule)
    .add_schedule(last_schedule)
    .configure_sets(Update, GenSet::Structure.before(GenSet::Light))
    .add_systems(First, collect_requests)
    .add_systems(
        Update,
        (
            generate_structure.in_set(GenSet::Structure),
            init_light.in_set(GenSet::Light),
        ),
    )
    .add_systems(Last, dispatch_requests);

    let _ = std::thread::Builder::new()
        .name("WorldGen".into())
        .spawn(move || {
            trace!("Starting world gen app");
            app.run();
            trace!("Stopping world gen app");
        });
}

#[derive(SystemSet, Debug, Clone, Eq, PartialEq, Hash)]
enum GenSet {
    Structure,
    Light,
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
            .spawn((
                ChunkRequest(msg),
                ChunkKind::default(),
                ChunkLight::default(),
            ))
            .id();

        let existing = chunk_map.insert(chunk, entity);
        debug_assert_eq!(existing, None, "Can't replace existing chunk {chunk}");
        count += 1;
    }

    trace!("[collect_request] {count} chunks requests received.");
}

fn generate_structure(mut q: Query<(&mut ChunkKind, &ChunkRequest)>, noise: Local<Noise>) {
    if q.is_empty() {
        return;
    }

    let mut count = 0;
    for (mut kind, req) in q.iter_mut() {
        count += 1;
        genesis::generate_chunk(&noise, req.chunk, &mut kind);
    }

    trace!("[generate_structure] {count} chunks structures generated.");
}

fn init_light(mut q: Query<(&mut ChunkLight, &ChunkKind, &ChunkRequest)>) {
    if q.is_empty() {
        return;
    }

    let mut count = 0;
    for (mut chunk_light, chunk_kind, req) in q.iter_mut() {
        count += 1;
        genesis::init_light(req.chunk, chunk_kind, &mut chunk_light);
    }

    trace!("[init_light] {count} chunks light initialized.");
}

fn dispatch_requests(world: &mut World) {
    let entities = world
        .query_filtered::<Entity, With<ChunkRequest>>()
        .iter(world)
        .collect::<Vec<_>>();

    entities.into_iter().for_each(|entity| {
        let (ChunkRequest(req), ChunkKind(kind), ChunkLight(light)) = world
            .entity_mut(entity)
            .take::<(ChunkRequest, ChunkKind, ChunkLight)>()
            .expect("All components to exists");

        world.despawn(entity);

        let asset = ChunkAsset {
            chunk: req.chunk,
            light,
            kind,
            ..Default::default()
        };

        trace!(
            "[dispatch_requests] Chunk {} generated: {asset:?}",
            req.chunk
        );

        if let Ok(bytes) = bincode::serialize(&asset) {
            trace!(
                "[dispatch_requests] Chunk {} serialized. Size: {} bytes",
                req.chunk,
                bytes.len()
            );
            req.finish(Ok(bytes));
        } else {
            let chunk = asset.chunk;
            error!("Failed to serialize chunk {chunk:?}.");
        }
    });
}
