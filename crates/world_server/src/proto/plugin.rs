use bevy::prelude::*;

use crate::{
    bundle::{ChunkLocal, ChunkVertex},
    proto::LandscapeSpawnReq,
    set::{ChunkLoad, Landscape},
};

use super::{
    channel::{WorldChannel, WorldChannelPair},
    ChunkLoadReq, ChunkVertexNfy, MessageType,
};

pub(crate) struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        let WorldChannelPair { client, server } = WorldChannel::new_pair();
        app.insert_resource(WorldClientChannel(client))
            .insert_resource(WorldServerChannel(server))
            .add_systems(
                Update,
                (
                    handle_request.run_if(has_request),
                    notify_chunk_vertex_updated,
                ),
            );
    }
}

#[derive(Resource, Debug, Deref)]
pub struct WorldServerChannel(WorldChannel);

#[derive(Resource, Debug, Clone, Deref)]
pub struct WorldClientChannel(WorldChannel);

fn has_request(channel: Res<WorldServerChannel>) -> bool {
    !channel.is_empty()
}

fn handle_request(
    channel: Res<WorldServerChannel>,
    mut commands: Commands,
    mut writer: EventWriter<ChunkLoad>,
) {
    while let Some(message) = channel.recv() {
        trace!("Handling request: {:?}", message.msg_type());
        match message.msg_type() {
            MessageType::ChunkLoadReq => {
                let ChunkLoadReq { chunk } = message.downcast().unwrap();
                writer.send(ChunkLoad(chunk));
            }
            MessageType::LandscapeSpawnReq => {
                let LandscapeSpawnReq { center, radius } = message.downcast().unwrap();
                commands.insert_resource(Landscape { center, radius });
            }
            _ => todo!(),
        }
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
        channel.send(ChunkVertexNfy {
            chunk: *chunk,
            vertex: vertex.clone(),
        });
    }
}
