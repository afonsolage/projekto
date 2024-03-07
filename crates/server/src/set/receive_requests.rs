use std::sync::Arc;

use bevy::prelude::*;

use crate::proto::{client, RegisterMessageHandler};

use super::Landscape;

pub(crate) struct ReceiveRequestsPlugin;

impl Plugin for ReceiveRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message_handler(handle_landscape_update);
    }
}

fn handle_landscape_update(In(msg): In<Arc<client::LandscapeUpdate>>, mut commands: Commands) {
    commands.insert_resource(Landscape {
        center: msg.center,
        radius: msg.radius,
    });
}
