use bevy::prelude::*;

use crate::proto::handle_client_messages;

pub(crate) struct ReceiveRequestsPlugin;

impl Plugin for ReceiveRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, handle_client_messages);
    }
}
