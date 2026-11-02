use bevy::{
    prelude::*,
    window::{PresentMode, WindowPlugin},
};
use projekto_client::ClientPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    present_mode: PresentMode::AutoNoVsync,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(ImagePlugin::default_nearest()),
        ClientPlugin,
    ));

    #[cfg(feature = "debug-tools")]
    app.add_plugins(projekto_debug::ClientDebugPlugins::new(15712));

    app.run();
}
