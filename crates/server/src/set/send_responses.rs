use bevy::{platform::collections::HashMap, prelude::*};
use projekto_core::coords::Chunk;

use crate::{
    WorldSet,
    bundle::{ChunkLocal, ChunkVertex},
    net::{ClientSync, Clients},
};
use projekto_messages as messages;

pub(crate) struct SendResponsesPlugin;

impl Plugin for SendResponsesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (notify_chunk_vertex_updated, resync_clients)
                .chain()
                .in_set(WorldSet::SendResponses),
        );
    }
}

fn notify_chunk_vertex_updated(
    clients: Res<Clients>,
    mut sync: ResMut<ClientSync>,
    q: Query<(&ChunkLocal, &ChunkVertex), Changed<ChunkVertex>>,
) {
    if q.is_empty() {
        return;
    }

    if clients.is_empty() {
        debug!("No clients connected. Skipping chunk update notify");
        return;
    }

    for (ChunkLocal(chunk), ChunkVertex(vertex)) in &q {
        for id in clients.keys() {
            if should_queue_update(sync.is_sent(*id, *chunk), vertex.is_empty()) {
                sync.queue_update(*id, *chunk);
            }
        }
    }
}

/// Empty updates must be sent so clients drop stale meshes for chunks that became fully
/// occluded. A client that never had the chunk does not need an empty mesh.
fn should_queue_update(already_sent: bool, vertex_is_empty: bool) -> bool {
    !vertex_is_empty || already_sent
}

/// Sends queued updates and unloads that did not fit in a client's bounded channel.
fn resync_clients(
    clients: Res<Clients>,
    mut sync: ResMut<ClientSync>,
    q: Query<(&ChunkLocal, &ChunkVertex)>,
) {
    if clients.is_empty() {
        return;
    }

    let need_vertices = clients.keys().any(|id| sync.has_pending_updates(*id));

    let vertices: HashMap<Chunk, &Vec<projekto_core::voxel::Vertex>> = if need_vertices {
        q.iter()
            .map(|(ChunkLocal(chunk), ChunkVertex(vertex))| (*chunk, vertex))
            .collect()
    } else {
        HashMap::default()
    };

    for (id, client) in clients.iter() {
        for chunk in sync.pending_unloads(*id) {
            let sent = client
                .channel()
                .send(messages::ChunkUnload { chunk })
                .is_ok();

            if sent {
                sync.mark_unload_sent(*id, chunk);
            } else {
                break;
            }
        }

        for chunk in sync.pending_updates(*id) {
            let Some(vertex) = vertices.get(&chunk) else {
                // The chunk was unloaded before the update was delivered. If the client had
                // already received it, it also needs the unload message.
                if sync.remove_chunk(*id, chunk) {
                    sync.queue_unload(*id, chunk);
                }
                continue;
            };

            let sent = client
                .channel()
                .send(messages::ChunkVertex {
                    chunk,
                    vertex: (*vertex).clone(),
                })
                .is_ok();

            if sent {
                sync.mark_update_sent(*id, chunk);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use projekto_core::coords::Chunk;
    use projekto_proto::ClientId;

    use super::*;

    #[test]
    fn empty_update_is_queued_only_for_clients_that_had_the_chunk() {
        assert!(should_queue_update(true, true));
        assert!(!should_queue_update(false, true));
        assert!(should_queue_update(false, false));
    }

    #[test]
    fn queued_empty_update_is_kept_for_resync() {
        let id = ClientId::default();
        let chunk = Chunk::new(1, 2);
        let mut sync = ClientSync::default();

        sync.mark_update_sent(id, chunk);
        sync.queue_update(id, chunk);

        assert!(sync.has_pending_updates(id));
        assert_eq!(sync.pending_updates(id), vec![chunk]);
    }
}
