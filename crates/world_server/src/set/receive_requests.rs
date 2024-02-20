use bevy::prelude::*;

use crate::{
    proto::{ChunkLoadReq, LandscapeSpawnReq, MessageType, WorldServerChannel},
    set::Landscape,
    WorldSet,
};

use super::ChunkLoad;

pub(crate) struct ReceiveRequestsPlugin;

impl Plugin for ReceiveRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            handle_request
                .run_if(has_request)
                .in_set(WorldSet::ReceiveRequests),
        );
    }
}

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
