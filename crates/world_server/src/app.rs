use std::time::Duration;

use bevy::{
    app::ScheduleRunnerPlugin,
    log::LogPlugin,
    prelude::*,
    tasks::{block_on, AsyncComputeTaskPool, Task},
    utils::HashSet,
};
use projekto_core::chunk::Chunk;

use crate::{
    channel::WorldServerChannelPlugin, gen::setup_gen_app, set::ChunkGen, WorldServerPlugin,
};

const TICK_EVERY_MILLIS: u64 = 50;

pub fn new() -> App {
    let mut app = App::new();

    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
        LogPlugin::default(),
        WorldServerPlugin,
        WorldServerChannelPlugin,
    ));
    app.add_systems(Update, terrain_generator);

    app
}

#[derive(Default, Debug)]
struct TerrainGenCache {
    task: Option<Task<()>>,
    pending: HashSet<Chunk>,
    running: Vec<Chunk>,
}

fn terrain_generator(mut local: Local<TerrainGenCache>, mut reader: EventReader<ChunkGen>) {
    if !reader.is_empty() {
        reader.read().for_each(|&ChunkGen(chunk)| {
            local.pending.insert(chunk);
        });
    }

    if local.task.is_none() && !local.pending.is_empty() {
        local.running = local.pending.drain().collect();

        let chunks = local.running.clone();
        let task = AsyncComputeTaskPool::get().spawn(async {
            setup_gen_app(chunks).run();
        });

        local.task = Some(task);
    } else if let Some(ref mut task) = local.task {
        if let Some(_result) = block_on(futures_lite::future::poll_once(task)) {
            //
            local.task = None;
            local.running.clear();
        }
    }
}
