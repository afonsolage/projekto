use std::time::Duration;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    log::LogPlugin,
    prelude::*,
};
use projekto_server::{set::Landscape, ChunkAssetHandle, WorldServerPlugin};

const TICK_EVERY_MILLIS: u64 = 50;

fn main() {
    let mut app = App::new();

    app.add_plugins(LogPlugin::default());

    // TODO: Rework this when plugins dependencies is a thing in bevy
    projekto_server::setup_chunk_asset_loader(&mut app);

    app.add_plugins((
        AssetPlugin::default(),
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
        WorldServerPlugin,
    ))
    .add_systems(Startup, setup)
    .add_systems(Last, check_if_finished)
    .run();
}

fn setup(mut commands: Commands) {
    commands.insert_resource(Landscape {
        center: IVec2::ZERO,
        radius: 4,
    });
}

fn check_if_finished(q: Query<(Entity, &ChunkAssetHandle)>, mut exit: EventWriter<AppExit>) {
    if q.is_empty() {
        exit.write(AppExit::Success);
    }
}
