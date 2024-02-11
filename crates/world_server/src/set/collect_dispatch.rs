use bevy::{
    prelude::*,
    tasks::{block_on, AsyncComputeTaskPool, Task},
    utils::HashSet,
};
use projekto_core::chunk::Chunk;

use crate::{gen::setup_gen_app, WorldSet};

use super::{ChunkGen, ChunkLoad};

pub struct CollectDispatchPlugin;

impl Plugin for CollectDispatchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGenContext>()
            .add_systems(Startup, setup_chunk_cache)
            .add_systems(PreUpdate, collect_world_gen.in_set(WorldSet::CollectAsync))
            .add_systems(
                PostUpdate,
                dispatch_world_gen.in_set(WorldSet::DispatchAsync),
            );
    }
}

fn setup_chunk_cache() {
    #[cfg(not(test))]
    crate::cache::ChunkCache::init("");
}

#[derive(Resource, Default, Debug)]
struct WorldGenContext {
    task: Option<Task<()>>,
    pending: HashSet<Chunk>,
    running: Vec<Chunk>,
}

fn collect_world_gen(mut context: ResMut<WorldGenContext>, mut writer: EventWriter<ChunkLoad>) {
    if let Some(ref mut task) = context.task {
        if block_on(futures_lite::future::poll_once(task)).is_some() {
            context.task = None;
            context.running.drain(..).for_each(|chunk| {
                writer.send(ChunkLoad(chunk));
            });
        }
    }
}

fn dispatch_world_gen(mut context: ResMut<WorldGenContext>, mut reader: EventReader<ChunkGen>) {
    if !reader.is_empty() {
        reader.read().for_each(|&ChunkGen(chunk)| {
            context.pending.insert(chunk);
        });
    }

    if context.task.is_none() && !context.pending.is_empty() {
        context.running = context.pending.drain().collect();

        let chunks = context.running.clone();
        let task = AsyncComputeTaskPool::get().spawn(async {
            setup_gen_app(chunks).run();
        });

        context.task = Some(task);
    }
}
