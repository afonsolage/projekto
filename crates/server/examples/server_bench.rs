use std::time::Duration;

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    log::LogPlugin,
    prelude::*,
    tasks::Task,
};
use projekto_archive::ArchiveServer;
use projekto_archive::MaintenanceResult;
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
    mut exit: EventWriter<AppExit>,
    mut archive_server: ResMut<ArchiveServer<ChunkAsset>>,
    mut local: Local<Option<Task<MaintenanceResult>>>,
) {
    if map.len() >= 4225 {
        if local.is_none() {
            *local = Some(archive_server.do_maintenance_stuff());
        } else if let Some(task) = &*local
            && task.is_finished()
        {
            exit.write(AppExit::Success);
        }
    }
}
