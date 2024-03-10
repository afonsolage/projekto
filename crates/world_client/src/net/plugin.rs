use std::io;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool},
};
use futures_lite::future::{block_on, poll_once};
use projekto_server::proto::{client::ClientMessage, server::ServerMessage, MessageType};

use super::Server;

pub(crate) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (
                reconnect_to_server.run_if(resource_exists::<ServerConnection>),
                connect_to_server.run_if(not(resource_exists::<ServerConnection>)),
                handle_messages.run_if(resource_exists::<ServerConnection>),
            ),
        );
    }
}

#[derive(Resource, Debug, Deref, DerefMut)]
struct ServerConnection(Server<ClientMessage, ServerMessage>);

impl ServerConnection {
    pub(crate) fn is_active(&self) -> bool {
        !self.is_closed() || !self.channel().is_empty()
    }
}

fn reconnect_to_server(connection: Res<ServerConnection>, mut commands: Commands) {
    if !connection.is_active() {
        let _ = commands.remove_resource::<ServerConnection>();
    }
}

type ConnectToServerResult = Result<Server<ClientMessage, ServerMessage>, io::Error>;
fn connect_to_server(
    mut commands: Commands,
    mut connecting_task: Local<Option<Task<ConnectToServerResult>>>,
) {
    if let Some(ref mut task) = *connecting_task {
        if let Some(result) = block_on(poll_once(task)) {
            match result {
                Ok(server) => commands.insert_resource(ServerConnection(server)),
                Err(err) => {
                    error!("Failed to connect to server. Error: {err}");
                }
            }
            let _ = connecting_task.take();
        }
    } else {
        let task = AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move { super::connect_to_server().await });
        *connecting_task = Some(task);
    }
}

fn handle_messages(world: &mut World) {
    world.resource_scope(|world, server: Mut<ServerConnection>| {
        while let Some(boxed) = server.channel().recv() {
            let msg_type = boxed.msg_type();
            msg_type.run_handlers(boxed, world);
        }
    });
}
