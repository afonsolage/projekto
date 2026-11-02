use bevy::prelude::*;
use projekto_client::ClientPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(ClientPlugin);

    #[cfg(feature = "debug-tools")]
    app.add_plugins(projekto_client::debug::DebugToolsPlugins);

    app.run();
}
