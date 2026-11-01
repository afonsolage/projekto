use std::sync::mpsc::{self, Receiver};

use bevy::{
    platform::{
        cell::SyncCell,
        collections::{HashMap, HashSet},
    },
    prelude::*,
    tasks::{AsyncComputeTaskPool, TaskPool},
};
use projekto_core::coords::Chunk;
use projekto_messages::{ClientMessage, ServerMessage};
use projekto_proto::{Client, ClientId, MessageType};

pub(crate) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clients>()
            .init_resource::<ClientSync>()
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
pub struct Clients(HashMap<ClientId, Client<ClientMessage, ServerMessage>>);

#[derive(Default)]
pub struct ClientSyncState {
    sent: HashSet<Chunk>,
    pending_updates: HashSet<Chunk>,
    pending_unloads: HashSet<Chunk>,
}

/// Tracks which chunks each client already received and which updates still need to be sent.
///
/// The server-to-client channel is bounded, so sends are best-effort. Updates and unloads that
/// do not fit are queued here and retried by the resync system instead of blocking the ECS
/// frame. Queuing updates per chunk also coalesces repeated changes into one last-write-wins
/// send.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct ClientSync(HashMap<ClientId, ClientSyncState>);

impl ClientSync {
    pub fn insert_client(&mut self, client: ClientId) {
        self.insert(client, ClientSyncState::default());
    }

    /// Queues the latest vertex state of `chunk` to be sent to `client`.
    pub fn queue_update(&mut self, client: ClientId, chunk: Chunk) {
        self.entry(client)
            .or_default()
            .pending_updates
            .insert(chunk);
    }

    pub fn pending_updates(&self, client: ClientId) -> Vec<Chunk> {
        self.get(&client)
            .map(|state| state.pending_updates.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn has_pending_updates(&self, client: ClientId) -> bool {
        self.get(&client)
            .is_some_and(|state| !state.pending_updates.is_empty())
    }

    /// Marks an update as delivered. This also cancels an obsolete pending unload when the
    /// same chunk was reloaded before its unload message was sent.
    pub fn mark_update_sent(&mut self, client: ClientId, chunk: Chunk) {
        let state = self.entry(client).or_default();
        state.pending_updates.remove(&chunk);
        state.pending_unloads.remove(&chunk);
        state.sent.insert(chunk);
    }

    pub fn is_sent(&self, client: ClientId, chunk: Chunk) -> bool {
        self.get(&client)
            .is_some_and(|state| state.sent.contains(&chunk))
    }

    /// Forgets a chunk that the client no longer has, including any queued update.
    ///
    /// Returns `true` when the client had received the chunk, meaning it also needs an unload.
    pub fn remove_chunk(&mut self, client: ClientId, chunk: Chunk) -> bool {
        self.get_mut(&client).is_some_and(|state| {
            state.pending_updates.remove(&chunk);
            state.sent.remove(&chunk)
        })
    }

    /// Remembers an unload that could not be queued, so it can be retried.
    pub fn queue_unload(&mut self, client: ClientId, chunk: Chunk) {
        self.entry(client)
            .or_default()
            .pending_unloads
            .insert(chunk);
    }

    pub fn pending_unloads(&self, client: ClientId) -> Vec<Chunk> {
        self.get(&client)
            .map(|state| state.pending_unloads.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn mark_unload_sent(&mut self, client: ClientId, chunk: Chunk) {
        if let Some(state) = self.get_mut(&client) {
            state.pending_unloads.remove(&chunk);
        }
    }
}

#[derive(Resource, Deref, DerefMut)]
struct OnClientConnectedReceiver(SyncCell<Receiver<Client<ClientMessage, ServerMessage>>>);

fn start_network_server(mut commands: Commands) {
    let (sender, receiver) = mpsc::channel();
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            let _ = projekto_proto::start_server("127.0.0.1:11223", |client| {
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

fn remove_disconnected_clients(mut clients: ResMut<Clients>, mut sync: ResMut<ClientSync>) {
    let mut removed = Vec::new();

    clients.retain(|&id, client| {
        if client.is_closed() {
            let addr = client.addr();
            debug!("[Networking] Removing disconnected client {id}({addr})");
            removed.push(id);
            false
        } else {
            true
        }
    });

    for id in removed {
        sync.remove(&id);
    }
}

fn new_client_connected(
    mut receiver: ResMut<OnClientConnectedReceiver>,
    mut clients: ResMut<Clients>,
    mut sync: ResMut<ClientSync>,
) {
    for new_client in receiver.get().try_iter() {
        let id = new_client.id();
        sync.insert_client(id);
        if clients.insert(id, new_client).is_some() {
            panic!("Duplicated client detected {id}.");
        }
    }
}

fn handle_messages(world: &mut World) {
    let clients = world
        .resource::<Clients>()
        .iter()
        .map(|(id, client)| (*id, client.channel().try_recv_all()))
        .collect::<Vec<_>>();

    for (id, messages) in clients {
        for boxed in messages {
            let msg_type = boxed.msg_type();
            msg_type.run_handlers(boxed, id, world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_updates_are_deduplicated() {
        let chunk = Chunk::new(1, 2);
        let id = ClientId::default();
        let mut sync = ClientSync::default();

        sync.queue_update(id, chunk);
        sync.queue_update(id, chunk);

        assert_eq!(sync.pending_updates(id), vec![chunk]);
    }

    #[test]
    fn delivered_update_marks_chunk_as_sent_and_cancels_unload() {
        let chunk = Chunk::new(1, 2);
        let id = ClientId::default();
        let mut sync = ClientSync::default();

        sync.queue_update(id, chunk);
        sync.queue_unload(id, chunk);
        sync.mark_update_sent(id, chunk);

        assert!(sync.is_sent(id, chunk));
        assert!(sync.pending_updates(id).is_empty());
        assert!(sync.pending_unloads(id).is_empty());
    }

    #[test]
    fn remove_chunk_forgets_sent_state_and_queued_update() {
        let chunk = Chunk::new(1, 2);
        let id = ClientId::default();
        let mut sync = ClientSync::default();

        sync.mark_update_sent(id, chunk);
        sync.queue_update(id, chunk);

        assert!(sync.remove_chunk(id, chunk));
        assert!(!sync.remove_chunk(id, chunk));
        assert!(!sync.is_sent(id, chunk));
        assert!(sync.pending_updates(id).is_empty());
    }

    #[test]
    fn pending_unloads_can_be_retried() {
        let id = ClientId::default();
        let chunk = Chunk::new(1, 2);
        let mut sync = ClientSync::default();

        sync.queue_unload(id, chunk);
        assert_eq!(sync.pending_unloads(id), vec![chunk]);
        sync.mark_unload_sent(id, chunk);
        assert!(sync.pending_unloads(id).is_empty());
    }
}
