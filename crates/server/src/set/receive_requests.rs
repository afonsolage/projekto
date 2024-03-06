use std::sync::Arc;

use bevy::prelude::*;

use crate::proto::{client, handle_client_messages, RegisterMessageHandler};

use super::Landscape;

pub(crate) struct ReceiveRequestsPlugin;

impl Plugin for ReceiveRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, handle_client_messages)
            .add_message_handler(handle_landscape_update);
    }
}

fn handle_landscape_update(In(msg): In<Arc<client::LandscapeUpdate>>, mut commands: Commands) {
    commands.insert_resource(Landscape {
        center: msg.center,
        radius: msg.radius,
    });
}
