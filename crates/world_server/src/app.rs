use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};

use crate::WorldServerPlugin;

const TICK_EVERY_MILLIS: u64 = 50;

pub fn new() -> App {
    let mut app = App::new();

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
