use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use projekto_server::{WorldServerPlugin, debug, set::Landscape};

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
    .add_systems(PostStartup, set_landscape)
    .add_systems(PostUpdate, print_metrics)
    .run();
}

fn set_landscape(mut commands: Commands) {
    commands.insert_resource(Landscape {
        center: IVec2::ZERO,
        radius: 32,
    });
}

fn print_metrics(time: Res<Time>, metrics: Res<debug::Metrics>, mut last_run: Local<f64>) {
    *last_run += time.elapsed_secs_f64();

    if *last_run < 5.0 {
        return;
    }

    *last_run = 0.0;

    metrics.print();
}
