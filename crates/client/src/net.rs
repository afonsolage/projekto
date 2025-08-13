use std::{
    io,
    time::{Duration, Instant},
};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool},
};
use futures_lite::future::{block_on, poll_once};
use projekto_messages::{ClientMessage, ServerMessage};
use projekto_proto::{ClientId, MessageType, Server, connect_to_server};

pub(crate) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ServerDisconnected>().add_systems(
            PreUpdate,
            (
                reconnect_to_server.run_if(resource_exists::<ServerConnection>),
                server_connection.run_if(not(resource_exists::<ServerConnection>)),
                handle_messages.run_if(resource_exists::<ServerConnection>),
            ),
        );
    }
}

#[derive(Resource, Debug, Deref, DerefMut)]
pub struct ServerConnection(Server<ClientMessage, ServerMessage>);

#[derive(Event, Debug)]
pub struct ServerDisconnected;

impl ServerConnection {
    pub(crate) fn is_active(&self) -> bool {
        !self.is_closed() || !self.channel().is_empty()
    }
}

fn reconnect_to_server(
    connection: Res<ServerConnection>,
    mut writer: EventWriter<ServerDisconnected>,
    mut commands: Commands,
) {
    if !connection.is_active() {
        info!("Server connected is broken. Reconnecting...");
        commands.remove_resource::<ServerConnection>();
        writer.write(ServerDisconnected);
    }
}

type ConnectToServerResult = Result<Server<ClientMessage, ServerMessage>, io::Error>;

struct Meta {
    task: Option<Task<ConnectToServerResult>>,
    next_try: Instant,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            task: Default::default(),
            next_try: Instant::now(),
        }
    }
}

fn server_connection(mut commands: Commands, mut meta: Local<Meta>) {
    if let Some(ref mut task) = meta.task {
        if let Some(result) = block_on(poll_once(task)) {
            match result {
                Ok(server) => {
                    info!("Connected to server!");
                    commands.insert_resource(ServerConnection(server));
                }
                Err(err) => {
                    error!("Failed to connect to server. Error: {err}");
                    meta.next_try = Instant::now() + Duration::from_secs(1);
                }
            }
            let _ = meta.task.take();
        }
    } else if meta.next_try <= Instant::now() {
        let task = AsyncComputeTaskPool::get_or_init(TaskPool::default)
            .spawn(async move { connect_to_server("127.0.0.1:11223").await });
        meta.task = Some(task);
    }
}

fn handle_messages(world: &mut World) {
    world.resource_scope(|world, server: Mut<ServerConnection>| {
        while let Some(boxed) = server.channel().try_recv() {
            let msg_type = boxed.msg_type();
            msg_type.run_handlers(boxed, ClientId::default(), world);
        }
    });
}
