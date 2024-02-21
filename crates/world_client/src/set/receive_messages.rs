use bevy::prelude::*;
use projekto_world_server::proto::{
    ChunkVertexNfy, MessageQueue, MessageSource, ServerMessage, WorldClientChannel,
};

use crate::WorldClientSet;

pub(crate) struct ReceiveMessagesPlugin;

impl Plugin for ReceiveMessagesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_messages.run_if(has_messages),).in_set(WorldClientSet::ReceiveMessages),
        );
    }
}

fn has_messages(channel: Res<WorldClientChannel>) -> bool {
    !channel.is_empty()
}

fn handle_messages(world: &mut World) {
    let channel = world.resource::<WorldClientChannel>();

    for message in channel.recv_all() {
        let MessageSource::Server(message_type) = message.msg_source() else {
            error!("Ignoring server message received: {message:?}");
            continue;
        };

        trace!("Handling server message: {:?}", message.msg_source());
        match message_type {
            ServerMessage::ChunkVertex => {
                let message = message.downcast().unwrap();
                world
                    .get_resource_or_insert_with::<MessageQueue<ChunkVertexNfy>>(Default::default)
                    .push(message);
            }
        }
    }
}
