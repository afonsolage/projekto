use bevy::prelude::*;

use crate::{
    bundle::{ChunkLocal, ChunkVertex},
    net::Clients,
    WorldSet,
};
use projekto_messages as messages;

pub(crate) struct SendResponsesPlugin;

impl Plugin for SendResponsesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            notify_chunk_vertex_updated.in_set(WorldSet::SendResponses),
        );
    }
}

fn notify_chunk_vertex_updated(
    clients: Res<Clients>,
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
        if vertex.is_empty() {
            continue;
        }
        for client in clients.values() {
            let _ = client.channel().send(messages::ChunkVertex {
                chunk: *chunk,
                vertex: vertex.clone(),
            });
        }
    }
}
