use std::{thread, time::Duration};

use projekto_world_server::{
    app,
    channel::{ServerCommand, ServerCommandSender},
};

fn main() {
    let mut server_app = app::new();

    let sender = (*server_app.world.resource::<ServerCommandSender>()).clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1000));
        sender
            .send(ServerCommand::LandscapeAdd(
                Default::default(),
                Default::default(),
            ))
            .unwrap();
        thread::sleep(Duration::from_millis(1000));
        sender.send(ServerCommand::LandscapeRemove).unwrap();
    });

    server_app.run();
}
