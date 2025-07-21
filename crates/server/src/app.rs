use std::time::{Duration, Instant};

use bevy::{
    app::{AppExit, PluginsState, ScheduleRunnerPlugin},
    ecs::event::ManualEventReader,
    prelude::*,
    tasks::AsyncComputeTaskPool,
};

use crate::{setup_chunk_asset_loader, WorldServerPlugin};

const TICK_EVERY_MILLIS: u64 = 50;

pub fn create() -> App {
    trace!("Creating app");
    let mut app = App::new();

    setup_chunk_asset_loader(&mut app);

    app.add_plugins((
        AssetPlugin::default(),
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
        WorldServerPlugin,
        AsyncRunnnerPlugin::new("WorldServer", Duration::from_millis(TICK_EVERY_MILLIS)),
    ));

    app
}

pub trait RunAsync {
    fn run_async(&mut self);
}

impl RunAsync for App {
    fn run_async(&mut self) {
        trace!("Running async!");
        let plugins = self.get_added_plugins::<AsyncRunnnerPlugin>();
        let async_plugin = plugins
            .last()
            .expect("AsyncRunnerPlugin must be added in order to use run_async");

        let name = async_plugin.0.clone();
        let tick_interval = async_plugin.1;
        self.set_runner(move |app| {
                AsyncRunnnerPlugin::run(app, name, tick_interval);
                AppExit::Success
        });
        self.run();
    }
}

enum TickResult {
    Exit,
    Wait(Duration),
    Over(Duration),
}

pub(crate) struct AsyncRunnnerPlugin(String, Duration);

impl AsyncRunnnerPlugin {
    pub(crate) fn new(name: impl ToString, tick_interval: Duration) -> Self {
        Self(name.to_string(), tick_interval)
    }

    fn run(mut app: App, name: String, tick_interval: Duration) {
        let app = app.main_mut();
        AsyncComputeTaskPool::get_or_init(Default::default)
            .spawn(async move {
                trace!("[{name}] starting runner.");
                let plugins_state = app.plugins_state();
                if plugins_state != PluginsState::Cleaned {
                    while app.plugins_state() == PluginsState::Adding {
                        futures_lite::future::yield_now().await;
                    }
                    app.finish();
                    app.cleanup();
                }

                let millis = tick_interval.as_millis();
                info!("[{name}] runner has started. Tick every {millis}ms");

                let mut reader = ManualEventReader::<AppExit>::default();
                loop {
                    match Self::tick(&mut app, tick_interval, &mut reader) {
                        TickResult::Wait(delay) => {
                            async_io::Timer::after(delay).await;
                        }
                        TickResult::Over(exe_time) => {
                            let millis = exe_time.as_millis();
                            warn!("[{name}] Tick duration greater than interval: {millis}ms");
                        }
                        TickResult::Exit => break,
                    }
                }
            })
            .detach();
    }

    fn tick(
        app: &mut SubApp,
        interval: Duration,
        reader: &mut ManualEventReader<AppExit>,
    ) -> TickResult {
        let start_time = Instant::now();

        app.update();

        if let Some(app_exit_events) = app.world_mut().get_resource_mut::<Events<AppExit>>() {
            if reader.read(&app_exit_events).last().is_some() {
                return TickResult::Exit;
            }
        }

        let end_time = Instant::now();

        let exe_time = end_time - start_time;

        if exe_time < interval {
            TickResult::Wait(interval - exe_time)
        } else {
            TickResult::Over(exe_time)
        }
    }
}

impl Plugin for AsyncRunnnerPlugin {
    fn build(&self, _app: &mut App) {}
}
