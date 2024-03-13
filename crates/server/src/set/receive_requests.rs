use bevy::prelude::*;

use projekto_messages::LandscapeUpdate;
use projekto_proto::RegisterMessageHandler;

use super::Landscape;

pub(crate) struct ReceiveRequestsPlugin;

impl Plugin for ReceiveRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message_handler(handle_landscape_update);
    }
}

fn handle_landscape_update(In(msg): In<LandscapeUpdate>, mut commands: Commands) {
    commands.insert_resource(Landscape {
        center: msg.center,
        radius: msg.radius,
    });
}
