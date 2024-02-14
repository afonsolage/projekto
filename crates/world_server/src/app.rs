use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};

use crate::{setup_chunk_asset_loader, WorldServerPlugin};

const TICK_EVERY_MILLIS: u64 = 50;

pub fn new() -> App {
    let mut app = App::new();

    setup_chunk_asset_loader(&mut app);

    app.add_plugins((
        WorldServerPlugin,
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
        LogPlugin::default(),
        AssetPlugin::default(),
    ));

    app
}
