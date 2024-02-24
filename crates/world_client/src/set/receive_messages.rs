use bevy::prelude::*;
use projekto_world_server::proto::handle_server_messages;

pub(crate) struct ReceiveMessagesPlugin;

impl Plugin for ReceiveMessagesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, handle_server_messages);
    }
}
