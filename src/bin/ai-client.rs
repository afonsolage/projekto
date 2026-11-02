use std::time::Duration;

use bevy::{
    app::{PluginGroup, ScheduleRunnerPlugin},
    prelude::*,
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use projekto_client::ClientPlugin;
use projekto_debug::{AiClientPlugin, ClientDebugPlugins};

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..Default::default()
            })
            .disable::<WinitPlugin>()
            .set(ImagePlugin::default_nearest()),
        ScheduleRunnerPlugin::run_loop(Duration::from_secs(1)),
        ClientPlugin,
        ClientDebugPlugins::new(15722),
        AiClientPlugin,
    ))
    .run();
}
