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

fn handle_landscape_update(In((id, msg)): In<(u32, LandscapeUpdate)>, mut commands: Commands) {
    trace!("[{id}], handle_landscape_update");
    commands.insert_resource(Landscape {
        center: msg.center,
        radius: msg.radius,
    });
}
