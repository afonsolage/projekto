use projekto_world_server::{app, Landscape};

fn main() {
    let mut server_app = app::new();
    server_app.world.insert_resource(Landscape::default());
    server_app.run();
}
