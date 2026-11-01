use bevy::prelude::*;

use projekto_messages::LandscapeUpdate;
use projekto_proto::{ClientId, RegisterMessageHandler};

use crate::{
    bundle::{ChunkLocal, ChunkVertex},
    net::{ClientSync, Clients},
};

use super::{Landscape, MAX_LANDSCAPE_RADIUS};

pub(crate) struct ReceiveRequestsPlugin;

impl Plugin for ReceiveRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message_handler(handle_landscape_update);
    }
}

fn handle_landscape_update(
    In((id, msg)): In<(ClientId, LandscapeUpdate)>,
    q: Query<(&ChunkLocal, &ChunkVertex)>,
    clients: Res<Clients>,
    landscape: Option<Res<Landscape>>,
    mut sync: ResMut<ClientSync>,
    mut commands: Commands,
) {
    trace!("[{id}], handle_landscape_update");

    if clients.get(&id).is_none() {
        return;
    }

    let radius = msg.radius.min(MAX_LANDSCAPE_RADIUS);
    let changed = landscape
        .as_ref()
        .is_none_or(|current| current.center != msg.center || current.radius != radius);

    if changed {
        commands.insert_resource(Landscape {
            center: msg.center,
            radius,
        });
    }

    // Queue the chunks this client has not received yet. Updates for chunks it already knows
    // about are queued by `notify_chunk_vertex_updated` when their vertices change.
    for (ChunkLocal(chunk), ChunkVertex(vertex)) in &q {
        if !vertex.is_empty() && !sync.is_sent(id, *chunk) {
            sync.queue_update(id, *chunk);
        }
    }
}
