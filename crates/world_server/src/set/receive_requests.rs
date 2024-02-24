use bevy::prelude::*;

use crate::{
    proto::{client, MessageSource, WorldServerChannel},
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
        let MessageSource::Client(message_type) = message.msg_source() else {
            error!("Ignoring server message received: {message:?}");
            continue;
        };

        trace!("Handling client message: {:?}", message.msg_source());

        match message_type {
            client::ClientMessage::ChunkLoad => {
                let client::ChunkLoad { chunk } = message.downcast().unwrap();
                writer.send(ChunkLoad(chunk));
            }
            client::ClientMessage::LandscapeUpdate => {
                let client::LandscapeUpdate { center, radius } = message.downcast().unwrap();
                commands.insert_resource(Landscape { center, radius });
            }
        }
    }
}
