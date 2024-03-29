use bevy::prelude::*;

use crate::{net::ServerConnection, ClientSet};

pub(crate) struct SendInputPlugin;

impl Plugin for SendInputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerLandscape {
            radius: 32,
            ..Default::default()
        })
        .add_systems(
            PostUpdate,
            (
                update_player_landscape.run_if(resource_changed::<PlayerLandscape>),
                send_welcome_message.run_if(resource_added::<ServerConnection>),
            )
                .in_set(ClientSet::SendInput),
        );
    }
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PlayerLandscape {
    pub center: IVec2,
    pub radius: u8,
}

fn update_player_landscape(server: Res<ServerConnection>, landscape: Res<PlayerLandscape>) {
    let PlayerLandscape { center, radius } = *landscape;
    let _ = server
        .channel()
        .send(projekto_messages::LandscapeUpdate { center, radius });
}

fn send_welcome_message(server: Res<ServerConnection>, landscape: Res<PlayerLandscape>) {
    let PlayerLandscape { center, radius } = *landscape;
    let _ = server
        .channel()
        .send(projekto_messages::LandscapeUpdate { center, radius });
}
