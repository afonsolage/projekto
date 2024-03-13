use bevy::prelude::*;
use projekto_client::ClientPlugin;

fn main() {
    App::new().add_plugins(ClientPlugin).run();
}
