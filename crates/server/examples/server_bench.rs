use std::time::Duration;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    log::LogPlugin,
    prelude::*,
    tasks::block_on,
};
use projekto_archive::ArchiveServer;
use projekto_server::{ChunkAsset, WorldServerPlugin, bundle::ChunkMap, set::Landscape};

const TICK_EVERY_MILLIS: u64 = 50;

fn main() {
    let mut app = App::new();

    app.add_plugins((
        LogPlugin::default(),
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
        radius: 32,
    });
}

fn check_if_finished(
    map: Res<ChunkMap>,
    mut archive_server: ResMut<ArchiveServer<ChunkAsset>>,
    mut exit: EventWriter<AppExit>,
) {
    if map.len() >= 4225 {
        exit.write(AppExit::Success);
        block_on(archive_server.do_maintenance_stuff());
    }
}
