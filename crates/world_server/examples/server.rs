use projekto_world_server::set::Landscape;

fn main() {
    let mut app = projekto_world_server::app::create();
    app.insert_resource(Landscape {
        radius: 1,
        ..Default::default()
    })
    .run();
}
