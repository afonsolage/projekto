use bevy::prelude::*;
use projekto_server::proto::{client, WorldClientChannel};

use crate::WorldClientSet;

pub(crate) struct SendInputPlugin;

impl Plugin for SendInputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerLandscape {
            radius: 16,
            ..Default::default()
        })
        .add_systems(
            Update,
            update_player_landscape
                .run_if(resource_changed::<PlayerLandscape>)
                .in_set(WorldClientSet::SendInput),
        );
    }
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PlayerLandscape {
    pub center: IVec2,
    pub radius: u8,
}

fn update_player_landscape(channel: Res<WorldClientChannel>, landscape: Res<PlayerLandscape>) {
    let PlayerLandscape { center, radius } = *landscape;
    channel.send(client::LandscapeUpdate { center, radius });
}
