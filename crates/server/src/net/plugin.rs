use std::sync::mpsc::{self, Receiver};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, TaskPool},
    utils::{synccell::SyncCell, HashMap},
};

use crate::proto::{client::ClientMessage, server::ServerMessage, MessageType};

use super::Client;

pub(crate) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clients>()
            .add_systems(Startup, start_network_server)
            .add_systems(
                PreUpdate,
                (
                    new_client_connected,
                    remove_disconnected_clients,
                    handle_messages,
                ),
            );
    }
}

#[derive(Resource, Default, Deref, DerefMut)]
struct Clients(HashMap<u32, Client<ClientMessage, ServerMessage>>);

#[derive(Resource, Deref, DerefMut)]
struct OnClientConnectedReceiver(SyncCell<Receiver<Client<ClientMessage, ServerMessage>>>);

fn start_network_server(mut commands: Commands) {
    let (sender, receiver) = mpsc::channel();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            let _ = super::start_server(|client| {
                let id = client.id();
                if let Err(err) = sender.send(client) {
                    error!("Failed to get client {id}. Error: {err}");
                }
            })
            .await;
        })
        .detach();

    commands.insert_resource(OnClientConnectedReceiver(SyncCell::new(receiver)));
}

fn remove_disconnected_clients(mut clients: ResMut<Clients>) {
    clients.retain(|_, client| {
        if client.is_closed() {
            let id = client.id();
            let addr = client.addr();
            debug!("[Networking] Removing disconnected client {id}({addr})");
            false
        } else {
            true
        }
    });
}

fn new_client_connected(
    mut receiver: ResMut<OnClientConnectedReceiver>,
    mut clients: ResMut<Clients>,
) {
    for new_client in receiver.get().try_iter() {
        let id = new_client.id();
        if clients.insert(id, new_client).is_some() {
            panic!("Duplicated client detected {id}.");
        }
    }
}

fn handle_messages(world: &mut World) {
    world.resource_scope(|world, clients: Mut<Clients>| {
        for client in clients.values() {
            while let Some(boxed) = client.channel().recv() {
                let msg_type = boxed.msg_type();
                msg_type.run_handlers(boxed, world);
            }
        }
    });
}
