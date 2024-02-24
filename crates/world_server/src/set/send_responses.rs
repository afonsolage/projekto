use bevy::prelude::*;

use crate::{
    bundle::{ChunkLocal, ChunkVertex},
    proto::{server, WorldServerChannel},
    WorldSet,
};

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
    channel: Res<WorldServerChannel>,
    q: Query<(&ChunkLocal, &ChunkVertex), Changed<ChunkVertex>>,
) {
    for (ChunkLocal(chunk), ChunkVertex(vertex)) in &q {
        if vertex.is_empty() {
            continue;
        }
        channel.send(server::ChunkVertex {
            chunk: *chunk,
            vertex: vertex.clone(),
        });
    }
}
