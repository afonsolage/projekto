use bevy::prelude::*;

use projekto_messages::LandscapeUpdate;
use projekto_proto::{ClientId, RegisterMessageHandler};

use crate::{
    bundle::{ChunkLocal, ChunkVertex},
    net::Clients,
};

use super::Landscape;

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
    mut commands: Commands,
) {
    trace!("[{id}], handle_landscape_update");

    commands.insert_resource(Landscape {
        center: msg.center,
        radius: msg.radius,
    });

    for (ChunkLocal(chunk), ChunkVertex(vertex)) in &q {
        if vertex.is_empty() {
            continue;
        }

        if let Some(client) = clients.get(&id) {
            let _ = client.channel().send(projekto_messages::ChunkVertex {
                chunk: *chunk,
                vertex: vertex.clone(),
            });
        }
    }
}
