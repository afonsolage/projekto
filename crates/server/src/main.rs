use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use projekto_server::WorldServerPlugin;

const TICK_EVERY_MILLIS: u64 = 50;

fn main() {
    let mut app = App::new();

    projekto_server::setup_chunk_asset_loader(&mut app);

    app.add_plugins((
        LogPlugin::default(),
        AssetPlugin::default(),
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
        WorldServerPlugin,
    ))
    .run();
}
