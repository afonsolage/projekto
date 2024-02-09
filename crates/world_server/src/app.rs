use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};

use crate::{channel::WorldServerChannelPlugin, WorldServerPlugin};

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

    app
}
