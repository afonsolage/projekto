use bevy::asset::Assets;
use projekto_world_server::app;

fn main() {
    let mut server_app = app::new();

    let mut assets = server_app.world.resource::<Assets<()>>();

    server_app.run();
}
